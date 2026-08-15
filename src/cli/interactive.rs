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
//! that all lives in [`crate::app::update`].

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
    let args = connection.to_args()?;
    let resolved = crate::connection::resolve(
        target,
        &args,
        &EnvSnapshot::from_process(),
        &loaded.config.connection,
    )?;

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
    let result = runtime.block_on(event_loop(resolved, loaded.config, presentation, history));

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
    target: ConnectionTarget,
    config: Config,
    presentation: &Presentation,
    mut history: crate::history::History,
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

    let keymap = Keymap::new();
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
    model.connection = crate::app::ConnectionState::Connecting;
    model.editor.set_text(starter_query());
    model.history_disabled = !config.history.enabled;
    model.history_paused = history.is_paused();

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
    // drawn and responsive while it happens.
    let session: Arc<tokio::sync::RwLock<Option<Arc<Session>>>> =
        Arc::new(tokio::sync::RwLock::new(None));
    // The tree gets a connection of its own so a long statement cannot delay it.
    // It is opened from the same resolved target, so it reaches the same server
    // by the same route with the same credentials and the same protection; the
    // only differences are deliberate and visible on the server.
    let metadata: Arc<tokio::sync::RwLock<Option<Arc<Session>>>> =
        Arc::new(tokio::sync::RwLock::new(None));
    spawn_connect(
        tx.clone(),
        Arc::clone(&session),
        Arc::clone(&metadata),
        target,
        config.clone(),
    );

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
                    // Reconnection arrives with Feature 002; the initial connect
                    // is started above.
                }
                Effect::Execute { job, sql } => {
                    spawn_execute(tx.clone(), Arc::clone(&session), job, sql, model.row_cap);
                }
                Effect::Cancel { job } => {
                    spawn_cancel(tx.clone(), Arc::clone(&session), job);
                }
                Effect::LoadSchemas => {
                    spawn_load_schemas(tx.clone(), reader(&session, &metadata));
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
                    );
                }
                Effect::LoadDefinition { request, object } => {
                    spawn_load_definition(
                        tx.clone(),
                        reader(&session, &metadata),
                        request,
                        *object,
                    );
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
    target: ConnectionTarget,
    config: Config,
) {
    tokio::spawn(async move {
        let timeout = Duration::from_millis(config.query.statement_timeout_ms);
        match session::connect(&target, timeout).await {
            Ok(opened) => {
                let info = opened.info().clone();
                *slot.write().await = Some(Arc::new(opened));
                let _ = tx.send(Message::Connected(Box::new(info)));
                // The tree is populated as soon as there is something to read it
                // from, so the sidebar is useful the moment it appears.
                let _ = tx.send(Message::Action(crate::app::Action::ReloadObjects));
                spawn_metadata_connect(tx, metadata, target, timeout);
            }
            Err(diagnostic) => {
                let _ = tx.send(Message::ConnectionFailed(Box::new(diagnostic)));
            }
        }
    });
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
    target: ConnectionTarget,
    timeout: Duration,
) {
    let _ = tx.send(Message::MetadataConnection(
        crate::app::model::MetadataLink::Opening,
    ));
    tokio::spawn(async move {
        let mut target = target;
        target.application_name = format!("{} (objects)", target.application_name);
        target.read_only = true;
        let link = match session::connect(&target, timeout).await {
            Ok(opened) => {
                *slot.write().await = Some(Arc::new(opened));
                crate::app::model::MetadataLink::Dedicated
            }
            Err(diagnostic) => crate::app::model::MetadataLink::Unavailable(diagnostic.headline),
        };
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
) {
    tokio::spawn(async move {
        let Some(session) = slot.read().await.clone() else {
            return;
        };
        let result = session.schemas().await;
        let _ = tx.send(Message::SchemasLoaded(Box::new(result)));
    });
}

fn spawn_load_metadata(
    tx: mpsc::UnboundedSender<Message>,
    slot: Arc<tokio::sync::RwLock<Option<Arc<Session>>>>,
    request: crate::app::tree::RequestId,
    path: crate::app::tree::NodePath,
    query: crate::app::tree::MetadataQuery,
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
        let _ = tx.send(Message::MetadataLoaded {
            request,
            path,
            payload: Box::new(payload),
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
) {
    tokio::spawn(async move {
        let Some(session) = slot.read().await.clone() else {
            return;
        };
        let result = session.definition(&object).await;
        let _ = tx.send(Message::DefinitionLoaded {
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
    let timeout = Duration::from_millis(config.query.statement_timeout_ms);
    let session = session::connect(&target, timeout).await?;
    Ok(execute_cancellable(&session, &sql, row_cap, JobId(1)).await)
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
    let cancel = session.cancel_handle();
    let execution = session.execute(sql, row_cap, job);
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

        let stream = session.stream(&sql, &mut sink);
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
}
