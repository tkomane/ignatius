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
use crate::query::result::{Execution, ExecutionStatus, JobId};
use crate::ui::keymap::Keymap;
use crate::ui::layout;
use crate::ui::theme::Theme;
use crossterm::event::{self, Event};
use std::io::Write;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

/// How often the input thread checks whether it should stop.
const INPUT_POLL: Duration = Duration::from_millis(100);

/// How often the interface redraws while something is happening.
///
/// Ticks are sent only while there is something to animate, so an idle client
/// wakes nothing and costs nothing.
const TICK: Duration = Duration::from_millis(90);

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
    let mut guard = crate::ui::TerminalGuard::acquire(options).map_err(|err| {
        Diagnostic::new(
            DiagnosticKind::Internal,
            "could not take control of the terminal",
            "opening the interactive client",
        )
        .likely_cause(err.to_string())
        .next_action("check that the terminal supports full-screen applications, or use --plain")
    })?;

    let history = crate::history::History::open(paths, &loaded.config.history, no_history);
    let library = crate::queries::Library::new(&paths.queries_dir);
    let result = runtime.block_on(event_loop(
        resolved,
        loaded.config,
        keymap,
        presentation,
        history,
        library,
    ));

    // Restore explicitly so any error below is printed on a working terminal.
    let restore = guard.restore();
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
        Ok(code)
    })
}

async fn event_loop(
    target: Option<ConnectionTarget>,
    config: Config,
    keymap: Keymap,
    presentation: &Presentation,
    mut history: crate::history::History,
    library: crate::queries::Library,
) -> Result<ExitCode, Diagnostic> {
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
    let ui = layout::Presentation::new(
        Theme::new(config.ui.theme, presentation.color),
        crate::ui::Glyphs::new(tier),
        config.ui.reduced_motion,
    );
    let mut model = Model::new(config.query.max_buffered_rows);
    model.keymap_snapshot = keymap.snapshot();
    let configured_providers = crate::cli::auth_providers(&config).unwrap_or_default();
    let providers = crate::connection::cloud::registry(&configured_providers);
    model.connection_profiles =
        crate::app::connection_picker::summaries(&config.profiles, &providers);
    model.connection = if target.is_some() {
        crate::app::ConnectionState::Connecting
    } else {
        crate::app::ConnectionState::Disconnected
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
    let input_stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    spawn_input_thread(tx.clone(), Arc::clone(&input_stop), &keymap);
    spawn_signal_watcher(tx.clone());

    // The clock lives here, not in the reducer. The ticker measures how long the
    // current activity has been running and passes it in, which is what keeps
    // every animated state reproducible in a test.
    let (activity, activity_rx) = tokio::sync::watch::channel(Some(std::time::Instant::now()));
    spawn_ticker(tx.clone(), activity_rx);

    // The connection is opened as an effect like any other, so the interface is
    // drawn and responsive while it happens. When the picker was shown, this
    // branch deliberately does not start a connection before a choice.
    let session: Arc<tokio::sync::RwLock<Option<Arc<Session>>>> =
        Arc::new(tokio::sync::RwLock::new(None));
    // The tree gets a connection of its own so a long statement cannot delay it.
    // It is opened from the same resolved target, so it reaches the same server
    // by the same route with the same credentials and the same protection; the
    // only differences are deliberate and visible on the server.
    let metadata: Arc<tokio::sync::RwLock<Option<Arc<Session>>>> =
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
            Arc::clone(&session),
            Arc::clone(&metadata),
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

    draw(&mut terminal, &model, &keymap, &ui)?;

    while let Some(message) = rx.recv().await {
        let effects = update(&mut model, message);
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
                        Arc::clone(&session),
                        Arc::clone(&metadata),
                        retry,
                        config.clone(),
                        Arc::clone(&connection_generation),
                        generation,
                    );
                }
                Effect::ConnectProfile { profile } => {
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
                    let selected_target = Arc::clone(&current_target);
                    let selected_session = Arc::clone(&session);
                    let selected_metadata = Arc::clone(&metadata);
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
                                *selected_target.write().await = Some(Arc::clone(&target));
                                if selected_generation.load(std::sync::atomic::Ordering::SeqCst)
                                    != generation
                                {
                                    return;
                                }
                                spawn_connect(
                                    selected_tx,
                                    selected_session,
                                    selected_metadata,
                                    target,
                                    selected_config,
                                    selected_generation,
                                    generation,
                                );
                            }
                            Err(diagnostic) => {
                                let _ = selected_tx
                                    .send(Message::ConnectionFailed(Box::new(diagnostic)));
                            }
                        }
                    });
                }
                Effect::Execute { job, sql } => {
                    spawn_execute(tx.clone(), Arc::clone(&session), job, sql, model.row_cap);
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
                    );
                }
                Effect::Explain { job, sql, analyze } => {
                    spawn_explain(tx.clone(), Arc::clone(&session), job, sql, analyze);
                }
                Effect::CopyValue { payload } => {
                    let message = copy_to_terminal(terminal.backend_mut(), &payload);
                    let _ = tx.send(message);
                }
                Effect::Cancel { job } => {
                    spawn_cancel(tx.clone(), Arc::clone(&session), job);
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

        draw(&mut terminal, &model, &keymap, &ui)?;
    }

    Ok(ExitCode::Success)
}

