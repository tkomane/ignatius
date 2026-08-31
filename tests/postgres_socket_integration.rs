//! A live Unix-domain socket check for the PostgreSQL connection boundary.
//!
//! The ordinary integration fixture uses a TCP URI because that is the
//! portable route in CI. This focused check is enabled by
//! `IGNATIUS_TEST_PG_SOCKET_URI` so a Linux runner can point it at a real
//! PostgreSQL socket without changing the TCP fixture or silently treating a
//! skipped environment as evidence.

#![cfg(unix)]
#![allow(clippy::print_stderr)]

use ignatius::config::Config;
use ignatius::connection::{ConnectionArgs, EnvSnapshot, Host, resolve};
use ignatius::postgres::session;
use ignatius::query::result::{ExecutionStatus, JobId};
use std::time::Duration;

#[test]
fn a_live_unix_socket_target_executes_a_query() {
    let uri = match std::env::var("IGNATIUS_TEST_PG_SOCKET_URI") {
        Ok(uri) if !uri.is_empty() => uri,
        _ => {
            eprintln!(
                "skipping: IGNATIUS_TEST_PG_SOCKET_URI is not set; provide a real PostgreSQL Unix-socket URI to run this check"
            );
            return;
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

    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let opened = runtime
        .block_on(session::connect(&target, Duration::ZERO))
        .expect("socket session opens");
    let execution = runtime.block_on(opened.execute("SELECT 1 AS socket_probe", 10, JobId(1)));

    assert_eq!(
        execution.status,
        ExecutionStatus::Succeeded,
        "socket query failed: {:?}",
        execution.error
    );
    let result = execution.statements[0]
        .result_set
        .as_ref()
        .expect("socket query result");
    assert_eq!(result.columns, vec!["socket_probe".to_owned()]);
    assert_eq!(result.rows[0][0].display(), "1");
}
