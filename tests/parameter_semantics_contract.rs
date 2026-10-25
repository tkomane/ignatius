//! Fresh string-semantics checks through the production session boundary.
//!
//! Every database and value is synthetic. Missing fixture access is an explicit
//! skip; these checks do not start, stop or modify shared database fixtures.

#![allow(clippy::print_stderr)]

use ignatius::config::Config;
use ignatius::connection::{ConnectionArgs, EnvSnapshot, resolve};
use ignatius::diagnostics::DiagnosticKind;
use ignatius::postgres::{Session, StreamEvent, StreamStop, session};
use ignatius::query::ParameterBindings;
use ignatius::query::result::{Execution, ExecutionStatus, JobId, TransactionState};
use ignatius::query::value::Cell;
use secrecy::SecretString;
use std::time::Duration;

async fn fixture() -> Option<Session> {
    let Ok(uri) = std::env::var("IGNATIUS_TEST_PG_URI") else {
        eprintln!("skipping: IGNATIUS_TEST_PG_URI is not set");
        return None;
    };
    if uri.is_empty() {
        eprintln!("skipping: IGNATIUS_TEST_PG_URI is empty");
        return None;
    }
    let target = resolve(
        Some(&uri),
        &ConnectionArgs::default(),
        &EnvSnapshot::default(),
        &Config::default().connection,
    )
    .expect("synthetic fixture target resolves");
    Some(
        tokio::time::timeout(
            Duration::from_secs(5),
            session::connect(&target, Duration::ZERO),
        )
        .await
        .expect("fixture connection is bounded")
        .expect("disposable fixture connects"),
    )
}

fn bindings(value: &str) -> ParameterBindings {
    ParameterBindings::from_secrets(
        vec!["value".to_owned()],
        vec![SecretString::from(value.to_owned())],
    )
    .expect("synthetic named binding")
}

async fn execute(session: &Session, sql: &str) -> Execution {
    session.execute(sql, 10, JobId(800)).await
}

fn first_cell(execution: &Execution) -> &Cell {
    &execution.statements[0]
        .result_set
        .as_ref()
        .expect("result")
        .rows[0][0]
}

fn sqlstate(execution: &Execution) -> Option<&str> {
    execution.error.as_ref().and_then(|diagnostic| {
        diagnostic
            .technical
            .iter()
            .find(|field| field.label == "SQLSTATE")
            .map(|field| field.value.as_str())
    })
}

#[tokio::test]
async fn a_later_off_setting_refuses_before_binding_or_sending_and_can_be_restored_explicitly() {
    let Some(session) = fixture().await else {
        return;
    };
    assert_eq!(
        execute(&session, "CREATE TEMP TABLE semantics_marker (n integer)")
            .await
            .status,
        ExecutionStatus::Succeeded
    );
    assert_eq!(
        execute(&session, "SET standard_conforming_strings = off")
            .await
            .status,
        ExecutionStatus::Succeeded
    );

    // The semantics refusal must precede binding, including local NUL refusal.
    let refused = session
        .execute_with_parameters(
            "INSERT INTO semantics_marker VALUES (:value::integer)",
            &bindings("not-sent\0synthetic-private-value"),
            10,
            JobId(801),
        )
        .await;
    assert_eq!(refused.status, ExecutionStatus::Failed);
    assert!(refused.statements.is_empty());
    assert_eq!(refused.transaction, TransactionState::Autocommit);
    let diagnostic = refused.error.expect("semantics refusal");
    assert_eq!(diagnostic.kind, DiagnosticKind::Query);
    assert!(diagnostic.headline.contains("standard_conforming_strings"));
    assert!(!format!("{diagnostic:?}").contains("synthetic-private-value"));
    assert_eq!(
        first_cell(&execute(&session, "SHOW standard_conforming_strings").await),
        &Cell::Text("off".into()),
        "the client must not change the setting"
    );
    assert_eq!(
        first_cell(&execute(&session, "SELECT count(*) FROM semantics_marker").await),
        &Cell::Text("0".into())
    );

    assert_eq!(
        execute(&session, "SET standard_conforming_strings = on")
            .await
            .status,
        ExecutionStatus::Succeeded
    );
    let restored = session
        .execute_with_parameters(
            "INSERT INTO semantics_marker VALUES (:value::integer)",
            &bindings("7"),
            10,
            JobId(802),
        )
        .await;
    assert_eq!(restored.status, ExecutionStatus::Succeeded);
    assert_eq!(restored.statements[0].rows_affected, Some(1));
    assert_eq!(
        first_cell(&execute(&session, "SELECT n FROM semantics_marker").await),
        &Cell::Text("7".into())
    );
}

