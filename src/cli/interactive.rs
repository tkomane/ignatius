//! The interactive runtime.
//!
//! This is the only place where the pure reducer meets real input, a real
//! terminal, and a real database. It owns three things and nothing else:
//!
//! - the terminal, through an RAII guard that restores it on every exit path
//! - a channel of messages, fed by the keyboard, by signals, and by async work
//! - an executor that turns [`Effect`] values into tasks
//!
//! Input is read on its own thread and database work runs on the async runtime,
//! so neither can block drawing. Nothing here decides application behaviour;
//! that all lives in [`crate::app::update()`].

use crate::ExitCode;
use crate::app::{Effect, Message, Model, update};
use crate::cli::{ConnectionOptions, Presentation};
use crate::config::{Config, Paths};
use crate::connection::{ConnectionTarget, EnvSnapshot};
use crate::diagnostics::doctor::{Check, CheckStatus};
use crate::diagnostics::{Diagnostic, DiagnosticKind};
use crate::postgres::{Session, session};
use crate::query::result::{
    Execution, ExecutionStatus, JobId, NoticeCounts, NoticeReport, Reported,
};
use crate::ui::keymap::Keymap;
use crate::ui::layout;
use crossterm::event::{self, Event};
use std::io::Write;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

/// How often the input thread checks whether it should stop.
const INPUT_POLL: Duration = Duration::from_millis(100);

/// How often the interface redraws while something is happening.
///
/// Animation ticks pause while idle. Notice accounting has its own bounded
/// polling interval below, independent of animation activity.
const TICK: Duration = Duration::from_millis(90);

type SessionSlot = Arc<tokio::sync::RwLock<Option<Arc<InstalledSession>>>>;

#[derive(Debug)]
struct InstalledSession {
    session: Arc<Session>,
    session_id: crate::app::model::NoticeSessionId,
    generation: u64,
    source: crate::app::model::NoticeSource,
    retired: std::sync::atomic::AtomicBool,
}
impl std::ops::Deref for InstalledSession {
    type Target = Session;
    fn deref(&self) -> &Self::Target {
        &self.session
    }
}
struct SessionInstall {
    opened: Arc<InstalledSession>,
    acknowledged: tokio::sync::oneshot::Sender<bool>,
}

async fn request_session_install(
    tx: &mpsc::Sender<SessionInstall>,
    opened: Session,
    source: crate::app::model::NoticeSource,
    generation: u64,
) -> bool {
    let (acknowledged, result) = tokio::sync::oneshot::channel();
    let install = SessionInstall {
        opened: Arc::new(InstalledSession {
            session: Arc::new(opened),
            session_id: crate::app::model::NoticeSessionId::new(),
            generation,
            source,
            retired: std::sync::atomic::AtomicBool::new(false),
        }),
        acknowledged,
    };
    if tx.send(install).await.is_err() {
        return false;
    }
    result.await.unwrap_or(false)
}

/// Nonblocking installation keeps a held metadata reader from freezing input.
fn try_session_install(
    install: SessionInstall,
    slot: &SessionSlot,
    model: &mut Model,
    generation: u64,
) -> Option<SessionInstall> {
    if model.should_quit
        || install.opened.generation != generation
        || model.connection_generation != generation
    {
        let _ = install.acknowledged.send(false);
        return None;
    }
    let Ok(mut held) = slot.try_write() else {
        return Some(install);
    };
    if let Some(old) = held.as_ref() {
        retire_notice_source(model, old);
        update(model, Message::NoticeRetirementFinished(old.generation));
    }
    let opened = Arc::clone(&install.opened);
    update(
        model,
        connection_message(
            opened.generation,
            Message::NoticeSourceInstalled {
                source: opened.source,
                session_id: opened.session_id.clone(),
            },
        ),
    );
    *held = Some(opened);
    let _ = install.acknowledged.send(true);
    None
}

/// This is called only at actual replacement/Quit/error retirement boundaries.
fn retire_notice_source(model: &mut Model, source: &InstalledSession) {
    if source
        .retired
        .swap(true, std::sync::atomic::Ordering::SeqCst)
    {
        return;
    }
    let generation = source.generation;
    if model.notice_state.retiring_generation != Some(generation) {
        update(model, Message::NoticeRetirementStarted(generation));
    }
    // Explicit retiring-source authority covers a physical install whose Model
    // event lagged, including a profile reducer that already advanced generation.
    update(
        model,
        Message::NoticeRetiringSourceInstalled {
            generation,
            source: source.source,
            session_id: source.session_id.clone(),
        },
    );
    update(
        model,
        Message::NoticeRetired {
            generation,
            source: source.source,
            session_id: source.session_id.clone(),
            counts: source.notice_retirement_counts(),
        },
    );
}

async fn retire_notice_sources(
    model: &mut Model,
    user: &SessionSlot,
    metadata: &SessionSlot,
) -> [Option<Arc<InstalledSession>>; 2] {
    let user = { user.read().await.clone() };
    let metadata = { metadata.read().await.clone() };
    if let Some(user) = user.as_ref() {
        retire_notice_source(model, user);
    }
    if let Some(metadata) = metadata.as_ref().filter(|metadata| {
        user.as_ref()
            .is_none_or(|user| !Arc::ptr_eq(user, metadata))
    }) {
        retire_notice_source(model, metadata);
    }
    [user, metadata]
}

fn same_notice_session(left: &InstalledSession, right: &InstalledSession) -> bool {
    left.session_id == right.session_id || Arc::ptr_eq(&left.session, &right.session)
}

/// Only the latest replaced attempt per route; weak identity does not retain a client.
struct RetiredNoticeAttempt {
    session: std::sync::Weak<Session>,
    session_id: crate::app::model::NoticeSessionId,
    generation: u64,
}
impl RetiredNoticeAttempt {
    fn capture(opened: &InstalledSession) -> Self {
        Self {
            session: Arc::downgrade(&opened.session),
            session_id: opened.session_id.clone(),
            generation: opened.generation,
        }
    }
    fn matches(&self, opened: &InstalledSession) -> bool {
        self.session_id == opened.session_id
            || std::sync::Weak::ptr_eq(&self.session, &Arc::downgrade(&opened.session))
    }
}

/// One bounded admission point; current deferred loss is handed off before drop.
fn admit_pending_install(
    pending: &mut [Option<SessionInstall>; 2],
    install: SessionInstall,
    model: &mut Model,
    installed: &[Option<Arc<InstalledSession>>; 2],
    remembered: &mut [Option<RetiredNoticeAttempt>; 2],
    generation: u64,
) {
    if model.should_quit
        || generation != model.connection_generation
        || install.opened.generation != generation
        || install
            .opened
            .retired
            .load(std::sync::atomic::Ordering::SeqCst)
        || pending
            .iter()
            .flatten()
            .any(|known| same_notice_session(&known.opened, &install.opened))
        || installed
            .iter()
            .flatten()
            .any(|known| same_notice_session(known, &install.opened))
        || remembered
            .iter()
            .flatten()
            .any(|known| known.generation == generation && known.matches(&install.opened))
    {
        let _ = install.acknowledged.send(false);
        return;
    }
    let index = install.opened.source.index();
    if let Some(replaced) = pending[index].take() {
        let opened = &replaced.opened;
        let still_owned = pending
            .iter()
            .flatten()
            .any(|known| same_notice_session(&known.opened, opened))
            || installed
                .iter()
                .flatten()
                .any(|known| same_notice_session(known, opened));
        if opened.generation == generation
            && !still_owned
            && !opened
                .retired
                .swap(true, std::sync::atomic::Ordering::SeqCst)
        {
            let counts = opened.notice_retirement_counts();
            remembered[index] = Some(RetiredNoticeAttempt::capture(opened));
            if counts.needs_report() {
                update(
                    model,
                    connection_message(
                        generation,
                        Message::NoticeAttemptReport {
                            source: opened.source,
                            attempt_id: opened.session_id.clone(),
                            // This projection supplies loss counts only, not an admitted-entry count.
                            summary: crate::app::model::NoticeSummary {
                                scope: crate::query::result::NoticeScope::Connection,
                                retained: None,
                                counts,
                            },
                        },
                    ),
                );
            }
        }
        let _ = replaced.acknowledged.send(false);
    }
    pending[index] = Some(install);
}

async fn retire_profile_notice_sources(
    model: &mut Model,
    user: &SessionSlot,
    metadata: &SessionSlot,
    receiver: &mut mpsc::Receiver<SessionInstall>,
    pending: &mut [Option<SessionInstall>; 2],
    remembered: &mut [Option<RetiredNoticeAttempt>; 2],
    generation: u64,
) {
    let installed = retire_notice_sources(model, user, metadata).await;
    let pending = std::mem::take(pending);
    let remembered = std::mem::take(remembered);
    retire_pending_installs_at_boundary(
        model, receiver, pending, installed, remembered, generation, false,
    );
}

/// Bounded retirement of locally held install requests; no database await.
fn retire_pending_installs_at_boundary(
    model: &mut Model,
    receiver: &mut mpsc::Receiver<SessionInstall>,
    pending: [Option<SessionInstall>; 2],
    installed: [Option<Arc<InstalledSession>>; 2],
    remembered: [Option<RetiredNoticeAttempt>; 2],
    generation: u64,
    close: bool,
) {
    if close {
        receiver.close();
    }
    update(
        model,
        Message::NoticeUninstalledRetirementStarted(generation),
    );
    // Installed2 plus remembered2 identities and only pending2/channel2. Weak
    // control blocks prevent address reuse without retaining database clients.
    let mut seen: [Option<RetiredNoticeAttempt>; 8] = std::array::from_fn(|_| None);
    for (index, source) in installed.into_iter().enumerate() {
        seen[index] = source.as_deref().map(RetiredNoticeAttempt::capture);
    }
    for (index, source) in remembered.into_iter().enumerate() {
        seen[index + 2] = source.filter(|source| source.generation == generation);
    }
    let mut retire = |install: SessionInstall| {
        let opened = install.opened;
        let duplicate = seen.iter().flatten().any(|known| known.matches(&opened));
        if opened.generation == generation
            && !duplicate
            && !opened
                .retired
                .swap(true, std::sync::atomic::Ordering::SeqCst)
        {
            if let Some(empty) = seen.iter_mut().find(|slot| slot.is_none()) {
                *empty = Some(RetiredNoticeAttempt::capture(&opened));
            }
            update(
                model,
                Message::NoticeUninstalledRetired {
                    generation,
                    session_id: opened.session_id.clone(),
                    counts: opened.notice_retirement_counts(),
                },
            );
        }
        let _ = install.acknowledged.send(false);
    };
    for install in pending.into_iter().flatten() {
        retire(install);
    }
    for _ in 0..2 {
        match receiver.try_recv() {
            Ok(install) => retire(install),
            Err(_) => break,
        }
    }
    update(
        model,
        Message::NoticeUninstalledRetirementFinished(generation),
    );
    update(model, Message::NoticeRetirementFinished(generation));
}

