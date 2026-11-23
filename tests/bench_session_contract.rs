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
use ignatius::postgres::{Session, session};
use ignatius::query::result::{ExecutionStatus, JobId};
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

    // A transaction can still be opened and closed afterwards: the
    // benchmark left nothing open behind it.
    let begun = session.execute("BEGIN", 10, JobId(9110)).await;
    assert_eq!(begun.status, ExecutionStatus::Succeeded);
    let ended = session.execute("ROLLBACK", 10, JobId(9111)).await;
    assert_eq!(ended.status, ExecutionStatus::Succeeded);
}