#[tokio::test]
async fn set_config_off_is_observed_by_both_binding_routes_without_sink_output() {
    let Some(session) = fixture().await else {
        return;
    };
    assert_eq!(
        execute(
            &session,
            "SELECT set_config('standard_conforming_strings', 'off', false)"
        )
        .await
        .status,
        ExecutionStatus::Succeeded
    );
    let supplied = bindings("synthetic-off-value");
    let execution = session
        .execute_with_parameters("SELECT :value", &supplied, 10, JobId(803))
        .await;
    assert_eq!(execution.status, ExecutionStatus::Failed);
    assert!(execution.statements.is_empty());

    let mut events = Vec::new();
    let result = session
        .stream_with_parameters("SELECT :value", &supplied, &mut |event| {
            events.push(event);
            Ok(())
        })
        .await;
    let Err(StreamStop::Server(diagnostic)) = result else {
        panic!("the incompatible setting must refuse export");
    };
    assert_eq!(diagnostic.kind, DiagnosticKind::Query);
    assert!(diagnostic.headline.contains("standard_conforming_strings"));
    assert!(events.is_empty(), "no requested export output is produced");
}

#[tokio::test]
async fn a_setting_change_in_a_buffer_keeps_its_prefix_once_and_never_sends_the_suffix() {
    let Some(session) = fixture().await else {
        return;
    };
    assert_eq!(
        execute(&session, "CREATE TEMP TABLE semantics_marker (n integer)")
            .await
            .status,
        ExecutionStatus::Succeeded
    );
    let execution = session
        .execute_with_parameters(
            "INSERT INTO semantics_marker VALUES (1); \
             SET standard_conforming_strings = off; \
             INSERT INTO semantics_marker VALUES (:value::integer)",
            &bindings("2"),
            10,
            JobId(804),
        )
        .await;
    assert_eq!(execution.status, ExecutionStatus::Failed);
    assert_eq!(
        execution.statements.len(),
        2,
        "completed statements stay visible"
    );
    assert_eq!(execution.statements[0].rows_affected, Some(1));
    assert_eq!(
        execution.error.expect("setting refusal").statement_number,
        Some(3)
    );
    assert_eq!(execution.transaction, TransactionState::Autocommit);
    assert_eq!(
        first_cell(
            &execute(
                &session,
                "SELECT array_agg(n ORDER BY n)::text FROM semantics_marker"
            )
            .await
        ),
        &Cell::Text("{1}".into()),
        "the prefix is not replayed and the suffix is never sent"
    );
    assert_eq!(
        first_cell(&execute(&session, "SHOW standard_conforming_strings").await),
        &Cell::Text("off".into())
    );
}