fn shutdown_pending_installs(
    model: &mut Model,
    receiver: &mut mpsc::Receiver<SessionInstall>,
    pending: [Option<SessionInstall>; 2],
    installed: [Option<Arc<InstalledSession>>; 2],
    remembered: [Option<RetiredNoticeAttempt>; 2],
) {
    let generation = model.connection_generation;
    retire_pending_installs_at_boundary(
        model, receiver, pending, installed, remembered, generation, true,
    );
}

async fn poll_notice_sources(
    model: &mut Model,
    user: &SessionSlot,
    metadata: &SessionSlot,
) -> bool {
    let user = { user.read().await.clone() };
    let metadata = { metadata.read().await.clone() };
    let mut changed = false;
    for (index, source) in [user.as_ref(), metadata.as_ref()]
        .into_iter()
        .enumerate()
        .filter_map(|(index, source)| source.map(|source| (index, source)))
    {
        if source.retired.load(std::sync::atomic::Ordering::SeqCst)
            || source.generation != model.connection_generation
        {
            continue;
        }
        if index == 1 && user.as_ref().is_some_and(|user| Arc::ptr_eq(user, source)) {
            continue;
        }
        let report = source.take_connection_notice_report();
        let checkpoint = source.notice_checkpoint();
        let prior = model.notice_state.sources[source.source.index()].as_ref();
        changed |= report.is_some()
            || prior.is_none_or(|prior| {
                prior.session_id != source.session_id || prior.checkpoint != checkpoint
            })
            || checkpoint.revision == u64::MAX;
        update(
            model,
            connection_message(
                source.generation,
                Message::NoticeSourceInstalled {
                    source: source.source,
                    session_id: source.session_id.clone(),
                },
            ),
        );
        update(
            model,
            connection_message(
                source.generation,
                Message::NoticeStatus {
                    source: source.source,
                    session_id: source.session_id.clone(),
                    checkpoint,
                    latest: report.as_ref().map(crate::app::model::NoticeSummary::from),
                },
            ),
        );
    }
    changed
}

fn send_operation_notice(
    tx: &mpsc::UnboundedSender<Message>,
    source: &InstalledSession,
    report: &NoticeReport,
) {
    let _ = tx.send(connection_message(
        source.generation,
        Message::NoticeStatus {
            source: source.source,
            session_id: source.session_id.clone(),
            checkpoint: report.checkpoint,
            latest: Some(crate::app::model::NoticeSummary::from(report)),
        },
    ));
}

fn send_attempt_notice(
    tx: &mpsc::UnboundedSender<Message>,
    generation: u64,
    source: crate::app::model::NoticeSource,
    report: &NoticeReport,
) {
    if report.counts.needs_report() {
        let _ = tx.send(connection_message(
            generation,
            Message::NoticeAttemptReport {
                source,
                attempt_id: crate::app::model::NoticeSessionId::new(),
                summary: crate::app::model::NoticeSummary::from(report),
            },
        ));
    }
}

/// Opens the interactive client.
pub fn run(
    target: Option<&str>,
    connection: &ConnectionOptions,
    paths: &Paths,
    presentation: &Presentation,
    no_history: bool,
) -> Result<ExitCode, Diagnostic> {
    let loaded = crate::config::load(paths)?;
    // When named profiles exist and no route was typed, let the first frame ask
    // which database the person means. Explicit targets and flags retain the
    // existing pre-terminal resolution and authentication path.
    let choose_before_connecting = crate::cli::should_open_connection_picker(
        target,
        connection,
        !loaded.config.profiles.is_empty(),
    );
    let resolved = if choose_before_connecting {
        None
    } else {
        let (requested, args) =
            crate::cli::resolve_target_and_profile(target, connection, &loaded.config)?;
        let resolved = crate::connection::resolve(
            requested,
            &args,
            &EnvSnapshot::from_process(),
            &loaded.config.connection,
        )?;
        // The credential is obtained before the terminal is taken, so a
        // provider that needs to say something - or to ask for a sign-in - does
        // it on an ordinary terminal rather than underneath a full-screen app.
        Some(crate::cli::authenticate_target(resolved, &loaded.config)?)
    };

    // Key bindings are read before the terminal is taken, so a file that binds a
    // key this build cannot read is an ordinary configuration error on an
    // ordinary terminal rather than a failure inside a full-screen application.
    let keymap = Keymap::from_config(&loaded.config.keys)?;

    let facts = crate::ui::terminal::capabilities();
    if !facts.is_terminal {
        return Err(Diagnostic::new(
            DiagnosticKind::Usage,
            "the interactive client needs a terminal",
            "opening the interactive client",
        )
        .likely_cause("standard output is redirected or piped")
        .next_action(format!(
            "run `{} query` for non-interactive execution",
            crate::branding::BINARY_NAME
        )));
    }
    let (presentation, color_depth) = configured_presentation(&loaded, presentation, &facts);

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|err| {
            Diagnostic::new(
                DiagnosticKind::Internal,
                "could not start the async runtime",
                "opening the interactive client",
            )
            .likely_cause(err.to_string())
        })?;

    // From here on the terminal is ours. The guard restores it when this scope
    // ends, whatever ends it, and the panic hook covers the path a guard cannot.
    crate::ui::terminal::install_panic_hook();
    let options = crate::ui::TerminalOptions {
        mouse: loaded.config.ui.mouse,
        ..crate::ui::TerminalOptions::default()
    };
    let acquire = || {
        crate::ui::TerminalGuard::acquire(options).map_err(|err| {
            Diagnostic::new(
                DiagnosticKind::Internal,
                "could not take control of the terminal",
                "opening the interactive client",
            )
            .likely_cause(err.to_string())
            .next_action(
                "check that the terminal supports full-screen applications, or use --plain",
            )
        })
    };
    #[cfg(unix)]
    let (mut guard, termination) = prepare_terminal(&runtime, acquire)?;
    #[cfg(not(unix))]
    let mut guard = acquire()?;

    let history = crate::history::History::open(paths, &loaded.config.history, no_history);
    let library = crate::queries::Library::new(&paths.queries_dir);
    let mut exit_notice_counts = NoticeCounts::default();
    let result = runtime.block_on(event_loop(
        resolved,
        loaded,
        keymap,
        &presentation,
        color_depth,
        history,
        library,
        &mut exit_notice_counts,
        #[cfg(unix)]
        termination,
    ));

    // Restore explicitly so any error below is printed on a working terminal.
    let restore = guard.restore();
    let notice_output =
        crate::cli::write_notice_counts(&mut std::io::stderr().lock(), exit_notice_counts)
            .map_err(crate::cli::io_diagnostic);
    result.and_then(|code| {
        restore.map_err(|err| {
            Diagnostic::new(
                DiagnosticKind::Internal,
                "the terminal could not be fully restored",
                "leaving the interactive client",
            )
            .likely_cause(err.to_string())
            .next_action("run `reset` to return the terminal to a known state")
        })?;
        notice_output?;
        Ok(code)
    })
}

fn configured_presentation(
    loaded: &crate::config::Loaded,
    presentation: &Presentation,
    facts: &crate::diagnostics::doctor::TerminalFacts,
) -> (Presentation, crate::ui::theme::ColorDepth) {
    let presentation = presentation.with_configured_color(loaded.config.ui.color, facts);
    let (depth, _) = crate::cli::resolve_depth(
        presentation.color_depth,
        loaded.config.ui.color_depth,
        presentation.color,
        facts,
    );
    (presentation, depth)
}

