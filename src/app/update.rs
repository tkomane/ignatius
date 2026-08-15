//! The reducer.
//!
//! One pure function owns every state transition. It performs no I/O, takes no
//! locks, and reads no clock, so every rule below is testable in microseconds and
//! stays true regardless of timing:
//!
//! - A result whose job identity is not the one in flight is discarded. A slow
//!   query that finishes after a newer one cannot overwrite the newer result.
//! - Cancelling moves to `CancellationRequested`, never to "cancelled". Only the
//!   server's answer produces the cancelled state.
//! - Statements are never sent on a connection that is not usable.
//! - Nothing is ever re-run automatically.

use crate::app::message::{Action, Direction, Effect, Message};
use crate::app::model::{ConnectionState, Focus, Model, QueryPhase};
use crate::query::result::ExecutionStatus;
use crate::query::statements;

/// Applies a message to the model and returns the effects to perform.
pub fn update(model: &mut Model, message: Message) -> Vec<Effect> {
    match message {
        Message::Action(action) => apply_action(model, action),
        Message::Resized(columns, rows) => {
            model.size = (columns, rows);
            Vec::new()
        }
        Message::Connected(info) => {
            model.connection = ConnectionState::Connected(info);
            model.error = None;
            Vec::new()
        }
        Message::ConnectionFailed(diagnostic) => {
            model.error = Some((*diagnostic).clone());
            model.connection = ConnectionState::Failed(diagnostic);
            model.phase = QueryPhase::Idle;
            Vec::new()
        }
        Message::ConnectionLost => {
            if let Some(info) = model.connection.info() {
                model.connection = ConnectionState::Lost {
                    info: Box::new(info.clone()),
                };
            }
            model.phase = QueryPhase::Idle;
            Vec::new()
        }
        Message::ExecutionFinished(execution) => {
            // A result only counts if it belongs to the job still in flight.
            if model.phase.job() != Some(execution.job) {
                return Vec::new();
            }
            model.phase = QueryPhase::Idle;
            model.running_for = None;
            model.transaction = execution.transaction;
            model.last_elapsed = Some(execution.elapsed);
            model.error = execution.error.clone();
            model.error_expanded = false;
            model.notices = execution
                .statements
                .iter()
                .flat_map(|s| s.notices.clone())
                .collect();
            if execution.status == ExecutionStatus::ConnectionLost
                && let Some(info) = model.connection.info()
            {
                model.connection = ConnectionState::Lost {
                    info: Box::new(info.clone()),
                };
            }
            model.selected_row = 0;
            model.selected_column = 0;
            model.last_execution = Some(*execution);
            Vec::new()
        }
        Message::CancellationDelivered(_) => Vec::new(),
        Message::CancellationFailed(diagnostic) => {
            model.error = Some(*diagnostic);
            Vec::new()
        }
        Message::Notices(notices) => {
            model.notices.extend(notices);
            Vec::new()
        }
        Message::SchemasLoaded(result) => {
            match *result {
                Ok(schemas) => model.tree.set_schemas(schemas),
                Err(error) => model.tree.set_error(error),
            }
            Vec::new()
        }
        Message::MetadataLoaded {
            request,
            path,
            payload,
        } => {
            match *payload {
                Ok(loaded) => {
                    model.tree.apply(request, &path, loaded);
                }
                Err(error) => {
                    // The reason belongs in the tree, next to the node that
                    // could not be opened, rather than in a modal somewhere else.
                    model.tree.apply_failure(request, &path, &error.headline);
                }
            }
            Vec::new()
        }
        Message::Tick { running_for } => {
            model.frame = model.frame.wrapping_add(1);
            model.running_for = running_for;
            Vec::new()
        }
    }
}

fn apply_action(model: &mut Model, action: Action) -> Vec<Effect> {
    // Modes are peeled in a fixed order, highest first. Getting this wrong is
    // how typing in the editor starts doing surprising things, so the order is
    // stated once here and tested directly.
    if model.prefix_pending {
        return resolve_prefix(model, action);
    }
    if model.pending_run.is_some() {
        return confirmation_action(model, action);
    }
    if model.palette.is_some() {
        return palette_action(model, action);
    }
    if model.tree.filtering {
        return filter_action(model, action);
    }
    if model.focus == Focus::Objects
        && !matches!(action, Action::Quit | Action::ToggleHelp)
        && let Some(effects) = objects_action(model, &action)
    {
        return effects;
    }

    match action {
        Action::Quit => {
            model.should_quit = true;
            vec![Effect::Quit]
        }
        Action::ToggleHelp => {
            model.help_open = !model.help_open;
            Vec::new()
        }
        Action::Dismiss => {
            // One layer at a time, in the order they were opened. Dismissing an
            // error the user has not read is the more expensive mistake, so it
            // goes last.
            if model.help_open {
                model.help_open = false;
            } else if !model.tree.filter.is_empty() {
                model.tree.filter.clear();
            } else {
                model.error = None;
                model.error_expanded = false;
            }
            Vec::new()
        }
        Action::ToggleErrorDetail => {
            model.error_expanded = !model.error_expanded;
            Vec::new()
        }
        Action::FocusNext => {
            model.focus = model.focus.next(model.sidebar_visible);
            Vec::new()
        }
        Action::ToggleSidebar => {
            model.sidebar_visible = !model.sidebar_visible;
            if !model.sidebar_visible && model.focus == Focus::Objects {
                model.focus = Focus::Editor;
            }
            // The tree loads the first time it is shown, not at startup, so a
            // user who never opens it never pays for it.
            if model.sidebar_visible && model.tree.roots.is_empty() && !model.tree.loading {
                model.tree.begin_loading();
                return vec![Effect::LoadSchemas];
            }
            Vec::new()
        }
        Action::OpenPalette => {
            model.palette = Some(crate::app::palette::Palette::new(palette_entries(model)));
            Vec::new()
        }
        Action::BeginPrefix => {
            model.prefix_pending = true;
            Vec::new()
        }
        Action::StartFilter => {
            model.focus = Focus::Objects;
            model.sidebar_visible = true;
            model.tree.filtering = true;
            Vec::new()
        }
        Action::ReloadObjects => {
            model.tree.begin_loading();
            vec![Effect::LoadSchemas]
        }
        Action::Activate if model.focus == Focus::Editor => {
            model.editor.insert('\n');
            Vec::new()
        }
        Action::Activate => Vec::new(),
        Action::RunBuffer => run(model, model.editor.text().to_owned()),
        Action::RunStatement => {
            let sql = statements::statement_at(model.editor.text(), model.editor.cursor())
                .map(|s| s.text)
                .unwrap_or_default();
            run(model, sql)
        }
        Action::Cancel => match model.phase {
            QueryPhase::Running { job, .. } => {
                model.phase = QueryPhase::CancellationRequested { job };
                vec![Effect::Cancel { job }]
            }
            // Cancelling twice must not send a second request or change wording.
            QueryPhase::CancellationRequested { .. } | QueryPhase::Idle => Vec::new(),
        },
        Action::Insert(ch) if model.focus == Focus::Editor => {
            model.editor.insert(ch);
            Vec::new()
        }
        Action::Backspace if model.focus == Focus::Editor => {
            model.editor.backspace();
            Vec::new()
        }
        Action::Newline if model.focus == Focus::Editor => {
            model.editor.insert('\n');
            Vec::new()
        }
        Action::Insert(_) | Action::Backspace | Action::Newline => Vec::new(),
        Action::Move(direction) => {
            move_selection(model, direction);
            Vec::new()
        }
    }
}

