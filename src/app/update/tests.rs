use super::*;
use crate::connection::Environment;
use crate::diagnostics::{Diagnostic, DiagnosticKind};
use crate::postgres::{SessionInfo, TlsState};
use crate::query::result::{Execution, JobId, ResultSet, StatementResult};
use crate::query::value::Cell;
use std::collections::BTreeMap;
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

fn failed_execution(job: JobId, error: Diagnostic) -> Box<Execution> {
    Box::new(Execution {
        job,
        statements: Vec::new(),
        status: ExecutionStatus::Failed,
        elapsed: Duration::from_millis(6),
        error: Some(error),
        transaction: crate::query::result::TransactionState::Autocommit,
    })
}

fn copy_model(rows: Vec<Vec<Cell>>) -> Model {
    let mut model = connected();
    model.focus = Focus::Results;
    let mut set = ResultSet::new(vec!["id".into(), "value".into()], 100);
    for row in rows {
        set.push(row);
    }
    model.last_execution = Some(Execution {
        job: JobId(7),
        statements: vec![StatementResult {
            result_set: Some(set),
            rows_affected: None,
            elapsed: Duration::from_millis(1),
            notices: Vec::new(),
        }],
        status: ExecutionStatus::Succeeded,
        elapsed: Duration::from_millis(1),
        error: None,
        transaction: crate::query::result::TransactionState::Autocommit,
    });
    model
        .result_grid
        .reset_for_result(&model.visible_result().expect("result").columns.clone());
    model.selected_column = 1;
    model
}

fn update_relation() -> crate::postgres::metadata::UpdateRelation {
    crate::postgres::metadata::UpdateRelation {
        schema: "public".into(),
        relation: "orders".into(),
        kind: crate::postgres::metadata::ObjectKind::Table,
        readable: true,
        writable: true,
        columns: vec![
            crate::postgres::metadata::ColumnInfo {
                name: "order_id".into(),
                data_type: "integer".into(),
                nullable: false,
                primary_key: true,
                default: None,
            },
            crate::postgres::metadata::ColumnInfo {
                name: "note".into(),
                data_type: "text".into(),
                nullable: true,
                primary_key: false,
                default: None,
            },
        ],
    }
}

fn cell_update_model() -> Model {
    let mut model = connected();
    model.focus = Focus::Results;
    model.last_sql = Some("SELECT order_id, note FROM public.orders".into());
    let mut set = ResultSet::new(vec!["order_id".into(), "note".into()], 100);
    set.push(vec![Cell::Text("7".into()), Cell::Text("old".into())]);
    model.last_execution = Some(Execution {
        job: JobId(77),
        statements: vec![StatementResult {
            result_set: Some(set),
            rows_affected: None,
            elapsed: Duration::from_millis(1),
            notices: Vec::new(),
        }],
        status: ExecutionStatus::Succeeded,
        elapsed: Duration::from_millis(1),
        error: None,
        transaction: crate::query::result::TransactionState::Autocommit,
    });
    model
        .result_grid
        .reset_for_result(&model.visible_result().expect("result").columns.clone());
    model.selected_column = 1;
    model
}

fn refresh_model(sql: &str) -> Model {
    let mut model = connected();
    model.focus = Focus::Results;
    model
        .editor
        .set_text("SELECT editor_buffer FROM somewhere;");
    model.last_sql = Some(sql.to_owned());
    model.last_execution = Some(*execution(JobId(7), ExecutionStatus::Succeeded, &["old"]));
    model
}

#[test]
fn refreshing_uses_the_retained_source_and_leaves_the_editor_unchanged() {
    let mut model = refresh_model("SELECT value FROM orders;");
    let before_text = model.editor.text().to_owned();
    let before_revision = model.editor.revision();

    let effects = update(&mut model, Message::Action(Action::RefreshResult));

    assert_eq!(
        effects,
        vec![Effect::Execute {
            job: JobId(1),
            sql: "SELECT value FROM orders;".to_owned(),
        }]
    );
    assert!(model.running_refresh);
    assert!(
        model
            .refresh_notice
            .as_deref()
            .is_some_and(|notice| notice.contains("Refreshing the retained result"))
    );
    assert_eq!(model.editor.text(), before_text);
    assert_eq!(model.editor.revision(), before_revision);

    let effects = update(
        &mut model,
        Message::ExecutionFinished(execution(JobId(1), ExecutionStatus::Succeeded, &["fresh"])),
    );
    assert!(matches!(
        effects.as_slice(),
        [Effect::RecordHistory { sql, .. }] if sql == "SELECT value FROM orders;"
    ));
    assert!(!model.running_refresh);
    assert_eq!(
        model.visible_result().expect("refreshed result").rows[0][0].display(),
        "fresh"
    );
    assert!(
        model
            .refresh_notice
            .as_deref()
            .is_some_and(|notice| notice.contains("refreshed once"))
    );
    assert_eq!(model.editor.text(), before_text);
    assert_eq!(model.editor.revision(), before_revision);
}

#[test]
fn refreshing_parameterized_source_prompts_again_before_sending() {
    let mut model = refresh_model("SELECT :value AS value, :other AS other;");
    let before_text = model.editor.text().to_owned();

    assert!(update(&mut model, Message::Action(Action::RefreshResult)).is_empty());
    assert!(model.parameter_prompt.is_some());
    assert!(model.refresh_pending);
    for character in "synthetic-secret".chars() {
        update(&mut model, Message::Action(Action::Insert(character)));
    }
    assert!(update(&mut model, Message::Action(Action::Activate)).is_empty());
    assert!(
        model.parameter_prompt.is_some(),
        "the second value is still due"
    );

    let effects = update(&mut model, Message::Action(Action::Activate));
    assert!(matches!(
        effects.as_slice(),
        [Effect::ExecuteParameterized { job: JobId(1), sql, parameters }]
            if sql == "SELECT :value AS value, :other AS other;"
                && parameters.names() == vec!["value", "other"]
    ));
    assert!(!model.refresh_pending);
    assert!(model.running_refresh);
    assert_eq!(model.editor.text(), before_text);
    assert!(!format!("{effects:?}").contains("synthetic-secret"));

    update(
        &mut model,
        Message::ExecutionFinished(execution(JobId(1), ExecutionStatus::Succeeded, &["fresh"])),
    );
    assert!(!model.running_refresh);
    assert!(update(&mut model, Message::Action(Action::RefreshResult)).is_empty());
    assert!(
        model.parameter_prompt.is_some(),
        "refresh prompts on every run"
    );
}

#[test]
fn refreshing_never_replays_multiple_or_write_classified_sources() {
    for sql in [
        "SELECT value FROM orders; SELECT other FROM orders;",
        "UPDATE orders SET value = 'changed';",
        "DROP TABLE orders;",
        "DO $$ BEGIN PERFORM 1; END $$;",
    ] {
        let mut model = refresh_model(sql);
        let before = model.last_execution.clone();
        let effects = update(&mut model, Message::Action(Action::RefreshResult));
        assert!(effects.is_empty(), "{sql}");
        assert_eq!(model.last_execution, before, "{sql}");
        assert!(!model.refresh_pending, "{sql}");
        assert!(
            model
                .refresh_notice
                .as_deref()
                .is_some_and(|notice| !notice.is_empty()),
            "{sql}"
        );
    }
}

#[test]
fn refreshing_a_zero_row_result_is_still_an_explicit_read() {
    let mut model = refresh_model("SELECT value FROM orders;");
    model.last_execution = Some(Execution {
        job: JobId(7),
        statements: vec![StatementResult {
            result_set: Some(ResultSet::new(vec!["value".into()], 100)),
            rows_affected: None,
            elapsed: Duration::from_millis(1),
            notices: Vec::new(),
        }],
        status: ExecutionStatus::Succeeded,
        elapsed: Duration::from_millis(1),
        error: None,
        transaction: crate::query::result::TransactionState::Autocommit,
    });

    let effects = update(&mut model, Message::Action(Action::RefreshResult));
    assert!(matches!(effects.as_slice(), [Effect::Execute { .. }]));
}

#[test]
fn refresh_refusals_preserve_the_retained_result_and_editor() {
    let mut no_result = connected();
    no_result.focus = Focus::Results;
    no_result.editor.set_text("SELECT editor_only;");
    let before_editor = no_result.editor.text().to_owned();
    assert!(update(&mut no_result, Message::Action(Action::RefreshResult)).is_empty());
    assert_eq!(no_result.editor.text(), before_editor);
    assert!(
        no_result
            .refresh_notice
            .as_deref()
            .is_some_and(|notice| notice.contains("completed retained result"))
    );

    let mut busy = refresh_model("SELECT value FROM orders;");
    busy.phase = QueryPhase::Running {
        job: JobId(99),
        statements: 1,
    };
    let before_result = busy.last_execution.clone();
    assert!(update(&mut busy, Message::Action(Action::RefreshResult)).is_empty());
    assert_eq!(busy.last_execution, before_result);
    assert!(
        busy.refresh_notice
            .as_deref()
            .is_some_and(|notice| notice.contains("current query"))
    );

    let mut plan = refresh_model("SELECT value FROM orders;");
    plan.plan = crate::app::PlanView::loading(JobId(99), false);
    let before_result = plan.last_execution.clone();
    assert!(update(&mut plan, Message::Action(Action::RefreshResult)).is_empty());
    assert_eq!(plan.last_execution, before_result);
    assert!(
        plan.refresh_notice
            .as_deref()
            .is_some_and(|notice| notice.contains("query plan"))
    );

    let mut failed_transaction = refresh_model("SELECT value FROM orders;");
    failed_transaction.transaction = crate::query::result::TransactionState::Failed;
    let before_result = failed_transaction.last_execution.clone();
    assert!(
        update(
            &mut failed_transaction,
            Message::Action(Action::RefreshResult)
        )
        .is_empty()
    );
    assert_eq!(failed_transaction.last_execution, before_result);
    assert!(
        failed_transaction
            .refresh_notice
            .as_deref()
            .is_some_and(|notice| notice.contains("failed transaction"))
    );

    let mut lost = refresh_model("SELECT value FROM orders;");
    update(&mut lost, Message::ConnectionLost);
    let before_result = lost.last_execution.clone();
    assert!(update(&mut lost, Message::Action(Action::RefreshResult)).is_empty());
    assert_eq!(lost.last_execution, before_result);
    assert!(
        lost.refresh_notice
            .as_deref()
            .is_some_and(|notice| notice.contains("reconnect"))
    );
}

#[test]
fn a_refresh_parameter_error_clears_refresh_intent_before_the_next_run() {
    let mut model = refresh_model("SELECT :value AS value;");
    assert!(update(&mut model, Message::Action(Action::RefreshResult)).is_empty());
    update(&mut model, Message::Action(Action::Insert('\0')));
    assert!(update(&mut model, Message::Action(Action::Activate)).is_empty());
    assert!(model.parameter_prompt.is_none());
    assert!(!model.refresh_pending);
    assert!(!model.running_refresh);
    assert!(
        model
            .refresh_notice
            .as_deref()
            .is_some_and(|notice| notice.contains("refused before sending"))
    );

    model.focus = Focus::Editor;
    model.editor.set_text("SELECT 1;");
    assert!(matches!(
        update(&mut model, Message::Action(Action::RunBuffer)).as_slice(),
        [Effect::Execute { sql, .. }] if sql == "SELECT 1;"
    ));
    assert!(!model.running_refresh);
}

#[test]
fn passive_result_actions_do_not_refresh_or_replay_the_source() {
    for action in [
        Action::FocusNext,
        Action::ToggleExpandedRow,
        Action::ToggleInspector,
        Action::OpenResultControls,
        Action::StartFilter,
    ] {
        let mut model = refresh_model("SELECT value FROM orders;");
        let before = model.last_execution.clone();
        let effects = update(&mut model, Message::Action(action.clone()));
        assert!(
            !effects.iter().any(|effect| matches!(
                effect,
                Effect::Execute { .. } | Effect::ExecuteParameterized { .. }
            )),
            "{action:?} emitted query work: {effects:?}"
        );
        assert_eq!(model.last_execution, before, "{action:?}");
        assert!(!model.running_refresh, "{action:?}");
    }

    let mut resized = refresh_model("SELECT value FROM orders;");
    let before = resized.last_execution.clone();
    assert!(update(&mut resized, Message::Resized(160, 40)).is_empty());
    assert_eq!(resized.last_execution, before);
    assert!(!resized.running_refresh);
}

#[test]
fn refresh_cancellation_has_one_outcome_and_no_automatic_retry() {
    let mut model = refresh_model("SELECT value FROM orders;");
    assert!(matches!(
        update(&mut model, Message::Action(Action::RefreshResult)).as_slice(),
        [Effect::Execute { job: JobId(1), .. }]
    ));
    assert_eq!(
        update(&mut model, Message::Action(Action::Cancel)),
        vec![Effect::Cancel { job: JobId(1) }]
    );
    let effects = update(
        &mut model,
        Message::ExecutionFinished(execution(JobId(1), ExecutionStatus::Cancelled, &[])),
    );
    assert!(matches!(
        effects.as_slice(),
        [Effect::RecordHistory { sql, .. }] if sql == "SELECT value FROM orders;"
    ));
    assert!(!model.running_refresh);
    assert!(
        model
            .refresh_notice
            .as_deref()
            .is_some_and(|notice| notice.contains("cancelled"))
    );
    let before = model.last_execution.clone();
    assert!(update(&mut model, Message::Resized(160, 40)).is_empty());
    assert_eq!(model.last_execution, before);
    assert!(!model.running_refresh);
}

#[test]
fn refresh_connection_loss_keeps_the_old_snapshot_and_reports_unknown() {
    let mut model = refresh_model("SELECT value FROM orders;");
    assert!(matches!(
        update(&mut model, Message::Action(Action::RefreshResult)).as_slice(),
        [Effect::Execute { job: JobId(1), .. }]
    ));
    let before = model.last_execution.clone();
    assert!(update(&mut model, Message::ConnectionLost).is_empty());
    assert_eq!(model.last_execution, before);
    assert_eq!(model.phase, QueryPhase::Idle);
    assert!(!model.running_refresh);
    assert!(
        model
            .refresh_notice
            .as_deref()
            .is_some_and(|notice| notice.contains("outcome is unknown"))
    );
}

#[test]
fn editing_the_buffer_during_refresh_does_not_change_the_retained_source() {
    let mut model = refresh_model("SELECT value FROM orders;");
    let effects = update(&mut model, Message::Action(Action::RefreshResult));
    assert!(
        matches!(effects.as_slice(), [Effect::Execute { job: JobId(1), sql }] if sql == "SELECT value FROM orders;")
    );

    model.focus = Focus::Editor;
    update(&mut model, Message::Action(Action::Insert('X')));
    assert_eq!(model.editor.text(), "SELECT editor_buffer FROM somewhere;X");

    update(
        &mut model,
        Message::ExecutionFinished(execution(JobId(1), ExecutionStatus::Succeeded, &["fresh"])),
    );
    assert_eq!(model.last_sql.as_deref(), Some("SELECT value FROM orders;"));
    assert_eq!(
        model.visible_result().expect("refreshed result").rows[0][0],
        Cell::Text("fresh".into())
    );
    assert_eq!(model.editor.text(), "SELECT editor_buffer FROM somewhere;X");
}

