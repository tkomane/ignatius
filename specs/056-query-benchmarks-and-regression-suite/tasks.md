# Tasks: Query Benchmarks & Regression Suite (056)

- [x] T001: Author `specs/056-query-benchmarks-and-regression-suite/spec.md`, `plan.md`, and `tasks.md`.
- [x] T002: Implement `src/bench/mod.rs` pure domain model, clamped `BenchConfig`, Welford/percentile `compute_bench_stats`, 12-bucket ASCII/Unicode sparklines, `compare_benchmarks`, and strictly plain-ASCII `BenchReport::{render_table, render_plain, render_json}`.
- [x] T003: Implement `src/bench/tests.rs` sidecar unit tests covering clamping, statistical math, 12-bucket sparklines, comparison verdicts/thresholds, credential redaction, and ASCII/JSON rendering.
- [x] T004: Implement `src/postgres/bench.rs` with `run_benchmark` supporting single-statement validation, warmup/measured iterations, `Instant::now()` timing, and unconditional `BEGIN`/`ROLLBACK` cleanup when `rollback == true`.
- [x] T005: Coordinator wiring (`tea-wire`) in `src/lib.rs`, `src/postgres/mod.rs`, `src/postgres/session.rs`, `src/cli/*`, `src/app/*`, and `src/ui/*`.
- [x] T006: Run full `cargo xtask verify` and live PostgreSQL integration contract (`tests/bench_contract.rs`) once wired.