/// Handles input while a run is waiting to be confirmed.
fn confirmation_action(model: &mut Model, action: Action) -> Vec<Effect> {
    let Some(pending) = model.pending_run.as_mut() else {
        return Vec::new();
    };
    match action {
        Action::Insert(ch) => {
            pending.typed.push(ch);
            Vec::new()
        }
        Action::Backspace => {
            pending.typed.pop();
            Vec::new()
        }
        Action::Activate | Action::RunBuffer => {
            if !pending.is_satisfied() {
                // Nothing happens until the word is typed. Saying so is the
                // prompt's job; silently refusing would be worse than a modal.
                return Vec::new();
            }
            let sql = pending.sql.clone();
            model.pending_run = None;
            start(model, sql)
        }
        Action::Dismiss | Action::Cancel => {
            model.pending_run = None;
            Vec::new()
        }
        Action::Quit => {
            model.should_quit = true;
            vec![Effect::Quit]
        }
        _ => Vec::new(),
    }
}

/// Starts an execution, refusing when there is nothing to run or nowhere to run
/// it, and holding it back when the target is production and it is not a read.
fn run(model: &mut Model, sql: String) -> Vec<Effect> {
    if !model.connection.is_usable() || model.phase.is_busy() || sql.trim().is_empty() {
        return Vec::new();
    }
    let parsed = statements::split(&sql);
    if parsed.is_empty() {
        return Vec::new();
    }

    // The classification is advisory and the interface says so. What it buys is
    // that nobody writes to production without a deliberate second action.
    let impact = crate::query::classify_all(&parsed);
    if model.environment().is_production() && impact.needs_confirmation() {
        let required = model
            .connection
            .info()
            .map_or_else(String::new, |info| info.database.clone());
        model.pending_run = Some(crate::app::model::PendingRun {
            sql,
            impact,
            typed: String::new(),
            required,
        });
        return Vec::new();
    }

    start(model, sql)
}

/// Begins an execution that has already been allowed.
fn start(model: &mut Model, sql: String) -> Vec<Effect> {
    let parsed = statements::split(&sql);
    if parsed.is_empty() {
        return Vec::new();
    }
    let job = model.allocate_job();
    model.phase = QueryPhase::Running {
        job,
        statements: parsed.len(),
    };
    model.running_for = Some(std::time::Duration::ZERO);
    model.error = None;
    vec![Effect::Execute { job, sql }]
}

fn move_selection(model: &mut Model, direction: Direction) {
    match model.focus {
        Focus::Objects => match direction {
            Direction::Up => model.tree.move_selection(-1),
            Direction::Down => model.tree.move_selection(1),
            Direction::Left => model.tree.collapse_selected(),
            Direction::Right => {}
        },
        Focus::Editor => match direction {
            Direction::Left => model.editor.move_left(),
            Direction::Right => model.editor.move_right(),
            // Vertical movement in the editor arrives with Feature 003; until
            // then it does nothing rather than doing something surprising.
            Direction::Up | Direction::Down => {}
        },
        Focus::Results => {
            let (rows, columns) = model
                .visible_result()
                .map_or((0, 0), |set| (set.rows.len(), set.columns.len()));
            match direction {
                Direction::Up => model.selected_row = model.selected_row.saturating_sub(1),
                Direction::Down => {
                    model.selected_row = (model.selected_row + 1).min(rows.saturating_sub(1));
                }
                Direction::Left => {
                    model.selected_column = model.selected_column.saturating_sub(1);
                }
                Direction::Right => {
                    model.selected_column =
                        (model.selected_column + 1).min(columns.saturating_sub(1));
                }
            }
        }
    }
}

/// Resolves the second key of a chord.
///
/// Whatever it is, the chord ends here: an unrecognised key cancels rather than
/// leaving the interface waiting in a state the user cannot see out of.
fn resolve_prefix(model: &mut Model, action: Action) -> Vec<Effect> {
    model.prefix_pending = false;
    match action {
        Action::Insert(ch) => match crate::ui::keymap::chord_action(ch) {
            Some(chord) => apply_action(model, chord),
            None => Vec::new(),
        },
        _ => Vec::new(),
    }
}