#[test]
fn connection_loss_discards_server_prompts_but_keeps_local_copy_available() {
    let mut parameter = connected();
    parameter.editor.set_text("SELECT :value;");
    assert!(update(&mut parameter, Message::Action(Action::RunBuffer)).is_empty());
    assert!(parameter.parameter_prompt.is_some());
    update(&mut parameter, Message::ConnectionLost);
    assert!(parameter.parameter_prompt.is_none());
    assert!(update(&mut parameter, Message::Action(Action::RunBuffer)).is_empty());

    let mut refreshing = refresh_model("SELECT :value;");
    assert!(update(&mut refreshing, Message::Action(Action::RefreshResult)).is_empty());
    assert!(refreshing.parameter_prompt.is_some());
    assert!(refreshing.refresh_pending);
    update(&mut refreshing, Message::ConnectionLost);
    assert!(refreshing.parameter_prompt.is_none());
    assert!(!refreshing.refresh_pending);
    assert!(!refreshing.running_refresh);
    assert!(
        refreshing
            .refresh_notice
            .as_deref()
            .is_some_and(|notice| notice.contains("outcome is unknown"))
    );

    let mut production = connected();
    let mut info = session();
    info.environment = Environment::Production;
    update(&mut production, Message::Connected(info));
    production.editor.set_text("UPDATE orders SET value = 0;");
    assert!(update(&mut production, Message::Action(Action::RunBuffer)).is_empty());
    assert!(production.pending_run.is_some());
    update(&mut production, Message::ConnectionLost);
    assert!(production.pending_run.is_none());
    assert!(update(&mut production, Message::Action(Action::RunBuffer)).is_empty());

    let mut plan = connected();
    plan.editor.set_text("SELECT 1;");
    assert!(update(&mut plan, Message::Action(Action::AnalyzePlan)).is_empty());
    assert!(plan.pending_plan.is_some());
    update(&mut plan, Message::ConnectionLost);
    assert!(plan.pending_plan.is_none());
    assert!(update(&mut plan, Message::Action(Action::RunBuffer)).is_empty());

    let mut cell_update = cell_update_model();
    let update_effects = update(
        &mut cell_update,
        Message::Action(Action::GenerateCellUpdate),
    );
    let [Effect::LoadUpdateTarget { request, .. }] = update_effects.as_slice() else {
        panic!("expected an update metadata effect");
    };
    let request = *request;
    update(&mut cell_update, Message::ConnectionLost);
    assert!(cell_update.update_lookup.is_none());
    assert!(cell_update.update_prompt.is_none());
    assert!(cell_update.pending_update.is_none());
    assert!(
        cell_update
            .cell_update_notice
            .as_deref()
            .is_some_and(|notice| notice.contains("no cell update was retried"))
    );
    update(
        &mut cell_update,
        Message::UpdateTargetLoaded {
            request,
            result: Box::new(Ok(update_relation())),
        },
    );
    assert!(cell_update.update_prompt.is_none());

    let mut copy = copy_model(vec![vec![
        Cell::Text("7".into()),
        Cell::Text("retained".into()),
    ]]);
    copy.clipboard_osc52 = true;
    assert!(update(&mut copy, Message::Action(Action::CopyValue)).is_empty());
    assert!(copy.pending_copy.is_some());
    update(&mut copy, Message::ConnectionLost);
    let effects = update(&mut copy, Message::Action(Action::Activate));
    assert!(matches!(effects.as_slice(), [Effect::CopyValue { .. }]));
}

#[test]
fn connection_loss_clears_running_identity_before_the_late_execution_result() {
    let mut model = connected();
    model.editor.set_text("SELECT new_value;");
    model.last_sql = Some("SELECT old_value;".into());
    model.last_execution = Some(*execution(JobId(9), ExecutionStatus::Succeeded, &["old"]));
    let effects = update(&mut model, Message::Action(Action::RunBuffer));
    let [Effect::Execute { job, .. }] = effects.as_slice() else {
        panic!("expected an execute effect: {effects:?}");
    };
    let job = *job;
    let retained = model.last_execution.clone();

    update(&mut model, Message::ConnectionLost);
    assert_eq!(model.phase, QueryPhase::Idle);
    assert_eq!(
        model.transaction,
        crate::query::result::TransactionState::Unknown
    );
    assert!(model.running_sql.is_none());
    assert!(!model.running_parameterized);
    assert!(!model.running_cell_update);
    assert!(model.running_editor_revision.is_none());
    assert_eq!(model.last_execution, retained);

    update(
        &mut model,
        Message::ExecutionFinished(execution(job, ExecutionStatus::ConnectionLost, &[])),
    );
    assert_eq!(model.last_execution, retained);
    assert_eq!(model.phase, QueryPhase::Idle);
    assert!(model.running_sql.is_none());
}

#[test]
fn cell_update_completion_does_not_start_a_refresh() {
    let mut model = refresh_model("SELECT value FROM orders;");
    model.running_cell_update = true;
    model.running_sql = Some("UPDATE orders SET value = $1 WHERE id = $2".into());
    model.phase = QueryPhase::Running {
        job: JobId(1),
        statements: 1,
    };
    let effects = update(
        &mut model,
        Message::ExecutionFinished(execution(
            JobId(1),
            ExecutionStatus::Succeeded,
            &["ignored"],
        )),
    );
    assert!(matches!(
        effects.as_slice(),
        [Effect::RecordHistory { sql, .. }] if sql.starts_with("UPDATE orders")
    ));
    assert!(!model.running_refresh);
    assert!(!model.refresh_pending);
    assert!(model.refresh_notice.is_none());
    assert_eq!(model.last_sql.as_deref(), Some("SELECT value FROM orders;"));
}

fn connection_profiles() -> Vec<crate::app::ConnectionProfileSummary> {
    crate::app::connection_picker::summaries(
        &BTreeMap::from([
            (
                "orders-dev".to_owned(),
                crate::config::schema::Profile {
                    host: Some("127.0.0.1".to_owned()),
                    port: Some(55432),
                    dbname: Some("ignatius_demo".to_owned()),
                    user: Some("ignatius_test".to_owned()),
                    sslmode: Some("disable".to_owned()),
                    environment: Some("development".to_owned()),
                    description: Some("safe local fixtures".to_owned()),
                    ..crate::config::schema::Profile::default()
                },
            ),
            (
                "orders-prod".to_owned(),
                crate::config::schema::Profile {
                    host: Some("db.example.net".to_owned()),
                    dbname: Some("orders".to_owned()),
                    user: Some("app".to_owned()),
                    sslmode: Some("verify-full".to_owned()),
                    environment: Some("production".to_owned()),
                    read_only: true,
                    auth: Some("entra".to_owned()),
                    ..crate::config::schema::Profile::default()
                },
            ),
        ]),
        &BTreeMap::new(),
    )
}

#[test]
fn the_connection_picker_is_searchable_without_preselecting_a_route() {
    let mut model = Model::new(100);
    model.editor.set_text("SELECT 1;");
    model.connection_profiles = connection_profiles();

    assert!(update(&mut model, Message::Action(Action::OpenConnectionPicker)).is_empty());
    let picker = model.palette.as_ref().expect("connection picker");
    assert_eq!(picker.purpose, crate::app::palette::Purpose::Connections);
    assert_eq!(picker.entries.len(), 3);
    assert_eq!(picker.selected, 0);
    assert_eq!(model.connection, ConnectionState::Disconnected);
    assert_eq!(model.editor.text(), "SELECT 1;");

    for ch in "read-only".chars() {
        update(&mut model, Message::Action(Action::Insert(ch)));
    }
    assert_eq!(
        model
            .palette
            .as_ref()
            .expect("picker remains open")
            .selected_entry()
            .expect("read-only profile")
            .label,
        "orders-prod"
    );
    update(&mut model, Message::Action(Action::Dismiss));
    assert!(model.palette.is_none());
    assert_eq!(model.connection, ConnectionState::Disconnected);
    assert_eq!(model.editor.text(), "SELECT 1;");
}

#[test]
fn the_connection_picker_is_reachable_from_the_general_palette() {
    let mut model = connected();
    model.connection_profiles = connection_profiles();

    update(&mut model, Message::Action(Action::OpenPalette));
    let palette = model.palette.as_mut().expect("command palette");
    palette.query = "choose".to_owned();
    assert_eq!(
        palette
            .selected_entry()
            .expect("connection command")
            .command,
        crate::app::palette::PaletteCommand::Run(Action::OpenConnectionPicker)
    );

    update(&mut model, Message::Action(Action::Activate));
    assert_eq!(
        model.palette.as_ref().expect("connection picker").purpose,
        crate::app::palette::Purpose::Connections
    );
}

#[test]
fn choosing_a_profile_emits_only_its_name_and_clears_server_facts() {
    let mut model = connected();
    model.editor.set_text("SELECT 1;");
    model.connection_profiles = connection_profiles();
    model.last_execution = Some(*execution(
        JobId(9),
        ExecutionStatus::Succeeded,
        &["retained"],
    ));
    model.error_location_note = Some("server position 4".to_owned());
    model.result_grid.show_types = false;
    model.result_grid.freeze_first = true;

    update(&mut model, Message::Action(Action::OpenConnectionPicker));
    for ch in "orders-prod".chars() {
        update(&mut model, Message::Action(Action::Insert(ch)));
    }
    let effects = update(&mut model, Message::Action(Action::Activate));

    assert_eq!(
        effects,
        vec![Effect::ConnectProfile {
            profile: Some("orders-prod".to_owned()),
        }]
    );
    assert_eq!(model.connection, ConnectionState::Connecting);
    assert_eq!(model.credential_provider.as_deref(), Some("entra"));
    assert_eq!(model.editor.text(), "SELECT 1;");
    assert!(model.last_execution.is_none());
    assert!(model.tree.roots.is_empty());
    assert!(matches!(
        model.completion.catalog,
        crate::app::completion::CatalogStatus::NotLoaded
    ));
    assert!(model.error_location_note.is_none());
    assert!(model.pending_copy.is_none());
    assert!(!model.result_grid.show_types);
    assert!(model.result_grid.freeze_first);
    assert!(model.palette.is_none());
}

#[test]
fn choosing_default_settings_clears_profile_identity() {
    let mut model = Model::new(100);
    model.connection_profiles = connection_profiles();
    model.credential_provider = Some("entra".to_owned());

    update(&mut model, Message::Action(Action::OpenConnectionPicker));
    let effects = update(&mut model, Message::Action(Action::Activate));

    assert_eq!(effects, vec![Effect::ConnectProfile { profile: None }]);
    assert_eq!(model.connection, ConnectionState::Connecting);
    assert!(model.credential_provider.is_none());
}

#[test]
fn a_busy_session_cannot_open_the_connection_picker() {
    let mut model = connected();
    model.connection_profiles = connection_profiles();
    model.phase = QueryPhase::Running {
        job: JobId(3),
        statements: 1,
    };

    assert!(!crate::app::discovery::action_is_available(
        &model,
        &Action::OpenConnectionPicker
    ));
    assert!(update(&mut model, Message::Action(Action::OpenConnectionPicker)).is_empty());
    assert!(model.palette.is_none());
    assert!(model.phase.is_busy());
}