// The Unix receiver is prepared before terminal acquisition and moved with the
// existing session resources; keeping it explicit avoids deferred registration.
#[allow(clippy::too_many_arguments)]
async fn event_loop(
    target: Option<ConnectionTarget>,
    loaded: crate::config::Loaded,
    keymap: Keymap,
    presentation: &Presentation,
    color_depth: crate::ui::theme::ColorDepth,
    mut history: crate::history::History,
    library: crate::queries::Library,
    exit_notice_counts: &mut NoticeCounts,
    #[cfg(unix)] termination: tokio::signal::unix::Signal,
) -> Result<ExitCode, Diagnostic> {
    let crate::config::Loaded { config, themes, .. } = loaded;
    let backend = ratatui::backend::CrosstermBackend::new(std::io::stdout());
    let mut terminal = ratatui::Terminal::new(backend).map_err(|err| {
        Diagnostic::new(
            DiagnosticKind::Internal,
            "could not start the renderer",
            "opening the interactive client",
        )
        .likely_cause(err.to_string())
    })?;

    // An explicit flag wins; otherwise configuration decides, and only then the
    // environment. The Nerd tier is never reached by inference.
    let tier = presentation.glyph_override.unwrap_or_else(|| {
        use crate::config::GlyphMode;
        use crate::ui::GlyphTier;
        match config.ui.glyphs {
            GlyphMode::Auto => {
                if presentation.unicode_capable {
                    GlyphTier::Unicode
                } else {
                    GlyphTier::Ascii
                }
            }
            GlyphMode::Unicode => GlyphTier::Unicode,
            GlyphMode::Ascii => GlyphTier::Ascii,
            GlyphMode::NerdFont => GlyphTier::Nerd,
        }
    });
    let mut model = Model::new(config.query.max_buffered_rows);
    model.theme_catalog = themes;
    model.presentation = layout::Presentation::new(
        model
            .theme_catalog
            .selected_theme(presentation.color, color_depth),
        crate::ui::Glyphs::new(tier),
        config.ui.reduced_motion,
    )
    .with_density(config.ui.density);
    model.keymap_snapshot = keymap.snapshot();
    let configured_providers = crate::cli::auth_providers(&config).unwrap_or_default();
    let providers = crate::connection::cloud::registry(&configured_providers);
    model.connection_profiles =
        crate::app::connection_picker::summaries(&config.profiles, &providers);
    model.connection = match target.as_ref() {
        Some(target) => {
            let step = connecting_step_for(target.sslmode);
            model.connecting_step = Some(step);
            crate::app::ConnectionState::Connecting {
                step,
                target: target.safe_display(),
            }
        }
        None => crate::app::ConnectionState::Disconnected,
    };
    model.editor.set_text(starter_query());
    model.completion.enabled = config.ui.completion;
    model.clipboard_osc52 = config.clipboard.osc52;
    model.history_disabled = !config.history.enabled;
    model.history_paused = history.is_paused();
    if let Some(target) = target.as_ref() {
        model.credential_provider.clone_from(&target.auth);
    }
    // Resolve only safe provider presentation metadata here. The credential was
    // already fetched before the terminal was taken; opening the in-app trust
    // surface must never execute a provider or refresh a token.
    model.credential_presentation = target.as_ref().and_then(|target| {
        target
            .auth
            .as_deref()
            .and_then(|name| providers.get(name).map(|provider| provider.presentation()))
    });

    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();
    let (install_tx, mut install_rx) = mpsc::channel::<SessionInstall>(2);
    let mut pending_installs: [Option<SessionInstall>; 2] = [None, None];
    let mut remembered_attempts: [Option<RetiredNoticeAttempt>; 2] = [None, None];
    let mut notice_tick = tokio::time::interval(Duration::from_millis(250));
    notice_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let input_stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    spawn_input_thread(tx.clone(), Arc::clone(&input_stop), &keymap);
    spawn_signal_watcher(
        tx.clone(),
        #[cfg(unix)]
        termination,
    );

    // The clock lives here, not in the reducer. The ticker measures how long the
    // current activity has been running and passes it in, which is what keeps
    // every animated state reproducible in a test.
    let (activity, activity_rx) = tokio::sync::watch::channel(Some(std::time::Instant::now()));
    spawn_ticker(tx.clone(), activity_rx);

    // The connection is opened as an effect like any other, so the interface is
    // drawn and responsive while it happens. When the picker was shown, this
    // branch deliberately does not start a connection before a choice.
    let session: Arc<tokio::sync::RwLock<Option<Arc<InstalledSession>>>> =
        Arc::new(tokio::sync::RwLock::new(None));
    // The tree gets a connection of its own so a long statement cannot delay it.
    // It is opened from the same resolved target, so it reaches the same server
    // by the same route with the same credentials and the same protection; the
    // only differences are deliberate and visible on the server.
    let metadata: Arc<tokio::sync::RwLock<Option<Arc<InstalledSession>>>> =
        Arc::new(tokio::sync::RwLock::new(None));
    // Held so a retry with a password the user types can be made from exactly
    // the target that was resolved, rather than resolved again. It remains
    // outside Model because it may carry a password or cloud token.
    let current_target: Arc<tokio::sync::RwLock<Option<Arc<ConnectionTarget>>>> =
        Arc::new(tokio::sync::RwLock::new(None));
    let connection_generation = Arc::new(std::sync::atomic::AtomicU64::new(1));
    if let Some(target) = target {
        let target = Arc::new(target);
        *current_target.write().await = Some(Arc::clone(&target));
        spawn_connect(
            tx.clone(),
            install_tx.clone(),
            target,
            config.clone(),
            Arc::clone(&connection_generation),
            1,
        );
    } else {
        crate::app::update(
            &mut model,
            Message::Action(crate::app::Action::OpenConnectionPicker),
        );
    }

    // Reading the history is a file read, not a query, so it happens once here
    // and the interface never waits on it.
    let _ = tx.send(Message::HistoryLoaded(
        history
            .recent(crate::history::IN_MEMORY_LIMIT)
            .unwrap_or_default(),
    ));

    let result=async {
    draw(&mut terminal, &mut model, &keymap)?;

    loop {
        for (index,pending) in pending_installs.iter_mut().enumerate() {
            if let Some(install)=pending.take() {
                let slot=if index==0{&session}else{&metadata};
                *pending=try_session_install(install,slot,&mut model,connection_generation.load(std::sync::atomic::Ordering::SeqCst));
            }
        }
        let message=tokio::select! {
            install=install_rx.recv()=> {
                if let Some(install)=install {
                    let installed=[session.try_read().ok().and_then(|held|held.clone()),metadata.try_read().ok().and_then(|held|held.clone())];
                    let revision=model.notice_state.revision;
                    admit_pending_install(&mut pending_installs,install,&mut model,&installed,&mut remembered_attempts,connection_generation.load(std::sync::atomic::Ordering::SeqCst));
                    if model.notice_state.revision!=revision {draw(&mut terminal,&mut model,&keymap)?;}
                    continue;
                }
                continue;
            },
            _=notice_tick.tick()=> {
                if poll_notice_sources(&mut model,&session,&metadata).await {draw(&mut terminal,&mut model,&keymap)?;}
                continue;
            },
            message=rx.recv()=>match message {Some(message)=>message,None=>break},
        };
        poll_notice_sources(&mut model,&session,&metadata).await;
        let effects = reduce_terminal_input(&terminal, &mut model, message)?;
        for effect in effects {
            match effect {
                Effect::Quit => {
                    input_stop.store(true, std::sync::atomic::Ordering::SeqCst);
                    return Ok(ExitCode::Success);
                }
                Effect::Connect => {
                    // Reconnection on a dropped connection arrives with Feature
                    // 002; the initial connect is started above.
                }
                Effect::Reconnect { password } => {
                    // The password goes straight into a connection attempt and
                    // is dropped with it. Nothing keeps it: not the model, not
                    // configuration, not the history.
                    let Some(target) = current_target.read().await.clone() else {
                        continue;
                    };
                    let retry = Arc::new(
                        target.with_password(secrecy::SecretString::from(password.into_inner())),
                    );
                    let generation =
                        connection_generation.load(std::sync::atomic::Ordering::SeqCst);
                    spawn_connect(
                        tx.clone(),
                        install_tx.clone(),
                        retry,
                        config.clone(),
                        Arc::clone(&connection_generation),
                        generation,
                    );
                }
                Effect::ConnectProfile { profile } => {
                    retire_profile_notice_sources(&mut model,&session,&metadata,&mut install_rx,&mut pending_installs,&mut remembered_attempts,connection_generation.load(std::sync::atomic::Ordering::SeqCst)).await;
                    let generation =
                        connection_generation.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
                    // Drop the old runtime routes before resolving the new one.
                    // The generation and lock ordering keep a late old task
                    // from replacing either slot after this point.
                    *current_target.write().await = None;
                    *session.write().await = None;
                    *metadata.write().await = None;
                    let selected_config = config.clone();
                    let selected_tx = tx.clone();
                    let selected_installs = install_tx.clone();
                    let selected_target = Arc::clone(&current_target);
                    let selected_generation = Arc::clone(&connection_generation);
                    tokio::spawn(async move {
                        let result =
                            prepare_profile_target(profile.as_deref(), &selected_config).await;
                        if selected_generation.load(std::sync::atomic::Ordering::SeqCst)
                            != generation
                        {
                            return;
                        }
                        match result {
                            Ok(target) => {
                                let target = Arc::new(target);
                                if !install_connection_target(
                                    &selected_target,
                                    Arc::clone(&target),
                                    &selected_generation,
                                    generation,
                                )
                                .await
                                {
                                    return;
                                }
                                spawn_connect(
                                    selected_tx,
                                    selected_installs,
                                    target,
                                    selected_config,
                                    selected_generation,
                                    generation,
                                );
                            }
                            Err(diagnostic) => {
                                send_connection_message(
                                    &selected_tx,
                                    generation,
                                    Message::ConnectionFailed(Box::new(diagnostic)),
                                );
                            }
                        }
                    });
                }
                Effect::Execute { job, sql } => {
                    spawn_execute(
                        tx.clone(),
                        Arc::clone(&session),
                        job,
                        sql,
                        model.row_cap,
                        ConnectionScope {
                            generation: Arc::clone(&connection_generation),
                            request: connection_generation
                                .load(std::sync::atomic::Ordering::SeqCst),
                        },
                    );
                }
                Effect::ExecuteParameterized {
                    job,
                    sql,
                    parameters,
                } => {
                    spawn_execute_parameterized(
                        tx.clone(),
                        Arc::clone(&session),
                        job,
                        sql,
                        parameters,
                        model.row_cap,
                        ConnectionScope {
                            generation: Arc::clone(&connection_generation),
                            request: connection_generation
                                .load(std::sync::atomic::Ordering::SeqCst),
                        },
                    );
                }
                Effect::Explain { job, sql, analyze } => {
                    spawn_explain(
                        tx.clone(),
                        Arc::clone(&session),
                        job,
                        sql,
                        analyze,
                        ConnectionScope {
                            generation: Arc::clone(&connection_generation),
                            request: connection_generation
                                .load(std::sync::atomic::Ordering::SeqCst),
                        },
                    );
                }
                Effect::CopyValue { payload } => {
                    let message = copy_retained_to_terminal(terminal.backend_mut(), &model, &payload);
                    let _ = tx.send(message);
                }
                Effect::Cancel { job } => {
                    spawn_cancel(
                        tx.clone(),
                        Arc::clone(&session),
                        job,
                        ConnectionScope {
                            generation: Arc::clone(&connection_generation),
                            request: connection_generation
                                .load(std::sync::atomic::Ordering::SeqCst),
                        },
                    );
                }
                Effect::LoadSchemas => {
                    let generation =
                        connection_generation.load(std::sync::atomic::Ordering::SeqCst);
                    spawn_load_schemas(
                        tx.clone(),
                        reader(&session, &metadata),
                        Arc::clone(&connection_generation),
                        generation,
                    );
                }
                Effect::LoadCompletionCatalog { request } => {
                    let generation =
                        connection_generation.load(std::sync::atomic::Ordering::SeqCst);
                    spawn_load_completion_catalog(
                        tx.clone(),
                        reader(&session, &metadata),
                        request,
                        Arc::clone(&connection_generation),
                        generation,
                    );
                }
                Effect::LoadMetadata {
                    request,
                    path,
                    query,
                } => {
                    spawn_load_metadata(
                        tx.clone(),
                        reader(&session, &metadata),
                        request,
                        path,
                        query,
                        Arc::clone(&connection_generation),
                        connection_generation.load(std::sync::atomic::Ordering::SeqCst),
                    );
                }
                Effect::LoadUpdateTarget { request, relation } => {
                    spawn_load_update_target(
                        tx.clone(),
                        reader(&session, &metadata),
                        request,
                        relation,
                        Arc::clone(&connection_generation),
                        connection_generation.load(std::sync::atomic::Ordering::SeqCst),
                    );
                }
                Effect::LoadDefinition { request, object } => {
                    spawn_load_definition(
                        tx.clone(),
                        reader(&session, &metadata),
                        request,
                        *object,
                        Arc::clone(&connection_generation),
                        connection_generation.load(std::sync::atomic::Ordering::SeqCst),
                    );
                }
                Effect::LoadDependencies { request, object } => {
                    spawn_load_dependencies(
                        tx.clone(),
                        reader(&session, &metadata),
                        request,
                        *object,
                        Arc::clone(&connection_generation),
                        connection_generation.load(std::sync::atomic::Ordering::SeqCst),
                    );
                }
                Effect::ExportRows { path, format } => {
                    let _ = tx.send(Message::RowsExported(Box::new(export_visible_rows(
                        &model, &path, format,
                    ))));
                }
                Effect::ListQueries => {
                    let _ = tx.send(Message::QueriesListed(library.list().unwrap_or_default()));
                }
                Effect::SaveQuery { name, sql } => {
                    let _ = tx.send(Message::QuerySaved(Box::new(library.save(&name, &sql))));
                }
                Effect::LoadQuery { name } => {
                    let _ = tx.send(Message::QueryLoaded(Box::new(library.load(&name))));
                }
                Effect::LoadHistory => {
                    let _ = tx.send(Message::HistoryLoaded(
                        history
                            .recent(crate::history::IN_MEMORY_LIMIT)
                            .unwrap_or_default(),
                    ));
                }
                Effect::RecordHistory {
                    sql,
                    outcome,
                    elapsed,
                } => {
                    let (entry, recorded) =
                        record_history(&history, &model, &sql, outcome, elapsed);
                    let _ = tx.send(Message::HistoryRecorded {
                        entry: Box::new(entry),
                        recorded,
                    });
                }
            }
        }
        // The session's own pause lives in the model, where the key that toggles
        // it is handled; the writer is told about it here.
        if model.history_paused {
            history.pause();
        } else {
            history.resume();
        }

        // Start and stop the ticker with the animation, so nothing spins while
        // the client is sitting idle.
        let animating = model.is_animating();
        let ticking = activity.borrow().is_some();
        if animating && !ticking {
            let _ = activity.send(Some(std::time::Instant::now()));
        } else if !animating && ticking {
            let _ = activity.send(None);
        }

        draw(&mut terminal, &mut model, &keymap)?;
    }

    Ok(ExitCode::Success)
    }.await;
    install_rx.close();
    let installed = retire_notice_sources(&mut model, &session, &metadata).await;
    shutdown_pending_installs(
        &mut model,
        &mut install_rx,
        pending_installs,
        installed,
        remembered_attempts,
    );
    *exit_notice_counts = model.notice_state.pending;
    input_stop.store(true, std::sync::atomic::Ordering::SeqCst);
    result
}

