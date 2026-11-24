//! Rollback-safe asynchronous PostgreSQL runner for query benchmarks
//! (Feature `056-query-benchmarks-and-regression-suite`).
//!
//! Validates that exactly one non-empty SQL statement was provided, executes
//! unrecorded warmup iterations followed by measured iterations timed with
//! `std::time::Instant::now()`, and when `config.rollback` is `true` brackets
//! every iteration in `BEGIN` and an unconditional `ROLLBACK` (on both `Ok`
//! and `Err`) so DML statements never persist mutations.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use futures_util::StreamExt;
use tokio::sync::Notify;
use tokio_postgres::{Client, SimpleQueryMessage, Statement, types::ToSql};

use crate::bench::{BenchConfig, BenchSample, BenchStats, compute_bench_stats};
use crate::diagnostics::{Diagnostic, DiagnosticKind};
use crate::postgres::error::from_query_error;
use crate::postgres::session::CancelHandle;
use crate::query::statements;

/// Thread-safe, non-blocking cancel controller for a running benchmark.
///
/// Cancellation sets an atomic flag checked before `BEGIN` and before each
/// iteration, and dispatches [`CancelHandle::cancel`] only while the benchmark
/// owns the session's SQL gate and is actively inside `execute_timed_statement`
/// (never during `BEGIN`, `ROLLBACK`, or between iterations).
#[derive(Clone, Default)]
pub struct BenchmarkCancelHandle {
    state: Arc<BenchCancelState>,
}

#[derive(Default)]
struct BenchCancelState {
    cancelled: AtomicBool,
    cancelled_notify: Notify,
    executing_cancel: std::sync::Mutex<Option<CancelHandle>>,
}

impl std::fmt::Debug for BenchmarkCancelHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BenchmarkCancelHandle")
            .field("cancelled", &self.is_cancelled())
            .finish_non_exhaustive()
    }
}

impl BenchmarkCancelHandle {
    /// Creates a fresh cancel handle for a benchmark run.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether cancellation has been requested for this benchmark run.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.state.cancelled.load(Ordering::SeqCst)
    }

    /// Whether two handles refer to the same underlying benchmark run state.
    #[must_use]
    pub fn ptr_eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.state, &other.state)
    }

    /// Subscribes to cancellation notifications for gate-wait racing.
    pub(super) fn notified(&self) -> tokio::sync::futures::Notified<'_> {
        self.state.cancelled_notify.notified()
    }

    /// Requests cancellation without blocking on the session's SQL gate.
    ///
    /// Marks the benchmark cancelled for all upcoming iterations and, if a
    /// timed user statement is currently executing, sends PostgreSQL's cancel
    /// packet after releasing the internal lock.
    pub async fn request_cancel(&self) -> Result<(), Diagnostic> {
        let handle = {
            let slot = self
                .state
                .executing_cancel
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            self.state.cancelled.store(true, Ordering::SeqCst);
            self.state.cancelled_notify.notify_waiters();
            slot.clone()
        };
        if let Some(handle) = handle {
            handle.cancel().await?;
        }
        Ok(())
    }

    /// Installs the active [`CancelHandle`] only if cancellation has not already
    /// been requested, returning a guard that clears it before `ROLLBACK` runs.
    fn install_executing(
        &self,
        handle: CancelHandle,
    ) -> Result<ExecutingCancelGuard<'_>, Diagnostic> {
        let mut slot = self
            .state
            .executing_cancel
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if self.state.cancelled.load(Ordering::SeqCst) {
            return Err(cancelled_diagnostic());
        }
        *slot = Some(handle);
        Ok(ExecutingCancelGuard { state: &self.state })
    }
}

struct ExecutingCancelGuard<'a> {
    state: &'a BenchCancelState,
}

impl Drop for ExecutingCancelGuard<'_> {
    fn drop(&mut self) {
        let mut slot = self
            .state
            .executing_cancel
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *slot = None;
    }
}