#[tokio::test]
async fn a_local_setting_refusal_leaves_the_explicit_transaction_open_until_owner_rollback() {
    let Some(session) = fixture().await else {
        return;
    };
    assert_eq!(
        execute(&session, "CREATE TEMP TABLE semantics_marker (n integer)")
            .await
            .status,
        ExecutionStatus::Succeeded
    );
    let execution = session
        .execute_with_parameters(
            "BEGIN; INSERT INTO semantics_marker VALUES (1); \
             SELECT set_config('standard_conforming_strings', 'off', true); \
             INSERT INTO semantics_marker VALUES (:value::integer)",
            &bindings("2"),
            10,
            JobId(805),
        )
        .await;
    assert_eq!(execution.status, ExecutionStatus::Failed);
    assert_eq!(execution.statements.len(), 3);
    assert_eq!(execution.transaction, TransactionState::Open);
    assert_eq!(
        first_cell(&execute(&session, "SELECT count(*) FROM semantics_marker").await),
        &Cell::Text("1".into()),
        "the client must not implicitly roll back"
    );
    assert_eq!(
        execute(&session, "ROLLBACK").await.status,
        ExecutionStatus::Succeeded
    );
    assert_eq!(
        first_cell(&execute(&session, "SELECT count(*) FROM semantics_marker").await),
        &Cell::Text("0".into())
    );
    assert_eq!(
        first_cell(&execute(&session, "SHOW standard_conforming_strings").await),
        &Cell::Text("on".into())
    );
}

#[tokio::test]
async fn streaming_keeps_initial_whole_message_parsing_when_the_message_changes_the_setting() {
    let Some(session) = fixture().await else {
        return;
    };
    let mut events = Vec::new();
    let rows = session
        .stream_with_parameters(
            "SET standard_conforming_strings = off; SELECT 'literal\\' AS literal, :value AS supplied",
            &bindings("synthetic-value"),
            &mut |event| { events.push(event); Ok(()) },
        )
        .await
        .expect("the Query message is parsed initially while the setting is on");
    assert_eq!(rows, 1);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, StreamEvent::Row(row)
        if row == &vec![Cell::Text("literal\\".into()), Cell::Text("synthetic-value".into())]))
    );
    assert_eq!(
        first_cell(&execute(&session, "SHOW standard_conforming_strings").await),
        &Cell::Text("off".into())
    );
}

#[tokio::test]
async fn streaming_retains_the_implicit_transaction_and_rolls_back_its_prefix_on_server_error() {
    let Some(session) = fixture().await else {
        return;
    };
    assert_eq!(
        execute(&session, "CREATE TEMP TABLE semantics_marker (n integer)")
            .await
            .status,
        ExecutionStatus::Succeeded
    );
    let result = session
        .stream_with_parameters(
            "INSERT INTO semantics_marker VALUES (:value::integer); \
             SET standard_conforming_strings = off; SELECT 1 / 0",
            &bindings("7"),
            &mut |_| Ok(()),
        )
        .await;
    let Err(StreamStop::Server(diagnostic)) = result else {
        panic!("the server must report division by zero");
    };
    assert!(
        diagnostic
            .technical
            .iter()
            .any(|field| field.label == "SQLSTATE" && field.value == "22012")
    );
    assert_eq!(
        first_cell(&execute(&session, "SELECT count(*) FROM semantics_marker").await),
        &Cell::Text("0".into()),
        "splitting the Query message would wrongly commit its prefix"
    );
    assert_eq!(
        first_cell(&execute(&session, "SHOW standard_conforming_strings").await),
        &Cell::Text("on".into()),
        "the server rolls back SET with the implicit transaction"
    );
}