/// Performs the one-way clipboard write at the already-held terminal boundary.
///
/// The reducer has already made the opt-in, confirmation, and stale-candidate
/// decisions. This helper only writes and flushes the safe OSC 52 sequence, and
/// reports metadata or an I/O error without carrying the result value further.
fn copy_retained_to_terminal<W: Write>(
    writer: &mut W,
    model: &Model,
    payload: &crate::clipboard::ClipboardPayload,
) -> Message {
    if model
        .visible_result()
        .is_some_and(|set| !set.is_available())
    {
        return Message::RetainedDataUnavailable;
    }
    copy_to_terminal(writer, payload)
}

fn copy_to_terminal<W: Write>(
    writer: &mut W,
    payload: &crate::clipboard::ClipboardPayload,
) -> Message {
    let bytes = payload.len();
    let characters = payload.characters();
    match crate::clipboard::write_osc52(writer, payload) {
        Ok(()) => Message::ClipboardSent { bytes, characters },
        Err(error) => Message::ClipboardFailed(Box::new(
            Diagnostic::new(
                DiagnosticKind::Internal,
                "could not send the selected value through the terminal",
                "copying a result value through the terminal",
            )
            .likely_cause(error.to_string())
            .next_action("check terminal output, or use an explicit file export"),
        )),
    }
}

/// Offers a statement to the history and reports what was kept.
///
/// A history that cannot be written must not stop anyone working, so a failure
/// here is swallowed after the fact is recorded: the statement already ran.
fn record_history(
    history: &crate::history::History,
    model: &Model,
    sql: &str,
    outcome: crate::history::Outcome,
    elapsed: Duration,
) -> (Option<crate::history::Entry>, crate::history::Recorded) {
    let Some(info) = model.connection.info() else {
        return (None, crate::history::Recorded::Empty);
    };
    let entry = crate::history::Entry::now(
        &info.target,
        &info.database,
        &model.environment().label(),
        sql,
        outcome,
        elapsed,
    );
    match history.record(&entry) {
        Ok(recorded) if recorded.was_written() => (Some(entry), recorded),
        Ok(recorded) => (None, recorded),
        // A history that cannot be written must not stop anyone working. The
        // statement already ran; what is lost is the record of it.
        Err(_) => (None, crate::history::Recorded::Empty),
    }
}

/// Emits a frame of elapsed time while there is something to animate.
fn spawn_ticker(
    tx: mpsc::UnboundedSender<Message>,
    mut activity: tokio::sync::watch::Receiver<Option<std::time::Instant>>,
) {
    tokio::spawn(async move {
        loop {
            let started = *activity.borrow_and_update();
            match started {
                Some(started) => {
                    let message = Message::Tick {
                        running_for: Some(started.elapsed()),
                    };
                    if tx.send(message).is_err() {
                        return;
                    }
                    tokio::time::sleep(TICK).await;
                }
                // Nothing is happening: sleep until it does, rather than
                // waking up to discover that again.
                None => {
                    if activity.changed().await.is_err() {
                        return;
                    }
                }
            }
        }
    });
}

fn reduce_terminal_input<B: ratatui::backend::Backend>(
    terminal: &ratatui::Terminal<B>,
    model: &mut Model,
    message: Message,
) -> Result<Vec<Effect>, Diagnostic> {
    reduce_input_with_size(model, message, || {
        terminal.size().map_err(|err| {
            Diagnostic::new(
                DiagnosticKind::Internal,
                "could not read the terminal size",
                "preparing interactive input",
            )
            .likely_cause(err.to_string())
        })
    })
}

/// Sending and navigation use current backend geometry, not an initial default
/// or a queued resize that may already be stale. Recovery and theme keys need no size.
fn reduce_input_with_size(
    model: &mut Model,
    message: Message,
    read_size: impl FnOnce() -> Result<ratatui::layout::Size, Diagnostic>,
) -> Result<Vec<Effect>, Diagnostic> {
    match message {
        Message::Action(
            action @ (crate::app::Action::Quit
            | crate::app::Action::Cancel
            | crate::app::Action::Dismiss
            | crate::app::Action::SwitchTheme),
        ) => Ok(update(model, Message::Action(action))),
        Message::Action(action) => {
            let size = read_size()?;
            Ok(update(
                model,
                Message::InputAtSize {
                    action,
                    columns: size.width,
                    rows: size.height,
                },
            ))
        }
        Message::Resized(..) => {
            // The current backend replaces an older event's dimensions.
            let size = read_size()?;
            synchronize_model_size(model, size.width, size.height);
            Ok(Vec::new())
        }
        message => Ok(update(model, message)),
    }
}

fn synchronize_model_size(model: &mut Model, columns: u16, rows: u16) {
    if model.size != (columns, rows) {
        // Use the reducer's resize transition so review state and any future
        // geometry-dependent state invalidate through the same pure boundary.
        let effects = update(model, Message::Resized(columns, rows));
        debug_assert!(effects.is_empty(), "resize is a local state transition");
    }
}

fn draw<B: ratatui::backend::Backend>(
    terminal: &mut ratatui::Terminal<B>,
    model: &mut Model,
    keymap: &Keymap,
) -> Result<(), Diagnostic> {
    terminal
        .draw(|frame| {
            let area = frame.area();
            synchronize_model_size(model, area.width, area.height);
            layout::render(model, keymap, area, frame.buffer_mut());
        })
        .map(|_| ())
        .map_err(|err| {
            Diagnostic::new(
                DiagnosticKind::Internal,
                "could not draw the interface",
                "rendering",
            )
            .likely_cause(err.to_string())
        })
}

/// Reads input on its own thread so a slow key never blocks rendering.
fn spawn_input_thread(
    tx: mpsc::UnboundedSender<Message>,
    stop: Arc<std::sync::atomic::AtomicBool>,
    keymap: &Keymap,
) {
    let keymap = keymap.clone();
    std::thread::spawn(move || {
        while !stop.load(std::sync::atomic::Ordering::SeqCst) {
            match event::poll(INPUT_POLL) {
                Ok(true) => match event::read() {
                    Ok(Event::Key(key)) => {
                        if let Some(action) = keymap.resolve(&key)
                            && tx.send(Message::Action(action)).is_err()
                        {
                            return;
                        }
                    }
                    Ok(Event::Resize(columns, rows)) => {
                        if tx.send(Message::Resized(columns, rows)).is_err() {
                            return;
                        }
                    }
                    // The terminal's event vocabulary stops here: the reducer
                    // sees only the application's own mouse and paste messages.
                    // Capture is not enabled by this translation, so these
                    // events stay absent until the mouse phase asks for them.
                    Ok(Event::Mouse(mouse)) => {
                        let message = Message::Mouse {
                            kind: mouse_kind(mouse.kind),
                            column: mouse.column,
                            row: mouse.row,
                            modifiers: mouse_modifiers(mouse.modifiers),
                        };
                        if tx.send(message).is_err() {
                            return;
                        }
                    }
                    Ok(Event::Paste(text)) => {
                        if tx.send(Message::Pasted(text)).is_err() {
                            return;
                        }
                    }
                    Ok(_) => {}
                    Err(_) => return,
                },
                Ok(false) => {}
                Err(_) => return,
            }
        }
    });
}

