//! A benchmark through the production session boundary never shares the
//! session with a transaction it could end.
//!
//! Every database and value is synthetic. Missing fixture access is an explicit
//! skip; these checks do not start, stop or modify shared database fixtures,
//! and every write they attempt is refused or rolled back.

#![allow(clippy::print_stderr)]

use ignatius::bench::BenchConfig;
use ignatius::config::Config;
use ignatius::connection::{ConnectionArgs, EnvSnapshot, resolve};
use ignatius::diagnostics::DiagnosticKind;
use ignatius::postgres::{BenchmarkCancelHandle, Session, session};
use ignatius::query::result::{ExecutionStatus, JobId, TransactionState};
use std::path::PathBuf;
use std::time::{Duration, Instant};

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

#[tokio::test]
async fn a_benchmark_refuses_to_start_inside_an_open_transaction() {
    let Some(session) = fixture().await else {
        return;
    };
    let begun = session.execute("BEGIN", 10, JobId(9100)).await;
    assert_eq!(begun.status, ExecutionStatus::Succeeded);
    let marker = session
        .execute(
            "CREATE TEMP TABLE bench_guard_marker (id int)",
            10,
            JobId(9101),
        )
        .await;
    assert_eq!(marker.status, ExecutionStatus::Succeeded);

    let refused = session
        .run_benchmark("SELECT 1", &BenchConfig::new(0, 1, true), "Baseline (A)")
        .await
        .expect_err("a benchmark inside a transaction would end it");
    assert!(
        refused.headline.contains("not outside a transaction"),
        "{}",
        refused.headline
    );

    // The user's transaction is still open and its work is still there.
    let still_there = session
        .execute("SELECT count(*) FROM bench_guard_marker", 10, JobId(9102))
        .await;
    assert_eq!(still_there.status, ExecutionStatus::Succeeded);
    let ended = session.execute("ROLLBACK", 10, JobId(9103)).await;
    assert_eq!(ended.status, ExecutionStatus::Succeeded);
}

#[tokio::test]
async fn a_benchmark_runs_in_autocommit_and_leaves_the_session_there() {
    let Some(session) = fixture().await else {
        return;
    };
    let stats = session
        .run_benchmark("SELECT 1", &BenchConfig::new(0, 2, true), "Baseline (A)")
        .await
        .expect("a read benchmarks in autocommit");
    assert_eq!(stats.samples.len(), 2);

    // The benchmark left nothing open behind it.
    let after = session.execute("SELECT 1", 10, JobId(9110)).await;
    assert_eq!(after.status, ExecutionStatus::Succeeded);
    assert_eq!(after.transaction, TransactionState::Autocommit);
}

#[tokio::test]
async fn workbench_work_waits_for_a_running_benchmark_instead_of_landing_inside_it() {
    let Some(session) = fixture().await else {
        return;
    };
    let session = std::sync::Arc::new(session);
    let bench_session = std::sync::Arc::clone(&session);
    let bench = tokio::spawn(async move {
        bench_session
            .run_benchmark(
                "SELECT pg_sleep(0.1)",
                &BenchConfig::new(0, 5, true),
                "Baseline (A)",
            )
            .await
    });
    // Land in the middle of the benchmark's BEGIN/ROLLBACK iterations.
    tokio::time::sleep(Duration::from_millis(150)).await;
    let created = session
        .execute(
            "CREATE TEMP TABLE bench_gate_probe (id int)",
            10,
            JobId(9120),
        )
        .await;
    assert_eq!(created.status, ExecutionStatus::Succeeded);
    bench
        .await
        .expect("benchmark task joins")
        .expect("the read benchmark completes");

    // Without the gate the benchmark's ROLLBACK would have discarded this.
    let survived = session
        .execute("SELECT count(*) FROM bench_gate_probe", 10, JobId(9121))
        .await;
    assert_eq!(
        survived.status,
        ExecutionStatus::Succeeded,
        "the workbench statement ran outside the benchmark's transaction"
    );
}

#[tokio::test]
async fn an_analysed_plan_comparison_refuses_to_end_an_open_transaction() {
    let Some(session) = fixture().await else {
        return;
    };
    assert_eq!(
        session.execute("BEGIN", 10, JobId(9130)).await.status,
        ExecutionStatus::Succeeded
    );
    assert_eq!(
        session
            .execute(
                "CREATE TEMP TABLE plan_guard_marker (id int)",
                10,
                JobId(9131)
            )
            .await
            .status,
        ExecutionStatus::Succeeded
    );
    let refused = session
        .explain_extended("SELECT 1", true, "Baseline (A)")
        .await
        .expect_err("an analysed plan inside a transaction would end it");
    assert!(
        refused.headline.contains("not outside a transaction"),
        "{}",
        refused.headline
    );
    let still_there = session
        .execute("SELECT count(*) FROM plan_guard_marker", 10, JobId(9132))
        .await;
    assert_eq!(still_there.status, ExecutionStatus::Succeeded);
    assert_eq!(still_there.transaction, TransactionState::Open);
    assert_eq!(
        session.execute("ROLLBACK", 10, JobId(9133)).await.status,
        ExecutionStatus::Succeeded
    );
}