/// Diagnostic returned when a benchmark stops because cancellation was
/// requested locally before `BEGIN` or between iterations.
pub(super) fn cancelled_diagnostic() -> Diagnostic {
    Diagnostic::new(
        DiagnosticKind::Cancelled,
        "the benchmark was cancelled",
        "running a query benchmark",
    )
    .next_action("review any completed baseline or run the benchmark again")
}

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
    run_benchmark_inner(client, sql, config, label, None).await
}

/// Executes a cancellable benchmark run with the provided cancel controller.
pub(super) async fn run_benchmark_with_cancel(
    client: &Client,
    sql: &str,
    config: &BenchConfig,
    label: &str,
    cancel_handle: CancelHandle,
    controller: &BenchmarkCancelHandle,
) -> Result<BenchStats, Diagnostic> {
    run_benchmark_inner(
        client,
        sql,
        config,
        label,
        Some((&cancel_handle, controller)),
    )
    .await
}

async fn run_benchmark_inner(
    client: &Client,
    sql: &str,
    config: &BenchConfig,
    label: &str,
    cancel: Option<(&CancelHandle, &BenchmarkCancelHandle)>,
) -> Result<BenchStats, Diagnostic> {
    if cancel.is_some_and(|(_, controller)| controller.is_cancelled()) {
        return Err(cancelled_diagnostic());
    }
    let stmt_text = validate_single_statement(sql)?;
    let effective = config.clamped();
    let controller = cancel.map(|(_, controller)| controller);
    collect_samples_cancellable(label, &stmt_text, &effective, controller, || {
        run_iteration(client, &stmt_text, effective.rollback, cancel)
    })
    .await
}

/// Collect warmup and measured samples using the caller's execution route.
pub(super) async fn collect_samples<F, Fut>(
    label: &str,
    sql: &str,
    config: &BenchConfig,
    iteration: F,
) -> Result<BenchStats, Diagnostic>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<(f64, u64), Diagnostic>>,
{
    collect_samples_cancellable(label, sql, config, None, iteration).await
}

async fn collect_samples_cancellable<F, Fut>(
    label: &str,
    sql: &str,
    config: &BenchConfig,
    cancel: Option<&BenchmarkCancelHandle>,
    mut iteration: F,
) -> Result<BenchStats, Diagnostic>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<(f64, u64), Diagnostic>>,
{
    for _ in 0..config.warmup_runs {
        if cancel.is_some_and(BenchmarkCancelHandle::is_cancelled) {
            return Err(cancelled_diagnostic());
        }
        let _ = iteration().await?;
    }

    let mut samples = Vec::with_capacity(config.measured_runs);
    for sample in 1..=config.measured_runs {
        if cancel.is_some_and(BenchmarkCancelHandle::is_cancelled) {
            return Err(cancelled_diagnostic());
        }
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
    cancel: Option<(&CancelHandle, &BenchmarkCancelHandle)>,
) -> Result<(f64, u64), Diagnostic> {
    if cancel.is_some_and(|(_, controller)| controller.is_cancelled()) {
        return Err(cancelled_diagnostic());
    }
    if rollback {
        client
            .simple_query("BEGIN")
            .await
            .map_err(|err| from_query_error(&err, 1))?;

        let exec_res = execute_with_cancel_guard(client, stmt_text, cancel).await;
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
        execute_with_cancel_guard(client, stmt_text, cancel).await
    }
}

async fn execute_with_cancel_guard(
    client: &Client,
    stmt_text: &str,
    cancel: Option<(&CancelHandle, &BenchmarkCancelHandle)>,
) -> Result<(f64, u64), Diagnostic> {
    let guard = match cancel {
        Some((handle, controller)) => Some(controller.install_executing(handle.clone())?),
        None => None,
    };
    let res = execute_timed_statement(client, stmt_text).await;
    drop(guard);
    res
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

#[cfg(test)]
mod tests;
