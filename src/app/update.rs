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
    }
}

fn apply_action(model: &mut Model, action: Action) -> Vec<Effect> {
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
            if model.help_open {
                model.help_open = false;
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
            model.focus = model.focus.next();
            Vec::new()
        }
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

/// Starts an execution, refusing when there is nothing to run or nowhere to run it.
fn run(model: &mut Model, sql: String) -> Vec<Effect> {
    if !model.connection.is_usable() || model.phase.is_busy() || sql.trim().is_empty() {
        return Vec::new();
    }
    let parsed = statements::split(&sql);
    if parsed.is_empty() {
        return Vec::new();
    }
    let job = model.allocate_job();
    model.phase = QueryPhase::Running {
        job,
        statements: parsed.len(),
    };
    model.error = None;
    vec![Effect::Execute { job, sql }]
}

fn move_selection(model: &mut Model, direction: Direction) {
    match model.focus {
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