/// Translates a terminal mouse action into the application's vocabulary.
fn mouse_kind(kind: event::MouseEventKind) -> crate::app::message::MouseKind {
    use crate::app::message::{MouseButton, MouseKind};
    use crossterm::event::{MouseButton as TerminalButton, MouseEventKind};
    let translate = |button: TerminalButton| match button {
        TerminalButton::Left => MouseButton::Left,
        TerminalButton::Right => MouseButton::Right,
        TerminalButton::Middle => MouseButton::Middle,
    };
    match kind {
        MouseEventKind::Down(button) => MouseKind::Down(translate(button)),
        MouseEventKind::Up(button) => MouseKind::Up(translate(button)),
        MouseEventKind::Drag(button) => MouseKind::Drag(translate(button)),
        MouseEventKind::Moved => MouseKind::Moved,
        MouseEventKind::ScrollDown => MouseKind::ScrollDown,
        MouseEventKind::ScrollUp => MouseKind::ScrollUp,
        MouseEventKind::ScrollLeft => MouseKind::ScrollLeft,
        MouseEventKind::ScrollRight => MouseKind::ScrollRight,
    }
}

/// Translates terminal modifier bits into the application's mouse modifiers.
fn mouse_modifiers(modifiers: event::KeyModifiers) -> crate::app::message::MouseModifiers {
    crate::app::message::MouseModifiers {
        shift: modifiers.contains(event::KeyModifiers::SHIFT),
        control: modifiers.contains(event::KeyModifiers::CONTROL),
        alt: modifiers.contains(event::KeyModifiers::ALT),
    }
}

/// Registers termination before acquisition, while the runtime is entered but
/// does not need to have polled its driver or any spawned task.
#[cfg(unix)]
fn prepare_terminal<G>(
    runtime: &tokio::runtime::Runtime,
    acquire: impl FnOnce() -> Result<G, Diagnostic>,
) -> Result<(G, tokio::signal::unix::Signal), Diagnostic> {
    let _entered = runtime.enter();
    register_then_acquire(
        || tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()),
        acquire,
    )
}

/// The constructor boundary keeps a registration failure ahead of every
/// terminal side effect without adding an application flag or signal mode.
#[cfg(unix)]
fn register_then_acquire<G>(
    register: impl FnOnce() -> std::io::Result<tokio::signal::unix::Signal>,
    acquire: impl FnOnce() -> Result<G, Diagnostic>,
) -> Result<(G, tokio::signal::unix::Signal), Diagnostic> {
    let termination = register().map_err(|error| {
        Diagnostic::new(
            DiagnosticKind::Internal,
            "could not prepare SIGTERM handling",
            "opening the interactive client",
        )
        .likely_cause(error.to_string())
        .next_action("check operating system signal support, or use --plain")
    })?;
    let guard = acquire()?;
    Ok((guard, termination))
}

/// Turns a registered termination signal into an ordinary quit, so the terminal
/// is restored. Windows retains its existing Ctrl+C receiver.
fn spawn_signal_watcher(
    tx: mpsc::UnboundedSender<Message>,
    #[cfg(unix)] mut term: tokio::signal::unix::Signal,
) {
    #[cfg(unix)]
    tokio::spawn(async move {
        if term.recv().await.is_some() {
            let _ = tx.send(Message::Action(crate::app::Action::Quit));
        }
    });
    #[cfg(not(unix))]
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            let _ = tx.send(Message::Action(crate::app::Action::Quit));
        }
    });
}

/// The connecting stage the runtime can truthfully name for a target.
///
/// A target that requires encryption is negotiating TLS during the connect;
/// anything else is the ordinary server handshake. `Prefer` may or may not
/// negotiate TLS, so it is named as the broader server handshake rather than
/// claiming an encryption step that might not happen.
pub(crate) fn connecting_step_for(
    sslmode: crate::connection::SslMode,
) -> crate::app::model::ConnectingStep {
    use crate::app::model::ConnectingStep;
    use crate::connection::SslMode;
    match sslmode {
        SslMode::Require | SslMode::VerifyCa | SslMode::VerifyFull => ConnectingStep::TlsHandshake,
        SslMode::Disable | SslMode::Prefer => ConnectingStep::ServerHandshake,
    }
}

fn spawn_connect(
    tx: mpsc::UnboundedSender<Message>,
    install_tx: mpsc::Sender<SessionInstall>,
    target: Arc<ConnectionTarget>,
    config: Config,
    generation: Arc<std::sync::atomic::AtomicU64>,
    request: u64,
) {
    tokio::spawn(async move {
        let timeout = Duration::from_millis(config.query.statement_timeout_ms);
        if !connection_generation_is_current(&generation, request) {
            return;
        }
        // The runtime reports the stage it can honestly observe: opening the
        // session. A target that requires encryption is negotiating TLS on the
        // way, so that is the stage named; a plain target names the server
        // handshake. Resolution and credential work already happened before the
        // terminal was taken, so they are not re-run or shown as done here.
        let _ = tx.send(connection_message(
            request,
            Message::ConnectingStep {
                step: connecting_step_for(target.sslmode),
                target: target.safe_display(),
            },
        ));
        let reported = session::connect_reported(&target, timeout).await;
        send_attempt_notice(
            &tx,
            request,
            crate::app::model::NoticeSource::User,
            &reported.notice_report,
        );
        match reported.value {
            Ok(opened) => {
                let info = opened.info().clone();
                if !connection_generation_is_current(&generation, request) {
                    return;
                }
                if !request_session_install(
                    &install_tx,
                    opened,
                    crate::app::model::NoticeSource::User,
                    request,
                )
                .await
                {
                    return;
                }
                if !connection_generation_is_current(&generation, request) {
                    return;
                }
                let _ = tx.send(connection_message(
                    request,
                    Message::Connected(Box::new(info)),
                ));
                // The tree is populated as soon as there is something to read it
                // from, so the sidebar is useful the moment it appears.
                if !connection_generation_is_current(&generation, request) {
                    return;
                }
                send_connection_message(
                    &tx,
                    request,
                    Message::Action(crate::app::Action::ReloadObjects),
                );
                spawn_metadata_connect(tx, install_tx, target, timeout, generation, request);
            }
            Err(diagnostic) => {
                if !connection_generation_is_current(&generation, request) {
                    return;
                }
                let _ = tx.send(connection_message(
                    request,
                    Message::ConnectionFailed(Box::new(diagnostic)),
                ));
            }
        }
    });
}

/// Resolves one picker choice through the same profile and credential routes as
/// the command line. This runs in an async task because a cloud provider may
/// need to start a process or wait for its configured timeout.
async fn prepare_profile_target(
    profile: Option<&str>,
    config: &Config,
) -> Result<ConnectionTarget, Diagnostic> {
    prepare_profile_target_in(profile, config, &EnvSnapshot::from_process()).await
}

/// The body of [`prepare_profile_target`], with the environment passed in.
///
/// Tests supply a fixed snapshot so resolution never reads the machine's own
/// `PGPASSWORD` or password file. Hosted Windows runners ship `PGPASSWORD` set
/// for their preinstalled PostgreSQL, which made a no-password assertion fail
/// for reasons that had nothing to do with the profile under test.
async fn prepare_profile_target_in(
    profile: Option<&str>,
    config: &Config,
    env: &EnvSnapshot,
) -> Result<ConnectionTarget, Diagnostic> {
    let connection = ConnectionOptions {
        profile: profile.map(str::to_owned),
        ..ConnectionOptions::default()
    };
    let (requested, args) = crate::cli::resolve_target_and_profile(None, &connection, config)?;
    let target = crate::connection::resolve(requested, &args, env, &config.connection)?;
    if target.auth.is_none() {
        return Ok(target);
    }
    let providers = crate::cli::auth_providers(config)?;
    crate::connection::cloud::authenticate(target, &providers)
        .await
        .map_err(|error| *error)
}

fn connection_generation_is_current(
    generation: &std::sync::atomic::AtomicU64,
    request: u64,
) -> bool {
    generation.load(std::sync::atomic::Ordering::SeqCst) == request
}

/// Installs the resolved target used by a later reconnect.
/// Check identity while holding the slot, since acquiring it may yield.
async fn install_connection_target(
    slot: &tokio::sync::RwLock<Option<Arc<ConnectionTarget>>>,
    target: Arc<ConnectionTarget>,
    generation: &std::sync::atomic::AtomicU64,
    request: u64,
) -> bool {
    let mut held = slot.write().await;
    if !connection_generation_is_current(generation, request) {
        return false;
    }
    *held = Some(target);
    true
}

/// Wraps a server-bound result before it enters the shared application queue.
///
/// The atomic check in a worker prevents unnecessary work after a switch. The
/// envelope is the stronger boundary: it also rejects a result that won a race
/// with the switch and was queued before the reducer saw the profile choice.
fn connection_message(generation: u64, message: Message) -> Message {
    Message::ForConnection {
        generation,
        message: Box::new(message),
    }
}

/// Queues an asynchronous connection outcome with its source identity.
/// A pre-send generation check cannot protect a message already in the queue.
fn send_connection_message(tx: &mpsc::UnboundedSender<Message>, generation: u64, message: Message) {
    let _ = tx.send(connection_message(generation, message));
}

/// The connection generation a server-bound worker belongs to.
struct ConnectionScope {
    generation: Arc<std::sync::atomic::AtomicU64>,
    request: u64,
}

impl ConnectionScope {
    fn is_current(&self) -> bool {
        connection_generation_is_current(&self.generation, self.request)
    }
}

