# Tasks: Extended EXPLAIN (BUFFERS, WAL, SETTINGS) & A/B Plan Diff (054)

- [x] T001: Author `specs/054-explain-buffers-wal-and-plan-diff/spec.md`, `plan.md`, and `tasks.md`.
- [x] T002: Implement `src/plandiff/mod.rs` pure domain model, extended JSON parser (`parse_extended_explain_json`), A/B diff engine (`diff_plans`), and strictly plain-ASCII `render_table`, `render_plain`, and `render_json` methods.
- [x] T003: Implement `src/plandiff/tests.rs` sidecar unit tests covering PG 16/18 JSON fixtures with Buffers + WAL + Settings, `ResponseTooLarge`, plan A/B improvement/regression/mixed/unchanged verdicts, settings diff, and ASCII/JSON output.
- [x] T004: Implement `src/postgres/plandiff.rs` with `explain_extended` supporting estimate-only and rollback-safe `BEGIN` / `EXPLAIN (ANALYZE, BUFFERS, WAL, SETTINGS, FORMAT JSON)` / `ROLLBACK`.
- [x] T005: Coordinator wiring in `src/lib.rs`, `src/postgres/mod.rs`, `src/postgres/session.rs`, `src/cli/*`, and `src/app/*`.
- [x] T006: Run full `cargo xtask verify` and live PostgreSQL integration tests once wired.