/// Handles input while the palette is open.
fn palette_action(model: &mut Model, action: Action) -> Vec<Effect> {
    let Some(palette) = model.palette.as_mut() else {
        return Vec::new();
    };
    match action {
        Action::Insert(ch) => {
            palette.push(ch);
            Vec::new()
        }
        Action::Backspace => {
            palette.backspace();
            Vec::new()
        }
        Action::Move(Direction::Up) => {
            palette.move_selection(-1);
            Vec::new()
        }
        Action::Move(Direction::Down) => {
            palette.move_selection(1);
            Vec::new()
        }
        Action::Activate => {
            let chosen = palette.selected_entry();
            model.palette = None;
            match chosen.map(|entry| entry.command) {
                Some(crate::app::palette::PaletteCommand::Run(next)) => apply_action(model, next),
                Some(crate::app::palette::PaletteCommand::Insert(text)) => {
                    model.focus = Focus::Editor;
                    for ch in text.chars() {
                        model.editor.insert(ch);
                    }
                    Vec::new()
                }
                None => Vec::new(),
            }
        }
        Action::Dismiss => {
            model.palette = None;
            Vec::new()
        }
        Action::Quit => {
            model.palette = None;
            model.should_quit = true;
            vec![Effect::Quit]
        }
        _ => Vec::new(),
    }
}

/// Handles input while the object filter is being typed.
fn filter_action(model: &mut Model, action: Action) -> Vec<Effect> {
    match action {
        Action::Insert(ch) => {
            model.tree.filter.push(ch);
            model.tree.selected = 0;
            Vec::new()
        }
        Action::Backspace => {
            model.tree.filter.pop();
            model.tree.selected = 0;
            Vec::new()
        }
        Action::Move(Direction::Up) => {
            model.tree.move_selection(-1);
            Vec::new()
        }
        Action::Move(Direction::Down) => {
            model.tree.move_selection(1);
            Vec::new()
        }
        // Enter keeps the filter and returns to navigating; Esc clears it.
        Action::Activate => {
            model.tree.filtering = false;
            Vec::new()
        }
        Action::Dismiss => {
            model.tree.filtering = false;
            model.tree.filter.clear();
            Vec::new()
        }
        Action::Quit => {
            model.should_quit = true;
            vec![Effect::Quit]
        }
        _ => Vec::new(),
    }
}

/// Handles the actions that mean something different in the object tree.
///
/// Returns `None` for anything the tree does not claim, which then falls through
/// to the ordinary handling.
fn objects_action(model: &mut Model, action: &Action) -> Option<Vec<Effect>> {
    match action {
        // The arrows navigate: right opens, left closes.
        Action::Move(Direction::Right) => Some(expand_selected(model)),
        // Enter uses the thing under the cursor. For something with a name that
        // means putting the name where SQL is written; for a container it means
        // opening it, because a container has no name worth pasting.
        Action::Activate => {
            let row = model.tree.selected_row()?;
            let node = model.tree.node(&row.path)?;
            let insertable = match &node.kind {
                crate::app::tree::NodeKind::Object(object) => Some(object.qualified_sql()),
                crate::app::tree::NodeKind::Column(column) => {
                    Some(crate::postgres::metadata::quote_identifier(&column.name))
                }
                _ => None,
            };
            match insertable {
                Some(name) => {
                    model.focus = Focus::Editor;
                    for ch in name.chars() {
                        model.editor.insert(ch);
                    }
                    Some(Vec::new())
                }
                None => Some(expand_selected(model)),
            }
        }
        // `/` starts a filter, the way it does in every tree people already use.
        Action::Insert('/') => {
            model.tree.filtering = true;
            Some(Vec::new())
        }
        // Ordinary typing must not leak into the editor from here.
        Action::Insert(_) | Action::Backspace => Some(Vec::new()),
        _ => None,
    }
}

/// Opens the selected node, asking for a load only when one is needed.
fn expand_selected(model: &mut Model) -> Vec<Effect> {
    let Some(row) = model.tree.selected_row() else {
        return Vec::new();
    };
    if !row.expandable {
        return Vec::new();
    }
    if matches!(row.row_kind, crate::app::tree::RowKind::Schema { .. }) {
        if row.expanded {
            model.tree.collapse_selected();
        } else {
            // Counts arrived with the schema list, so this needs no round trip.
            model.tree.expand_schema(&row.path);
        }
        return Vec::new();
    }
    if row.expanded {
        model.tree.collapse_selected();
        return Vec::new();
    }
    match model.tree.expand_selected() {
        Some((request, path, query)) => vec![Effect::LoadMetadata {
            request,
            path,
            query,
        }],
        None => Vec::new(),
    }
}