/// Opens the connection the object tree reads on.
///
/// The target is the one that was already resolved, cloned rather than derived
/// again: the same host, the same credential route, the same TLS outcome. Two
/// things are changed on purpose, and both are visible on the server:
///
/// - the `application_name` says which connection this is, so someone reading
///   `pg_stat_activity` can tell the tree from the query;
/// - the session is asked to be read-only, because reading the catalogue is all
///   it ever does.
///
/// Failing to open it is not a failure of the session. The tree falls back to
/// the connection that is already there and says which one it is using, because
/// a server with a connection limit is a real place and this is a second
/// connection to it.
fn spawn_metadata_connect(
    tx: mpsc::UnboundedSender<Message>,
    install_tx: mpsc::Sender<SessionInstall>,
    target: Arc<ConnectionTarget>,
    timeout: Duration,
    generation: Arc<std::sync::atomic::AtomicU64>,
    request: u64,
) {
    if !connection_generation_is_current(&generation, request) {
        return;
    }
    let _ = tx.send(connection_message(
        request,
        Message::MetadataConnection(crate::app::model::MetadataLink::Opening),
    ));
    tokio::spawn(async move {
        let target = target.for_object_tree();
        let reported = session::connect_reported(&target, timeout).await;
        send_attempt_notice(
            &tx,
            request,
            crate::app::model::NoticeSource::Metadata,
            &reported.notice_report,
        );
        let link = match reported.value {
            Ok(opened) => {
                if !connection_generation_is_current(&generation, request) {
                    return;
                }
                if !request_session_install(
                    &install_tx,
                    opened,
                    crate::app::model::NoticeSource::Metadata,
                    request,
                )
                .await
                {
                    return;
                }
                if !connection_generation_is_current(&generation, request) {
                    return;
                }
                crate::app::model::MetadataLink::Dedicated
            }
            Err(diagnostic) => crate::app::model::MetadataLink::Unavailable(diagnostic.headline),
        };
        if !connection_generation_is_current(&generation, request) {
            return;
        }
        let _ = tx.send(connection_message(
            request,
            Message::MetadataConnection(link),
        ));
    });
}

fn spawn_execute(
    tx: mpsc::UnboundedSender<Message>,
    slot: Arc<tokio::sync::RwLock<Option<Arc<InstalledSession>>>>,
    job: JobId,
    sql: String,
    row_cap: usize,
    scope: ConnectionScope,
) {
    tokio::spawn(async move {
        if !scope.is_current() {
            return;
        }
        let opened = {
            let held = slot.read().await;
            held.clone()
        };
        let Some(session) = opened else {
            return;
        };
        if !scope.is_current() {
            return;
        }
        let execution = session.execute(&sql, row_cap, job).await;
        if !scope.is_current() {
            return;
        }
        send_operation_notice(&tx, &session, &execution.notice_report);
        if execution.status == ExecutionStatus::ConnectionLost {
            let _ = tx.send(connection_message(scope.request, Message::ConnectionLost));
        }
        let _ = tx.send(connection_message(
            scope.request,
            Message::ExecutionFinished(Box::new(execution)),
        ));
    });
}

fn spawn_execute_parameterized(
    tx: mpsc::UnboundedSender<Message>,
    slot: Arc<tokio::sync::RwLock<Option<Arc<InstalledSession>>>>,
    job: JobId,
    sql: String,
    parameters: crate::query::ParameterBindings,
    row_cap: usize,
    scope: ConnectionScope,
) {
    tokio::spawn(async move {
        if !scope.is_current() {
            return;
        }
        let opened = {
            let held = slot.read().await;
            held.clone()
        };
        let Some(session) = opened else {
            return;
        };
        if !scope.is_current() {
            return;
        }
        let execution = session
            .execute_with_parameters(&sql, &parameters, row_cap, job)
            .await;
        if !scope.is_current() {
            return;
        }
        send_operation_notice(&tx, &session, &execution.notice_report);
        if execution.status == ExecutionStatus::ConnectionLost {
            let _ = tx.send(connection_message(scope.request, Message::ConnectionLost));
        }
        let _ = tx.send(connection_message(
            scope.request,
            Message::ExecutionFinished(Box::new(execution)),
        ));
    });
}

fn spawn_explain(
    tx: mpsc::UnboundedSender<Message>,
    slot: Arc<tokio::sync::RwLock<Option<Arc<InstalledSession>>>>,
    job: JobId,
    sql: String,
    analyzed: bool,
    scope: ConnectionScope,
) {
    tokio::spawn(async move {
        if !scope.is_current() {
            return;
        }
        let opened = {
            let held = slot.read().await;
            held.clone()
        };
        let Some(session) = opened else {
            let _ = tx.send(connection_message(
                scope.request,
                Message::PlanFinished(Box::new(crate::app::PlanExecution {
                    notice_report: NoticeReport::default(),
                    job,
                    analyzed,
                    elapsed: Duration::ZERO,
                    transaction: crate::query::result::TransactionState::Unknown,
                    result: Err(Diagnostic::new(
                        DiagnosticKind::Connection,
                        "the connection was unavailable while reading the query plan",
                        "reading a structured query plan",
                    )
                    .next_action("reconnect before asking for a plan")),
                    connection_lost: true,
                })),
            ));
            return;
        };
        let started = std::time::Instant::now();
        let reported = session.explain_reported(&sql, analyzed).await;
        let result = match reported.value.result {
            Ok(json) => crate::query::PlanDocument::parse(&json, analyzed).map_err(|error| {
                Diagnostic::new(
                    DiagnosticKind::Query,
                    "the server returned a plan the client could not read",
                    "reading a structured query plan",
                )
                .likely_cause(error.to_string())
                .next_action(
                    "run the statement normally, or inspect the plan with PostgreSQL tools",
                )
            }),
            Err(error) => Err(error),
        };
        let connection_lost = reported.value.connection_lost;
        let transaction = reported.value.transaction;
        if !scope.is_current() {
            return;
        }
        send_operation_notice(&tx, &session, &reported.notice_report);
        let _ = tx.send(connection_message(
            scope.request,
            Message::PlanFinished(Box::new(crate::app::PlanExecution {
                notice_report: reported.notice_report,
                job,
                analyzed,
                elapsed: started.elapsed(),
                transaction,
                result,
                connection_lost,
            })),
        ));
    });
}

fn spawn_cancel(
    tx: mpsc::UnboundedSender<Message>,
    slot: Arc<tokio::sync::RwLock<Option<Arc<InstalledSession>>>>,
    job: JobId,
    scope: ConnectionScope,
) {
    tokio::spawn(async move {
        if !scope.is_current() {
            return;
        }
        let opened = {
            let held = slot.read().await;
            held.clone()
        };
        let Some(session) = opened else {
            return;
        };
        if !scope.is_current() {
            return;
        }
        let handle = session.cancel_handle();
        match handle.cancel().await {
            Ok(()) => {
                if !scope.is_current() {
                    return;
                }
                let _ = tx.send(connection_message(
                    scope.request,
                    Message::CancellationDelivered(job),
                ));
            }
            Err(diagnostic) => {
                if !scope.is_current() {
                    return;
                }
                let _ = tx.send(connection_message(
                    scope.request,
                    Message::CancellationFailed(Box::new(diagnostic)),
                ));
            }
        }
    });
}

fn spawn_load_schemas(
    tx: mpsc::UnboundedSender<Message>,
    slot: Arc<tokio::sync::RwLock<Option<Arc<InstalledSession>>>>,
    generation: Arc<std::sync::atomic::AtomicU64>,
    request_generation: u64,
) {
    tokio::spawn(async move {
        let opened = {
            let held = slot.read().await;
            held.clone()
        };
        let Some(session) = opened else {
            return;
        };
        let result = session.schemas().await;
        if !connection_generation_is_current(&generation, request_generation) {
            return;
        }
        let _ = tx.send(connection_message(
            request_generation,
            Message::SchemasLoaded(Box::new(result)),
        ));
    });
}

fn spawn_load_completion_catalog(
    tx: mpsc::UnboundedSender<Message>,
    slot: Arc<tokio::sync::RwLock<Option<Arc<InstalledSession>>>>,
    request: u64,
    generation: Arc<std::sync::atomic::AtomicU64>,
    request_generation: u64,
) {
    tokio::spawn(async move {
        let opened = {
            let held = slot.read().await;
            held.clone()
        };
        let Some(session) = opened else {
            return;
        };
        let result = session.completion_catalog().await;
        let loaded_at = chrono::Local::now()
            .format("%Y-%m-%d %H:%M:%S %:z")
            .to_string();
        if !connection_generation_is_current(&generation, request_generation) {
            return;
        }
        let _ = tx.send(connection_message(
            request_generation,
            Message::CompletionLoaded {
                request,
                loaded_at,
                result: Box::new(result),
            },
        ));
    });
}

fn spawn_load_metadata(
    tx: mpsc::UnboundedSender<Message>,
    slot: Arc<tokio::sync::RwLock<Option<Arc<InstalledSession>>>>,
    request: crate::app::tree::RequestId,
    path: crate::app::tree::NodePath,
    query: crate::app::tree::MetadataQuery,
    generation: Arc<std::sync::atomic::AtomicU64>,
    request_generation: u64,
) {
    use crate::app::tree::{MetadataPayload, MetadataQuery};
    tokio::spawn(async move {
        let opened = {
            let held = slot.read().await;
            held.clone()
        };
        let Some(session) = opened else {
            return;
        };
        // This runs on the tree's own connection where there is one, so a long
        // statement on the session cannot delay it. Where there is not, it
        // shares and waits behind that statement on the server; the interface
        // stays responsive either way and the node shows that it is waiting.
        let payload = match query {
            MetadataQuery::Schemas => session.schemas().await.map(MetadataPayload::Schemas),
            MetadataQuery::Objects { schema, kind } => session
                .objects(&schema, kind)
                .await
                .map(MetadataPayload::Objects),
            MetadataQuery::Relation { schema, relation } => {
                // Two catalogue queries, one answer: a relation's columns and
                // its indexes arrive together so the node fills in one step.
                match session.columns(&schema, &relation).await {
                    Ok(columns) => session
                        .indexes(&schema, &relation)
                        .await
                        .map(|indexes| MetadataPayload::Relation { columns, indexes }),
                    Err(error) => Err(error),
                }
            }
            MetadataQuery::Extensions => {
                session
                    .extensions()
                    .await
                    .map(|extensions| MetadataPayload::Relation {
                        columns: Vec::new(),
                        indexes: extensions,
                    })
            }
        };
        if !connection_generation_is_current(&generation, request_generation) {
            return;
        }
        let _ = tx.send(connection_message(
            request_generation,
            Message::MetadataLoaded {
                request,
                path,
                payload: Box::new(payload),
            },
        ));
    });
}

