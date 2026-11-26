//! Live Unix-domain socket checks for PostgreSQL queries and cancellation.
//!
//! The ordinary integration fixture uses a TCP URI because that is the
//! portable route in CI. These checks are enabled by
//! `IGNATIUS_TEST_PG_SOCKET_URI` so a Linux runner can point it at a real
//! PostgreSQL socket without changing the TCP fixture or silently treating a
//! skipped environment as evidence.

#![cfg(unix)]
#![allow(clippy::print_stderr)]

use ignatius::config::Config;
use ignatius::connection::{ConnectionArgs, ConnectionTarget, EnvSnapshot, Host, resolve};
use ignatius::postgres::session::{self, Session};
use ignatius::query::result::{Execution, ExecutionStatus, JobId, ResultSet, TransactionState};
use std::time::Duration;

fn socket_target() -> Option<ConnectionTarget> {
    let uri = match std::env::var("IGNATIUS_TEST_PG_SOCKET_URI") {
        Ok(uri) if !uri.is_empty() => uri,
        _ => {
            eprintln!(
                "skipping: IGNATIUS_TEST_PG_SOCKET_URI is not set; provide a disposable PostgreSQL Unix-socket URI to run this check"
            );
            return None;
        }
    };

    let target = resolve(
        Some(&uri),
        &ConnectionArgs::default(),
        &EnvSnapshot::default(),
        &Config::default().connection,
    )
    .expect("socket target resolves");
    assert!(
        matches!(target.host, Host::Socket(_)),
        "the fixture must exercise a Unix socket, not a TCP fallback: {:?}",
        target.host
    );

    Some(target)
}

fn query_result(execution: &Execution) -> &ResultSet {
    assert_eq!(
        execution.status,
        ExecutionStatus::Succeeded,
        "socket query failed: {:?}",
        execution.error
    );
    execution.statements[0]
        .result_set
        .as_ref()
        .expect("socket query result")
}

#[test]
fn a_live_unix_socket_target_executes_a_query() {
    let Some(target) = socket_target() else {
        return;
    };

    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let opened = runtime
        .block_on(session::connect(&target, Duration::ZERO))
        .expect("socket session opens");
    let execution = runtime.block_on(opened.execute("SELECT 1 AS socket_probe", 10, JobId(1)));

    let result = query_result(&execution);
    assert_eq!(result.columns, vec!["socket_probe".to_owned()]);
    assert_eq!(result.rows[0][0].display(), "1");
}

async fn socket_backend_pid(session: &Session, job: JobId) -> i32 {
    let execution = session
        .execute(
            "SELECT pg_catalog.pg_backend_pid() WHERE pg_catalog.inet_client_addr() IS NULL",
            1,
            job,
        )
        .await;
    let result = query_result(&execution);
    assert_eq!(
        result.rows.len(),
        1,
        "PostgreSQL must report a NULL client address for a Unix socket"
    );
    assert_eq!(execution.transaction, TransactionState::Autocommit);
    result.rows[0][0].display().parse().expect("backend PID")
}

async fn cancellation_sessions(target: &ConnectionTarget) -> (Session, Session, i32) {
    let opened = session::connect(target, Duration::ZERO)
        .await
        .expect("socket session opens");
    let observer = session::connect(target, Duration::ZERO)
        .await
        .expect("observer socket session opens");
    let pid = socket_backend_pid(&opened, JobId(1)).await;
    let observer_pid = socket_backend_pid(&observer, JobId(2)).await;
    assert_ne!(
        pid, observer_pid,
        "observation needs an independent backend"
    );

    // A fixture setting must not cancel the statement independently.
    let setup = opened
        .execute("SET statement_timeout = 0", 1, JobId(3))
        .await;
    assert_eq!(
        setup.status,
        ExecutionStatus::Succeeded,
        "{:?}",
        setup.error
    );
    (opened, observer, pid)
}

