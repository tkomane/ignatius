//! Rollback-safe asynchronous PostgreSQL runner for query benchmarks
//! (Feature `056-query-benchmarks-and-regression-suite`).
//!
//! Validates that exactly one non-empty SQL statement was provided, executes
//! unrecorded warmup iterations followed by measured iterations timed with
//! `std::time::Instant::now()`, and when `config.rollback` is `true` brackets
//! every iteration in `BEGIN` and an unconditional `ROLLBACK` (on both `Ok`
//! and `Err`) so DML statements never persist mutations.

use std::time::Instant;

use futures_util::StreamExt;
use tokio_postgres::{Client, SimpleQueryMessage, Statement, types::ToSql};

use crate::bench::{BenchConfig, BenchSample, BenchStats, compute_bench_stats};
use crate::diagnostics::{Diagnostic, DiagnosticKind};
use crate::postgres::error::from_query_error;
use crate::query::statements;

/// Executes warmup and measured iterations of a single SQL statement against a
/// live PostgreSQL connection and returns aggregated [`BenchStats`].
///
/// When `config.rollback` is `true`, each warmup and measured iteration runs
/// inside `BEGIN` followed unconditionally by `ROLLBACK` (even when the query
/// fails) so write statements leave database state unchanged.
pub async fn run_benchmark(
    client: &Client,
    sql: &str,
    config: &BenchConfig,
    label: &str,
) -> Result<BenchStats, Diagnostic> {
    let stmt_text = validate_single_statement(sql)?;
    let effective = config.clamped();
    collect_samples(label, &stmt_text, &effective, || {
        run_iteration(client, &stmt_text, effective.rollback)
    })
    .await
}

/// Collect warmup and measured samples using the caller's execution route.
pub(super) async fn collect_samples<F, Fut>(
    label: &str,
    sql: &str,
    config: &BenchConfig,
    mut iteration: F,
) -> Result<BenchStats, Diagnostic>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<(f64, u64), Diagnostic>>,
{
    for _ in 0..config.warmup_runs {
        let _ = iteration().await?;
    }

    let mut samples = Vec::with_capacity(config.measured_runs);
    for sample in 1..=config.measured_runs {
        let (duration_ms, rows_returned) = iteration().await?;
        samples.push(BenchSample {
            iteration: sample,
            duration_ms,
            rows_returned,
        });
    }

    Ok(compute_bench_stats(label, sql, config, &samples))
}

/// Time one parameterless prepared read without retaining its result rows.
pub(super) async fn timed_prepared_read(
    client: &Client,
    statement: &Statement,
) -> Result<(f64, u64), Diagnostic> {
    let started = Instant::now();
    let rows = client
        .query_raw(statement, std::iter::empty::<&(dyn ToSql + Sync)>())
        .await
        .map_err(|error| from_query_error(&error, 1))?;
    tokio::pin!(rows);
    let mut count = 0u64;
    while let Some(row) = rows.next().await {
        row.map_err(|error| from_query_error(&error, 1))?;
        count = count.saturating_add(1);
    }
    Ok((started.elapsed().as_secs_f64() * 1000.0, count))
}

fn validate_single_statement(sql: &str) -> Result<String, Diagnostic> {
    let parsed = statements::split(sql);
    if parsed.is_empty() {
        return Err(Diagnostic::new(
            DiagnosticKind::Usage,
            "no SQL statement to benchmark",
            "validating statement for query benchmark",
        )
        .next_action("provide a single non-empty SQL statement"));
    }
    if parsed.len() > 1 {
        return Err(Diagnostic::new(
            DiagnosticKind::Usage,
            format!(
                "expected 1 SQL statement to benchmark, found {}",
                parsed.len()
            ),
            "validating statement for query benchmark",
        )
        .likely_cause("query benchmarks measure one statement at a time")
        .next_action("pass a single SQL statement without extra trailing statements"));
    }
    Ok(parsed[0].text.clone())
}

async fn run_iteration(
    client: &Client,
    stmt_text: &str,
    rollback: bool,
) -> Result<(f64, u64), Diagnostic> {
    if rollback {
        client
            .simple_query("BEGIN")
            .await
            .map_err(|err| from_query_error(&err, 1))?;

        let exec_res = execute_timed_statement(client, stmt_text).await;
        let rollback_res = client.simple_query("ROLLBACK").await;

        match exec_res {
            Ok(measured) => {
                if let Err(rb_err) = rollback_res {
                    return Err(from_query_error(&rb_err, 1).likely_cause(
                        "benchmark iteration finished, but the cleanup ROLLBACK failed",
                    ));
                }
                Ok(measured)
            }
            Err(mut diag) => {
                if let Err(rb_err) = rollback_res {
                    diag = diag.technical("RollbackError", rb_err.to_string());
                }
                Err(diag)
            }
        }
    } else {
        execute_timed_statement(client, stmt_text).await
    }
}

async fn execute_timed_statement(
    client: &Client,
    stmt_text: &str,
) -> Result<(f64, u64), Diagnostic> {
    let started = Instant::now();
    let stream = client
        .simple_query_raw(stmt_text)
        .await
        .map_err(|err| from_query_error(&err, 1))?;
    tokio::pin!(stream);

    let mut row_count: u64 = 0;
    let mut command_rows: Option<u64> = None;

    while let Some(message) = stream.next().await {
        match message.map_err(|err| from_query_error(&err, 1))? {
            SimpleQueryMessage::RowDescription(_) => {}
            SimpleQueryMessage::Row(_) => {
                row_count = row_count.saturating_add(1);
            }
            SimpleQueryMessage::CommandComplete(count) => {
                command_rows = Some(count);
            }
            _ => {}
        }
    }

    let duration_ms = started.elapsed().as_secs_f64() * 1000.0;
    let rows_returned = if row_count > 0 {
        row_count
    } else {
        command_rows.unwrap_or(0)
    };
    Ok((duration_ms, rows_returned))
}
