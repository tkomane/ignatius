# Implementation Plan: Query Benchmarks & Regression Suite (056)

## 1. Pure Domain & Statistics Engine (`src/bench/mod.rs`)

- **Constants & Bounds**:
  - `DEFAULT_WARMUP_RUNS = 2`, `MAX_WARMUP_RUNS = 20`.
  - `DEFAULT_MEASURED_RUNS = 10`, `MAX_MEASURED_RUNS = 200`.
  - `BENCH_SPARKLINE_BUCKETS = 12`.
  - `MAX_BENCH_LABEL_CHARS = 120`, `MAX_SQL_PREVIEW_CHARS = 120`.
- **Configuration & Sample Types**:
  - `BenchConfig { warmup_runs, measured_runs, rollback }` with `Default`
    (`2`, `10`, `true`), `const fn new(...)`, `const fn clamped(self)`, and
    builder helpers.
  - `BenchSample { iteration, duration_ms, rows_returned }`.
  - `BenchStats` with sanitized/redacted `label` and `sql_preview`, latency
    summary (`min_ms`, `mean_ms`, `stddev_ms`, `p50_ms`, `p95_ms`, `p99_ms`,
    `max_ms`), `qps`, `total_rows`, `rollback`, `samples`, `sparkline_ascii`,
    and `sparkline_unicode`.
- **Statistical Computation (`compute_bench_stats`)**:
  - Sanitize incoming durations (`0.0` floor for non-finite/negative values).
  - Compute mean and population standard deviation in a single Welford pass.
  - Sort a working copy of durations via `f64::total_cmp` and compute
    linear-interpolated `p50_ms`, `p95_ms`, and `p99_ms`.
  - Compute finite `qps = 1000.0 / mean_ms` when `mean_ms > 1e-12` (else `0.0`).
  - Resample per-iteration latencies into `BENCH_SPARKLINE_BUCKETS = 12`
    chronological buckets (sampling for `n <= 12`, bucket mean for `n > 12`)
    and scale onto `' .:-=+*#%` (ASCII) and `' \u2581\u2582\u2583\u2584\u2585\u2586\u2587\u2588'`
    (Unicode).
- **A/B Comparison & Gate Engine (`compare_benchmarks`)**:
  - `BenchVerdict`: `Faster`, `Slower`, `Unchanged`.
  - `BenchComparison`: `speedup_ratio` (`baseline.p50 / candidate.p50`, with
    mean fallback and finite `1.0` zero-latency guard), `p50_delta_ms`,
    `p50_delta_pct`, `p95_delta_ms`, `p95_delta_pct` (`candidate - baseline`),
    `regression_detected`, `verdict`.
  - `BenchReport`: optional `candidate` and `comparison`, normalized
    `threshold_pct`, and `failed_gate` (true when a threshold gate is set and
    `regression_detected` is true).
  - Renderers on `BenchReport`: strictly plain-ASCII `render_table()`,
    `render_plain()`, and `render_json()`.

## 2. Rollback-Safe Asynchronous Runner (`src/postgres/bench.rs`)

- `pub async fn run_benchmark(client: &Client, sql: &str, config: &BenchConfig, label: &str) -> Result<BenchStats, Diagnostic>`:
  1. Validate exactly one non-empty statement using
     `crate::query::statements::split`.
  2. Clamp configuration via `config.clamped()`.
  3. Execute `warmup_runs` unrecorded iterations, then `measured_runs`
     iterations timed with `std::time::Instant::now()`.
  4. For each iteration:
     - When `rollback == true`: run `client.simple_query("BEGIN")`, execute the
       query via `client.simple_query_raw(stmt_text)` counting `Row` and
       `CommandComplete` rows in O(1) memory, and unconditionally issue
       `client.simple_query("ROLLBACK")` on both `Ok` and `Err` (preserving
       `RollbackError` technical field on error, matching `src/postgres/plandiff.rs`).
     - When `rollback == false`: execute the statement directly without
       transaction bracketing.
  5. Map driver and server errors via `crate::postgres::error::from_query_error`
     and aggregate samples via `crate::bench::compute_bench_stats`.

## 3. Sidecar Unit Tests (`src/bench/tests.rs`)

- Configuration defaults and `[0..=20]` / `[1..=200]` clamping.
- Empty, single-sample, and multi-sample Welford mean/stddev, linear
  percentiles (`p50`/`p95`/`p99`), and finite QPS.
- 12-bucket ASCII and Unicode sparkline accuracy for 0, 1, 10, 12, and 36
  samples, including flat and spiked latency traces.
- A/B comparison across `Faster`, `Slower`, and `Unchanged` verdicts, p95 tail
  regressions, threshold pass/fail gate behavior, and single-query reports.
- Label/SQL credential redaction and strictly plain-ASCII + JSON roundtrip
  guarantees on `render_table()`, `render_plain()`, and `render_json()`.

## 4. Coordinator Integration (Phase 2 - `tea-wire`)

- Export `pub mod bench;` in `src/lib.rs` and `pub mod bench;` in
  `src/postgres/mod.rs`.
- Add `Session::run_benchmark` in `src/postgres/session.rs` and wire CLI/TUI
  surfaces and `tests/bench_contract.rs`.