async fn cancel_observed_sleep(
    opened: &Session,
    observer: &Session,
    pid: i32,
    sleep_sql: &str,
) -> Execution {
    let activity_sql = format!(
        "SELECT 1 FROM pg_catalog.pg_stat_activity \
         WHERE pid = {pid} AND state = 'active' AND query = '{sleep_sql}' \
         AND wait_event_type = 'Timeout' AND wait_event = 'PgSleep'"
    );
    let cancel = opened.cancel_handle();
    let running = opened.execute(sleep_sql, 1, JobId(5));
    let stop = async {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let observed = observer.execute(&activity_sql, 1, JobId(6)).await;
                if !query_result(&observed).rows.is_empty() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("the observer sees the owned backend executing the tagged sleep");
        cancel
            .cancel()
            .await
            .expect("cancellation request is delivered");
    };
    let (execution, ()) = tokio::join!(running, stop);
    execution
}

fn assert_sqlstate(execution: &Execution, expected: &str) {
    let error = execution.error.as_ref().expect("server diagnostic");
    assert_eq!(
        error
            .technical
            .iter()
            .find(|field| field.label == "SQLSTATE")
            .map(|field| field.value.as_str()),
        Some(expected),
        "{error:?}"
    );
}

#[tokio::test]
async fn unix_socket_cancellation_is_confirmed_and_the_same_backend_runs_again() {
    let Some(target) = socket_target() else {
        return;
    };
    const SLEEP_SQL: &str = "SELECT pg_catalog.pg_sleep(30) /* ignatius_socket_cancel_probe */";

    tokio::time::timeout(Duration::from_secs(20), async {
        let (opened, observer, pid) = cancellation_sessions(&target).await;
        let execution = cancel_observed_sleep(&opened, &observer, pid, SLEEP_SQL).await;
        assert_eq!(
            execution.status,
            ExecutionStatus::Cancelled,
            "{:?}",
            execution.error
        );
        assert_eq!(execution.transaction, TransactionState::Autocommit);
        assert_sqlstate(&execution, "57014");
        assert_eq!(socket_backend_pid(&opened, JobId(7)).await, pid);
    })
    .await
    .expect("cancellation and same-backend recovery finish within 20 seconds");
}

#[tokio::test]
async fn unix_socket_cancellation_inside_a_transaction_requires_explicit_rollback() {
    let Some(target) = socket_target() else {
        return;
    };
    const SLEEP_SQL: &str = "SELECT pg_catalog.pg_sleep(30) /* ignatius_socket_cancel_in_tx */";

    tokio::time::timeout(Duration::from_secs(20), async {
        let (opened, observer, pid) = cancellation_sessions(&target).await;
        let begin = opened.execute("BEGIN", 1, JobId(4)).await;
        assert_eq!(
            begin.status,
            ExecutionStatus::Succeeded,
            "{:?}",
            begin.error
        );
        assert_eq!(begin.transaction, TransactionState::Open);

        let cancelled = cancel_observed_sleep(&opened, &observer, pid, SLEEP_SQL).await;
        assert_eq!(
            cancelled.status,
            ExecutionStatus::Cancelled,
            "{:?}",
            cancelled.error
        );
        assert_eq!(cancelled.transaction, TransactionState::Failed);
        assert_sqlstate(&cancelled, "57014");

        let blocked = opened.execute("SELECT 1", 1, JobId(7)).await;
        assert_eq!(blocked.status, ExecutionStatus::Failed);
        assert_eq!(blocked.transaction, TransactionState::Failed);
        assert_sqlstate(&blocked, "25P02");

        let rollback = opened.execute("ROLLBACK", 1, JobId(8)).await;
        assert_eq!(
            rollback.status,
            ExecutionStatus::Succeeded,
            "{:?}",
            rollback.error
        );
        assert_eq!(rollback.transaction, TransactionState::Autocommit);
        assert_eq!(socket_backend_pid(&opened, JobId(9)).await, pid);
    })
    .await
    .expect("cancellation, explicit rollback, and same-backend recovery finish within 20 seconds");
}
