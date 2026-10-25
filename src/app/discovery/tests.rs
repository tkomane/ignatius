use super::*;

#[test]
fn density_comfortable_can_teach_six_available_actions() {
    let mut model = Model::new(100);
    connected(&mut model);
    model.focus = Focus::Results;
    model.last_execution = Some(rows_execution());
    assert_eq!(hint_actions(&model).len(), 6);
    let compact = model
        .presentation
        .clone()
        .with_density(crate::config::Density::Compact);
    assert_eq!(hint_actions_for(&model, &compact).len(), 4);
    assert_eq!(
        hint_actions(&model).len(),
        6,
        "an explicit override does not change the model"
    );
}

#[test]
fn pane_commands_are_discoverable_without_database_work_and_restore_is_current() {
    let mut model = Model::new(100);
    for action in [Action::ShrinkPane, Action::GrowPane, Action::ZoomPane] {
        assert!(action_is_available(&model, &action));
        assert!(palette_actions(&model).contains(&action));
        assert_eq!(action_prerequisite(&model, &action), None);
    }
    model.splits.zoomed = true;
    for focus in [Focus::Editor, Focus::Results, Focus::Objects] {
        model.focus = focus;
        assert_eq!(hint_actions(&model).first(), Some(&Action::ZoomPane));
    }
}
use crate::connection::Environment;
use crate::postgres::{SessionInfo, TlsState};
use crate::query::result::{Execution, ExecutionStatus, JobId, ResultSet, StatementResult};
use crate::query::value::Cell;
use std::time::Duration;

fn connected(model: &mut Model) {
    model.connection = ConnectionState::Connected(Box::new(SessionInfo {
        target: "app@localhost:5432/orders".into(),
        database: "orders".into(),
        user: "app".into(),
        server_version: "18.6".into(),
        backend_pid: "42".into(),
        search_path: "public".into(),
        read_only: false,
        tls: TlsState::Disabled,
        environment: Environment::Local,
    }));
}

fn rows_execution() -> Execution {
    let mut set = ResultSet::new(vec!["value".into()], 100);
    set.push(vec![Cell::Text("one".into())]);
    Execution {
        notice_report: crate::query::result::NoticeReport::default(),
        job: JobId(1),
        statements: vec![StatementResult {
            result_set: Some(set),
            rows_affected: Some(1),
            elapsed: Duration::from_millis(1),
            notices: Vec::new(),
        }],
        status: ExecutionStatus::Succeeded,
        elapsed: Duration::from_millis(1),
        error: None,
        transaction: TransactionState::Autocommit,
    }
}

#[test]
fn context_names_an_unusable_fresh_session() {
    let model = Model::new(100);
    let ctx = context(&model);
    assert_eq!(ctx.connection, ConnectionPosture::Disconnected);
    assert_eq!(ctx.result, ResultPosture::None);
    assert!(!action_is_available(&model, &Action::RunBuffer));
    assert!(action_is_available(&model, &Action::OpenPalette));
}

#[test]
fn local_formatting_is_discoverable_without_a_connection() {
    let mut model = Model::new(100);
    model.editor.set_text("SELECT 1;");
    assert!(action_is_available(&model, &Action::FormatBuffer));
    assert!(hint_actions(&model).contains(&Action::FormatBuffer));
    assert!(palette_actions(&model).contains(&Action::FormatBuffer));

    model.editor.set_text("");
    assert!(!action_is_available(&model, &Action::FormatBuffer));
    assert_eq!(
        action_prerequisite(&model, &Action::FormatBuffer),
        Some("SQL in the editor".to_owned())
    );
}

#[test]
fn connected_empty_editor_teaches_completion_and_safe_navigation() {
    let mut model = Model::new(100);
    connected(&mut model);
    let hints = hint_actions(&model);
    assert!(hints.contains(&Action::Complete));
    assert!(hints.contains(&Action::FocusNext));
    assert!(!hints.contains(&Action::RunBuffer));
    assert!(hints.len() <= MAX_CONTEXTUAL_HINTS);
}