#[tokio::test]
async fn cancelling_an_in_flight_dml_benchmark_rolls_back_and_leaves_autocommit() {
    let Some(session) = fixture().await else {
        return;
    };
    let setup = session
        .execute(
            "CREATE TEMP TABLE cancel_bench_probe (id int); \
             INSERT INTO cancel_bench_probe (id) VALUES (1), (2);",
            10,
            JobId(9140),
        )
        .await;
    assert_eq!(setup.status, ExecutionStatus::Succeeded);

    let session = std::sync::Arc::new(session);
    let cancel = BenchmarkCancelHandle::new();
    let bench_session = std::sync::Arc::clone(&session);
    let bench_cancel = cancel.clone();
    let started = Instant::now();
    let bench = tokio::spawn(async move {
        bench_session
            .run_benchmark_cancellable(
                "DELETE FROM cancel_bench_probe WHERE id = 1 OR (id = 2 AND pg_sleep(5) IS NOT NULL)",
                &BenchConfig::new(0, 2, true),
                "Baseline (A)",
                &bench_cancel,
            )
            .await
    });

    tokio::time::sleep(Duration::from_millis(150)).await;
    cancel
        .request_cancel()
        .await
        .expect("cancel request delivered and server closed cancel socket");

    let err = tokio::time::timeout(Duration::from_secs(4), bench)
        .await
        .expect("cancelled benchmark finishes promptly")
        .expect("benchmark task joins")
        .expect_err("cancelled benchmark returns Diagnostic");
    assert_eq!(err.kind, DiagnosticKind::Cancelled, "{err:?}");
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "cancellation did not wait for 5s sleep: {:?}",
        started.elapsed()
    );

    let after = session
        .execute("SELECT count(*) FROM cancel_bench_probe", 10, JobId(9141))
        .await;
    assert_eq!(after.status, ExecutionStatus::Succeeded);
    assert_eq!(
        after.transaction,
        TransactionState::Autocommit,
        "cancelled rollback benchmark must leave the session in Autocommit"
    );
    let count = after.statements[0]
        .result_set
        .as_ref()
        .expect("count result")
        .rows[0][0]
        .display();
    assert_eq!(count, "2", "rolled-back DML must leave row count untouched");
}

#[tokio::test]
async fn cancelling_a_benchmark_waiting_on_the_gate_does_not_block_or_cancel_workbench() {
    let Some(session) = fixture().await else {
        return;
    };
    let session = std::sync::Arc::new(session);
    let wb_session = std::sync::Arc::clone(&session);
    let workbench = tokio::spawn(async move {
        wb_session
            .execute("SELECT pg_sleep(0.4)", 10, JobId(9150))
            .await
    });

    tokio::time::sleep(Duration::from_millis(50)).await;
    let cancel = BenchmarkCancelHandle::new();
    let bench_session = std::sync::Arc::clone(&session);
    let bench_cancel = cancel.clone();
    let bench = tokio::spawn(async move {
        bench_session
            .run_benchmark_cancellable(
                "SELECT 1",
                &BenchConfig::new(0, 1, true),
                "Baseline (A)",
                &bench_cancel,
            )
            .await
    });

    tokio::time::sleep(Duration::from_millis(50)).await;
    let cancel_started = Instant::now();
    cancel
        .request_cancel()
        .await
        .expect("cancelling while waiting on gate does not fail");
    assert!(
        cancel_started.elapsed() < Duration::from_millis(150),
        "request_cancel must never block on user_sql_gate"
    );

    let bench_err = tokio::time::timeout(Duration::from_secs(2), bench)
        .await
        .expect("bench stops promptly while gate is busy")
        .expect("bench joins")
        .expect_err("cancelled before BEGIN");
    assert_eq!(bench_err.kind, DiagnosticKind::Cancelled);

    let wb_res = workbench.await.expect("workbench task joins");
    assert_eq!(
        wb_res.status,
        ExecutionStatus::Succeeded,
        "benchmark cancel must never cancel an unrelated workbench statement"
    );
}

#[tokio::test]
async fn cancelling_an_in_flight_tls_benchmark_drains_cancel_eof_and_recovers() {
    let Ok(raw_uri) = std::env::var("IGNATIUS_TEST_PG_TLS_URI") else {
        eprintln!("skipping: IGNATIUS_TEST_PG_TLS_URI is not set");
        return;
    };
    if raw_uri.is_empty() {
        eprintln!("skipping: IGNATIUS_TEST_PG_TLS_URI is empty");
        return;
    }
    let ca = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docker/tls/generated/ca.crt");
    let uri = if raw_uri.contains('?') {
        raw_uri
    } else {
        format!("{raw_uri}?sslmode=verify-full&sslrootcert={}", ca.display())
    };
    let target = resolve(
        Some(&uri),
        &ConnectionArgs::default(),
        &EnvSnapshot::default(),
        &Config::default().connection,
    )
    .expect("TLS fixture resolves");
    let session = tokio::time::timeout(
        Duration::from_secs(5),
        session::connect(&target, Duration::ZERO),
    )
    .await
    .expect("TLS connect bounded")
    .expect("TLS fixture connects");
    assert!(session.info().tls.is_encrypted());

    let session = std::sync::Arc::new(session);
    let cancel = BenchmarkCancelHandle::new();
    let bench_session = std::sync::Arc::clone(&session);
    let bench_cancel = cancel.clone();
    let bench = tokio::spawn(async move {
        bench_session
            .run_benchmark_cancellable(
                "SELECT pg_sleep(5)",
                &BenchConfig::new(0, 1, true),
                "Baseline (A)",
                &bench_cancel,
            )
            .await
    });

    tokio::time::sleep(Duration::from_millis(150)).await;
    cancel
        .request_cancel()
        .await
        .expect("TLS cancel packet delivered and EOF drained");

    let err = tokio::time::timeout(Duration::from_secs(4), bench)
        .await
        .expect("TLS benchmark cancelled promptly")
        .expect("bench joins")
        .expect_err("cancelled TLS benchmark errors");
    assert_eq!(err.kind, DiagnosticKind::Cancelled, "{err:?}");

    let follow_up = session.execute("SELECT 1", 10, JobId(9160)).await;
    assert_eq!(follow_up.status, ExecutionStatus::Succeeded);
    assert_eq!(follow_up.transaction, TransactionState::Autocommit);
}