#[tokio::test]
async fn an_unreadable_probe_preserves_failed_transaction_state_and_sqlstate_without_rollback() {
    let Some(session) = fixture().await else {
        return;
    };
    let failed = execute(&session, "BEGIN; SELECT 1 / 0").await;
    assert_eq!(failed.status, ExecutionStatus::Failed);
    assert_eq!(failed.transaction, TransactionState::Failed);
    let refused = session
        .execute_with_parameters("SELECT :value", &bindings("private-value"), 10, JobId(806))
        .await;
    assert_eq!(refused.status, ExecutionStatus::Failed);
    assert!(refused.statements.is_empty());
    assert_eq!(refused.transaction, TransactionState::Failed);
    assert_eq!(sqlstate(&refused), Some("25P02"));
    assert!(
        refused
            .error
            .as_ref()
            .expect("probe error")
            .attempted
            .contains("checking standard_conforming_strings")
    );
    assert!(!format!("{:?}", refused.error).contains("private-value"));
    let stream = session
        .stream_with_parameters("SELECT :value", &bindings("private-value"), &mut |_| Ok(()))
        .await;
    let Err(StreamStop::Server(diagnostic)) = stream else {
        panic!("the failed transaction must refuse the export probe")
    };
    assert!(
        diagnostic
            .technical
            .iter()
            .any(|field| field.label == "SQLSTATE" && field.value == "25P02")
    );
    assert_eq!(
        execute(&session, "ROLLBACK").await.status,
        ExecutionStatus::Succeeded
    );
    assert_eq!(
        session
            .execute_with_parameters("SELECT :value", &bindings("restored"), 10, JobId(807))
            .await
            .status,
        ExecutionStatus::Succeeded
    );
}

#[tokio::test]
async fn a_closed_session_probe_preserves_connection_loss_and_unknown_transaction_state() {
    let Some(session) = fixture().await else {
        return;
    };
    let Some(control) = fixture().await else {
        return;
    };
    let pid = session
        .info()
        .backend_pid
        .parse::<u32>()
        .expect("synthetic backend PID");
    let ended = execute(&control, &format!("SELECT pg_terminate_backend({pid})")).await;
    assert_eq!(ended.status, ExecutionStatus::Succeeded);
    assert_eq!(first_cell(&ended), &Cell::Text("t".into()));
    let execution = session
        .execute_with_parameters("SELECT :value", &bindings("private-value"), 10, JobId(808))
        .await;
    assert_eq!(execution.status, ExecutionStatus::ConnectionLost);
    assert_eq!(execution.transaction, TransactionState::Unknown);
    assert!(execution.statements.is_empty());
    assert_eq!(
        execution.error.expect("connection diagnostic").kind,
        DiagnosticKind::Connection
    );
}

#[tokio::test]
async fn cancellation_remains_independent_while_parameterized_execution_holds_the_gate() {
    let Some(session) = fixture().await else {
        return;
    };
    let Some(observer) = fixture().await else {
        return;
    };
    let pid = session
        .info()
        .backend_pid
        .parse::<u32>()
        .expect("synthetic backend PID");
    let supplied = bindings("30");
    let cancel = session.cancel_handle();
    let running = session.execute_with_parameters(
        "SELECT pg_sleep(:value::double precision)",
        &supplied,
        10,
        JobId(809),
    );
    let stopper =
        async {
            // Observe actual server execution before cancellation. The observer is
            // a separate fixture connection, and each observation is a protocol
            // round trip rather than a timing guess about when pg_sleep starts.
            tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    let observed = execute(&observer, &format!(
                    "SELECT coalesce(wait_event, '') FROM pg_stat_activity WHERE pid = {pid}"
                )).await;
                    assert_eq!(observed.status, ExecutionStatus::Succeeded);
                    if first_cell(&observed) == &Cell::Text("PgSleep".into()) {
                        break;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("the requested statement reaches its server wait");
            cancel
                .cancel()
                .await
                .expect("cancellation delivery is independent of the gate");
        };
    let (execution, ()) = tokio::time::timeout(Duration::from_secs(10), async {
        tokio::join!(running, stopper)
    })
    .await
    .expect("cancellation must complete without a gate deadlock");
    assert_eq!(execution.status, ExecutionStatus::Cancelled);
    assert_eq!(sqlstate(&execution), Some("57014"));
    assert_eq!(execution.transaction, TransactionState::Autocommit);
    assert_eq!(
        execute(&session, "SELECT 1").await.status,
        ExecutionStatus::Succeeded
    );
}