/// Reads the live relation facts for a pending result-cell update.
fn spawn_load_update_target(
    tx: mpsc::UnboundedSender<Message>,
    slot: Arc<tokio::sync::RwLock<Option<Arc<InstalledSession>>>>,
    request: u64,
    relation: crate::query::RelationReference,
    generation: Arc<std::sync::atomic::AtomicU64>,
    request_generation: u64,
) {
    tokio::spawn(async move {
        let opened = {
            let held = slot.read().await;
            held.clone()
        };
        let result = match opened {
            Some(session) => {
                session
                    .update_relation(relation.schema.as_deref(), &relation.relation)
                    .await
            }
            None => Err(Diagnostic::new(
                DiagnosticKind::Connection,
                "the connection was unavailable while reading update target metadata",
                "preparing a result-cell update",
            )
            .next_action("reconnect before preparing a cell update")),
        };
        if !connection_generation_is_current(&generation, request_generation) {
            return;
        }
        let _ = tx.send(connection_message(
            request_generation,
            Message::UpdateTargetLoaded {
                request,
                result: Box::new(result),
            },
        ));
    });
}

/// The connection the catalogue is read on: its own where there is one.
///
/// Falling back to the session's connection rather than failing is the point.
/// A tree that waits behind a long query is worse than one that does not, and
/// both are better than no tree.
fn reader(
    session: &Arc<tokio::sync::RwLock<Option<Arc<InstalledSession>>>>,
    metadata: &Arc<tokio::sync::RwLock<Option<Arc<InstalledSession>>>>,
) -> Arc<tokio::sync::RwLock<Option<Arc<InstalledSession>>>> {
    if metadata.try_read().is_ok_and(|held| held.is_some()) {
        Arc::clone(metadata)
    } else {
        Arc::clone(session)
    }
}

/// Reads an object's definition.
fn spawn_load_definition(
    tx: mpsc::UnboundedSender<Message>,
    slot: Arc<tokio::sync::RwLock<Option<Arc<InstalledSession>>>>,
    request: crate::app::tree::RequestId,
    object: crate::postgres::metadata::ObjectSummary,
    generation: Arc<std::sync::atomic::AtomicU64>,
    request_generation: u64,
) {
    tokio::spawn(async move {
        let opened = {
            let held = slot.read().await;
            held.clone()
        };
        let Some(session) = opened else {
            return;
        };
        let result = session.definition(&object).await;
        if !connection_generation_is_current(&generation, request_generation) {
            return;
        }
        let _ = tx.send(connection_message(
            request_generation,
            Message::DefinitionLoaded {
                request,
                result: Box::new(result),
            },
        ));
    });
}

/// Writes the selected shape of the rows on screen to a file.
///
/// What is written is what the pane shows: the rows the result kept, narrowed by
/// the filter if one is on. That is smaller than what the query returned
/// whenever the result was truncated, which is why the format palette and path
/// prompt say so before this runs and why the message afterwards says how many
/// rows there were.
///
/// The file is written through the same export machinery a scripted export
/// uses, so it lands complete or not at all, and never replaces a file that is
/// already there.
fn export_visible_rows(
    model: &Model,
    path: &str,
    format: crate::app::model::ExportFormat,
) -> Result<String, Diagnostic> {
    let Some(set) = model.visible_result() else {
        return Err(Diagnostic::new(
            DiagnosticKind::Usage,
            "there are no rows on screen to write",
            "writing the rows on screen",
        )
        .next_action("run a statement that returns rows first"));
    };
    if !set.is_available() {
        return Err(Diagnostic::new(
            DiagnosticKind::Usage,
            crate::query::result::RETAINED_DATA_UNAVAILABLE,
            "writing the retained result",
        )
        .next_action(crate::query::result::RETAINED_DATA_GUIDANCE));
    }
    let rows = model.filtered_rows();
    let mut visible = set.clone();
    visible.rows = rows
        .iter()
        .filter_map(|index| set.rows.get(*index).cloned())
        .collect();
    let visible_row_count = visible.rows.len();

    let destination = expand_home(path);
    // The advice is written for the reader who will see it: someone in the
    // client, who has no command-line flags to pass.
    let mut export = crate::query::Export::create_with_advice(
        &destination,
        false,
        "choose another name, or move the file that is there",
    )?;
    let execution = crate::query::result::Execution {
        notice_report: crate::query::result::NoticeReport::default(),
        job: crate::query::result::JobId(0),
        statements: vec![crate::query::result::StatementResult {
            result_set: Some(visible),
            rows_affected: None,
            elapsed: Duration::ZERO,
            notices: Vec::new(),
        }],
        status: crate::query::result::ExecutionStatus::Succeeded,
        elapsed: Duration::ZERO,
        error: None,
        transaction: crate::query::result::TransactionState::Autocommit,
    };
    let options = crate::cli::output::OutputOptions {
        format: match format {
            crate::app::model::ExportFormat::Csv => crate::cli::output::Format::Csv,
            crate::app::model::ExportFormat::Tsv => crate::cli::output::Format::Tsv,
            crate::app::model::ExportFormat::Json => crate::cli::output::Format::Json,
            crate::app::model::ExportFormat::Ndjson => crate::cli::output::Format::Ndjson,
            crate::app::model::ExportFormat::Markdown => crate::cli::output::Format::Markdown,
        },
        unicode: false,
        ..crate::cli::output::OutputOptions::default()
    };
    crate::cli::output::write_execution(export.writer(), &execution, &options)
        .map_err(|err| write_failure(path, &err))?;
    for _ in 0..visible_row_count {
        export.count_row();
    }
    let finished = export.finish()?;
    Ok(format!(
        "{} row(s) written to {}",
        finished.rows,
        finished.path.display()
    ))
}

/// Expands a leading `~`, because there is no shell here to do it.
///
/// A path typed into a prompt is not a path typed into a shell. Without this,
/// `~/exports/rows.csv` would quietly create a directory named `~` in whatever
/// the working directory happens to be, which is a mess someone finds weeks
/// later.
fn expand_home(path: &str) -> std::path::PathBuf {
    let Some(rest) = path.strip_prefix('~') else {
        return std::path::PathBuf::from(path);
    };
    let rest = rest.trim_start_matches(['/', '\\']);
    directories::BaseDirs::new().map_or_else(
        || std::path::PathBuf::from(path),
        |dirs| dirs.home_dir().join(rest),
    )
}

fn write_failure(path: &str, error: &std::io::Error) -> Diagnostic {
    Diagnostic::new(
        DiagnosticKind::ExportInterrupted,
        format!("could not write {path}"),
        "writing the rows on screen",
    )
    .likely_cause(error.to_string())
    .next_action("check the directory's permissions and free space")
}

/// Reads what an object depends on and what depends on it.
fn spawn_load_dependencies(
    tx: mpsc::UnboundedSender<Message>,
    slot: Arc<tokio::sync::RwLock<Option<Arc<InstalledSession>>>>,
    request: crate::app::tree::RequestId,
    object: crate::postgres::metadata::ObjectSummary,
    generation: Arc<std::sync::atomic::AtomicU64>,
    request_generation: u64,
) {
    tokio::spawn(async move {
        let opened = {
            let held = slot.read().await;
            held.clone()
        };
        let Some(session) = opened else {
            return;
        };
        let result = session.dependencies(&object).await;
        if !connection_generation_is_current(&generation, request_generation) {
            return;
        }
        let _ = tx.send(connection_message(
            request_generation,
            Message::DependenciesLoaded {
                request,
                result: Box::new(result),
            },
        ));
    });
}

/// The buffer the client opens with: something useful, not a blank screen.
fn starter_query() -> &'static str {
    "-- Ctrl+R runs the whole buffer. Ctrl+T runs the statement at the cursor.\n\
     SELECT current_database(), current_user, version();"
}

/// Connects, runs SQL once, and closes. Used by `query`.
///
/// Ctrl+C asks the server to cancel rather than killing the process. A script
/// that interrupts a long statement gets the documented cancellation exit code
/// and a statement that is actually stopped, instead of a half-run statement and
/// a signal death.
pub async fn execute_once(
    target: ConnectionTarget,
    config: &Config,
    sql: String,
    row_cap: usize,
) -> Result<Execution, Diagnostic> {
    execute_once_with_parameters(target, config, sql, None, row_cap).await
}

/// Connects, binds any supplied named values, runs SQL once, and closes.
pub async fn execute_once_with_parameters(
    target: ConnectionTarget,
    config: &Config,
    sql: String,
    parameters: Option<crate::query::ParameterBindings>,
    row_cap: usize,
) -> Result<Execution, Diagnostic> {
    execute_once_with_parameters_reported(target, config, sql, parameters, row_cap)
        .await
        .map_err(|failure| failure.value)
}

/// Connects and executes while preserving observed notices on connection failure.
pub async fn execute_once_with_parameters_reported(
    target: ConnectionTarget,
    config: &Config,
    sql: String,
    parameters: Option<crate::query::ParameterBindings>,
    row_cap: usize,
) -> Result<Execution, Reported<Diagnostic>> {
    let timeout = Duration::from_millis(config.query.statement_timeout_ms);
    let connected = session::connect_reported(&target, timeout).await;
    let session = match connected.value {
        Ok(session) => session,
        Err(value) => {
            return Err(Reported {
                value,
                notice_report: connected.notice_report,
            });
        }
    };
    Ok(
        execute_cancellable_with_parameters(&session, &sql, parameters.as_ref(), row_cap, JobId(1))
            .await,
    )
}

/// Runs a statement, letting Ctrl+C ask the server to cancel it.
///
/// The statement is awaited either way: whether it stopped is the server's
/// answer to give, and reporting anything else would be a guess.
pub async fn execute_cancellable(
    session: &Session,
    sql: &str,
    row_cap: usize,
    job: JobId,
) -> Execution {
    execute_cancellable_with_parameters(session, sql, None, row_cap, job).await
}

/// Runs SQL with optional named values, letting Ctrl+C ask the server to cancel.
pub async fn execute_cancellable_with_parameters(
    session: &Session,
    sql: &str,
    parameters: Option<&crate::query::ParameterBindings>,
    row_cap: usize,
    job: JobId,
) -> Execution {
    let cancel = session.cancel_handle();
    let execution = async {
        match parameters {
            Some(parameters) => {
                session
                    .execute_with_parameters(sql, parameters, row_cap, job)
                    .await
            }
            None => session.execute(sql, row_cap, job).await,
        }
    };
    tokio::pin!(execution);

    tokio::select! {
        result = &mut execution => result,
        signal = tokio::signal::ctrl_c() => {
            if signal.is_ok() {
                let _ = cancel.cancel().await;
            }
            execution.await
        }
    }
}

/// How an export ended.
#[derive(Debug)]
pub enum ExportOutcome {
    /// Every row was written and the file is in place.
    Completed(crate::query::export::Finished),
    /// It stopped early. The partial file holds what was written.
    Interrupted {
        /// What was left behind.
        abandoned: crate::query::export::Abandoned,
        /// Why it stopped.
        reason: Box<Diagnostic>,
    },
}