#[test]
fn plan_actions_are_discoverable_only_for_one_ready_non_explain_statement() {
    let mut model = Model::new(100);
    connected(&mut model);
    model.editor.set_text("SELECT 1;");
    assert!(action_is_available(&model, &Action::ExplainPlan));
    assert!(action_is_available(&model, &Action::AnalyzePlan));
    assert!(hint_actions(&model).contains(&Action::ExplainPlan));
    assert!(palette_actions(&model).contains(&Action::AnalyzePlan));

    model.editor.set_text("EXPLAIN SELECT 1;");
    assert!(!action_is_available(&model, &Action::ExplainPlan));
    assert!(
        action_prerequisite(&model, &Action::ExplainPlan)
            .expect("prerequisite")
            .contains("target statement")
    );

    let mut disconnected = Model::new(100);
    disconnected.editor.set_text("SELECT 1;");
    assert!(!action_is_available(&disconnected, &Action::AnalyzePlan));
    assert_eq!(
        action_prerequisite(&disconnected, &Action::AnalyzePlan),
        Some("connect first".to_owned())
    );
}

#[test]
fn hint_ordering_changes_with_results_and_object_focus() {
    let mut model = Model::new(100);
    connected(&mut model);
    model.editor.set_text("SELECT 1;");

    model.focus = Focus::Results;
    assert_eq!(context(&model).result, ResultPosture::None);
    assert!(!hint_actions(&model).contains(&Action::Move(Direction::Down)));

    model.last_execution = Some(rows_execution());
    let result_hints = hint_actions(&model);
    assert_eq!(result_hints.first(), Some(&Action::Move(Direction::Down)));
    assert!(result_hints.contains(&Action::CopyValue));
    assert!(palette_actions(&model).contains(&Action::CopyValue));
    assert!(action_is_available(&model, &Action::Move(Direction::Down)));

    model.sidebar_visible = true;
    model.focus = Focus::Objects;
    assert_eq!(context(&model).objects, ObjectPosture::Empty);
    assert_eq!(hint_actions(&model).first(), Some(&Action::ReloadObjects));

    model.tree.begin_loading();
    assert_eq!(context(&model).objects, ObjectPosture::Loading);
    assert!(!hint_actions(&model).contains(&Action::ReloadObjects));

    model.tree.set_schemas(vec![crate::postgres::SchemaSummary {
        name: "public".to_owned(),
        usable: true,
        counts: std::collections::BTreeMap::new(),
    }]);
    assert_eq!(context(&model).objects, ObjectPosture::Populated);
    let object_hints = hint_actions(&model);
    assert_eq!(object_hints.first(), Some(&Action::Activate));
    assert!(action_is_available(&model, &Action::Activate));
    assert!(object_hints.contains(&Action::StartFilter));
    assert!(object_hints.len() <= MAX_CONTEXTUAL_HINTS);
}

#[test]
fn unusable_connection_postures_never_offer_run() {
    let mut model = Model::new(100);
    model.editor.set_text("SELECT 1;");
    for connection in [
        ConnectionState::Connecting {
            step: crate::app::model::ConnectingStep::ServerHandshake,
            target: String::new(),
        },
        ConnectionState::Lost {
            info: Box::new(SessionInfo {
                target: "app@localhost:5432/orders".into(),
                database: "orders".into(),
                user: "app".into(),
                server_version: "18.6".into(),
                backend_pid: "42".into(),
                search_path: "public".into(),
                read_only: false,
                tls: TlsState::Disabled,
                environment: Environment::Local,
            }),
        },
        ConnectionState::Failed(Box::new(crate::diagnostics::Diagnostic::new(
            crate::diagnostics::DiagnosticKind::Connection,
            "unreachable",
            "connecting",
        ))),
    ] {
        model.connection = connection;
        assert!(!action_is_available(&model, &Action::RunBuffer));
        assert!(!hint_actions(&model).contains(&Action::RunBuffer));
    }
}

#[test]
fn busy_results_prioritise_cancellation_and_rows_teach_grid_actions() {
    let mut model = Model::new(100);
    connected(&mut model);
    model.focus = Focus::Results;
    model.phase = crate::app::model::QueryPhase::Running {
        job: JobId(1),
        statements: 1,
    };
    assert_eq!(context(&model).result, ResultPosture::Running);
    assert_eq!(hint_actions(&model).first(), Some(&Action::Cancel));

    model.phase = crate::app::model::QueryPhase::Idle;
    model.last_execution = Some(rows_execution());
    let hints = hint_actions(&model);
    assert_eq!(context(&model).result, ResultPosture::Rows);
    assert!(hints.contains(&Action::CopyValue));
    assert!(hints.contains(&Action::ToggleInspector));
    assert!(hints.contains(&Action::StartFilter));
    assert!(hints.len() <= MAX_CONTEXTUAL_HINTS);
}