/// The entries the palette offers: every command, then every loaded object.
fn palette_entries(model: &Model) -> Vec<crate::app::palette::PaletteEntry> {
    use crate::app::palette::{PaletteCommand, PaletteEntry};
    let keymap = crate::ui::keymap::Keymap::new();
    let mut entries: Vec<PaletteEntry> = keymap
        .bindings()
        .iter()
        .filter(|binding| {
            // Movement and typing are not commands anyone looks up in a palette.
            !matches!(
                binding.action,
                Action::Move(_) | Action::Insert(_) | Action::Backspace | Action::Activate
            )
        })
        .map(|binding| PaletteEntry {
            label: binding.description.to_owned(),
            detail: binding.key_label(),
            group: "Command",
            command: PaletteCommand::Run(binding.action.clone()),
        })
        .collect();
    entries.dedup_by(|a, b| a.label == b.label);

    // Objects already in the tree, so the palette costs no round trip.
    for row in model.tree.rows() {
        if let crate::app::tree::RowKind::Object { kind, .. } = row.row_kind
            && let Some(node) = model.tree.node(&row.path)
            && let Some(sql) = node.qualified_sql()
        {
            entries.push(PaletteEntry {
                label: row.label.clone(),
                detail: format!("{} {}", kind.singular(), sql),
                group: "Object",
                command: PaletteCommand::Insert(sql),
            });
        }
    }
    entries
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::Environment;
    use crate::diagnostics::{Diagnostic, DiagnosticKind};
    use crate::postgres::{SessionInfo, TlsState};
    use crate::query::result::{Execution, JobId, ResultSet, StatementResult};
    use crate::query::value::Cell;
    use std::time::Duration;

    fn session() -> Box<SessionInfo> {
        Box::new(SessionInfo {
            target: "app@localhost:5432/orders".into(),
            database: "orders".into(),
            user: "app".into(),
            server_version: "18.6".into(),
            backend_pid: "42".into(),
            search_path: "public".into(),
            read_only: false,
            tls: TlsState::Disabled,
            environment: Environment::Local,
        })
    }

    fn connected() -> Model {
        let mut model = Model::new(100);
        update(&mut model, Message::Connected(session()));
        model
    }

    fn execution(job: JobId, status: ExecutionStatus, rows: &[&str]) -> Box<Execution> {
        let mut set = ResultSet::new(vec!["value".into()], 100);
        for row in rows {
            set.push(vec![Cell::Text((*row).to_owned())]);
        }
        Box::new(Execution {
            job,
            statements: vec![StatementResult {
                result_set: Some(set),
                rows_affected: Some(rows.len() as u64),
                elapsed: Duration::from_millis(5),
                notices: Vec::new(),
            }],
            status,
            elapsed: Duration::from_millis(6),
            error: None,

            transaction: crate::query::result::TransactionState::Autocommit,
        })
    }

    #[test]
    fn running_a_buffer_starts_one_job_and_asks_for_execution() {
        let mut model = connected();
        model.editor.set_text("SELECT 1;");
        let effects = update(&mut model, Message::Action(Action::RunBuffer));

        assert_eq!(effects.len(), 1);
        let Effect::Execute { job, sql } = &effects[0] else {
            panic!("expected an execute effect, got {effects:?}");
        };
        assert_eq!(sql, "SELECT 1;");
        assert_eq!(
            model.phase,
            QueryPhase::Running {
                job: *job,
                statements: 1
            }
        );
        assert!(model.phase.is_busy());
    }

    #[test]
    fn nothing_runs_without_a_usable_connection() {
        let mut model = Model::new(100);
        model.editor.set_text("SELECT 1;");
        assert!(update(&mut model, Message::Action(Action::RunBuffer)).is_empty());
        assert_eq!(model.phase, QueryPhase::Idle);

        // A lost connection is equally unusable.
        let mut model = connected();
        update(&mut model, Message::ConnectionLost);
        model.editor.set_text("SELECT 1;");
        assert!(update(&mut model, Message::Action(Action::RunBuffer)).is_empty());
    }

    #[test]
    fn an_empty_or_comment_only_buffer_sends_nothing() {
        let mut model = connected();
        for text in ["", "   \n  ", "-- just a comment\n", ";"] {
            model.editor.set_text(text);
            assert!(
                update(&mut model, Message::Action(Action::RunBuffer)).is_empty(),
                "{text:?} should not be sent"
            );
        }
    }

    #[test]
    fn a_second_run_is_refused_while_one_is_in_flight() {
        let mut model = connected();
        model.editor.set_text("SELECT 1;");
        let first = update(&mut model, Message::Action(Action::RunBuffer));
        assert_eq!(first.len(), 1);
        let second = update(&mut model, Message::Action(Action::RunBuffer));
        assert!(second.is_empty(), "one statement at a time");
    }

    #[test]
    fn a_stale_result_cannot_overwrite_a_newer_query() {
        let mut model = connected();
        model.editor.set_text("SELECT 1;");
        update(&mut model, Message::Action(Action::RunBuffer));
        let first_job = model.phase.job().expect("running");

        // The first query finishes and a second is started.
        update(
            &mut model,
            Message::ExecutionFinished(execution(
                first_job,
                ExecutionStatus::Succeeded,
                &["first"],
            )),
        );
        model.editor.set_text("SELECT 2;");
        update(&mut model, Message::Action(Action::RunBuffer));
        let second_job = model.phase.job().expect("running");
        assert_ne!(first_job, second_job);

        // A late result from the first query arrives.
        update(
            &mut model,
            Message::ExecutionFinished(execution(
                first_job,
                ExecutionStatus::Succeeded,
                &["stale"],
            )),
        );

        assert_eq!(
            model.phase,
            QueryPhase::Running {
                job: second_job,
                statements: 1
            },
            "the in-flight job must still be running"
        );
        let shown = model.visible_result().expect("result").rows[0][0].clone();
        assert_eq!(
            shown,
            Cell::Text("first".into()),
            "stale rows must not be displayed"
        );
    }

    #[test]
    fn cancelling_requests_but_does_not_claim_cancellation() {
        let mut model = connected();
        model.editor.set_text("SELECT pg_sleep(30);");
        update(&mut model, Message::Action(Action::RunBuffer));
        let job = model.phase.job().expect("running");

        let effects = update(&mut model, Message::Action(Action::Cancel));
        assert_eq!(effects, vec![Effect::Cancel { job }]);
        assert_eq!(model.phase, QueryPhase::CancellationRequested { job });
        assert_eq!(model.phase.label(), "Cancellation requested");

        // Only the server's answer produces the cancelled wording.
        update(
            &mut model,
            Message::ExecutionFinished(Box::new(Execution {
                job,
                statements: Vec::new(),
                status: ExecutionStatus::Cancelled,
                elapsed: Duration::from_millis(20),
                error: None,

                transaction: crate::query::result::TransactionState::Autocommit,
            })),
        );
        assert_eq!(model.phase, QueryPhase::Idle);
        assert_eq!(
            model
                .last_execution
                .as_ref()
                .expect("execution")
                .status
                .label(),
            "Query cancelled by server"
        );
    }

    #[test]
    fn cancelling_twice_sends_one_request() {
        let mut model = connected();
        model.editor.set_text("SELECT pg_sleep(30);");
        update(&mut model, Message::Action(Action::RunBuffer));
        assert_eq!(update(&mut model, Message::Action(Action::Cancel)).len(), 1);
        assert!(update(&mut model, Message::Action(Action::Cancel)).is_empty());
    }

    #[test]
    fn cancelling_when_idle_does_nothing() {
        let mut model = connected();
        assert!(update(&mut model, Message::Action(Action::Cancel)).is_empty());
        assert_eq!(model.phase, QueryPhase::Idle);
    }

    #[test]
    fn a_lost_connection_during_execution_is_recorded_on_the_connection_too() {
        let mut model = connected();
        model.editor.set_text("SELECT 1;");
        update(&mut model, Message::Action(Action::RunBuffer));
        let job = model.phase.job().expect("running");
        update(
            &mut model,
            Message::ExecutionFinished(Box::new(Execution {
                job,
                statements: Vec::new(),
                status: ExecutionStatus::ConnectionLost,
                elapsed: Duration::from_millis(1),
                error: None,

                transaction: crate::query::result::TransactionState::Autocommit,
            })),
        );
        assert!(matches!(model.connection, ConnectionState::Lost { .. }));
        assert!(!model.connection.is_usable());
    }

    #[test]
    fn running_the_statement_at_the_cursor_sends_only_that_statement() {
        let mut model = connected();
        model.editor.set_text("SELECT 1;\nSELECT 2;\nSELECT 3;");
        // Cursor sits at the end, inside the third statement.
        let effects = update(&mut model, Message::Action(Action::RunStatement));
        let Effect::Execute { sql, .. } = &effects[0] else {
            panic!("expected execute");
        };
        assert_eq!(sql, "SELECT 3");
    }

    #[test]
    fn a_failed_execution_shows_its_diagnostic_collapsed() {
        let mut model = connected();
        model.editor.set_text("SELECT * FROM nope;");
        update(&mut model, Message::Action(Action::RunBuffer));
        let job = model.phase.job().expect("running");
        let diagnostic = Diagnostic::new(
            DiagnosticKind::Query,
            "relation does not exist",
            "running statement 1",
        )
        .technical("SQLSTATE", "42P01");
        update(
            &mut model,
            Message::ExecutionFinished(Box::new(Execution {
                job,
                statements: Vec::new(),
                status: ExecutionStatus::Failed,
                elapsed: Duration::from_millis(3),
                error: Some(diagnostic),

                transaction: crate::query::result::TransactionState::Autocommit,
            })),
        );

        assert!(model.error.is_some());
        assert!(!model.error_expanded, "detail starts collapsed");
        update(&mut model, Message::Action(Action::ToggleErrorDetail));
        assert!(model.error_expanded);
        update(&mut model, Message::Action(Action::Dismiss));
        assert!(model.error.is_none());
    }

    #[test]
    fn help_opens_and_dismiss_closes_help_before_errors() {
        let mut model = connected();
        model.error = Some(Diagnostic::new(DiagnosticKind::Query, "boom", "running"));
        update(&mut model, Message::Action(Action::ToggleHelp));
        assert!(model.help_open);

        update(&mut model, Message::Action(Action::Dismiss));
        assert!(!model.help_open);
        assert!(model.error.is_some(), "the error is still there to read");

        update(&mut model, Message::Action(Action::Dismiss));
        assert!(model.error.is_none());
    }

    #[test]
    fn result_selection_stays_inside_the_grid() {
        let mut model = connected();
        model.focus = Focus::Results;
        model.last_execution = Some(*execution(
            JobId(1),
            ExecutionStatus::Succeeded,
            &["a", "b"],
        ));

        for _ in 0..10 {
            update(&mut model, Message::Action(Action::Move(Direction::Down)));
        }
        assert_eq!(model.selected_row, 1, "cannot move past the last row");
        for _ in 0..10 {
            update(&mut model, Message::Action(Action::Move(Direction::Up)));
        }
        assert_eq!(model.selected_row, 0);
        for _ in 0..10 {
            update(&mut model, Message::Action(Action::Move(Direction::Right)));
        }
        assert_eq!(model.selected_column, 0, "single column grid");
    }

    #[test]
    fn typing_only_edits_when_the_editor_has_focus() {
        let mut model = connected();
        model.focus = Focus::Results;
        update(&mut model, Message::Action(Action::Insert('x')));
        assert_eq!(
            model.editor.text(),
            "",
            "results pane must not swallow text into the editor"
        );

        model.focus = Focus::Editor;
        update(&mut model, Message::Action(Action::Insert('x')));
        assert_eq!(model.editor.text(), "x");
    }

    #[test]
    fn a_tick_advances_the_frame_and_records_elapsed_time_without_a_clock() {
        let mut model = connected();
        assert!(!model.is_animating(), "an idle client animates nothing");

        model.editor.set_text("SELECT pg_sleep(5);");
        update(&mut model, Message::Action(Action::RunBuffer));
        assert!(
            model.is_animating(),
            "a running statement is worth animating"
        );

        let before = model.frame;
        update(
            &mut model,
            Message::Tick {
                running_for: Some(Duration::from_millis(1500)),
            },
        );
        assert_eq!(model.frame, before + 1);
        assert_eq!(model.running_for, Some(Duration::from_millis(1500)));
    }

    #[test]
    fn the_live_timer_is_cleared_when_the_statement_ends() {
        let mut model = connected();
        model.editor.set_text("SELECT 1;");
        update(&mut model, Message::Action(Action::RunBuffer));
        let job = model.phase.job().expect("running");
        update(
            &mut model,
            Message::Tick {
                running_for: Some(Duration::from_secs(2)),
            },
        );
        assert!(model.running_for.is_some());

        update(
            &mut model,
            Message::ExecutionFinished(execution(job, ExecutionStatus::Succeeded, &["x"])),
        );
        assert!(
            model.running_for.is_none(),
            "a stale timer would keep counting"
        );
        assert!(!model.is_animating());
    }

    #[test]
    fn ticks_change_nothing_a_user_could_mistake_for_progress() {
        // A frame counter must never move the query, the selection, or the state.
        let mut model = connected();
        model.editor.set_text("SELECT 1;");
        let before = model.clone();
        for _ in 0..50 {
            update(&mut model, Message::Tick { running_for: None });
        }
        assert_eq!(model.phase, before.phase);
        assert_eq!(model.editor.text(), before.editor.text());
        assert_eq!(model.selected_row, before.selected_row);
        assert!(model.last_execution.is_none());
    }

    // ------------------------------------------------- navigation and modes

    fn schema_summary(name: &str, tables: i64) -> crate::postgres::SchemaSummary {
        let mut counts = std::collections::BTreeMap::new();
        counts.insert(crate::postgres::ObjectKind::Table, tables);
        crate::postgres::SchemaSummary {
            name: name.to_owned(),
            usable: true,
            counts,
        }
    }

    fn with_tree() -> Model {
        let mut model = connected();
        update(
            &mut model,
            Message::SchemasLoaded(Box::new(Ok(vec![
                schema_summary("public", 2),
                schema_summary("reporting", 1),
            ]))),
        );
        model
    }

    #[test]
    fn the_tree_loads_the_first_time_the_sidebar_is_shown_and_not_again() {
        let mut model = connected();
        model.sidebar_visible = false;

        let effects = update(&mut model, Message::Action(Action::ToggleSidebar));
        assert_eq!(effects, vec![Effect::LoadSchemas], "showing it loads it");
        assert!(model.sidebar_visible);

        update(
            &mut model,
            Message::SchemasLoaded(Box::new(Ok(vec![schema_summary("public", 1)]))),
        );
        update(&mut model, Message::Action(Action::ToggleSidebar));
        let effects = update(&mut model, Message::Action(Action::ToggleSidebar));
        assert!(
            effects.is_empty(),
            "an already-loaded tree is not fetched again"
        );
    }

    #[test]
    fn hiding_the_sidebar_moves_focus_off_it() {
        let mut model = with_tree();
        model.focus = Focus::Objects;
        update(&mut model, Message::Action(Action::ToggleSidebar));
        assert!(!model.sidebar_visible);
        assert_eq!(
            model.focus,
            Focus::Editor,
            "focus cannot stay on a hidden pane"
        );
    }

    #[test]
    fn opening_a_schema_costs_no_round_trip_but_opening_a_group_does() {
        let mut model = with_tree();
        model.focus = Focus::Objects;

        let effects = update(&mut model, Message::Action(Action::Move(Direction::Right)));
        assert!(
            effects.is_empty(),
            "counts already arrived with the schema list"
        );
        assert_eq!(model.tree.rows()[1].label, "tables (2)");

        update(&mut model, Message::Action(Action::Move(Direction::Down)));
        let effects = update(&mut model, Message::Action(Action::Move(Direction::Right)));
        assert!(
            matches!(effects.as_slice(), [Effect::LoadMetadata { .. }]),
            "opening a group asks the server: {effects:?}"
        );
    }

    #[test]
    fn choosing_an_object_puts_its_quoted_name_in_the_editor() {
        let mut model = with_tree();
        model.focus = Focus::Objects;
        model.editor.set_text("SELECT * FROM ");
        update(&mut model, Message::Action(Action::Move(Direction::Right))); // open public
        update(&mut model, Message::Action(Action::Move(Direction::Down)));
        let effects = update(&mut model, Message::Action(Action::Move(Direction::Right))); // tables
        let Effect::LoadMetadata { request, path, .. } = &effects[0] else {
            panic!("expected a load");
        };
        update(
            &mut model,
            Message::MetadataLoaded {
                request: *request,
                path: path.clone(),
                payload: Box::new(Ok(crate::app::tree::MetadataPayload::Objects(vec![
                    crate::postgres::ObjectSummary {
                        kind: crate::postgres::ObjectKind::Table,
                        schema: "public".into(),
                        name: "we\"ird".into(),
                        readable: true,
                        detail: None,
                    },
                ]))),
            },
        );

        update(&mut model, Message::Action(Action::Move(Direction::Down)));
        // Enter uses the object: its quoted name goes where SQL is written.
        update(&mut model, Message::Action(Action::Activate));

        assert_eq!(
            model.editor.text(),
            "SELECT * FROM \"public\".\"we\"\"ird\"",
            "the name is quoted so a hostile one cannot change the statement"
        );
        assert_eq!(model.focus, Focus::Editor, "focus follows the insertion");
    }

    #[test]
    fn typing_in_the_object_tree_never_leaks_into_the_editor() {
        let mut model = with_tree();
        model.focus = Focus::Objects;
        model.editor.set_text("SELECT 1");

        for ch in "drop".chars() {
            update(&mut model, Message::Action(Action::Insert(ch)));
        }
        update(&mut model, Message::Action(Action::Backspace));
        assert_eq!(
            model.editor.text(),
            "SELECT 1",
            "the editor was not touched"
        );
    }

    #[test]
    fn slash_starts_a_filter_and_escape_clears_it() {
        let mut model = with_tree();
        model.focus = Focus::Objects;

        update(&mut model, Message::Action(Action::Insert('/')));
        assert!(model.tree.filtering);

        for ch in "rep".chars() {
            update(&mut model, Message::Action(Action::Insert(ch)));
        }
        assert_eq!(model.tree.filter, "rep");
        let labels: Vec<String> = model.tree.rows().iter().map(|r| r.label.clone()).collect();
        assert_eq!(labels, vec!["reporting".to_owned()]);

        update(&mut model, Message::Action(Action::Activate));
        assert!(
            !model.tree.filtering,
            "Enter keeps the filter and stops typing"
        );
        assert_eq!(model.tree.filter, "rep");

        update(&mut model, Message::Action(Action::Dismiss));
        assert!(model.tree.filter.is_empty(), "Esc clears it");
    }

    #[test]
    fn the_palette_intercepts_typing_and_leaves_the_editor_alone() {
        let mut model = with_tree();
        model.editor.set_text("SELECT 1");

        update(&mut model, Message::Action(Action::OpenPalette));
        assert!(model.palette.is_some());

        for ch in "quit".chars() {
            update(&mut model, Message::Action(Action::Insert(ch)));
        }
        assert_eq!(
            model.editor.text(),
            "SELECT 1",
            "the editor was not typed into"
        );
        assert_eq!(model.palette.as_ref().expect("open").query, "quit");

        update(&mut model, Message::Action(Action::Dismiss));
        assert!(model.palette.is_none());
        assert_eq!(model.editor.text(), "SELECT 1");
    }

    #[test]
    fn the_palette_runs_the_command_it_shows() {
        let mut model = with_tree();
        update(&mut model, Message::Action(Action::OpenPalette));
        for ch in "help".chars() {
            update(&mut model, Message::Action(Action::Insert(ch)));
        }
        update(&mut model, Message::Action(Action::Activate));

        assert!(model.palette.is_none(), "choosing closes it");
        assert!(model.help_open, "the chosen command actually ran");
    }

    #[test]
    fn a_chord_shows_its_continuations_and_the_next_key_ends_it() {
        let mut model = with_tree();
        model.sidebar_visible = true;

        update(&mut model, Message::Action(Action::BeginPrefix));
        assert!(model.prefix_pending, "the popup is open");

        update(&mut model, Message::Action(Action::Insert('b')));
        assert!(!model.prefix_pending, "the chord ended");
        assert!(!model.sidebar_visible, "and it did what it said");

        // An unrecognised second key cancels rather than leaving it hanging.
        update(&mut model, Message::Action(Action::BeginPrefix));
        update(&mut model, Message::Action(Action::Insert('z')));
        assert!(!model.prefix_pending);
    }

    #[test]
    fn a_pending_chord_swallows_the_next_key_instead_of_typing_it() {
        let mut model = with_tree();
        model.focus = Focus::Editor;
        model.editor.set_text("SELECT ");
        update(&mut model, Message::Action(Action::BeginPrefix));
        update(&mut model, Message::Action(Action::Insert('z')));
        assert_eq!(
            model.editor.text(),
            "SELECT ",
            "the chord key did not become text"
        );
    }

    #[test]
    fn escape_peels_one_layer_at_a_time_in_a_fixed_order() {
        let mut model = with_tree();
        model.error = Some(Diagnostic::new(DiagnosticKind::Query, "boom", "running"));
        model.tree.filter = "orders".into();
        update(&mut model, Message::Action(Action::ToggleHelp));
        update(&mut model, Message::Action(Action::OpenPalette));

        update(&mut model, Message::Action(Action::Dismiss));
        assert!(model.palette.is_none(), "the palette goes first");
        assert!(model.help_open);

        update(&mut model, Message::Action(Action::Dismiss));
        assert!(!model.help_open, "then help");
        assert_eq!(model.tree.filter, "orders");

        update(&mut model, Message::Action(Action::Dismiss));
        assert!(model.tree.filter.is_empty(), "then the filter");
        assert!(model.error.is_some(), "the unread error is still there");

        update(&mut model, Message::Action(Action::Dismiss));
        assert!(model.error.is_none(), "and only then the error");
    }

    #[test]
    fn a_pending_chord_is_dismissed_before_anything_else() {
        let mut model = with_tree();
        update(&mut model, Message::Action(Action::ToggleHelp));
        update(&mut model, Message::Action(Action::BeginPrefix));

        update(&mut model, Message::Action(Action::Dismiss));
        assert!(!model.prefix_pending, "the chord goes first");
        assert!(model.help_open, "and nothing beneath it was touched");
    }

    #[test]
    fn an_open_palette_swallows_a_chord_rather_than_stacking_two_popups() {
        let mut model = with_tree();
        update(&mut model, Message::Action(Action::OpenPalette));
        update(&mut model, Message::Action(Action::BeginPrefix));
        assert!(
            !model.prefix_pending,
            "two overlays at once would leave the user unsure which key applies"
        );
        assert!(model.palette.is_some());
    }

    #[test]
    fn enter_means_something_different_in_each_pane() {
        let mut model = with_tree();

        model.focus = Focus::Editor;
        model.editor.set_text("SELECT 1");
        update(&mut model, Message::Action(Action::Activate));
        assert_eq!(
            model.editor.text(),
            "SELECT 1\n",
            "a line break in the editor"
        );

        model.focus = Focus::Objects;
        update(&mut model, Message::Action(Action::Activate));
        assert!(model.tree.rows()[0].expanded, "opens a node in the tree");
    }

    #[test]
    fn a_failed_metadata_load_explains_itself_inside_the_tree() {
        let mut model = with_tree();
        model.focus = Focus::Objects;
        update(&mut model, Message::Action(Action::Move(Direction::Right)));
        update(&mut model, Message::Action(Action::Move(Direction::Down)));
        let effects = update(&mut model, Message::Action(Action::Move(Direction::Right)));
        let Effect::LoadMetadata { request, path, .. } = &effects[0] else {
            panic!("expected a load");
        };

        update(
            &mut model,
            Message::MetadataLoaded {
                request: *request,
                path: path.clone(),
                payload: Box::new(Err(Diagnostic::new(
                    DiagnosticKind::Query,
                    "permission denied for schema public",
                    "listing tables",
                ))),
            },
        );

        let labels: Vec<String> = model.tree.rows().iter().map(|r| r.label.clone()).collect();
        assert!(
            labels.contains(&"permission denied for schema public".to_owned()),
            "the reason appears where the user was looking: {labels:?}"
        );
        assert!(
            model.error.is_none(),
            "a tree failure is not a query failure"
        );
    }

    #[test]
    fn a_failure_to_load_the_tree_at_all_is_shown_in_the_tree() {
        let mut model = connected();
        update(
            &mut model,
            Message::SchemasLoaded(Box::new(Err(Diagnostic::new(
                DiagnosticKind::Connection,
                "the connection closed",
                "reading the catalogue",
            )))),
        );
        assert!(model.tree.error.is_some());
        assert!(!model.tree.loading);
    }

    #[test]
    fn the_transaction_state_comes_from_the_execution_that_reported_it() {
        let mut model = connected();
        model.editor.set_text("BEGIN;");
        update(&mut model, Message::Action(Action::RunBuffer));
        let job = model.phase.job().expect("running");

        let mut execution = execution(job, ExecutionStatus::Succeeded, &["x"]);
        execution.transaction = crate::query::result::TransactionState::Failed;
        update(&mut model, Message::ExecutionFinished(execution));

        assert_eq!(
            model.transaction,
            crate::query::result::TransactionState::Failed
        );
        assert!(
            model
                .transaction
                .recovery()
                .expect("a way out")
                .contains("ROLLBACK")
        );
    }

    #[test]
    fn a_write_to_production_is_held_back_until_it_is_confirmed() {
        let mut model = connected();
        // Reclassify the session as production.
        let mut info = session();
        info.environment = Environment::Production;
        update(&mut model, Message::Connected(info));

        model.editor.set_text("UPDATE orders SET total = 0;");
        let effects = update(&mut model, Message::Action(Action::RunBuffer));
        assert!(effects.is_empty(), "nothing runs yet");
        let pending = model
            .pending_run
            .as_ref()
            .expect("a confirmation is waiting");
        assert_eq!(pending.impact, crate::query::Impact::Write);

        let effects = update(&mut model, Message::Action(Action::Activate));
        assert!(
            matches!(effects.as_slice(), [Effect::Execute { .. }]),
            "confirming runs it: {effects:?}"
        );
        assert!(model.pending_run.is_none());
    }

    #[test]
    fn a_destructive_statement_needs_the_database_name_typed() {
        let mut model = connected();
        let mut info = session();
        info.environment = Environment::Production;
        update(&mut model, Message::Connected(info));

        model.editor.set_text("DROP TABLE orders;");
        update(&mut model, Message::Action(Action::RunBuffer));

        // Confirming without typing does nothing at all.
        assert!(update(&mut model, Message::Action(Action::Activate)).is_empty());
        assert!(model.pending_run.is_some(), "still waiting");

        for ch in "orders".chars() {
            update(&mut model, Message::Action(Action::Insert(ch)));
        }
        let effects = update(&mut model, Message::Action(Action::Activate));
        assert!(
            matches!(effects.as_slice(), [Effect::Execute { .. }]),
            "typing the name allows it: {effects:?}"
        );
    }

    #[test]
    fn cancelling_a_confirmation_runs_nothing_and_leaves_the_buffer_alone() {
        let mut model = connected();
        let mut info = session();
        info.environment = Environment::Production;
        update(&mut model, Message::Connected(info));

        model.editor.set_text("DELETE FROM orders;");
        update(&mut model, Message::Action(Action::RunBuffer));
        update(&mut model, Message::Action(Action::Dismiss));

        assert!(model.pending_run.is_none());
        assert_eq!(model.phase, QueryPhase::Idle, "nothing was sent");
        assert_eq!(
            model.editor.text(),
            "DELETE FROM orders;",
            "the SQL is still there"
        );
    }

    #[test]
    fn a_read_against_production_runs_without_asking() {
        let mut model = connected();
        let mut info = session();
        info.environment = Environment::Production;
        update(&mut model, Message::Connected(info));

        model.editor.set_text("SELECT count(*) FROM orders;");
        let effects = update(&mut model, Message::Action(Action::RunBuffer));
        assert!(
            matches!(effects.as_slice(), [Effect::Execute { .. }]),
            "a prompt on every read would be noise: {effects:?}"
        );
        assert!(model.pending_run.is_none());
    }

    #[test]
    fn a_write_to_anything_else_runs_without_asking() {
        // The guardrail follows the classification the user gave, never a guess
        // from a host name.
        let mut model = connected();
        model.editor.set_text("UPDATE orders SET total = 0;");
        let effects = update(&mut model, Message::Action(Action::RunBuffer));
        assert!(matches!(effects.as_slice(), [Effect::Execute { .. }]));
        assert!(model.pending_run.is_none());
    }

    #[test]
    fn typing_while_confirming_never_reaches_the_editor() {
        let mut model = connected();
        let mut info = session();
        info.environment = Environment::Production;
        update(&mut model, Message::Connected(info));

        model.editor.set_text("DROP TABLE orders;");
        update(&mut model, Message::Action(Action::RunBuffer));
        for ch in "xyz".chars() {
            update(&mut model, Message::Action(Action::Insert(ch)));
        }
        assert_eq!(model.editor.text(), "DROP TABLE orders;");
        assert_eq!(model.pending_run.as_ref().expect("pending").typed, "xyz");
    }

    #[test]
    fn quitting_sets_the_flag_and_asks_the_runtime_to_stop() {
        let mut model = connected();
        let effects = update(&mut model, Message::Action(Action::Quit));
        assert_eq!(effects, vec![Effect::Quit]);
        assert!(model.should_quit);
    }

    #[test]
    fn resizing_only_records_the_size() {
        let mut model = connected();
        let before = model.clone();
        update(&mut model, Message::Resized(200, 60));
        assert_eq!(model.size, (200, 60));
        assert_eq!(model.phase, before.phase);
        assert_eq!(model.editor.text(), before.editor.text());
    }

    #[test]
    fn a_new_run_clears_the_previous_error() {
        let mut model = connected();
        model.error = Some(Diagnostic::new(DiagnosticKind::Query, "old", "running"));
        model.editor.set_text("SELECT 1;");
        update(&mut model, Message::Action(Action::RunBuffer));
        assert!(model.error.is_none());
    }
}