/// Performs the one-way clipboard write at the already-held terminal boundary.
///
/// The reducer has already made the opt-in, confirmation, and stale-candidate
/// decisions. This helper only writes and flushes the safe OSC 52 sequence, and
/// reports metadata or an I/O error without carrying the result value further.
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

fn draw(
    terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    model: &Model,
    keymap: &Keymap,
    ui: &layout::Presentation,
) -> Result<(), Diagnostic> {
    terminal
        .draw(|frame| {
            let area = frame.area();
            layout::render(model, keymap, ui, area, frame.buffer_mut());
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
                    Ok(_) => {}
                    Err(_) => return,
                },
                Ok(false) => {}
                Err(_) => return,
            }
        }
    });
}

/// Turns a termination signal into an ordinary quit, so the terminal is restored.
fn spawn_signal_watcher(tx: mpsc::UnboundedSender<Message>) {
    #[cfg(unix)]
    tokio::spawn(async move {
        use tokio::signal::unix::{SignalKind, signal};
        let Ok(mut term) = signal(SignalKind::terminate()) else {
            return;
        };
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

fn spawn_connect(
    tx: mpsc::UnboundedSender<Message>,
    slot: Arc<tokio::sync::RwLock<Option<Arc<Session>>>>,
    metadata: Arc<tokio::sync::RwLock<Option<Arc<Session>>>>,
    target: Arc<ConnectionTarget>,
    config: Config,
    generation: Arc<std::sync::atomic::AtomicU64>,
    request: u64,
) {
    tokio::spawn(async move {
        let timeout = Duration::from_millis(config.query.statement_timeout_ms);
        match session::connect(&target, timeout).await {
            Ok(opened) => {
                let info = opened.info().clone();
                let mut held = slot.write().await;
                if !connection_generation_is_current(&generation, request) {
                    return;
                }
                *held = Some(Arc::new(opened));
                drop(held);
                let _ = tx.send(Message::Connected(Box::new(info)));
                // The tree is populated as soon as there is something to read it
                // from, so the sidebar is useful the moment it appears.
                if !connection_generation_is_current(&generation, request) {
                    return;
                }
                let _ = tx.send(Message::Action(crate::app::Action::ReloadObjects));
                spawn_metadata_connect(tx, metadata, target, timeout, generation, request);
            }
            Err(diagnostic) => {
                if !connection_generation_is_current(&generation, request) {
                    return;
                }
                let _ = tx.send(Message::ConnectionFailed(Box::new(diagnostic)));
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
    slot: Arc<tokio::sync::RwLock<Option<Arc<Session>>>>,
    target: Arc<ConnectionTarget>,
    timeout: Duration,
    generation: Arc<std::sync::atomic::AtomicU64>,
    request: u64,
) {
    if !connection_generation_is_current(&generation, request) {
        return;
    }
    let _ = tx.send(Message::MetadataConnection(
        crate::app::model::MetadataLink::Opening,
    ));
    tokio::spawn(async move {
        let target = target.for_object_tree();
        let link = match session::connect(&target, timeout).await {
            Ok(opened) => {
                let mut held = slot.write().await;
                if !connection_generation_is_current(&generation, request) {
                    return;
                }
                *held = Some(Arc::new(opened));
                crate::app::model::MetadataLink::Dedicated
            }
            Err(diagnostic) => crate::app::model::MetadataLink::Unavailable(diagnostic.headline),
        };
        if !connection_generation_is_current(&generation, request) {
            return;
        }
        let _ = tx.send(Message::MetadataConnection(link));
    });
}

fn spawn_execute(
    tx: mpsc::UnboundedSender<Message>,
    slot: Arc<tokio::sync::RwLock<Option<Arc<Session>>>>,
    job: JobId,
    sql: String,
    row_cap: usize,
) {
    tokio::spawn(async move {
        let Some(session) = slot.read().await.clone() else {
            return;
        };
        let execution = session.execute(&sql, row_cap, job).await;
        if execution.status == ExecutionStatus::ConnectionLost {
            let _ = tx.send(Message::ConnectionLost);
        }
        let _ = tx.send(Message::ExecutionFinished(Box::new(execution)));
    });
}

fn spawn_execute_parameterized(
    tx: mpsc::UnboundedSender<Message>,
    slot: Arc<tokio::sync::RwLock<Option<Arc<Session>>>>,
    job: JobId,
    sql: String,
    parameters: crate::query::ParameterBindings,
    row_cap: usize,
) {
    tokio::spawn(async move {
        let Some(session) = slot.read().await.clone() else {
            return;
        };
        let execution = session
            .execute_with_parameters(&sql, &parameters, row_cap, job)
            .await;
        if execution.status == ExecutionStatus::ConnectionLost {
            let _ = tx.send(Message::ConnectionLost);
        }
        let _ = tx.send(Message::ExecutionFinished(Box::new(execution)));
    });
}

fn spawn_explain(
    tx: mpsc::UnboundedSender<Message>,
    slot: Arc<tokio::sync::RwLock<Option<Arc<Session>>>>,
    job: JobId,
    sql: String,
    analyzed: bool,
) {
    tokio::spawn(async move {
        let Some(session) = slot.read().await.clone() else {
            let _ = tx.send(Message::PlanFinished(Box::new(crate::app::PlanExecution {
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
            })));
            return;
        };
        let started = std::time::Instant::now();
        let result = match session.explain(&sql, analyzed).await {
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
        let connection_lost = session.is_closed();
        let transaction = if connection_lost {
            crate::query::result::TransactionState::Unknown
        } else {
            session.transaction_state().await
        };
        let _ = tx.send(Message::PlanFinished(Box::new(crate::app::PlanExecution {
            job,
            analyzed,
            elapsed: started.elapsed(),
            transaction,
            result,
            connection_lost,
        })));
    });
}

fn spawn_cancel(
    tx: mpsc::UnboundedSender<Message>,
    slot: Arc<tokio::sync::RwLock<Option<Arc<Session>>>>,
    job: JobId,
) {
    tokio::spawn(async move {
        let Some(session) = slot.read().await.clone() else {
            return;
        };
        let handle = session.cancel_handle();
        match handle.cancel().await {
            Ok(()) => {
                let _ = tx.send(Message::CancellationDelivered(job));
            }
            Err(diagnostic) => {
                let _ = tx.send(Message::CancellationFailed(Box::new(diagnostic)));
            }
        }
    });
}

fn spawn_load_schemas(
    tx: mpsc::UnboundedSender<Message>,
    slot: Arc<tokio::sync::RwLock<Option<Arc<Session>>>>,
    generation: Arc<std::sync::atomic::AtomicU64>,
    request_generation: u64,
) {
    tokio::spawn(async move {
        let Some(session) = slot.read().await.clone() else {
            return;
        };
        let result = session.schemas().await;
        if !connection_generation_is_current(&generation, request_generation) {
            return;
        }
        let _ = tx.send(Message::SchemasLoaded(Box::new(result)));
    });
}

fn spawn_load_completion_catalog(
    tx: mpsc::UnboundedSender<Message>,
    slot: Arc<tokio::sync::RwLock<Option<Arc<Session>>>>,
    request: u64,
    generation: Arc<std::sync::atomic::AtomicU64>,
    request_generation: u64,
) {
    tokio::spawn(async move {
        let Some(session) = slot.read().await.clone() else {
            return;
        };
        let result = session.completion_catalog().await;
        let loaded_at = chrono::Local::now()
            .format("%Y-%m-%d %H:%M:%S %:z")
            .to_string();
        if !connection_generation_is_current(&generation, request_generation) {
            return;
        }
        let _ = tx.send(Message::CompletionLoaded {
            request,
            loaded_at,
            result: Box::new(result),
        });
    });
}

fn spawn_load_metadata(
    tx: mpsc::UnboundedSender<Message>,
    slot: Arc<tokio::sync::RwLock<Option<Arc<Session>>>>,
    request: crate::app::tree::RequestId,
    path: crate::app::tree::NodePath,
    query: crate::app::tree::MetadataQuery,
    generation: Arc<std::sync::atomic::AtomicU64>,
    request_generation: u64,
) {
    use crate::app::tree::{MetadataPayload, MetadataQuery};
    tokio::spawn(async move {
        let Some(session) = slot.read().await.clone() else {
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
        let _ = tx.send(Message::MetadataLoaded {
            request,
            path,
            payload: Box::new(payload),
        });
    });
}

/// Reads the live relation facts for a pending result-cell update.
fn spawn_load_update_target(
    tx: mpsc::UnboundedSender<Message>,
    slot: Arc<tokio::sync::RwLock<Option<Arc<Session>>>>,
    request: u64,
    relation: crate::query::RelationReference,
    generation: Arc<std::sync::atomic::AtomicU64>,
    request_generation: u64,
) {
    tokio::spawn(async move {
        let result = match slot.read().await.clone() {
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
        let _ = tx.send(Message::UpdateTargetLoaded {
            request,
            result: Box::new(result),
        });
    });
}

/// The connection the catalogue is read on: its own where there is one.
///
/// Falling back to the session's connection rather than failing is the point.
/// A tree that waits behind a long query is worse than one that does not, and
/// both are better than no tree.
fn reader(
    session: &Arc<tokio::sync::RwLock<Option<Arc<Session>>>>,
    metadata: &Arc<tokio::sync::RwLock<Option<Arc<Session>>>>,
) -> Arc<tokio::sync::RwLock<Option<Arc<Session>>>> {
    if metadata.try_read().is_ok_and(|held| held.is_some()) {
        Arc::clone(metadata)
    } else {
        Arc::clone(session)
    }
}

/// Reads an object's definition.
fn spawn_load_definition(
    tx: mpsc::UnboundedSender<Message>,
    slot: Arc<tokio::sync::RwLock<Option<Arc<Session>>>>,
    request: crate::app::tree::RequestId,
    object: crate::postgres::metadata::ObjectSummary,
    generation: Arc<std::sync::atomic::AtomicU64>,
    request_generation: u64,
) {
    tokio::spawn(async move {
        let Some(session) = slot.read().await.clone() else {
            return;
        };
        let result = session.definition(&object).await;
        if !connection_generation_is_current(&generation, request_generation) {
            return;
        }
        let _ = tx.send(Message::DefinitionLoaded {
            request,
            result: Box::new(result),
        });
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
    slot: Arc<tokio::sync::RwLock<Option<Arc<Session>>>>,
    request: crate::app::tree::RequestId,
    object: crate::postgres::metadata::ObjectSummary,
    generation: Arc<std::sync::atomic::AtomicU64>,
    request_generation: u64,
) {
    tokio::spawn(async move {
        let Some(session) = slot.read().await.clone() else {
            return;
        };
        let result = session.dependencies(&object).await;
        if !connection_generation_is_current(&generation, request_generation) {
            return;
        }
        let _ = tx.send(Message::DependenciesLoaded {
            request,
            result: Box::new(result),
        });
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
    let timeout = Duration::from_millis(config.query.statement_timeout_ms);
    let session = session::connect(&target, timeout).await?;
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
/// Nothing is buffered: rows go from the wire to the file. Ctrl+C asks the
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
    mut export: crate::query::export::Export,
    mut writer: crate::cli::output::StreamWriter,
) -> Result<ExportOutcome, Diagnostic> {
    let timeout = Duration::from_millis(config.query.statement_timeout_ms);
    let session = session::connect(&target, timeout).await?;
    let cancel = session.cancel_handle();

    let outcome = {
        let mut sink = |event: crate::postgres::StreamEvent| -> std::io::Result<()> {
            match event {
                crate::postgres::StreamEvent::Columns(columns) => {
                    let destination = export.writer();
                    writer.columns(destination, columns)
                }
                crate::postgres::StreamEvent::Row(row) => {
                    let result = {
                        let destination = export.writer();
                        writer.row(destination, &row)
                    };
                    export.count_row();
                    result
                }
            }
        };

        let stream = async {
            match parameters.as_ref() {
                Some(parameters) => {
                    session
                        .stream_with_parameters(&sql, parameters, &mut sink)
                        .await
                }
                None => session.stream(&sql, &mut sink).await,
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

    match outcome {
        Ok(_) => Ok(ExportOutcome::Completed(export.finish()?)),
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
    match session::connect(&target, timeout).await {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_connection_switch_rejects_late_work_from_the_old_generation() {
        let generation = std::sync::atomic::AtomicU64::new(7);
        assert!(connection_generation_is_current(&generation, 7));

        generation.store(8, std::sync::atomic::Ordering::SeqCst);

        assert!(!connection_generation_is_current(&generation, 7));
        assert!(connection_generation_is_current(&generation, 8));
    }

    #[test]
    fn the_starter_buffer_is_useful_and_names_the_run_key() {
        let text = starter_query();
        assert!(text.contains("Ctrl+R"), "{text}");
        assert!(text.contains("SELECT"), "{text}");
        // It must be safe to run against anything, including production.
        assert!(!text.to_uppercase().contains("DROP"));
        assert!(!text.to_uppercase().contains("DELETE"));
        assert!(!text.to_uppercase().contains("UPDATE"));
        assert!(
            crate::query::split(text).len() == 1,
            "one statement, not a surprise batch"
        );
    }

    #[tokio::test]
    async fn a_picker_profile_is_resolved_by_the_existing_connection_boundary() {
        let config: Config = toml::from_str(
            "[profiles.orders-dev]\n\
             host = \"127.0.0.1\"\n\
             port = 55432\n\
             dbname = \"ignatius_demo\"\n\
             user = \"ignatius_test\"\n\
             sslmode = \"disable\"\n\
             environment = \"development\"\n\
             read-only = true\n",
        )
        .expect("profile config");

        let target =
            prepare_profile_target_in(Some("orders-dev"), &config, &EnvSnapshot::default())
                .await
                .expect("resolve without opening a database");
        assert_eq!(
            target.safe_display(),
            "ignatius_test@127.0.0.1:55432/ignatius_demo"
        );
        assert_eq!(target.sslmode, crate::connection::SslMode::Disable);
        assert_eq!(
            target.environment,
            crate::connection::Environment::Development
        );
        assert!(target.read_only);
        assert!(target.password.is_none());
    }

    #[tokio::test]
    async fn a_picker_unknown_profile_refuses_without_falling_back_to_defaults() {
        let config: Config = toml::from_str("[profiles.orders-dev]\nhost = \"127.0.0.1\"\n")
            .expect("profile config");

        let error =
            prepare_profile_target_in(Some("orders-prod"), &config, &EnvSnapshot::default())
                .await
                .expect_err("unknown selection must not silently use defaults");
        assert_eq!(error.kind, DiagnosticKind::Config);
        assert!(
            error
                .likely_cause
                .as_deref()
                .is_some_and(|cause| cause.contains("orders-dev")),
            "known profiles should be named: {error:?}"
        );
    }

    #[tokio::test]
    async fn a_picker_cloud_route_keeps_the_existing_encryption_gate_before_provider_work() {
        let config: Config = toml::from_str(
            "[profiles.orders-local]\n\
             host = \"127.0.0.1\"\n\
             dbname = \"orders\"\n\
             sslmode = \"disable\"\n\
             auth = \"entra\"\n",
        )
        .expect("profile config");

        let error =
            prepare_profile_target_in(Some("orders-local"), &config, &EnvSnapshot::default())
                .await
                .expect_err("cloud credentials may not cross an unencrypted route");
        assert!(
            error.headline.contains("encrypted") || error.headline.contains("TLS"),
            "the existing encryption gate should explain the refusal: {error:?}"
        );
        assert!(
            !format!("{error:?}").contains("az account"),
            "the provider command is not exposed by the refusal: {error:?}"
        );
    }

    /// A model holding two rows, as if a query had just returned them.
    fn model_with_rows() -> Model {
        use crate::query::result::{Execution, ExecutionStatus, JobId, ResultSet, StatementResult};
        use crate::query::value::Cell;

        let mut set = ResultSet::new(vec!["name".into(), "note".into()], 100);
        set.push(vec![
            Cell::Text("alpha".into()),
            Cell::Text("has, a comma".into()),
        ]);
        set.push(vec![Cell::Text("beta".into()), Cell::Null]);

        let mut model = Model::new(100);
        model.last_execution = Some(Execution {
            job: JobId(1),
            statements: vec![StatementResult {
                result_set: Some(set),
                rows_affected: Some(2),
                elapsed: Duration::from_millis(1),
                notices: Vec::new(),
            }],
            status: ExecutionStatus::Succeeded,
            elapsed: Duration::from_millis(1),
            error: None,
            transaction: crate::query::result::TransactionState::Autocommit,
        });
        model
    }

    #[test]
    fn what_is_written_is_what_the_pane_shows_and_it_is_valid_csv() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("rows.csv");
        let model = model_with_rows();

        let message = export_visible_rows(
            &model,
            path.to_str().expect("utf-8"),
            crate::app::model::ExportFormat::Csv,
        )
        .expect("writes");
        assert!(message.contains("2 row(s)"), "{message}");

        let written = std::fs::read_to_string(&path).expect("read");
        assert_eq!(
            written, "name,note\nalpha,\"has, a comma\"\nbeta,\n",
            "the header, a quoted field, and a NULL as nothing at all"
        );

        // A filter narrows the file exactly as it narrows the pane.
        let mut filtered = model_with_rows();
        filtered.result_filter = "beta".into();
        let second = dir.path().join("filtered.csv");
        export_visible_rows(
            &filtered,
            second.to_str().expect("utf-8"),
            crate::app::model::ExportFormat::Csv,
        )
        .expect("writes");
        assert_eq!(
            std::fs::read_to_string(&second).expect("read"),
            "name,note\nbeta,\n"
        );
    }

    #[test]
    fn json_export_keeps_the_server_count_and_truncation_boundary() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("filtered.json");
        let mut model = model_with_rows();
        let set = model
            .last_execution
            .as_mut()
            .expect("execution")
            .statements
            .first_mut()
            .and_then(|statement| statement.result_set.as_mut())
            .expect("result");
        set.cap = 1;
        set.rows_seen = 3;
        model.result_filter = "beta".into();

        export_visible_rows(
            &model,
            path.to_str().expect("utf-8"),
            crate::app::model::ExportFormat::Json,
        )
        .expect("writes");

        let document: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read"))
                .expect("valid json");
        assert_eq!(document[0]["rows"].as_array().expect("rows").len(), 1);
        assert_eq!(document[0]["rows_seen"], 3);
        assert_eq!(document[0]["truncated"], true);
    }

    #[test]
    fn writing_never_replaces_a_file_and_says_what_can_be_done_about_it() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("rows.csv");
        std::fs::write(&path, "precious").expect("write");

        let error = export_visible_rows(
            &model_with_rows(),
            path.to_str().expect("utf-8"),
            crate::app::model::ExportFormat::Csv,
        )
        .expect_err("must refuse");
        let action = error.next_action.expect("an action");
        assert!(
            !action.contains("--force"),
            "there are no flags to pass in here: {action}"
        );
        assert!(action.contains("choose another name"), "{action}");
        assert_eq!(
            std::fs::read_to_string(&path).expect("read"),
            "precious",
            "and the file that was there is still there"
        );
    }

    #[test]
    fn a_leading_tilde_means_home_rather_than_a_directory_called_tilde() {
        // There is no shell behind this prompt, so the expansion is ours to do.
        let expanded = expand_home("~/exports/rows.csv");
        assert!(!expanded.starts_with("~"), "{}", expanded.display());
        assert!(
            expanded.ends_with("exports/rows.csv"),
            "{}",
            expanded.display()
        );
        assert_eq!(
            expand_home("rows.csv"),
            std::path::PathBuf::from("rows.csv")
        );
        assert_eq!(
            expand_home("/tmp/rows.csv"),
            std::path::PathBuf::from("/tmp/rows.csv")
        );
    }

    #[test]
    fn there_is_nothing_to_write_when_no_query_has_run() {
        let error = export_visible_rows(
            &Model::new(100),
            "rows.csv",
            crate::app::model::ExportFormat::Csv,
        )
        .expect_err("must refuse");
        assert!(error.next_action.is_some());
        assert!(!std::path::Path::new("rows.csv").exists());
    }

    #[tokio::test]
    async fn probing_an_unreachable_host_fails_with_actions_and_skips_the_rest() {
        let config = Config::default();
        let target = crate::connection::resolve(
            // Port 1 on loopback is not a PostgreSQL server.
            Some("postgres://app@127.0.0.1:1/orders?connect_timeout=2"),
            &crate::connection::ConnectionArgs::default(),
            &EnvSnapshot::default(),
            &config.connection,
        )
        .expect("resolve");

        let checks = probe_target(target, &config).await.expect("probe");
        let tcp = checks.iter().find(|c| c.name == "tcp").expect("tcp check");
        assert_eq!(tcp.status, CheckStatus::Fail);
        assert!(tcp.next_action.is_some(), "a failure must say what to do");

        let postgres = checks
            .iter()
            .find(|c| c.name == "postgres")
            .expect("postgres check");
        assert_eq!(
            postgres.status,
            CheckStatus::Skipped,
            "the same failure must not be reported twice"
        );
    }

    #[tokio::test]
    async fn probing_reports_the_requested_guarantee_before_connecting() {
        let config = Config::default();
        let target = crate::connection::resolve(
            Some("postgres://app@127.0.0.1:1/orders?sslmode=require&connect_timeout=2"),
            &crate::connection::ConnectionArgs::default(),
            &EnvSnapshot::default(),
            &config.connection,
        )
        .expect("resolve");

        let checks = probe_target(target, &config).await.expect("probe");
        let target_check = &checks[0];
        assert!(
            target_check.detail.contains("sslmode=require"),
            "{target_check:?}"
        );
        assert!(
            target_check.detail.contains("no identity check"),
            "the report must not let require pass for verification: {target_check:?}"
        );
    }

    #[test]
    fn clipboard_runtime_writes_one_sequence_and_flushes_before_reporting_sent() {
        let payload = crate::clipboard::ClipboardPayload::try_new("hé".to_owned()).expect("fits");
        let expected = crate::clipboard::osc52_sequence(&payload);
        let message = {
            let mut writer = RecordingWriter::default();
            let message = copy_to_terminal(&mut writer, &payload);
            assert_eq!(writer.writes, 1);
            assert_eq!(writer.flushes, 1);
            assert_eq!(writer.output, expected.as_bytes());
            message
        };
        assert_eq!(
            message,
            Message::ClipboardSent {
                bytes: 3,
                characters: 2
            }
        );
    }

    #[test]
    fn clipboard_runtime_failure_is_safe_and_actionable() {
        let payload =
            crate::clipboard::ClipboardPayload::try_new("sensitive".to_owned()).expect("fits");
        let message = copy_to_terminal(&mut FailingWriter, &payload);
        let Message::ClipboardFailed(diagnostic) = message else {
            panic!("expected a safe failure message");
        };
        assert!(diagnostic.next_action.as_deref().is_some_and(|action| {
            action.contains("terminal output") && action.contains("export")
        }));
        assert!(!format!("{diagnostic:?}").contains("sensitive"));
    }

    #[derive(Default)]
    struct RecordingWriter {
        output: Vec<u8>,
        writes: usize,
        flushes: usize,
    }

    impl Write for RecordingWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.writes += 1;
            self.output.extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            self.flushes += 1;
            Ok(())
        }
    }

    struct FailingWriter;

    impl Write for FailingWriter {
        fn write(&mut self, _bytes: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("synthetic terminal failure"))
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Err(std::io::Error::other("synthetic terminal flush failure"))
        }
    }
}