#[test]
fn column_resize_is_discoverable_only_for_a_retained_result() {
    let mut model = Model::new(100);
    connected(&mut model);
    model.focus = Focus::Results;
    model.last_execution = Some(rows_execution());

    assert!(action_is_available(&model, &Action::NarrowColumn));
    assert!(action_is_available(&model, &Action::WidenColumn));
    assert_eq!(action_prerequisite(&model, &Action::NarrowColumn), None);

    model.focus = Focus::Editor;
    assert!(!action_is_available(&model, &Action::NarrowColumn));
    assert_eq!(
        action_prerequisite(&model, &Action::NarrowColumn),
        Some("focus the Results pane".to_owned())
    );

    model.focus = Focus::Results;
    model.last_execution = None;
    assert!(!action_is_available(&model, &Action::WidenColumn));
    assert_eq!(
        action_prerequisite(&model, &Action::WidenColumn),
        Some("a retained result column to resize".to_owned())
    );
}

#[test]
fn cell_update_is_discoverable_only_for_a_retained_read_write_result_source() {
    let mut model = Model::new(100);
    connected(&mut model);
    model.focus = Focus::Results;
    model.last_execution = Some(rows_execution());
    model.last_sql = Some("SELECT value FROM public.orders".into());
    assert!(action_is_available(&model, &Action::GenerateCellUpdate));
    assert!(hint_actions(&model).contains(&Action::GenerateCellUpdate));
    assert!(palette_actions(&model).contains(&Action::GenerateCellUpdate));

    let mut info = model.connection.info().expect("connection").clone();
    info.environment = Environment::Production;
    model.connection = ConnectionState::Connected(Box::new(info));
    assert!(!action_is_available(&model, &Action::GenerateCellUpdate));
    assert_eq!(
        action_prerequisite(&model, &Action::GenerateCellUpdate),
        Some("a connection not classified as production".into())
    );
}

#[test]
fn retained_result_refresh_uses_only_the_retained_single_read_source() {
    let mut model = Model::new(100);
    connected(&mut model);
    model.focus = Focus::Results;
    model.last_execution = Some(rows_execution());
    model.last_sql = Some("SELECT value FROM public.orders".into());

    assert_eq!(
        retained_result_refresh_source(&model),
        Ok("SELECT value FROM public.orders".to_owned())
    );
    assert!(action_is_available(&model, &Action::RefreshResult));
    assert_eq!(hint_actions(&model).first(), Some(&Action::RefreshResult));
    assert!(palette_actions(&model).contains(&Action::RefreshResult));

    // Refresh is tied to the completed execution, never to whatever the
    // editor happens to contain now.
    model.editor.set_text("UPDATE editor_only SET value = 1;");
    assert!(action_is_available(&model, &Action::RefreshResult));

    for (source, prerequisite) in [
        (
            "SELECT value FROM public.orders; SELECT id FROM public.orders;",
            "one retained statement",
        ),
        ("UPDATE public.orders SET value = 1;", "writes"),
        (
            "ALTER TABLE public.orders ADD COLUMN note text;",
            "changes structure",
        ),
        ("DO $$ BEGIN PERFORM 1; END $$;", "writes"),
    ] {
        model.last_sql = Some(source.to_owned());
        assert!(
            !action_is_available(&model, &Action::RefreshResult),
            "{source}"
        );
        assert!(
            action_prerequisite(&model, &Action::RefreshResult)
                .expect("refresh prerequisite")
                .contains(prerequisite),
            "{source}"
        );
    }

    // A completed result with no rows is still a retained result and can
    // be refreshed when its source is a single read.
    model.last_sql = Some("SELECT value FROM public.orders;".into());
    model.last_execution.as_mut().expect("execution").statements[0]
        .result_set
        .as_mut()
        .expect("result")
        .rows
        .clear();
    assert!(action_is_available(&model, &Action::RefreshResult));
}

#[test]
fn filtered_empty_and_failed_transaction_are_distinct() {
    let mut model = Model::new(100);
    connected(&mut model);
    model.last_execution = Some(rows_execution());
    model.result_filter = "missing".into();
    assert_eq!(context(&model).result, ResultPosture::FilteredEmpty);

    model.transaction = TransactionState::Failed;
    assert_eq!(context(&model).result, ResultPosture::FailedTransaction);
    let note = palette_note(&model);
    assert!(note.contains("ROLLBACK"), "{note}");
}

#[test]
fn palette_note_names_prerequisites_without_doing_work() {
    let model = Model::new(100);
    let before = model.clone();
    let note = palette_note(&model);
    assert!(note.contains("Connect before running SQL."), "{note}");
    assert!(note.contains("SQL in the editor"), "{note}");
    assert_eq!(model.focus, before.focus);
    assert_eq!(model.editor.text(), before.editor.text());
}
