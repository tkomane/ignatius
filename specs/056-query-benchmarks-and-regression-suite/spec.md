# Feature Specification: Query Benchmarks & Regression Suite (056)

**Feature Branch**: `feat/m14-bench-snapshot-runbook-polish`
**Created**: 2026-11-19
**Status**: Phase 1 Implemented (Domain Engine, Async Runner, & Sidecar Tests)
**Authority**: Roadmap Milestone 14 (M14: Query Benchmarks, Schema Snapshots, Runbooks & Polish)

## 1. Context and Problem Statement

Single-shot query timings and `EXPLAIN ANALYZE` runs fluctuate due to buffer cache
warming, checkpoint timing, and transient host noise. When tuning indexes,
rewriting SQL, or checking for latency regressions in CI, engineers need
repeatable warmup-separated multi-iteration query benchmarks with distribution
percentiles (`min`, `mean`, `stddev`, `p50`, `p95`, `p99`, `max`, `qps`), a
compact per-iteration latency sparkline, and deterministic A/B regression gates
that never silently persist DML mutations.

Feature 056 introduces a pure, zero-I/O query benchmark statistics and
comparison engine (`src/bench/`) paired with a rollback-safe asynchronous
PostgreSQL runner (`src/postgres/bench.rs`) compatible with PostgreSQL 14
through 18.

## 2. User Scenarios

### Scenario 1: Benchmarking a single query safely

An operator benchmarks a `SELECT`, `INSERT`, `UPDATE`, or `DELETE` statement
with configurable warmup iterations (`0..=20`, default `2`) and measured
iterations (`1..=200`, default `10`). The runner validates that exactly one
non-empty SQL statement was supplied, runs warmup iterations first without
recording them, times each measured iteration with `std::time::Instant::now()`,
and when `rollback == true` (the default) wraps every iteration in `BEGIN` and
an unconditional `ROLLBACK` (on both success and SQL error) so DML mutations
never persist.

### Scenario 2: A/B benchmark comparison and regression gate

An operator compares a baseline query (`A`) against a candidate query (`B`),
optionally supplying a regression threshold percentage (`threshold_pct`). The
comparison engine computes `speedup_ratio`, signed `p50` and `p95` millisecond
and percentage deltas (`candidate - baseline`), classifies a deterministic
`BenchVerdict` (`Faster`, `Slower`, `Unchanged`), flags `regression_detected`,
and sets `BenchReport.failed_gate` when the configured threshold is exceeded.

### Scenario 3: Scriptable ASCII reports and per-iteration sparklines

An operator inspects benchmark output as an aligned plain-ASCII table
(`render_table`), line-oriented plain text (`render_plain`), or ASCII-escaped
JSON (`render_json`). Every `BenchStats` includes a 12-bucket latency sparkline
across per-iteration durations in both strictly plain ASCII (` .:-=+*#%`) and
opt-in Unicode blocks (` \u2581\u2582\u2583\u2584\u2585\u2586\u2587\u2588`).

## 3. Functional Requirements

- **FR-5601**: `src/bench/mod.rs` defines `DEFAULT_WARMUP_RUNS = 2`,
  `MAX_WARMUP_RUNS = 20`, `DEFAULT_MEASURED_RUNS = 10`,
  `MAX_MEASURED_RUNS = 200`, and `BENCH_SPARKLINE_BUCKETS = 12`.
- **FR-5602**: `BenchConfig` (`warmup_runs`, `measured_runs`, `rollback`)
  provides `Default` (`2`, `10`, `true`) and clamped constructors bounding
  `warmup_runs` to `0..=20` and `measured_runs` to `1..=200`.
- **FR-5603**: `BenchSample` captures `iteration`, `duration_ms`, and
  `rows_returned`.
- **FR-5604**: `BenchStats` captures `label`, `sql_preview`, `warmup_runs`,
  `measured_runs`, `min_ms`, `mean_ms`, `stddev_ms`, `p50_ms`, `p95_ms`,
  `p99_ms`, `max_ms`, `qps`, `total_rows`, `rollback`, `samples`,
  `sparkline_ascii`, and `sparkline_unicode`.
- **FR-5605**: `compute_bench_stats` computes Welford mean and population
  standard deviation, linear-interpolated `p50`/`p95`/`p99` quantiles, finite
  `qps`, credential-redacted bounded `label` and `sql_preview`, and 12-bucket
  per-iteration sparklines (` .:-=+*#%` ASCII, Unicode block ramp).
- **FR-5606**: `compare_benchmarks(baseline, candidate, threshold_pct)` returns
  `BenchReport` containing `BenchComparison` (`speedup_ratio`, `p50_delta_ms`,
  `p50_delta_pct`, `p95_delta_ms`, `p95_delta_pct`, `regression_detected`,
  `verdict`: `Faster`/`Slower`/`Unchanged`) when `candidate` is `Some`, and
  computes `failed_gate` when `threshold_pct` is configured and exceeded.
- **FR-5607**: `BenchReport` implements `render_table()`, `render_plain()`, and
  `render_json()`, all emitting strictly plain ASCII output.
- **FR-5608**: `src/postgres/bench.rs` exposes
  `run_benchmark(client, sql, config, label) -> Result<BenchStats, Diagnostic>`,
  validates a single non-empty statement via `crate::query::statements::split`,
  executes warmup and measured runs, counts returned/affected rows via the
  simple-query stream, and brackets every iteration with `BEGIN` and
  unconditional `ROLLBACK` on both `Ok` and `Err` when `config.rollback == true`.
- **FR-5609**: All SQL and driver errors map through
  `crate::postgres::error::from_query_error`, preserving SQLSTATE, position,
  and cleanup `RollbackError` details without automatic retries.

## 4. Non-Functional Requirements

- Pure `src/bench/` domain with zero network, filesystem, process, or
  terminal I/O dependencies.
- Zero `unsafe` code and zero new Cargo dependencies; compiles under Rust 1.90+
  (edition 2024).
- Strictly plain ASCII output across `render_table`, `render_plain`, and
  `render_json`, with opt-in `sparkline_unicode` stored separately on `BenchStats`.
- Labels and SQL previews pass through `crate::diagnostics::redaction` and
  `crate::query::value::sanitize_for_display`.
- Zero clippy warnings (`#[warn(clippy::all)]`) and sidecar tests in
  `src/bench/tests.rs` (CodeQL compliance).

## 5. Out of Scope

- Concurrent multi-client load generation or connection-pool stress testing.
- Silent persistence of benchmark writes when `rollback == true`.
- Phase 2 wiring in `src/lib.rs`, `src/postgres/mod.rs`, `src/postgres/session.rs`,
  CLI subcommands, and TUI overlays (owned by `tea-wire`).