fn completion_catalog() -> crate::query::completion::CompletionCatalog {
    crate::query::completion::CompletionCatalog {
        objects: vec![crate::query::completion::CatalogObject {
            kind: crate::query::completion::CatalogObjectKind::Table,
            schema: "public".into(),
            name: "orders".into(),
            readable: true,
            detail: None,
        }],
        relations: vec![crate::query::completion::CatalogRelation {
            schema: "public".into(),
            name: "orders".into(),
            columns: vec![crate::query::completion::CatalogColumn {
                name: "order_id".into(),
                data_type: "bigint".into(),
            }],
        }],
    }
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
fn completion_acceptance_is_explicit_quoted_and_undoable_once() {
    let mut model = connected();
    model.completion.catalog = crate::app::completion::CatalogStatus::Ready {
        catalog: completion_catalog(),
        loaded_at: "2026-09-04 10:00:00 +02:00".into(),
    };
    model.editor.set_text("SELECT * FROM ord");
    let before = model.editor.text().to_owned();

    assert!(update(&mut model, Message::Action(Action::Complete)).is_empty());
    assert_eq!(model.editor.text(), before, "opening never edits SQL");
    assert!(model.completion.menu.is_some());

    update(&mut model, Message::Action(Action::Activate));
    assert_eq!(model.editor.text(), "SELECT * FROM \"orders\"");
    assert!(model.completion.menu.is_none());

    update(&mut model, Message::Action(Action::Undo));
    assert_eq!(model.editor.text(), before);
}

#[test]
fn dismissing_completion_leaves_the_buffer_and_cursor_exactly_unchanged() {
    let mut model = connected();
    model.completion.catalog = crate::app::completion::CatalogStatus::Ready {
        catalog: completion_catalog(),
        loaded_at: "snapshot".into(),
    };
    model.editor.set_text("SELECT * FROM ord");
    let cursor = model.editor.cursor();
    let text = model.editor.text().to_owned();
    update(&mut model, Message::Action(Action::Complete));
    update(&mut model, Message::Action(Action::Dismiss));
    assert_eq!(model.editor.text(), text);
    assert_eq!(model.editor.cursor(), cursor);
    assert!(model.completion.menu.is_none());
}

#[test]
fn automatic_completion_off_keeps_the_same_buffer_as_completion_on() {
    for (prefix, typed) in [
        ("SELECT * FROM ", "ord"),
        ("SELECT ", "co"),
        ("SELECT * FROM orders o WHERE o.", "to"),
        ("WITH recent AS (SELECT 1) SELECT * FROM ", "rec"),
    ] {
        let mut on = connected();
        let mut off = connected();
        on.completion.catalog = crate::app::completion::CatalogStatus::Ready {
            catalog: completion_catalog(),
            loaded_at: "snapshot".into(),
        };
        off.completion = on.completion.clone();
        off.completion.enabled = false;
        on.editor.set_text(prefix);
        off.editor.set_text(prefix);

        for character in typed.chars() {
            update(&mut on, Message::Action(Action::Insert(character)));
            update(&mut off, Message::Action(Action::Insert(character)));
            assert_eq!(on.editor.text(), off.editor.text(), "prefix {prefix:?}");
        }
        assert_eq!(on.editor.text(), off.editor.text());
        // Exact, unambiguous words deliberately suppress automatic menus;
        // the invariant here is that disabling them never changes editing.
        assert!(off.completion.menu.is_none());
    }
}

#[test]
fn automatic_completion_does_not_capture_enter_after_an_exact_keyword() {
    let mut model = connected();
    for character in "SELECT".chars() {
        update(&mut model, Message::Action(Action::Insert(character)));
    }

    assert!(model.completion.menu.is_none());
    update(&mut model, Message::Action(Action::Activate));

    assert_eq!(model.editor.text(), "SELECT\n");
}

#[test]
fn a_late_completion_catalogue_cannot_replace_a_newer_reload() {
    let mut model = connected();
    let first = update(&mut model, Message::Action(Action::ReloadObjects));
    let first_request = first
        .iter()
        .find_map(|effect| match effect {
            Effect::LoadCompletionCatalog { request } => Some(*request),
            _ => None,
        })
        .expect("completion request");
    let second = update(&mut model, Message::Action(Action::ReloadObjects));
    let second_request = second
        .iter()
        .find_map(|effect| match effect {
            Effect::LoadCompletionCatalog { request } => Some(*request),
            _ => None,
        })
        .expect("second completion request");
    assert_ne!(first_request, second_request);

    update(
        &mut model,
        Message::CompletionLoaded {
            request: first_request,
            loaded_at: "old".into(),
            result: Box::new(Ok(completion_catalog())),
        },
    );
    assert!(
        matches!(model.completion.catalog, crate::app::completion::CatalogStatus::Loading { request } if request == second_request)
    );
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
        Message::ExecutionFinished(execution(first_job, ExecutionStatus::Succeeded, &["first"])),
    );
    model.editor.set_text("SELECT 2;");
    update(&mut model, Message::Action(Action::RunBuffer));
    let second_job = model.phase.job().expect("running");
    assert_ne!(first_job, second_job);

    // A late result from the first query arrives.
    update(
        &mut model,
        Message::ExecutionFinished(execution(first_job, ExecutionStatus::Succeeded, &["stale"])),
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
    assert!(model.cell_update_notice.is_none());
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
fn named_parameters_prompt_once_in_order_without_editing_the_template() {
    let mut model = connected();
    let template = "SELECT :customer_id, :customer_id, :status;";
    model.editor.set_text(template);

    assert!(update(&mut model, Message::Action(Action::RunBuffer)).is_empty());
    let prompt = model.parameter_prompt.as_ref().expect("first prompt");
    assert_eq!(prompt.names, ["customer_id", "status"]);
    assert_eq!(prompt.active_name(), Some("customer_id"));
    assert_eq!(model.editor.text(), template);
    assert_eq!(model.phase, QueryPhase::Idle);

    for character in "42".chars() {
        update(&mut model, Message::Action(Action::Insert(character)));
    }
    assert!(update(&mut model, Message::Action(Action::Activate)).is_empty());
    assert_eq!(
        model
            .parameter_prompt
            .as_ref()
            .and_then(|prompt| prompt.active_name()),
        Some("status")
    );

    for character in "open; -- not syntax".chars() {
        update(&mut model, Message::Action(Action::Insert(character)));
    }
    let effects = update(&mut model, Message::Action(Action::Activate));
    let [
        Effect::ExecuteParameterized {
            job,
            sql,
            parameters,
        },
    ] = effects.as_slice()
    else {
        panic!("expected one parameterized effect: {effects:?}");
    };
    assert_eq!(sql, template);
    assert_eq!(model.phase.job(), Some(*job));
    assert!(model.running_parameterized);
    assert!(model.parameter_prompt.is_none());
    assert_eq!(parameters.names(), vec!["customer_id", "status"]);
    let debug = format!("{effects:?}");
    assert!(!debug.contains("open; -- not syntax"));
    assert_eq!(model.editor.text(), template);
}

#[test]
fn cancelling_a_parameter_prompt_sends_nothing_and_keeps_the_template_ready() {
    let mut model = connected();
    let template = "SELECT :secret;";
    model.editor.set_text(template);
    update(&mut model, Message::Action(Action::RunBuffer));
    update(&mut model, Message::Action(Action::Insert('x')));
    assert!(update(&mut model, Message::Action(Action::Cancel)).is_empty());

    assert!(model.parameter_prompt.is_none());
    assert_eq!(model.phase, QueryPhase::Idle);
    assert_eq!(model.editor.text(), template);
    assert!(model.history.is_empty());
}

#[test]
fn an_empty_parameter_is_accepted_and_nul_is_refused_before_an_effect() {
    let mut empty = connected();
    empty.editor.set_text("SELECT :value;");
    update(&mut empty, Message::Action(Action::RunBuffer));
    let effects = update(&mut empty, Message::Action(Action::Activate));
    assert!(matches!(
        effects.as_slice(),
        [Effect::ExecuteParameterized { parameters, .. }] if parameters.names() == vec!["value"]
    ));

    let mut nul = connected();
    nul.editor.set_text("SELECT :value;");
    update(&mut nul, Message::Action(Action::RunBuffer));
    update(&mut nul, Message::Action(Action::Insert('\0')));
    assert!(update(&mut nul, Message::Action(Action::Activate)).is_empty());
    assert!(nul.parameter_prompt.is_none());
    let error = nul.error.as_ref().expect("NUL diagnostic");
    assert!(error.headline.contains(":value"));
    assert!(!error.headline.contains("never"));
}

#[test]
fn parameterized_server_positions_are_not_mapped_to_template_carets() {
    let mut model = connected();
    let template = "SELECT :value FROM orders;";
    model.editor.set_text(template);
    update(&mut model, Message::Action(Action::RunBuffer));
    update(&mut model, Message::Action(Action::Insert('x')));
    let effects = update(&mut model, Message::Action(Action::Activate));
    let job = match effects.as_slice() {
        [Effect::ExecuteParameterized { job, .. }] => *job,
        other => panic!("expected parameterized execution: {other:?}"),
    };
    let cursor = model.editor.cursor();
    update(
        &mut model,
        Message::ExecutionFinished(failed_execution(
            job,
            Diagnostic::new(DiagnosticKind::Query, "syntax error", "running statement 1")
                .in_statement(1)
                .at_position(17),
        )),
    );
    assert!(model.error_location.is_none());
    assert!(
        model
            .error_location_note
            .as_deref()
            .is_some_and(|note| note.contains("expanded request"))
    );
    assert_eq!(model.editor.cursor(), cursor);
}

fn plan_document(analyzed: bool) -> crate::query::PlanDocument {
    crate::query::PlanDocument::parse(
            r#"[{"Plan":{"Node Type":"Seq Scan","Relation Name":"orders","Startup Cost":0,"Total Cost":4,"Plan Rows":2,"Actual Rows":2,"Actual Total Time":0.4,"Actual Loops":1},"Planning Time":0.2,"Execution Time":0.7}]"#,
            analyzed,
        )
        .expect("plan fixture")
}

fn plan_finished(
    job: JobId,
    analyzed: bool,
    result: Result<crate::query::PlanDocument, Diagnostic>,
) -> Box<crate::app::PlanExecution> {
    Box::new(crate::app::PlanExecution {
        job,
        analyzed,
        elapsed: Duration::from_millis(2),
        transaction: crate::query::result::TransactionState::Autocommit,
        result,
        connection_lost: false,
    })
}

#[test]
fn a_plain_plan_uses_only_the_statement_under_the_cursor() {
    let mut model = connected();
    model.editor.set_text("SELECT 1;\nSELECT 2;");
    let effects = update(&mut model, Message::Action(Action::ExplainPlan));
    let Effect::Explain { job, sql, analyze } = &effects[0] else {
        panic!("expected a plan effect, got {effects:?}");
    };
    assert_eq!(sql, "SELECT 2");
    assert!(!analyze);
    assert_eq!(model.phase.job(), Some(*job));
    assert!(model.plan.is_loading());
    assert!(
        model.running_sql.is_none(),
        "plans are not history candidates"
    );
}

#[test]
fn analyze_plan_waits_for_explicit_confirmation_and_cancel_sends_no_sql() {
    let mut model = connected();
    model.editor.set_text("SELECT 1;");
    assert!(update(&mut model, Message::Action(Action::AnalyzePlan)).is_empty());
    assert!(model.pending_plan.is_some());
    assert_eq!(model.phase, QueryPhase::Idle);

    assert!(update(&mut model, Message::Action(Action::Dismiss)).is_empty());
    assert!(model.pending_plan.is_none());
    assert_eq!(model.phase, QueryPhase::Idle);
}

#[test]
fn confirmed_analyze_plan_executes_once_and_never_records_the_generated_sql() {
    let mut model = connected();
    model.editor.set_text("SELECT 1;");
    update(&mut model, Message::Action(Action::AnalyzePlan));
    let effects = update(&mut model, Message::Action(Action::Activate));
    let Effect::Explain { job, sql, analyze } = &effects[0] else {
        panic!("expected analyze effect, got {effects:?}");
    };
    assert_eq!(sql, "SELECT 1");
    assert!(*analyze);
    assert!(model.pending_plan.is_none());
    assert_eq!(model.phase.job(), Some(*job));
    assert!(model.running_sql.is_none());
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::RecordHistory { .. }))
    );
}

#[test]
fn a_plan_completion_preserves_the_retained_ordinary_result_and_history() {
    let mut model = connected();
    model.editor.set_text("SELECT 1;");
    model.last_execution = Some(*execution(
        JobId(99),
        ExecutionStatus::Succeeded,
        &["retained"],
    ));
    model.history.push(crate::history::Entry::now(
        "target",
        "orders",
        "local",
        "SELECT retained",
        crate::history::Outcome::Succeeded,
        Duration::from_millis(1),
    ));
    let before = model.last_execution.clone();
    let history_before = model.history.clone();
    update(&mut model, Message::Action(Action::ExplainPlan));
    let job = model.phase.job().expect("plan job");
    update(
        &mut model,
        Message::PlanFinished(plan_finished(job, false, Ok(plan_document(false)))),
    );
    assert!(matches!(
        model.plan.status,
        crate::app::PlanStatus::Ready(_)
    ));
    assert_eq!(model.last_execution, before);
    assert_eq!(model.history, history_before);
    assert_eq!(model.plan.selected_path, Vec::<usize>::new());
}

#[test]
fn stale_plan_completion_cannot_replace_the_current_plan_job() {
    let mut model = connected();
    model.editor.set_text("SELECT 1;");
    update(&mut model, Message::Action(Action::ExplainPlan));
    let job = model.phase.job().expect("plan job");
    let stale = JobId(job.0 + 1);
    update(
        &mut model,
        Message::PlanFinished(plan_finished(stale, false, Ok(plan_document(false)))),
    );
    assert_eq!(model.phase.job(), Some(job));
    assert!(model.plan.is_loading());
}

#[test]
fn plan_navigation_and_dismissal_are_local_and_restore_the_prior_view() {
    let mut model = connected();
    model.editor.set_text("SELECT 1;");
    model.last_execution = Some(*execution(JobId(9), ExecutionStatus::Succeeded, &["old"]));
    update(&mut model, Message::Action(Action::ExplainPlan));
    let job = model.phase.job().expect("plan job");
    update(
        &mut model,
        Message::PlanFinished(plan_finished(job, false, Ok(plan_document(false)))),
    );
    model.focus = Focus::Results;
    assert!(update(&mut model, Message::Action(Action::Move(Direction::Down))).is_empty());
    assert!(update(&mut model, Message::Action(Action::Activate)).is_empty());
    assert!(
        model.plan.collapsed.is_empty(),
        "leaf activation is local and harmless"
    );
    assert!(update(&mut model, Message::Action(Action::Dismiss)).is_empty());
    assert!(!model.plan.is_visible());
    assert_eq!(
        model.visible_result().expect("retained result").rows[0][0],
        Cell::Text("old".into())
    );
}