/// Connects, streams one statement into a file, and closes.
///
/// The complete result is not retained; individual rows and file writes are
/// buffered. Ctrl+C asks the
/// server to cancel and leaves the partial file behind, because rows that were
/// written are worth keeping and a truncated file at the destination would look
/// complete.
pub async fn export_once(
    target: ConnectionTarget,
    config: &Config,
    sql: String,
    export: crate::query::export::Export,
    writer: crate::cli::output::StreamWriter,
) -> Result<ExportOutcome, Diagnostic> {
    export_once_with_parameters(target, config, sql, None, export, writer).await
}

/// Connects, binds any supplied named values, streams one statement into a file,
/// and closes.
pub async fn export_once_with_parameters(
    target: ConnectionTarget,
    config: &Config,
    sql: String,
    parameters: Option<crate::query::ParameterBindings>,
    export: crate::query::export::Export,
    writer: crate::cli::output::StreamWriter,
) -> Result<ExportOutcome, Diagnostic> {
    export_once_with_parameters_reported(target, config, sql, parameters, export, writer)
        .await
        .value
}

/// Streaming/finalization outcome with observed accounting even on a finish error.
pub async fn export_once_with_parameters_reported(
    target: ConnectionTarget,
    config: &Config,
    sql: String,
    parameters: Option<crate::query::ParameterBindings>,
    mut export: crate::query::export::Export,
    mut writer: crate::cli::output::StreamWriter,
) -> Reported<Result<ExportOutcome, Diagnostic>> {
    let timeout = Duration::from_millis(config.query.statement_timeout_ms);
    let connected = session::connect_reported(&target, timeout).await;
    let session = match connected.value {
        Ok(session) => session,
        Err(error) => {
            return Reported {
                value: Err(error),
                notice_report: connected.notice_report,
            };
        }
    };
    let cancel = session.cancel_handle();

    let outcome = {
        let mut sink = |event: crate::postgres::StreamEvent| -> std::io::Result<()> {
            write_export_event(&mut export, &mut writer, event)
        };

        let stream = async {
            match parameters.as_ref() {
                Some(parameters) => {
                    session
                        .stream_with_parameters_reported(&sql, parameters, &mut sink)
                        .await
                }
                None => session.stream_reported(&sql, &mut sink).await,
            }
        };
        tokio::pin!(stream);
        tokio::select! {
            result = &mut stream => result,
            signal = tokio::signal::ctrl_c() => {
                if signal.is_ok() {
                    let _ = cancel.cancel().await;
                }
                stream.await
            }
        }
    };

    finish_stream_export(outcome, export, crate::query::export::Export::finish)
}

fn finish_stream_export(
    outcome: Reported<Result<u64, crate::postgres::StreamStop>>,
    export: crate::query::export::Export,
    finish: impl FnOnce(
        crate::query::export::Export,
    ) -> Result<crate::query::export::Finished, Diagnostic>,
) -> Reported<Result<ExportOutcome, Diagnostic>> {
    let value = match outcome.value {
        Ok(_) => finish(export).map(ExportOutcome::Completed),
        Err(crate::postgres::StreamStop::Server(reason)) => Ok(ExportOutcome::Interrupted {
            abandoned: export.abandon(),
            reason,
        }),
        Err(crate::postgres::StreamStop::Sink(err)) => Ok(ExportOutcome::Interrupted {
            abandoned: export.abandon(),
            reason: Box::new(
                Diagnostic::new(
                    DiagnosticKind::ExportInterrupted,
                    "the export destination could not be written",
                    "writing rows to a file",
                )
                .likely_cause(err.to_string())
                .next_action("check free space and permissions"),
            ),
        }),
    };
    Reported {
        value,
        notice_report: outcome.notice_report,
    }
}

/// The streaming sink's actual file/encoder boundary, shared with its tests.
fn write_export_event(
    export: &mut crate::query::export::Export,
    writer: &mut crate::cli::output::StreamWriter,
    event: crate::postgres::StreamEvent,
) -> std::io::Result<()> {
    match event {
        crate::postgres::StreamEvent::Columns(columns) => writer.columns(export.writer(), columns),
        crate::postgres::StreamEvent::Row(row) => {
            writer.row(export.writer(), &row)?;
            export.count_row();
            Ok(())
        }
    }
}

/// Tests a connection target, reporting each stage that can be told apart.
pub async fn probe(target: &str, config: &Config) -> Result<Vec<Check>, Diagnostic> {
    let resolved = crate::connection::resolve(
        Some(target),
        &crate::connection::ConnectionArgs::default(),
        &EnvSnapshot::from_process(),
        &config.connection,
    )?;
    probe_target(resolved, config).await
}

/// Tests an already-resolved target.
///
/// The stages are separated only where they can honestly be told apart: name
/// resolution and TCP are observable from here, and everything after that is
/// reported from how the driver failed.
pub async fn probe_target(
    target: ConnectionTarget,
    config: &Config,
) -> Result<Vec<Check>, Diagnostic> {
    let mut checks = Vec::new();

    checks.push(Check::ok(
        "target",
        format!(
            "{} with sslmode={} ({})",
            target.safe_display(),
            target.sslmode.as_str(),
            target.sslmode.guarantee()
        ),
    ));

    match &target.host {
        crate::connection::Host::Socket(path) => {
            checks.push(if path.exists() {
                Check::ok("socket directory", path.display().to_string())
            } else {
                Check::fail(
                    "socket directory",
                    format!("{} does not exist", path.display()),
                    "check that PostgreSQL is running locally and using this socket directory",
                )
            });
        }
        crate::connection::Host::Tcp(host) => {
            let address = format!("{host}:{}", target.port);
            match tokio::time::timeout(target.connect_timeout, tokio::net::lookup_host(&address))
                .await
            {
                Ok(Ok(addresses)) => {
                    let found: Vec<String> = addresses.map(|a| a.ip().to_string()).collect();
                    if found.is_empty() {
                        checks.push(Check::fail(
                            "address resolution",
                            format!("{host} resolved to no addresses"),
                            "check the host name and this machine's DNS settings",
                        ));
                    } else {
                        checks.push(Check::ok(
                            "address resolution",
                            format!("{host} resolves to {}", found.join(", ")),
                        ));
                    }
                }
                Ok(Err(err)) => checks.push(Check::fail(
                    "address resolution",
                    format!("{host} could not be resolved: {err}"),
                    "check the host name and this machine's DNS settings",
                )),
                Err(_) => checks.push(Check::fail(
                    "address resolution",
                    format!("resolving {host} timed out"),
                    "check this machine's DNS settings",
                )),
            }

            match tokio::time::timeout(
                target.connect_timeout,
                tokio::net::TcpStream::connect(&address),
            )
            .await
            {
                Ok(Ok(_)) => {
                    checks.push(Check::ok("tcp", format!("{address} accepts connections")))
                }
                Ok(Err(err)) => checks.push(Check::fail(
                    "tcp",
                    format!("{address} refused the connection: {err}"),
                    "check that PostgreSQL is listening on this port and that a firewall allows it",
                )),
                Err(_) => checks.push(Check::fail(
                    "tcp",
                    format!(
                        "{address} did not answer within {:?}",
                        target.connect_timeout
                    ),
                    "check the address and any firewall between here and the server",
                )),
            }
        }
    }

    // Only attempt the full connection when the transport looks usable, so the
    // report does not repeat the same failure twice.
    if checks.iter().any(|c| c.status == CheckStatus::Fail) {
        checks.push(Check::skipped(
            "postgres",
            "skipped because the server could not be reached",
        ));
        return Ok(checks);
    }

    let timeout = Duration::from_millis(config.query.statement_timeout_ms);
    let connected = session::connect_reported(&target, timeout).await;
    add_notice_check(&mut checks, &connected.notice_report);
    match connected.value {
        Ok(opened) => {
            let info = opened.info();
            checks.push(Check::ok(
                "postgres",
                format!(
                    "connected as {} to {} on PostgreSQL {}",
                    info.user, info.database, info.server_version
                ),
            ));
            checks.push(match &info.tls {
                crate::postgres::TlsState::Active { .. } => {
                    Check::ok("tls", info.tls.description())
                }
                crate::postgres::TlsState::Disabled => Check::warn(
                    "tls",
                    info.tls.description(),
                    "use sslmode=verify-full for anything that leaves this machine",
                ),
                crate::postgres::TlsState::NotNegotiated => Check::warn(
                    "tls",
                    info.tls.description(),
                    "the server does not offer TLS; enable it there, or accept that this \
                     connection is in the clear",
                ),
                crate::postgres::TlsState::Unknown => Check::warn(
                    "tls",
                    info.tls.description(),
                    "grant the role access to pg_stat_ssl to confirm the transport",
                ),
            });
            checks.push(Check::ok(
                "session",
                format!(
                    "backend pid {}, search_path {}, {}",
                    info.backend_pid,
                    info.search_path,
                    info.posture()
                ),
            ));
            if let Some(report) = opened.take_connection_notice_report() {
                add_notice_check(&mut checks, &report);
            }
        }
        Err(diagnostic) => {
            checks.push(Check::fail(
                "postgres",
                diagnostic.headline.clone(),
                diagnostic
                    .next_action
                    .clone()
                    .unwrap_or_else(|| "check the connection details".to_owned()),
            ));
        }
    }

    for note in &target.notes {
        checks.push(Check::warn(
            format!("note: {}", note.subject),
            note.message.clone(),
            "see docs/support/compatibility.md for what this release supports",
        ));
    }

    // A second connection is a real cost on a server with a connection limit or
    // behind a pooler, so the command that explains what connecting does says
    // that the interactive client opens one.
    checks.push(Check::ok(
        "object tree connection",
        "the interactive client opens a second, read-only connection for the \
         object tree, so a long query cannot delay it. If that connection \
         cannot be opened, the tree shares this one and says so.",
    ));

    Ok(checks)
}

fn add_notice_check(checks: &mut Vec<Check>, report: &NoticeReport) {
    if report.counts.needs_report() {
        checks.push(Check::warn(
            "server notice retention",
            report.counts.lines().join(" "),
            "Review server logs separately if complete notice history is required.",
        ));
    }
}

#[cfg(test)]
mod tests;