#[test]
fn a_visible_plan_keeps_ordinary_result_controls_behind_it() {
    let mut model = connected();
    model.editor.set_text("SELECT 1;");
    model.last_execution = Some(*execution(JobId(9), ExecutionStatus::Succeeded, &["old"]));
    model.expanded_row = false;
    update(&mut model, Message::Action(Action::ExplainPlan));
    let job = model.phase.job().expect("plan job");
    update(
        &mut model,
        Message::PlanFinished(plan_finished(job, false, Ok(plan_document(false)))),
    );
    model.focus = Focus::Results;

    assert!(update(&mut model, Message::Action(Action::StartFilter)).is_empty());
    assert!(!model.result_filtering);
    assert!(update(&mut model, Message::Action(Action::OpenResultControls)).is_empty());
    assert!(model.palette.is_none());
    assert!(update(&mut model, Message::Action(Action::ExportRows)).is_empty());
    assert!(model.name_prompt.is_none());
    assert!(update(&mut model, Message::Action(Action::ToggleExpandedRow)).is_empty());
    assert!(!model.expanded_row);
    assert!(update(&mut model, Message::Action(Action::ToggleInspector)).is_empty());
    assert!(model.inspector.is_none());
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
fn enter_in_the_editor_keeps_the_indentation_through_the_real_key_path() {
    // The editor's own test proves `insert_newline` indents. This proves the
    // key reaches it: the reducer used to call `insert('\n')`, so the rule
    // existed and no key press ever ran it.
    let mut model = connected();
    model.focus = Focus::Editor;
    model.editor.set_text("SELECT one\n    FROM two");
    model.editor.move_buffer_end();

    update(&mut model, Message::Action(Action::Activate));
    update(&mut model, Message::Action(Action::Insert('W')));

    assert_eq!(
        model.editor.text(),
        "SELECT one\n    FROM two\n    W",
        "Enter carried the four spaces down with it"
    );
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

#[test]
fn editing_keys_reach_the_editor_and_nothing_else() {
    let mut model = connected();
    model.editor.set_text("SELECT customer_id\nFROM orders");

    update(&mut model, Message::Action(Action::MoveBufferStart));
    assert_eq!(model.editor.position(), (1, 1));
    update(&mut model, Message::Action(Action::MoveLineEnd));
    assert_eq!(model.editor.position(), (1, 19));
    update(
        &mut model,
        Message::Action(Action::MoveWord(Direction::Left)),
    );
    assert_eq!(model.editor.position(), (1, 8));
    update(&mut model, Message::Action(Action::DeleteWordLeft));
    assert_eq!(model.editor.text(), "customer_id\nFROM orders");
    update(&mut model, Message::Action(Action::Undo));
    assert_eq!(model.editor.text(), "SELECT customer_id\nFROM orders");
    update(&mut model, Message::Action(Action::Redo));
    assert_eq!(model.editor.text(), "customer_id\nFROM orders");

    update(&mut model, Message::Action(Action::MoveBufferStart));
    update(&mut model, Message::Action(Action::DeleteForward));
    assert_eq!(model.editor.text(), "ustomer_id\nFROM orders");

    // The same keys in another pane leave the buffer alone.
    let before = model.editor.text().to_owned();
    model.focus = Focus::Results;
    for action in [
        Action::DeleteForward,
        Action::DeleteWordLeft,
        Action::MoveWord(Direction::Left),
        Action::MoveLineStart,
        Action::MoveLineEnd,
        Action::Undo,
        Action::Redo,
    ] {
        update(&mut model, Message::Action(action));
    }
    assert_eq!(model.editor.text(), before);
}

#[test]
fn vertical_movement_works_in_the_editor_where_it_used_to_do_nothing() {
    let mut model = connected();
    model
        .editor
        .set_text("SELECT customer_id\nFROM orders\nWHERE total > 100");
    update(&mut model, Message::Action(Action::MoveBufferStart));
    update(&mut model, Message::Action(Action::Move(Direction::Down)));
    assert_eq!(model.editor.position(), (2, 1));
    update(&mut model, Message::Action(Action::Move(Direction::Down)));
    update(&mut model, Message::Action(Action::Move(Direction::Up)));
    assert_eq!(model.editor.position(), (2, 1));
}

#[test]
fn a_page_key_moves_by_what_is_on_screen_in_whichever_pane_has_focus() {
    let mut model = connected();
    model.size = (120, 40);
    let text = (1..=200)
        .map(|n| format!("-- line {n}"))
        .collect::<Vec<_>>()
        .join("\n");
    model.editor.set_text(text);
    update(&mut model, Message::Action(Action::MoveBufferStart));

    let page = crate::ui::layout::editor_page(model.size);
    assert!(page > 1, "a full-size terminal shows more than one line");
    update(
        &mut model,
        Message::Action(Action::MovePage(Direction::Down)),
    );
    assert_eq!(model.editor.position().0, page + 1);
    update(&mut model, Message::Action(Action::MovePage(Direction::Up)));
    assert_eq!(model.editor.position().0, 1);

    // In the results it moves the selection, and stops at the last row.
    model.editor.set_text("SELECT 1;");
    let effects = update(&mut model, Message::Action(Action::RunBuffer));
    let Effect::Execute { job, .. } = effects[0] else {
        panic!("expected an execute effect");
    };
    let rows: Vec<String> = (0..500).map(|n| n.to_string()).collect();
    let borrowed: Vec<&str> = rows.iter().map(String::as_str).collect();
    update(
        &mut model,
        Message::ExecutionFinished(execution(job, ExecutionStatus::Succeeded, &borrowed)),
    );
    model.focus = Focus::Results;
    update(
        &mut model,
        Message::Action(Action::MovePage(Direction::Down)),
    );
    assert_eq!(
        model.selected_row,
        crate::ui::layout::results_page(model.size)
    );
    update(&mut model, Message::Action(Action::MoveBufferEnd));
    assert_eq!(
        model.selected_row, 99,
        "the last row is the last retained row, not the last row the server sent"
    );
    update(&mut model, Message::Action(Action::MoveBufferStart));
    assert_eq!(model.selected_row, 0);
}

fn history_entry(sql: &str) -> crate::history::Entry {
    crate::history::Entry::now(
        "app@localhost:5432/orders",
        "orders",
        "local",
        sql,
        crate::history::Outcome::Succeeded,
        Duration::from_millis(3),
    )
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
    assert_eq!(
        effects,
        vec![
            Effect::LoadSchemas,
            Effect::LoadCompletionCatalog { request: 1 },
        ],
        "showing it loads the tree and one schema snapshot"
    );
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

/// A tree with one table selected, ready to be asked about.
fn with_selected_table() -> Model {
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
            payload: Box::new(Ok(crate::app::tree::MetadataPayload::Objects(vec![
                crate::postgres::ObjectSummary {
                    kind: crate::postgres::ObjectKind::Table,
                    schema: "public".into(),
                    name: "orders".into(),
                    readable: true,
                    detail: None,
                },
            ]))),
        },
    );
    update(&mut model, Message::Action(Action::Move(Direction::Down)));
    model
}

fn definition_of(name: &str) -> crate::postgres::metadata::Definition {
    crate::postgres::metadata::Definition {
        kind: crate::postgres::ObjectKind::Table,
        schema: "public".into(),
        name: name.to_owned(),
        source: crate::postgres::metadata::DefinitionSource::Assembled,
        text: (1..=40)
            .map(|n| format!("    column_{n} text"))
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

#[test]
fn asking_what_depends_on_an_object_lists_both_directions() {
    let mut model = with_selected_table();
    let effects = update(&mut model, Message::Action(Action::ShowDependencies));
    let Effect::LoadDependencies { request, object } = &effects[0] else {
        panic!("expected a dependency load, got {effects:?}");
    };
    assert_eq!(object.name, "orders");
    let request = *request;

    let related = |name: &str, reason: &str| crate::postgres::ObjectSummary {
        kind: crate::postgres::ObjectKind::View,
        schema: "public".into(),
        name: name.to_owned(),
        readable: true,
        detail: Some(reason.to_owned()),
    };
    update(
        &mut model,
        Message::DependenciesLoaded {
            request,
            result: Box::new(Ok(crate::postgres::metadata::Dependencies {
                used_by: vec![related("recent_orders", "reads it")],
                depends_on: vec![related("customers", "is referred to by it")],
            })),
        },
    );

    let palette = model.palette.as_ref().expect("open");
    assert_eq!(palette.purpose, crate::app::palette::Purpose::Dependencies);
    assert_eq!(palette.entries.len(), 2);
    assert_eq!(palette.entries[0].group, "Used by");
    assert_eq!(palette.entries[1].group, "Depends on");
    assert!(palette.entries[0].label.contains("recent_orders"));
    assert!(
        palette
            .purpose
            .standing_note()
            .is_some_and(|note| note.contains("function body")),
        "what the answer cannot see is said where it is read"
    );

    // Choosing one puts its quoted, qualified name where SQL is written.
    model.editor.set_text("SELECT * FROM ");
    update(&mut model, Message::Action(Action::Activate));
    assert_eq!(
        model.editor.text(),
        "SELECT * FROM \"public\".\"recent_orders\""
    );
}

#[test]
fn a_dependency_answer_for_a_question_nobody_is_waiting_on_is_discarded() {
    let mut model = with_selected_table();
    let effects = update(&mut model, Message::Action(Action::ShowDependencies));
    let Effect::LoadDependencies { request, .. } = &effects[0] else {
        panic!("expected a dependency load");
    };
    let stale = *request;
    assert!(
        model.palette.as_ref().is_some_and(|p| p.loading),
        "the palette opens while it waits, as the definition panel does"
    );

    // Esc gives up on it, and the answer that arrives afterwards belongs to
    // nothing.
    update(&mut model, Message::Action(Action::Dismiss));
    assert!(model.palette.is_none());
    assert!(model.pending_dependencies.is_none());

    update(
        &mut model,
        Message::DependenciesLoaded {
            request: stale,
            result: Box::new(Ok(crate::postgres::metadata::Dependencies::default())),
        },
    );
    assert!(
        model.palette.is_none(),
        "an answer to a question nobody is waiting on opens nothing"
    );
}

#[test]
fn a_dependency_lookup_that_fails_says_so_where_errors_are_shown() {
    let mut model = with_selected_table();
    let effects = update(&mut model, Message::Action(Action::ShowDependencies));
    let Effect::LoadDependencies { request, .. } = &effects[0] else {
        panic!("expected a dependency load");
    };
    update(
        &mut model,
        Message::DependenciesLoaded {
            request: *request,
            result: Box::new(Err(Diagnostic::new(
                DiagnosticKind::Query,
                "permission denied for pg_depend",
                "reading dependencies",
            ))),
        },
    );
    assert!(model.palette.is_none());
    assert!(model.error.is_some(), "the failure is not silent");
}

#[test]
fn writing_the_rows_on_screen_says_how_many_that_is_and_what_it_is_not() {
    let mut model = with_rows(&["alpha", "beta", "gamma"]);
    update(&mut model, Message::Action(Action::ExportRows));
    let palette = model.palette.as_ref().expect("a format palette");
    assert_eq!(palette.purpose, crate::app::palette::Purpose::ExportFormats);
    assert_eq!(palette.entries.len(), 5);
    assert!(
        palette
            .context_note
            .as_deref()
            .is_some_and(|note| note.contains("3 row(s)"))
    );
    update(&mut model, Message::Action(Action::Activate));
    let prompt = model.name_prompt.as_ref().expect("a prompt");
    assert_eq!(prompt.purpose, crate::app::model::NamePurpose::ExportRows);
    assert_eq!(prompt.export_format, Some(ExportFormat::Csv));
    assert!(prompt.subject.contains("CSV"), "{}", prompt.subject);
    assert!(prompt.note.contains("3 row(s)"), "{}", prompt.note);
    assert!(
        !prompt.note.contains("not the"),
        "nothing was hidden, so nothing needs explaining: {}",
        prompt.note
    );

    // With a filter on, or a truncated result, the note says what the file
    // will not contain. That is the whole honesty of the feature.
    model.name_prompt = None;
    model.result_filter = "a".into();
    if let Some(execution) = model.last_execution.as_mut()
        && let Some(set) = execution.statements[0].result_set.as_mut()
    {
        set.rows_seen = 500;
        set.cap = 3;
    }
    update(&mut model, Message::Action(Action::ExportRows));
    update(&mut model, Message::Action(Action::Move(Direction::Down)));
    update(&mut model, Message::Action(Action::Move(Direction::Down)));
    update(&mut model, Message::Action(Action::Activate));
    let note = &model.name_prompt.as_ref().expect("a prompt").note;
    assert!(note.contains("what is on screen"), "{note}");
    assert!(note.contains("500"), "{note}");
    assert!(
        note.contains("--output"),
        "the way to get all of it: {note}"
    );

    for ch in "rows.csv".chars() {
        update(&mut model, Message::Action(Action::Insert(ch)));
    }
    let effects = update(&mut model, Message::Action(Action::Activate));
    match effects.as_slice() {
        [Effect::ExportRows { path, format }] => {
            assert_eq!(path, "rows.csv");
            assert_eq!(*format, ExportFormat::Json);
        }
        other => panic!("expected one export, got {other:?}"),
    }
}

#[test]
fn there_is_nothing_to_write_when_nothing_is_on_screen() {
    let mut model = connected();
    assert!(update(&mut model, Message::Action(Action::ExportRows)).is_empty());
    assert!(model.name_prompt.is_none());

    // A filter matching nothing is nothing on screen too.
    let mut model = with_rows(&["alpha"]);
    model.result_filter = "nothing".into();
    update(&mut model, Message::Action(Action::ExportRows));
    assert!(model.name_prompt.is_none());
}

#[test]
fn a_written_file_is_reported_where_the_user_is_looking() {
    let mut model = connected();
    update(
        &mut model,
        Message::RowsExported(Box::new(Ok("3 row(s) written to rows.csv".to_owned()))),
    );
    assert!(
        model
            .notices
            .iter()
            .any(|notice| notice.message.contains("rows.csv")),
        "{:?}",
        model.notices
    );

    update(
        &mut model,
        Message::RowsExported(Box::new(Err(Diagnostic::new(
            DiagnosticKind::Usage,
            "rows.csv already exists",
            "writing",
        )))),
    );
    assert!(model.error.is_some(), "a refusal is not silent");
}

#[test]
fn saving_the_buffer_asks_for_a_name_and_writes_what_is_in_it() {
    let mut model = connected();
    model.editor.set_text("SELECT count(*) FROM orders;");

    update(&mut model, Message::Action(Action::SaveQuery));
    let prompt = model.name_prompt.as_ref().expect("a prompt");
    assert!(
        prompt.typed.is_empty(),
        "nothing is suggested the first time"
    );
    assert!(prompt.subject.contains("Save"));

    for ch in "monthly".chars() {
        update(&mut model, Message::Action(Action::Insert(ch)));
    }
    assert_eq!(
        model.editor.text(),
        "SELECT count(*) FROM orders;",
        "typing a name never reaches the buffer being named"
    );

    let effects = update(&mut model, Message::Action(Action::Activate));
    match effects.as_slice() {
        [Effect::SaveQuery { name, sql }] => {
            assert_eq!(name, "monthly");
            assert_eq!(sql, "SELECT count(*) FROM orders;");
        }
        other => panic!("expected one save, got {other:?}"),
    }
    assert!(model.name_prompt.is_none());

    // What was saved is remembered, so saving again suggests the same name.
    update(&mut model, Message::Action(Action::SaveQuery));
    assert_eq!(model.name_prompt.as_ref().expect("prompt").typed, "monthly");
}

#[test]
fn an_empty_buffer_has_nothing_to_save_and_an_empty_name_saves_nothing() {
    let mut model = connected();
    model.editor.set_text("   \n  ");
    update(&mut model, Message::Action(Action::SaveQuery));
    assert!(model.name_prompt.is_none(), "there is nothing to name");

    model.editor.set_text("SELECT 1;");
    update(&mut model, Message::Action(Action::SaveQuery));
    assert!(
        update(&mut model, Message::Action(Action::Activate)).is_empty(),
        "a name of nothing is not a name"
    );
    assert!(
        model.name_prompt.is_some(),
        "so it is still being asked for"
    );

    update(&mut model, Message::Action(Action::Dismiss));
    assert!(model.name_prompt.is_none());
}

#[test]
fn a_saved_query_can_be_found_by_name_and_opened_into_the_buffer() {
    let mut model = connected();
    model.editor.set_text("SELECT 1;");

    let effects = update(&mut model, Message::Action(Action::OpenQuery));
    assert!(matches!(effects.as_slice(), [Effect::ListQueries]));

    update(
        &mut model,
        Message::QueriesListed(vec![
            crate::queries::SavedQuery {
                name: "monthly-revenue".into(),
                path: "/tmp/monthly-revenue.sql".into(),
                summary: "SELECT sum(total) FROM orders;".into(),
            },
            crate::queries::SavedQuery {
                name: "stale-sessions".into(),
                path: "/tmp/stale-sessions.sql".into(),
                summary: "SELECT * FROM pg_stat_activity;".into(),
            },
        ]),
    );
    let palette = model.palette.as_ref().expect("open");
    assert_eq!(palette.purpose, crate::app::palette::Purpose::SavedQueries);
    assert_eq!(palette.entries.len(), 2);

    for ch in "revenue".chars() {
        update(&mut model, Message::Action(Action::Insert(ch)));
    }
    let effects = update(&mut model, Message::Action(Action::Activate));
    match effects.as_slice() {
        [Effect::LoadQuery { name }] => assert_eq!(name, "monthly-revenue"),
        other => panic!("expected one load, got {other:?}"),
    }

    update(
        &mut model,
        Message::QueryLoaded(Box::new(Ok("SELECT sum(total) FROM orders;\n".to_owned()))),
    );
    assert_eq!(model.editor.text(), "SELECT sum(total) FROM orders;\n");
    assert_eq!(model.focus, Focus::Editor, "focus follows the text");
    assert!(
        !model.editor.is_modified(),
        "what was just loaded matches what is on disk"
    );

    // And it can be taken back, because loading is an edit like any other.
    model.editor.undo();
    assert_eq!(model.editor.text(), "SELECT 1;");
}

#[test]
fn a_query_that_could_not_be_written_or_read_says_so() {
    let mut model = connected();
    update(
        &mut model,
        Message::QuerySaved(Box::new(Err(Diagnostic::new(
            DiagnosticKind::Usage,
            "could not write /queries/x.sql",
            "saving",
        )))),
    );
    assert!(model.error.is_some());

    model.error = None;
    model.loaded_query = Some("gone".into());
    update(
        &mut model,
        Message::QueryLoaded(Box::new(Err(Diagnostic::new(
            DiagnosticKind::Usage,
            "could not read /queries/gone.sql",
            "opening",
        )))),
    );
    assert!(model.error.is_some());
    assert!(
        model.loaded_query.is_none(),
        "the buffer did not come from a file that could not be read"
    );
}

#[test]
fn a_server_asking_for_a_password_is_asked_back_rather_than_only_reported() {
    let mut model = Model::new(100);
    model.editor.set_text("SELECT 1;");
    update(
        &mut model,
        Message::ConnectionFailed(Box::new(Diagnostic::new(
            DiagnosticKind::Authentication,
            "the server requires a password for role \"app\"",
            "connecting",
        ))),
    );

    let prompt = model.password_prompt.as_ref().expect("a prompt");
    assert!(prompt.reason.contains("requires a password"));
    assert!(prompt.is_empty());
    assert!(
        model.error.is_some(),
        "the diagnostic is still there to read if the prompt is dismissed"
    );

    // Typing goes to the prompt and nowhere a character could be seen or run.
    for ch in "hunter2".chars() {
        update(&mut model, Message::Action(Action::Insert(ch)));
    }
    assert_eq!(model.editor.text(), "SELECT 1;");
    assert_eq!(model.password_prompt.as_ref().expect("open").length(), 7);
    update(&mut model, Message::Action(Action::Backspace));
    assert_eq!(model.password_prompt.as_ref().expect("open").length(), 6);

    // Nothing about it is printable, even by accident.
    let printed = format!("{:?}", model.password_prompt);
    assert!(!printed.contains("hunter"), "{printed}");
    assert!(printed.contains("<hidden>"), "{printed}");

    let effects = update(&mut model, Message::Action(Action::Activate));
    match effects.as_slice() {
        [Effect::Reconnect { password }] => {
            assert!(!format!("{password:?}").contains("hunter"));
            assert_eq!(password.clone().into_inner(), "hunter");
        }
        other => panic!("expected one reconnect, got {other:?}"),
    }
    assert!(
        model.password_prompt.is_none(),
        "the characters are gone from the model the moment they are sent"
    );
    assert_eq!(model.connection, ConnectionState::Connecting);
}

#[test]
fn an_empty_password_is_not_an_answer_and_the_prompt_can_be_abandoned() {
    let mut model = Model::new(100);
    update(
        &mut model,
        Message::ConnectionFailed(Box::new(Diagnostic::new(
            DiagnosticKind::Authentication,
            "password authentication failed",
            "connecting",
        ))),
    );

    assert!(
        update(&mut model, Message::Action(Action::Activate)).is_empty(),
        "sending nothing would earn the same refusal and look like a bug"
    );
    assert!(model.password_prompt.is_some());

    update(&mut model, Message::Action(Action::Dismiss));
    assert!(model.password_prompt.is_none());
    assert!(model.error.is_some(), "the reason is still readable");
}

#[test]
fn a_connection_failure_that_is_not_about_credentials_asks_nothing() {
    let mut model = Model::new(100);
    update(
        &mut model,
        Message::ConnectionFailed(Box::new(Diagnostic::new(
            DiagnosticKind::Connection,
            "could not reach db.example.net",
            "connecting",
        ))),
    );
    assert!(
        model.password_prompt.is_none(),
        "a password cannot fix a firewall"
    );
    assert!(model.error.is_some());
}

#[test]
fn where_the_tree_reads_from_is_recorded_and_said_in_words() {
    let mut model = connected();
    assert_eq!(model.metadata_link, crate::app::model::MetadataLink::Shared);

    update(
        &mut model,
        Message::MetadataConnection(crate::app::model::MetadataLink::Opening),
    );
    assert_eq!(model.metadata_link.label(), "opening its own connection");

    update(
        &mut model,
        Message::MetadataConnection(crate::app::model::MetadataLink::Dedicated),
    );
    assert_eq!(model.metadata_link.label(), "own connection");

    update(
        &mut model,
        Message::MetadataConnection(crate::app::model::MetadataLink::Unavailable(
            "too many connections for role".to_owned(),
        )),
    );
    assert!(
        model.metadata_link.label().contains("shared"),
        "falling back is stated rather than hidden: {}",
        model.metadata_link.label()
    );
}

#[test]
fn asking_what_an_object_is_loads_its_definition_and_shows_where_it_came_from() {
    let mut model = with_selected_table();
    model.size = (120, 40);

    let effects = update(&mut model, Message::Action(Action::ShowDefinition));
    let Effect::LoadDefinition { request, object } = &effects[0] else {
        panic!("expected a definition load, got {effects:?}");
    };
    assert_eq!(object.name, "orders");
    let request = *request;
    assert!(
        model
            .definition
            .as_ref()
            .is_some_and(|d| d.pending.is_some()),
        "the panel opens while it waits, rather than after"
    );

    update(
        &mut model,
        Message::DefinitionLoaded {
            request,
            result: Box::new(Ok(definition_of("orders"))),
        },
    );
    let open = model.definition.as_ref().expect("open");
    assert!(open.pending.is_none());
    assert!(open.heading.contains("orders"), "{}", open.heading);
    assert_eq!(open.lines().len(), 40);

    // Scrolling stops with the last line on screen, both ways.
    let height = crate::ui::layout::definition_viewport(model.size).1;
    for _ in 0..200 {
        update(&mut model, Message::Action(Action::Move(Direction::Down)));
    }
    assert_eq!(
        model.definition.as_ref().expect("open").scroll,
        40usize.saturating_sub(height)
    );
    for _ in 0..200 {
        update(&mut model, Message::Action(Action::Move(Direction::Up)));
    }
    assert_eq!(model.definition.as_ref().expect("open").scroll, 0);

    update(&mut model, Message::Action(Action::Dismiss));
    assert!(model.definition.is_none(), "Esc closes it");
}

#[test]
fn a_definition_that_arrives_late_for_a_closed_panel_is_discarded() {
    let mut model = with_selected_table();
    let effects = update(&mut model, Message::Action(Action::ShowDefinition));
    let Effect::LoadDefinition { request, .. } = &effects[0] else {
        panic!("expected a definition load");
    };
    let stale = *request;

    // Closed, then asked again: the first answer belongs to nothing.
    update(&mut model, Message::Action(Action::Dismiss));
    let effects = update(&mut model, Message::Action(Action::ShowDefinition));
    let Effect::LoadDefinition { request, .. } = &effects[0] else {
        panic!("expected a definition load");
    };
    assert_ne!(*request, stale);

    update(
        &mut model,
        Message::DefinitionLoaded {
            request: stale,
            result: Box::new(Ok(definition_of("stale"))),
        },
    );
    let open = model.definition.as_ref().expect("open");
    assert!(
        open.definition.is_none(),
        "a stale answer must not fill a panel that asked something else"
    );
    assert!(open.pending.is_some(), "it is still waiting for its own");
}

#[test]
fn a_definition_that_cannot_be_read_says_so_rather_than_showing_nothing() {
    let mut model = with_selected_table();
    let effects = update(&mut model, Message::Action(Action::ShowDefinition));
    let Effect::LoadDefinition { request, .. } = &effects[0] else {
        panic!("expected a definition load");
    };
    update(
        &mut model,
        Message::DefinitionLoaded {
            request: *request,
            result: Box::new(Err(Diagnostic::new(
                DiagnosticKind::Query,
                "permission denied for table orders",
                "reading an object definition",
            ))),
        },
    );
    let open = model.definition.as_ref().expect("open");
    assert!(
        open.error
            .as_deref()
            .is_some_and(|e| e.contains("permission")),
        "the reason belongs where the answer would have been"
    );
}

#[test]
fn asking_for_a_definition_of_something_that_has_none_does_nothing() {
    // A schema and a group are not objects. Opening an empty panel over them
    // would be worse than not opening one.
    let mut model = with_tree();
    model.focus = Focus::Objects;
    assert!(update(&mut model, Message::Action(Action::ShowDefinition)).is_empty());
    assert!(model.definition.is_none());
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
fn the_general_palette_is_contextual_and_uses_the_active_key_snapshot() {
    let mut model = connected();
    model.editor.set_text("SELECT 1;");
    let mut keys = std::collections::BTreeMap::new();
    keys.insert(
        "run-buffer".to_owned(),
        crate::config::schema::KeySpec::One("f2".to_owned()),
    );
    let keymap = crate::ui::keymap::Keymap::from_config(&keys).expect("valid keymap");
    model.keymap_snapshot = keymap.snapshot();

    let effects = update(&mut model, Message::Action(Action::OpenPalette));
    assert!(effects.is_empty(), "opening is presentation-only");
    let palette = model.palette.as_ref().expect("open");
    assert_eq!(palette.purpose, crate::app::palette::Purpose::Commands);
    let run = palette
        .entries
        .iter()
        .find(|entry| entry.label == "Run the whole buffer")
        .expect("run command");
    assert_eq!(run.detail, "F2");
    assert_eq!(run.group, "Editor");
    assert!(
        palette
            .context_note
            .as_deref()
            .is_some_and(|note| note.contains("Focus: Editor"))
    );

    for ch in "run".chars() {
        assert!(update(&mut model, Message::Action(Action::Insert(ch))).is_empty());
    }
    assert_eq!(
        model
            .palette
            .as_ref()
            .expect("still open")
            .selected_entry()
            .expect("run match")
            .detail,
        "F2"
    );
}

#[test]
fn unavailable_palette_searches_explain_their_prerequisite() {
    let mut disconnected = Model::new(100);
    update(&mut disconnected, Message::Action(Action::OpenPalette));
    let palette = disconnected.palette.as_mut().expect("open");
    for ch in "run".chars() {
        palette.push(ch);
    }
    assert!(
        !palette.matches().iter().any(|entry| {
            entry.label == "Run the whole buffer"
                || entry.label == "Run the statement at the cursor"
        }),
        "run actions are not actionable yet"
    );
    assert!(
        palette
            .context_note
            .as_deref()
            .is_some_and(|note| note.contains("Connect before running SQL")),
        "the missing prerequisite is visible"
    );

    let mut no_rows = connected();
    no_rows.focus = Focus::Results;
    update(&mut no_rows, Message::Action(Action::OpenPalette));
    let palette = no_rows.palette.as_mut().expect("open");
    for ch in "inspect".chars() {
        palette.push(ch);
    }
    let entry = palette.selected_entry().expect("inspect explanation");
    assert!(entry.detail.contains("retained result rows"), "{entry:?}");
}

#[test]
fn opening_searching_and_dismissing_discovery_has_no_model_side_effect() {
    let mut model = connected();
    model.editor.set_text("SELECT 1;");
    let before = (
        model.editor.text().to_owned(),
        model.focus,
        model.selected_row,
        model.selected_column,
        model.connection.clone(),
        model.history.clone(),
        model.last_execution.clone(),
    );
    assert!(update(&mut model, Message::Action(Action::OpenPalette)).is_empty());
    for ch in "result".chars() {
        assert!(update(&mut model, Message::Action(Action::Insert(ch))).is_empty());
    }
    assert!(update(&mut model, Message::Action(Action::Dismiss)).is_empty());
    assert_eq!(model.editor.text(), before.0);
    assert_eq!(model.focus, before.1);
    assert_eq!(model.selected_row, before.2);
    assert_eq!(model.selected_column, before.3);
    assert_eq!(model.connection, before.4);
    assert_eq!(model.history, before.5);
    assert_eq!(model.last_execution, before.6);
}

#[test]
fn connection_details_are_reachable_by_plain_language_and_are_read_only() {
    let mut model = connected();
    model.editor.set_text("SELECT 1");
    update(&mut model, Message::Action(Action::OpenPalette));

    for ch in "connection".chars() {
        update(&mut model, Message::Action(Action::Insert(ch)));
    }
    let palette = model.palette.as_ref().expect("palette open");
    assert_eq!(
        palette
            .matches()
            .first()
            .expect("connection command match")
            .label,
        "Connection and auth details"
    );

    update(&mut model, Message::Action(Action::Activate));
    assert!(model.connection_details, "the trust surface opened");
    assert!(model.palette.is_none(), "the palette peeled away");

    // Inspection must not leak input to the editor or alter selection.
    let editor = model.editor.text().to_owned();
    let selected = (model.selected_row, model.selected_column);
    for action in [
        Action::Insert('D'),
        Action::Backspace,
        Action::Move(Direction::Down),
        Action::RunBuffer,
    ] {
        assert!(
            update(&mut model, Message::Action(action.clone())).is_empty(),
            "a read-only panel emitted an effect for {action:?}"
        );
    }
    assert_eq!(model.editor.text(), editor);
    assert_eq!((model.selected_row, model.selected_column), selected);

    update(&mut model, Message::Action(Action::Dismiss));
    assert!(!model.connection_details, "Escape closes only the panel");
    assert_eq!(model.editor.text(), "SELECT 1");
}

/// A model holding a result of several rows, focused on it.
fn with_rows(rows: &[&str]) -> Model {
    let mut model = connected();
    model.size = (120, 40);
    model.editor.set_text("SELECT 1;");
    let effects = update(&mut model, Message::Action(Action::RunBuffer));
    let Effect::Execute { job, .. } = effects[0] else {
        panic!("expected an execute effect");
    };
    update(
        &mut model,
        Message::ExecutionFinished(execution(job, ExecutionStatus::Succeeded, rows)),
    );
    model.focus = Focus::Results;
    model
}

fn with_grid(columns: &[&str], rows: &[&[Cell]]) -> Model {
    let mut model = connected();
    model.size = (120, 40);
    let mut set = ResultSet::new(columns.iter().map(|c| (*c).to_owned()).collect(), 100);
    for row in rows {
        set.push((*row).to_vec());
    }
    let signature = set.columns.clone();
    model.last_execution = Some(Execution {
        job: JobId(1),
        statements: vec![StatementResult {
            result_set: Some(set),
            rows_affected: Some(rows.len() as u64),
            elapsed: Duration::from_millis(5),
            notices: Vec::new(),
        }],
        status: ExecutionStatus::Succeeded,
        elapsed: Duration::from_millis(6),
        error: None,
        transaction: crate::query::result::TransactionState::Autocommit,
    });
    model.result_grid.reset_for_result(&signature);
    model.focus = Focus::Results;
    model
}

#[test]
fn grid_sort_is_local_keeps_the_selected_record_and_emits_no_effect() {
    let mut model = with_grid(
        &["id", "amount"],
        &[
            &[Cell::Text("first".into()), Cell::Text("30".into())],
            &[Cell::Text("second".into()), Cell::Text("10".into())],
            &[Cell::Text("third".into()), Cell::Null],
        ],
    );
    model.selected_column = 1;
    model.selected_row = 0;
    assert_eq!(model.selected_source_row(), Some(0));

    for expected in [
        (
            Some(crate::app::grid::SortDirection::Ascending),
            vec![1, 0, 2],
        ),
        (
            Some(crate::app::grid::SortDirection::Descending),
            vec![0, 1, 2],
        ),
        (None, vec![0, 1, 2]),
    ] {
        assert!(
            update(&mut model, Message::Action(Action::OpenResultControls)).is_empty(),
            "view controls must not execute SQL"
        );
        let palette = model.palette.as_mut().expect("grid controls");
        palette.query = "sort".into();
        assert!(
            update(&mut model, Message::Action(Action::Activate)).is_empty(),
            "sorting is a view transition"
        );
        assert_eq!(
            model.result_grid.sort.map(|sort| sort.direction),
            expected.0
        );
        assert_eq!(model.displayed_rows(), expected.1);
        assert_eq!(model.selected_source_row(), Some(0));
    }

    let set = model.visible_result().expect("result");
    assert_eq!(set.rows[0][1], Cell::Text("30".into()));
}

#[test]
fn the_column_chooser_uses_source_positions_and_keeps_one_column_visible() {
    let mut model = with_grid(
        &["id", "name", "name"],
        &[&[
            Cell::Text("1".into()),
            Cell::Text("left".into()),
            Cell::Text("right".into()),
        ]],
    );

    update(&mut model, Message::Action(Action::OpenResultControls));
    model.palette.as_mut().expect("controls").query = "visible".into();
    update(&mut model, Message::Action(Action::Activate));
    assert_eq!(
        model.palette.as_ref().expect("column chooser").purpose,
        crate::app::palette::Purpose::ResultColumns
    );
    let labels: Vec<_> = model
        .palette
        .as_ref()
        .expect("column chooser")
        .entries
        .iter()
        .map(|entry| entry.label.as_str())
        .collect();
    assert_eq!(
        labels,
        vec!["Column 1: id", "Column 2: name", "Column 3: name"]
    );

    let palette = model.palette.as_mut().expect("column chooser");
    palette.query = "Column 2".into();
    update(&mut model, Message::Action(Action::Activate));
    assert_eq!(model.result_grid.visible_columns(3), vec![0, 2]);
    assert_eq!(
        model.selected_column, 0,
        "hiding an unselected column leaves selection"
    );

    update(&mut model, Message::Action(Action::OpenResultControls));
    model.palette.as_mut().expect("controls").query = "visible".into();
    update(&mut model, Message::Action(Action::Activate));
    model.palette.as_mut().expect("column chooser").query = "Column 3".into();
    update(&mut model, Message::Action(Action::Activate));
    assert_eq!(model.result_grid.visible_columns(3), vec![0]);

    update(&mut model, Message::Action(Action::OpenResultControls));
    model.palette.as_mut().expect("controls").query = "visible".into();
    update(&mut model, Message::Action(Action::Activate));
    model.palette.as_mut().expect("column chooser").query = "Column 1".into();
    update(&mut model, Message::Action(Action::Activate));
    assert_eq!(model.result_grid.visible_columns(3), vec![0]);
    assert!(
        model
            .result_grid
            .note
            .as_deref()
            .is_some_and(|note| note.contains("one result column")),
        "the final visible column cannot be hidden"
    );

    // The duplicate `name` entries remain distinct and the hidden ones can
    // be restored by their source position.
    update(&mut model, Message::Action(Action::OpenResultControls));
    model.palette.as_mut().expect("controls").query = "visible".into();
    update(&mut model, Message::Action(Action::Activate));
    model.palette.as_mut().expect("column chooser").query = "Column 2".into();
    update(&mut model, Message::Action(Action::Activate));
    assert_eq!(model.result_grid.visible_columns(3), vec![0, 1]);
    assert_eq!(
        model.selected_column, 1,
        "restoring selects that source column"
    );

    update(&mut model, Message::Action(Action::OpenResultControls));
    model.palette.as_mut().expect("controls").query = "visible".into();
    update(&mut model, Message::Action(Action::Activate));
    model.palette.as_mut().expect("column chooser").query = "Column 3".into();
    update(&mut model, Message::Action(Action::Activate));
    assert_eq!(model.result_grid.visible_columns(3), vec![0, 1, 2]);
    assert_eq!(
        model.selected_column, 2,
        "restoring selects the other duplicate"
    );
}

#[test]
fn grid_controls_reset_and_new_results_clear_shape_but_keep_preferences() {
    let mut model = with_grid(
        &["id", "label"],
        &[
            &[Cell::Text("1".into()), Cell::Text("one".into())],
            &[Cell::Text("2".into()), Cell::Text("two".into())],
        ],
    );
    model.result_grid.show_types = false;
    model.result_grid.freeze_first = true;
    model.result_grid.toggle_sort(1);
    model.result_grid.toggle_column(0, 2);
    let _ = model.result_grid.adjust_width(1, 10, 4);
    model.result_grid.horizontal_start = 1;

    update(&mut model, Message::Action(Action::OpenResultControls));
    model.palette.as_mut().expect("controls").query = "reset".into();
    assert!(update(&mut model, Message::Action(Action::Activate)).is_empty());
    assert!(model.result_grid.sort.is_none());
    assert!(model.result_grid.hidden_columns.is_empty());
    assert!(model.result_grid.widths.is_empty());
    assert_eq!(model.result_grid.horizontal_start, 0);
    assert!(!model.result_grid.show_types);
    assert!(model.result_grid.freeze_first);

    model.editor.set_text("SELECT 2;");
    let effects = update(&mut model, Message::Action(Action::RunBuffer));
    let Effect::Execute { job, .. } = effects[0] else {
        panic!("expected execute effect");
    };
    update(
        &mut model,
        Message::ExecutionFinished(execution(job, ExecutionStatus::Succeeded, &["new"])),
    );
    assert!(model.result_grid.sort.is_none());
    assert!(model.result_grid.hidden_columns.is_empty());
    assert!(model.result_grid.widths.is_empty());
    assert!(!model.result_grid.show_types);
    assert!(model.result_grid.freeze_first);
    assert_eq!(model.result_grid.columns_signature, vec!["value"]);
}

#[test]
fn result_grid_controls_are_safe_without_rows_or_without_results_focus() {
    let mut model = connected();
    model.focus = Focus::Results;
    assert!(update(&mut model, Message::Action(Action::OpenResultControls)).is_empty());
    assert!(model.palette.is_none());
    assert!(
        model
            .result_grid
            .note
            .as_deref()
            .is_some_and(|note| note.contains("Run a query"))
    );

    let mut model = with_rows(&["one"]);
    model.focus = Focus::Editor;
    update(&mut model, Message::Action(Action::OpenResultControls));
    assert!(model.palette.is_none());
    assert!(
        model
            .result_grid
            .note
            .as_deref()
            .is_some_and(|note| note.contains("Focus Results"))
    );
}

#[test]
fn the_filter_narrows_the_rows_and_the_selection_follows_the_real_row() {
    let mut model = with_rows(&["alpha", "beta", "gamma", "delta"]);
    update(&mut model, Message::Action(Action::StartFilter));
    assert!(model.result_filtering, "the filter key belongs to the pane");
    assert_eq!(model.focus, Focus::Results, "and does not move focus");

    for ch in "ta".chars() {
        update(&mut model, Message::Action(Action::Insert(ch)));
    }
    assert_eq!(
        model.filtered_rows(),
        vec![1, 3],
        "beta and delta, by their own positions in the result"
    );
    assert_eq!(model.selected_source_row(), Some(1));

    update(&mut model, Message::Action(Action::Move(Direction::Down)));
    assert_eq!(model.selected_row, 1, "the second match");
    assert_eq!(
        model.selected_source_row(),
        Some(3),
        "which is the fourth row of the result"
    );
    update(&mut model, Message::Action(Action::Move(Direction::Down)));
    assert_eq!(model.selected_row, 1, "and there is no third match");

    // Enter keeps the filter and returns to the rows; Esc clears it.
    update(&mut model, Message::Action(Action::Activate));
    assert!(!model.result_filtering);
    assert_eq!(model.result_filter, "ta");
    update(&mut model, Message::Action(Action::StartFilter));
    update(&mut model, Message::Action(Action::Dismiss));
    assert!(model.result_filter.is_empty());
    assert_eq!(model.filtered_rows().len(), 4);
}

#[test]
fn the_filter_is_typed_into_and_never_into_the_editor() {
    let mut model = with_rows(&["alpha", "beta"]);
    model.editor.set_text("SELECT 1;");
    update(&mut model, Message::Action(Action::StartFilter));
    for ch in "beta".chars() {
        update(&mut model, Message::Action(Action::Insert(ch)));
    }
    assert_eq!(model.result_filter, "beta");
    assert_eq!(model.editor.text(), "SELECT 1;");
    update(&mut model, Message::Action(Action::Backspace));
    assert_eq!(model.result_filter, "bet");
}

#[test]
fn the_filter_matches_any_column_and_ignores_case() {
    let mut model = with_rows(&["Alpha", "beta"]);
    model.result_filter = "ALP".into();
    assert_eq!(model.filtered_rows(), vec![0]);
    model.result_filter = "nothing here".into();
    assert!(model.filtered_rows().is_empty());
}

#[test]
fn a_filter_belongs_to_the_result_it_was_typed_against() {
    let mut model = with_rows(&["alpha", "beta"]);
    model.result_filter = "alpha".into();
    model.result_filtering = true;

    model.editor.set_text("SELECT 2;");
    let effects = update(&mut model, Message::Action(Action::RunBuffer));
    let Effect::Execute { job, .. } = effects[0] else {
        panic!("expected an execute effect");
    };
    update(
        &mut model,
        Message::ExecutionFinished(execution(job, ExecutionStatus::Succeeded, &["gamma"])),
    );
    assert!(
        model.result_filter.is_empty(),
        "carrying it over would hide rows nobody filtered"
    );
    assert!(!model.result_filtering);
    assert_eq!(model.filtered_rows(), vec![0]);
}

#[test]
fn the_count_says_what_was_searched_and_what_was_never_received() {
    // The filter can only ever search what is here. Saying "3 rows" when a
    // result was truncated is three numbers collapsed into the wrong one.
    let mut model = with_rows(&["alpha", "beta", "gamma"]);
    assert_eq!(model.result_window_label(), "3 rows");

    model.result_filter = "a".into();
    assert_eq!(model.result_window_label(), "matching 3 of 3 rows");

    if let Some(execution) = model.last_execution.as_mut()
        && let Some(set) = execution.statements[0].result_set.as_mut()
    {
        set.rows_seen = 5000;
        set.cap = 3;
    }
    let label = model.result_window_label();
    assert!(label.contains("matching 3 of 3 retained rows"), "{label}");
    assert!(label.contains("5000 returned"), "{label}");
    assert!(label.contains("limit 3 reached"), "{label}");
}

#[test]
fn the_inspector_opens_on_the_row_the_filter_is_pointing_at() {
    let mut model = with_rows(&["alpha", "beta", "gamma"]);
    model.result_filter = "gamma".into();
    assert_eq!(model.selected_source_row(), Some(2));
    update(&mut model, Message::Action(Action::ToggleInspector));
    assert!(model.inspector.is_some());

    // A filter matching nothing has nothing to inspect.
    model.inspector = None;
    model.result_filter = "nothing".into();
    update(&mut model, Message::Action(Action::ToggleInspector));
    assert!(model.inspector.is_none());
}

#[test]
fn a_finished_statement_is_offered_to_the_history_with_its_outcome() {
    let mut model = connected();
    model.editor.set_text("SELECT 1;");
    let effects = update(&mut model, Message::Action(Action::RunBuffer));
    let Effect::Execute { job, .. } = effects[0] else {
        panic!("expected an execute effect");
    };

    let effects = update(
        &mut model,
        Message::ExecutionFinished(execution(job, ExecutionStatus::Failed, &[])),
    );
    match effects.as_slice() {
        [Effect::RecordHistory { sql, outcome, .. }] => {
            assert_eq!(sql, "SELECT 1;");
            assert_eq!(
                *outcome,
                crate::history::Outcome::Failed,
                "a statement is recorded with what really happened to it"
            );
        }
        other => panic!("expected one record effect, got {other:?}"),
    }
    assert!(
        model.running_sql.is_none(),
        "the statement is recorded once, not on every later message"
    );
}

#[test]
fn a_paused_or_disabled_history_asks_for_no_recording_at_all() {
    for (paused, disabled) in [(true, false), (false, true), (true, true)] {
        let mut model = connected();
        model.history_paused = paused;
        model.history_disabled = disabled;
        model.editor.set_text("SELECT 1;");
        let effects = update(&mut model, Message::Action(Action::RunBuffer));
        let Effect::Execute { job, .. } = effects[0] else {
            panic!("expected an execute effect");
        };
        let effects = update(
            &mut model,
            Message::ExecutionFinished(execution(job, ExecutionStatus::Succeeded, &[])),
        );
        assert!(
            effects.is_empty(),
            "paused={paused} disabled={disabled} still asked to record"
        );
        assert!(!model.records_history());
    }
}

#[test]
fn a_private_session_can_be_turned_on_and_off_but_configuration_wins() {
    let mut model = connected();
    assert!(model.records_history());
    update(&mut model, Message::Action(Action::ToggleHistoryRecording));
    assert!(model.history_paused, "the session stops recording");
    update(&mut model, Message::Action(Action::ToggleHistoryRecording));
    assert!(!model.history_paused);

    model.history_disabled = true;
    update(&mut model, Message::Action(Action::ToggleHistoryRecording));
    assert!(
        !model.records_history(),
        "a session cannot switch on what the user switched off in their own file"
    );
}

#[test]
fn only_what_reached_the_file_comes_back_as_history() {
    let mut model = connected();
    update(
        &mut model,
        Message::HistoryLoaded(vec![history_entry("SELECT 1")]),
    );
    assert_eq!(model.history.len(), 1);

    // A statement the history refused is reported as nothing kept, and the
    // reason is carried so the interface can say it.
    update(
        &mut model,
        Message::HistoryRecorded {
            entry: Box::new(None),
            recorded: crate::history::Recorded::LooksLikeACredential,
        },
    );
    assert_eq!(model.history.len(), 1, "nothing was added");
    assert!(
        model
            .history_note
            .is_some_and(|note| note.contains("credential")),
        "a silently missing entry would be a mystery"
    );

    update(
        &mut model,
        Message::HistoryRecorded {
            entry: Box::new(Some(history_entry("SELECT 2"))),
            recorded: crate::history::Recorded::Written,
        },
    );
    assert_eq!(model.history.len(), 2);
    assert_eq!(model.history[0].sql, "SELECT 2", "newest first");
    assert!(model.history_note.is_none(), "the note is not sticky");

    // The in-memory list is bounded whether entries arrive at load or one
    // at a time, so a long session cannot walk past the limit.
    for index in 0..crate::history::IN_MEMORY_LIMIT * 2 {
        update(
            &mut model,
            Message::HistoryRecorded {
                entry: Box::new(Some(history_entry(&format!("SELECT {index}")))),
                recorded: crate::history::Recorded::Written,
            },
        );
    }
    assert_eq!(model.history.len(), crate::history::IN_MEMORY_LIMIT);
}

#[test]
fn the_history_search_puts_a_past_statement_in_the_editor() {
    let mut model = connected();
    update(
        &mut model,
        Message::HistoryLoaded(vec![
            history_entry("SELECT count(*) FROM orders"),
            history_entry("VACUUM ANALYZE orders"),
        ]),
    );
    model.editor.set_text("");

    update(&mut model, Message::Action(Action::OpenHistory));
    let palette = model.palette.as_ref().expect("open");
    assert_eq!(palette.purpose, crate::app::palette::Purpose::History);
    assert_eq!(palette.entries.len(), 2);

    for ch in "count".chars() {
        update(&mut model, Message::Action(Action::Insert(ch)));
    }
    update(&mut model, Message::Action(Action::Activate));
    assert!(model.palette.is_none());
    assert!(
        model.editor.text().contains("count(*)"),
        "the statement came back: {}",
        model.editor.text()
    );
}

#[test]
fn every_chord_is_reachable_by_name_from_the_palette() {
    // A two-key chord is fast once you know it and invisible until then.
    // The palette is how it is found the first time.
    let model = with_tree();
    let entries = palette_entries(&model);
    for (_, action, description) in crate::ui::keymap::CHORDS {
        assert!(
            entries
                .iter()
                .any(|entry| entry.command
                    == crate::app::palette::PaletteCommand::Run(action.clone())),
            "{description} cannot be found by name"
        );
    }

    let mut model = with_long_value();
    update(&mut model, Message::Action(Action::OpenPalette));
    for ch in "expandrow".chars() {
        update(&mut model, Message::Action(Action::Insert(ch)));
    }
    update(&mut model, Message::Action(Action::Activate));
    assert!(model.expanded_row, "the chosen command actually ran");
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

/// A model holding a result whose one row is wider than any pane.
fn with_long_value() -> Model {
    let mut model = connected();
    model.size = (120, 40);
    model.editor.set_text("SELECT 1;");
    let effects = update(&mut model, Message::Action(Action::RunBuffer));
    let Effect::Execute { job, .. } = effects[0] else {
        panic!("expected an execute effect");
    };

    let mut set = ResultSet::new(vec!["id".into(), "document".into()], 100);
    set.push(vec![Cell::Text("1".into()), Cell::Text("x".repeat(5000))]);
    let mut execution = execution(job, ExecutionStatus::Succeeded, &[]);
    execution.statements[0].result_set = Some(set);
    update(&mut model, Message::ExecutionFinished(execution));
    model.focus = Focus::Results;
    model
}

#[test]
fn enter_on_a_result_opens_the_inspector_and_closes_it_again() {
    let mut model = with_long_value();
    update(&mut model, Message::Action(Action::Activate));
    assert!(model.inspector.is_some(), "Enter on a cell inspects it");
    update(&mut model, Message::Action(Action::Activate));
    assert!(model.inspector.is_none());

    // The chord reaches it from anywhere, not only from the results pane.
    model.focus = Focus::Editor;
    update(&mut model, Message::Action(Action::ToggleInspector));
    assert!(model.inspector.is_some());
}

#[test]
fn the_inspector_does_not_open_onto_a_result_with_no_rows() {
    let mut model = connected();
    model.focus = Focus::Results;
    // No execution at all.
    update(&mut model, Message::Action(Action::ToggleInspector));
    assert!(model.inspector.is_none());

    // An execution that returned no rows.
    model.editor.set_text("SELECT 1;");
    let effects = update(&mut model, Message::Action(Action::RunBuffer));
    let Effect::Execute { job, .. } = effects[0] else {
        panic!("expected an execute effect");
    };
    update(
        &mut model,
        Message::ExecutionFinished(execution(job, ExecutionStatus::Succeeded, &[])),
    );
    update(&mut model, Message::Action(Action::ToggleInspector));
    assert!(
        model.inspector.is_none(),
        "a window onto nothing is worse than no window"
    );
}

#[test]
fn the_inspector_scrolls_within_the_value_and_stops_at_both_ends() {
    let mut model = with_long_value();
    model.selected_column = 1;
    update(&mut model, Message::Action(Action::ToggleInspector));
    let inspector = model.inspector.expect("open");
    assert_eq!(inspector.scroll(), 0);

    update(&mut model, Message::Action(Action::Move(Direction::Down)));
    assert_eq!(model.inspector.expect("open").scroll(), 1);

    for _ in 0..500 {
        update(&mut model, Message::Action(Action::Move(Direction::Down)));
    }
    let bottom = model.inspector.expect("open").scroll();
    update(&mut model, Message::Action(Action::Move(Direction::Down)));
    assert_eq!(
        model.inspector.expect("open").scroll(),
        bottom,
        "scrolling stops with the last line on screen"
    );

    for _ in 0..1000 {
        update(&mut model, Message::Action(Action::Move(Direction::Up)));
    }
    assert_eq!(model.inspector.expect("open").scroll(), 0);
}

#[test]
fn moving_sideways_in_the_inspector_reads_the_next_column_of_the_same_row() {
    let mut model = with_long_value();
    model.selected_column = 1;
    update(&mut model, Message::Action(Action::ToggleInspector));
    update(&mut model, Message::Action(Action::Move(Direction::Down)));
    assert_eq!(model.inspector.expect("open").scroll(), 1);

    update(&mut model, Message::Action(Action::Move(Direction::Left)));
    assert!(model.inspector.is_some(), "it stays open");
    assert_eq!(model.selected_column, 0);
    assert_eq!(
        model.inspector.expect("open").scroll(),
        0,
        "a different value starts at its beginning"
    );

    // The row does not move, and the last column is the last column.
    update(&mut model, Message::Action(Action::Move(Direction::Right)));
    update(&mut model, Message::Action(Action::Move(Direction::Right)));
    assert_eq!(model.selected_column, 1);
    assert_eq!(model.selected_row, 0);
    assert_eq!(model.focus, Focus::Results);
}

#[test]
fn the_inspector_closes_when_the_result_it_was_reading_is_replaced() {
    let mut model = with_long_value();
    update(&mut model, Message::Action(Action::ToggleInspector));
    assert!(model.inspector.is_some());

    model.editor.set_text("SELECT 2;");
    let effects = update(&mut model, Message::Action(Action::RunBuffer));
    let Effect::Execute { job, .. } = effects[0] else {
        panic!("expected an execute effect");
    };
    update(
        &mut model,
        Message::ExecutionFinished(execution(job, ExecutionStatus::Succeeded, &["a"])),
    );
    assert!(
        model.inspector.is_none(),
        "showing a cell from a result that no longer exists would be a lie"
    );
}

#[test]
fn typing_while_the_inspector_is_open_never_reaches_the_editor() {
    let mut model = with_long_value();
    model.editor.set_text("SELECT 1;");
    update(&mut model, Message::Action(Action::ToggleInspector));
    for ch in "DROP".chars() {
        update(&mut model, Message::Action(Action::Insert(ch)));
    }
    update(&mut model, Message::Action(Action::Backspace));
    assert_eq!(model.editor.text(), "SELECT 1;");
    assert!(model.inspector.is_some());

    update(&mut model, Message::Action(Action::Dismiss));
    assert!(model.inspector.is_none(), "Esc closes it");
}

#[test]
fn a_key_that_means_something_else_closes_the_inspector_and_does_it() {
    // A modal that swallows every other key is a modal people get stuck
    // behind. Typing is swallowed; a bound action is not.
    let mut model = with_long_value();
    update(&mut model, Message::Action(Action::ToggleInspector));
    update(&mut model, Message::Action(Action::OpenPalette));
    assert!(model.inspector.is_none(), "one overlay at a time");
    update(&mut model, Message::Action(Action::Insert('o')));
    assert_eq!(model.palette.expect("open").query, "o");
    assert_eq!(model.editor.text(), "SELECT 1;", "and not into the editor");

    let mut model = with_long_value();
    update(&mut model, Message::Action(Action::ToggleInspector));
    let effects = update(&mut model, Message::Action(Action::RunBuffer));
    assert!(model.inspector.is_none());
    assert!(
        matches!(effects.as_slice(), [Effect::Execute { .. }]),
        "Ctrl+R still runs: {effects:?}"
    );
}

#[test]
fn the_expanded_row_view_is_a_preference_that_survives_the_next_query() {
    let mut model = with_long_value();
    update(&mut model, Message::Action(Action::ToggleExpandedRow));
    assert!(model.expanded_row);

    model.editor.set_text("SELECT 2;");
    let effects = update(&mut model, Message::Action(Action::RunBuffer));
    let Effect::Execute { job, .. } = effects[0] else {
        panic!("expected an execute effect");
    };
    update(
        &mut model,
        Message::ExecutionFinished(execution(job, ExecutionStatus::Succeeded, &["a"])),
    );
    assert!(
        model.expanded_row,
        "someone who turned it on wants it on for the next result too"
    );

    update(&mut model, Message::Action(Action::ToggleExpandedRow));
    assert!(!model.expanded_row);
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
fn a_valid_server_position_focuses_the_editor_without_editing_sql_or_history() {
    let mut model = connected();
    let sql = "SELECT 1;\nSELECT café FROM orders WHERE id = 0;";
    model.editor.set_text(sql);
    model.editor.set_cursor(0);
    let can_undo = model.editor.can_undo();
    let effects = update(&mut model, Message::Action(Action::RunBuffer));
    let job = match effects.as_slice() {
        [Effect::Execute { job, .. }] => *job,
        other => panic!("expected execute effect, got {other:?}"),
    };
    let diagnostic = Diagnostic::new(
        DiagnosticKind::Query,
        "syntax error at or near orders",
        "running statement 2",
    )
    .in_statement(2)
    .at_position(19);
    update(
        &mut model,
        Message::ExecutionFinished(failed_execution(job, diagnostic)),
    );

    let location = model.error_location.as_ref().expect("location mapped");
    assert_eq!(location.statement_number, 2);
    assert_eq!((location.line, location.column), (2, 19));
    assert_eq!(model.editor.cursor(), location.cursor);
    assert_eq!(model.focus, Focus::Editor);
    assert_eq!(model.editor.text(), sql);
    assert_eq!(model.editor.can_undo(), can_undo);
    assert!(model.error_location_note.is_none());
}

#[test]
fn formatting_is_local_one_undoable_edit_and_preserves_the_logical_cursor() {
    let mut model = Model::new(100);
    let source = "select o.id,o.total from orders o where o.total>0;";
    let source_cursor = source.find("total>0").expect("token") + "total".len();
    model.editor = crate::app::editor::Editor::with_text(source);
    model.editor.set_cursor(source_cursor);

    let effects = update(&mut model, Message::Action(Action::FormatBuffer));

    assert!(effects.is_empty(), "formatting must not create effects");
    assert_eq!(
        model.editor.text(),
        "select o.id,\n  o.total\nfrom orders o\nwhere o.total > 0;"
    );
    let formatted_cursor = model.editor.cursor();
    assert_eq!(
        model.format_notice,
        Some(crate::app::model::FormatNotice::Applied {
            before_lines: 1,
            after_lines: 4
        })
    );
    assert!(model.editor.can_undo());

    update(&mut model, Message::Action(Action::Undo));
    assert_eq!(model.editor.text(), source);
    assert_eq!(model.editor.cursor(), source_cursor);
    assert_ne!(formatted_cursor, source_cursor);
    assert!(model.format_notice.is_none());

    update(&mut model, Message::Action(Action::Redo));
    assert_eq!(
        model.editor.text(),
        "select o.id,\n  o.total\nfrom orders o\nwhere o.total > 0;"
    );
    assert_eq!(model.editor.cursor(), formatted_cursor);
}

#[test]
fn formatting_needs_no_connection_and_refuses_without_editing_unsafe_input() {
    let mut model = Model::new(100);
    model.editor = crate::app::editor::Editor::with_text("SELECT 'unfinished");
    let revision = model.editor.revision();
    let cursor = model.editor.cursor();

    assert!(update(&mut model, Message::Action(Action::FormatBuffer)).is_empty());
    assert_eq!(model.editor.text(), "SELECT 'unfinished");
    assert_eq!(model.editor.revision(), revision);
    assert_eq!(model.editor.cursor(), cursor);
    assert!(matches!(
        model.format_notice,
        Some(crate::app::model::FormatNotice::Refused { .. })
    ));
    assert!(
        model
            .format_notice
            .as_ref()
            .expect("format notice")
            .message()
            .contains("Close it")
    );

    let mut empty = Model::new(100);
    empty.editor = crate::app::editor::Editor::with_text(" \n\t");
    update(&mut empty, Message::Action(Action::FormatBuffer));
    assert_eq!(empty.editor.text(), " \n\t");
    assert_eq!(
        empty.format_notice,
        Some(crate::app::model::FormatNotice::Empty)
    );
    assert!(!empty.editor.can_undo());

    let mut comments = Model::new(100);
    comments.editor = crate::app::editor::Editor::with_text("-- only a note");
    update(&mut comments, Message::Action(Action::FormatBuffer));
    assert_eq!(comments.editor.text(), "-- only a note");
    assert_eq!(
        comments.format_notice,
        Some(crate::app::model::FormatNotice::Empty)
    );
}

#[test]
fn formatting_invalidates_a_visible_server_location_but_not_when_nothing_changes() {
    let mut model = Model::new(100);
    model.editor = crate::app::editor::Editor::with_text("SELECT 1 FROM t;");
    model.error_location = Some(crate::query::error_location::ErrorLocation {
        cursor: 7,
        token: None,
        line: 1,
        column: 8,
        statement_number: 1,
        character: 8,
    });
    update(&mut model, Message::Action(Action::FormatBuffer));
    assert!(model.error_location.is_none());
    assert!(
        model
            .error_location_note
            .as_deref()
            .is_some_and(|note| note.contains("previous submission"))
    );

    let mut unchanged = Model::new(100);
    unchanged.editor = crate::app::editor::Editor::with_text("SELECT 1\nFROM t;");
    let revision = unchanged.editor.revision();
    update(&mut unchanged, Message::Action(Action::FormatBuffer));
    assert_eq!(unchanged.editor.revision(), revision);
    assert_eq!(unchanged.editor.text(), "SELECT 1\nFROM t;");
    assert_eq!(
        unchanged.format_notice,
        Some(crate::app::model::FormatNotice::AlreadyFormatted)
    );
}

#[test]
fn run_statement_maps_back_to_its_original_buffer_ordinal() {
    let mut model = connected();
    let sql = "SELECT 1;\nSELECT café FROM orders;";
    model.editor.set_text(sql);
    model.editor.set_cursor(sql.len());
    let effects = update(&mut model, Message::Action(Action::RunStatement));
    let (job, sent) = match effects.as_slice() {
        [Effect::Execute { job, sql }] => (*job, sql.as_str()),
        other => panic!("expected execute effect, got {other:?}"),
    };
    assert_eq!(sent, "SELECT café FROM orders");
    let diagnostic = Diagnostic::new(
        DiagnosticKind::Query,
        "syntax error at or near orders",
        "running statement 1",
    )
    .in_statement(1)
    .at_position(19);
    update(
        &mut model,
        Message::ExecutionFinished(failed_execution(job, diagnostic)),
    );
    assert_eq!(
        model
            .error_location
            .as_ref()
            .map(|location| location.statement_number),
        Some(2)
    );
    assert_eq!(model.editor.text(), sql);
}

#[test]
fn missing_or_out_of_range_positions_leave_the_existing_caret_alone() {
    for diagnostic in [
        Diagnostic::new(
            DiagnosticKind::Query,
            "permission denied",
            "running statement 1",
        )
        .in_statement(1),
        Diagnostic::new(DiagnosticKind::Query, "bad position", "running statement 1")
            .in_statement(1)
            .at_position(999),
    ] {
        let mut model = connected();
        model.editor.set_text("SELECT 1;");
        model.editor.set_cursor(3);
        let before = model.editor.cursor();
        let effects = update(&mut model, Message::Action(Action::RunBuffer));
        let job = match effects.as_slice() {
            [Effect::Execute { job, .. }] => *job,
            other => panic!("expected execute effect, got {other:?}"),
        };
        update(
            &mut model,
            Message::ExecutionFinished(failed_execution(job, diagnostic)),
        );
        assert_eq!(model.editor.cursor(), before);
        assert!(model.error_location.is_none());
        assert!(
            model
                .error_location_note
                .as_deref()
                .is_some_and(|note| note.contains("position") || note.contains("reported"))
        );
    }
}

#[test]
fn an_error_from_a_changed_buffer_is_not_applied_to_the_new_text() {
    let mut model = connected();
    model.editor.set_text("SELECT 1;");
    let before = model.editor.cursor();
    let effects = update(&mut model, Message::Action(Action::RunBuffer));
    let job = match effects.as_slice() {
        [Effect::Execute { job, .. }] => *job,
        other => panic!("expected execute effect, got {other:?}"),
    };
    update(&mut model, Message::Action(Action::Insert('x')));
    assert_ne!(model.editor.revision(), 0);
    update(
        &mut model,
        Message::ExecutionFinished(failed_execution(
            job,
            Diagnostic::new(DiagnosticKind::Query, "bad syntax", "running statement 1")
                .in_statement(1)
                .at_position(1),
        )),
    );
    assert!(model.error_location.is_none());
    assert!(
        model
            .error_location_note
            .as_deref()
            .is_some_and(|note| note.contains("previous submission"))
    );
    assert_eq!(model.editor.cursor(), before + 1);
}

#[test]
fn editing_after_a_mapped_error_removes_its_marker_and_says_why() {
    let mut model = connected();
    model.editor.set_text("SELECT 1;");
    let effects = update(&mut model, Message::Action(Action::RunBuffer));
    let job = match effects.as_slice() {
        [Effect::Execute { job, .. }] => *job,
        other => panic!("expected execute effect, got {other:?}"),
    };
    update(
        &mut model,
        Message::ExecutionFinished(failed_execution(
            job,
            Diagnostic::new(DiagnosticKind::Query, "bad syntax", "running statement 1")
                .in_statement(1)
                .at_position(1),
        )),
    );
    assert!(model.error_location.is_some());
    update(&mut model, Message::Action(Action::Insert('x')));
    assert!(model.error_location.is_none());
    assert!(
        model
            .error_location_note
            .as_deref()
            .is_some_and(|note| note.contains("previous submission"))
    );
}

#[test]
fn a_new_run_clears_the_previous_error() {
    let mut model = connected();
    model.error = Some(Diagnostic::new(DiagnosticKind::Query, "old", "running"));
    model.editor.set_text("SELECT 1;");
    update(&mut model, Message::Action(Action::RunBuffer));
    assert!(model.error.is_none());
}

#[test]
fn copy_is_off_by_default_and_explains_the_opt_in_without_an_effect() {
    let mut model = copy_model(vec![vec![
        Cell::Text("1".into()),
        Cell::Text("secret".into()),
    ]]);

    let effects = update(&mut model, Message::Action(Action::CopyValue));

    assert!(effects.is_empty());
    assert!(model.pending_copy.is_none());
    assert_eq!(
        model.clipboard_notice,
        Some(crate::app::model::ClipboardNotice::Disabled)
    );
    assert!(
        model
            .clipboard_notice
            .as_ref()
            .expect("notice")
            .message()
            .contains("[clipboard] osc52 = true")
    );
}

#[test]
fn copy_confirmation_contains_only_identity_and_enter_creates_one_redacted_effect() {
    let mut model = copy_model(vec![vec![
        Cell::Text("1".into()),
        Cell::Text("secret".into()),
    ]]);
    model.clipboard_osc52 = true;

    assert!(update(&mut model, Message::Action(Action::CopyValue)).is_empty());
    let pending = model.pending_copy.expect("confirmation");
    assert_eq!(pending.result_job, JobId(7));
    assert_eq!(pending.source_row, 0);
    assert_eq!(pending.column, 1);
    assert_eq!(pending.bytes, 6);
    assert_eq!(pending.characters, 6);
    assert!(
        !format!("{model:?}").contains("secret"),
        "the model should not duplicate the raw value"
    );

    let effects = update(&mut model, Message::Action(Action::Activate));
    assert!(model.pending_copy.is_none());
    let [Effect::CopyValue { payload }] = effects.as_slice() else {
        panic!("expected one copy effect, got {effects:?}");
    };
    assert_eq!(payload.len(), 6);
    assert_eq!(payload.characters(), 6);
    assert!(!format!("{effects:?}").contains("secret"));
    assert_eq!(payload.clone().into_inner(), "secret");
}

#[test]
fn copy_cancel_sends_nothing_and_keeps_the_result_selection() {
    let mut model = copy_model(vec![vec![
        Cell::Text("1".into()),
        Cell::Text("value".into()),
    ]]);
    model.clipboard_osc52 = true;
    model.selected_row = 0;
    model.selected_column = 1;

    update(&mut model, Message::Action(Action::CopyValue));
    let effects = update(&mut model, Message::Action(Action::Cancel));

    assert!(effects.is_empty());
    assert!(model.pending_copy.is_none());
    assert_eq!((model.selected_row, model.selected_column), (0, 1));
    assert!(model.last_execution.is_some());
}

#[test]
fn copy_distinguishes_null_and_refuses_values_over_the_client_bound() {
    let mut null_model = copy_model(vec![vec![Cell::Text("1".into()), Cell::Null]]);
    null_model.clipboard_osc52 = true;
    assert!(update(&mut null_model, Message::Action(Action::CopyValue)).is_empty());
    assert_eq!(
        null_model.clipboard_notice,
        Some(crate::app::model::ClipboardNotice::Null)
    );

    let mut large_model = copy_model(vec![vec![
        Cell::Text("1".into()),
        Cell::Text("x".repeat(crate::clipboard::MAX_OSC52_BYTES + 1)),
    ]]);
    large_model.clipboard_osc52 = true;
    assert!(update(&mut large_model, Message::Action(Action::CopyValue)).is_empty());
    assert!(matches!(
        large_model.clipboard_notice,
        Some(crate::app::model::ClipboardNotice::TooLarge { bytes })
            if bytes == crate::clipboard::MAX_OSC52_BYTES + 1
    ));
    assert!(large_model.pending_copy.is_none());
}

#[test]
fn stale_copy_confirmation_sends_nothing_when_selection_or_result_changes() {
    let mut model = copy_model(vec![
        vec![Cell::Text("1".into()), Cell::Text("first".into())],
        vec![Cell::Text("2".into()), Cell::Text("second".into())],
    ]);
    model.clipboard_osc52 = true;
    update(&mut model, Message::Action(Action::CopyValue));
    model.selected_row = 1;

    let effects = update(&mut model, Message::Action(Action::Activate));

    assert!(effects.is_empty());
    assert_eq!(
        model.clipboard_notice,
        Some(crate::app::model::ClipboardNotice::Stale)
    );
}

#[test]
fn filtered_copy_keeps_the_retained_source_row_identity() {
    let mut model = copy_model(vec![
        vec![Cell::Text("1".into()), Cell::Text("first".into())],
        vec![Cell::Text("2".into()), Cell::Text("second".into())],
    ]);
    model.clipboard_osc52 = true;
    model.result_filter = "second".into();

    update(&mut model, Message::Action(Action::CopyValue));
    assert_eq!(
        model
            .pending_copy
            .as_ref()
            .map(|candidate| candidate.source_row),
        Some(1)
    );
    let effects = update(&mut model, Message::Action(Action::Activate));
    let [Effect::CopyValue { payload }] = effects.as_slice() else {
        panic!("expected a copy effect, got {effects:?}");
    };
    assert_eq!(payload.clone().into_inner(), "second");
}

#[test]
fn inspector_copy_uses_the_underlying_results_selection() {
    let mut model = copy_model(vec![vec![
        Cell::Text("1".into()),
        Cell::Text("from inspector".into()),
    ]]);
    model.clipboard_osc52 = true;
    model.inspector = Some(crate::app::inspect::Inspector::new());

    update(&mut model, Message::Action(Action::CopyValue));

    assert!(model.pending_copy.is_some());
    assert!(update(&mut model, Message::Action(Action::Insert('x'))).is_empty());
    assert!(model.pending_copy.is_some());
}

#[test]
fn a_successful_copy_reports_only_bytes_and_unconfirmed_acceptance() {
    let mut model = copy_model(vec![vec![Cell::Text("1".into()), Cell::Text("hé".into())]]);

    update(
        &mut model,
        Message::ClipboardSent {
            bytes: 3,
            characters: 2,
        },
    );

    let notice = model.clipboard_notice.expect("notice");
    assert_eq!(
        notice,
        crate::app::model::ClipboardNotice::Sent {
            bytes: 3,
            characters: 2
        }
    );
    assert!(notice.message().contains("acceptance is unconfirmed"));
    assert!(!notice.message().contains("hé"));
}

#[test]
fn cell_update_reads_metadata_then_reviews_and_sends_one_parameterized_effect() {
    let mut model = cell_update_model();
    let effects = update(&mut model, Message::Action(Action::GenerateCellUpdate));
    let [Effect::LoadUpdateTarget { request, relation }] = effects.as_slice() else {
        panic!("expected metadata lookup, got {effects:?}");
    };
    assert_eq!(relation.schema.as_deref(), Some("public"));
    assert_eq!(relation.relation, "orders");
    assert!(model.update_prompt.is_none());
    assert!(model.pending_update.is_none());

    update(
        &mut model,
        Message::UpdateTargetLoaded {
            request: *request,
            result: Box::new(Ok(update_relation())),
        },
    );
    assert!(model.update_prompt.is_some());

    for character in "new note".chars() {
        assert!(update(&mut model, Message::Action(Action::Insert(character))).is_empty());
    }
    assert!(update(&mut model, Message::Action(Action::Activate)).is_empty());
    let pending = model.pending_update.as_ref().expect("review");
    assert!(pending.plan.sql_template.contains(":__ignatius_new_value"));
    assert!(pending.plan.sql_template.contains("order_id"));
    let reviewed = pending.plan.bound_sql().expect("review statement");
    assert!(reviewed.contains("new note"));

    let effects = update(&mut model, Message::Action(Action::Activate));
    let [
        Effect::ExecuteParameterized {
            job,
            sql,
            parameters,
        },
    ] = effects.as_slice()
    else {
        panic!("expected one parameterized update, got {effects:?}");
    };
    assert_eq!(*job, JobId(1));
    assert_eq!(sql, &pending_template());
    assert_eq!(
        parameters.names(),
        &["__ignatius_new_value", "__ignatius_key_0"]
    );
    let independently_bound = crate::query::discover_parameters(sql)
        .expect("the emitted template is discoverable")
        .bind(sql, parameters)
        .expect("the emitted bindings complete the emitted template");
    assert_eq!(
        independently_bound, reviewed,
        "the statement that executes must be the statement that was reviewed"
    );
    assert!(model.pending_update.is_none());
    assert!(model.running_cell_update);
}

fn pending_template() -> String {
    "UPDATE \"public\".\"orders\" SET \"note\" = :__ignatius_new_value WHERE \"order_id\" = :__ignatius_key_0".into()
}

#[test]
fn cell_update_cancel_sends_nothing_and_production_is_refused_before_metadata() {
    let mut model = cell_update_model();
    let effects = update(&mut model, Message::Action(Action::GenerateCellUpdate));
    assert!(matches!(
        effects.as_slice(),
        [Effect::LoadUpdateTarget { .. }]
    ));
    let request = model.update_lookup.as_ref().expect("lookup").request;
    update(
        &mut model,
        Message::UpdateTargetLoaded {
            request,
            result: Box::new(Ok(update_relation())),
        },
    );
    update(&mut model, Message::Action(Action::Activate));
    assert!(model.pending_update.is_some());
    assert!(update(&mut model, Message::Action(Action::Dismiss)).is_empty());
    assert!(model.pending_update.is_none());
    assert_eq!(model.phase, QueryPhase::Idle);

    let mut production = cell_update_model();
    let mut info = *session();
    info.environment = Environment::Production;
    update(&mut production, Message::Connected(Box::new(info)));
    let effects = update(&mut production, Message::Action(Action::GenerateCellUpdate));
    assert!(effects.is_empty());
    assert!(production.update_lookup.is_none());
    assert!(
        production
            .cell_update_notice
            .as_deref()
            .is_some_and(|notice| notice.contains("production"))
    );
}

#[test]
fn cell_update_refuses_non_table_and_privilege_targets_before_value_input() {
    for (relation, expected) in [
        (
            {
                let mut relation = update_relation();
                relation.kind = crate::postgres::metadata::ObjectKind::View;
                relation
            },
            "ordinary or partitioned tables",
        ),
        (
            {
                let mut relation = update_relation();
                relation.readable = false;
                relation
            },
            "can no longer read",
        ),
        (
            {
                let mut relation = update_relation();
                relation.writable = false;
                relation
            },
            "cannot UPDATE",
        ),
    ] {
        let mut model = cell_update_model();
        let effects = update(&mut model, Message::Action(Action::GenerateCellUpdate));
        let request = match effects.as_slice() {
            [Effect::LoadUpdateTarget { request, .. }] => *request,
            other => panic!("expected metadata lookup, got {other:?}"),
        };
        update(
            &mut model,
            Message::UpdateTargetLoaded {
                request,
                result: Box::new(Ok(relation)),
            },
        );
        assert!(model.update_prompt.is_none());
        assert!(model.pending_update.is_none());
        assert!(
            model
                .cell_update_notice
                .as_deref()
                .is_some_and(|notice| notice.contains(expected)),
            "notice {:?} did not contain {expected:?}",
            model.cell_update_notice
        );
    }

    let mut read_only = cell_update_model();
    let mut info = *session();
    info.read_only = true;
    update(&mut read_only, Message::Connected(Box::new(info)));
    assert!(update(&mut read_only, Message::Action(Action::GenerateCellUpdate)).is_empty());
    assert!(read_only.update_lookup.is_none());
    assert!(read_only.update_prompt.is_none());
    assert!(
        read_only
            .cell_update_notice
            .as_deref()
            .is_some_and(|notice| notice.contains("read-only"))
    );
}

#[test]
fn a_late_update_metadata_response_cannot_replace_a_changed_selection() {
    let mut model = cell_update_model();
    let effects = update(&mut model, Message::Action(Action::GenerateCellUpdate));
    let request = match effects.as_slice() {
        [Effect::LoadUpdateTarget { request, .. }] => *request,
        other => panic!("expected lookup, got {other:?}"),
    };
    model.selected_row = 1;
    update(
        &mut model,
        Message::UpdateTargetLoaded {
            request,
            result: Box::new(Ok(update_relation())),
        },
    );
    assert!(model.update_prompt.is_none());
    assert!(model.pending_update.is_none());
    assert!(
        model
            .cell_update_notice
            .as_deref()
            .is_some_and(|notice| notice.contains("changed"))
    );
}

#[test]
fn a_generated_update_completion_keeps_the_snapshot_and_never_reruns_the_select() {
    let mut model = cell_update_model();
    let snapshot = model.last_execution.clone();
    let source_sql = model.last_sql.clone();
    let effects = update(&mut model, Message::Action(Action::GenerateCellUpdate));
    let request = match effects.as_slice() {
        [Effect::LoadUpdateTarget { request, .. }] => *request,
        other => panic!("expected lookup, got {other:?}"),
    };
    update(
        &mut model,
        Message::UpdateTargetLoaded {
            request,
            result: Box::new(Ok(update_relation())),
        },
    );
    update(&mut model, Message::Action(Action::Activate));
    let effects = update(&mut model, Message::Action(Action::Activate));
    let job = match effects.as_slice() {
        [Effect::ExecuteParameterized { job, .. }] => *job,
        other => panic!("expected generated update, got {other:?}"),
    };
    let execution = execution(job, ExecutionStatus::Succeeded, &[]);
    let effects = update(&mut model, Message::ExecutionFinished(execution));
    assert!(effects.iter().all(|effect| !matches!(
        effect,
        Effect::Execute { .. } | Effect::ExecuteParameterized { .. }
    )));
    assert_eq!(model.last_execution, snapshot);
    assert_eq!(model.last_sql, source_sql);
    assert!(model.visible_result().is_some());
    assert!(model.cell_update_notice.as_deref().is_some_and(|notice| {
        notice.contains("0 row(s) affected")
            && notice.contains("snapshot")
            && notice.contains("again")
    }));
}
