# Tasks: Schema Snapshots & Drift Guard (057)

- [x] T001: Author `specs/057-schema-snapshots-and-drift-guard/spec.md`, `plan.md`, and `tasks.md`.
- [x] T002: Implement `src/snapshot/mod.rs` pure canonical snapshot model, SHA-256 finalizer, drift diff engine (`diff_snapshots`), threshold parser (`DriftFailThreshold::parse_loose`), and strictly plain-ASCII `render_table`, `render_plain`, and `render_json` renderers.
- [x] T003: Implement `src/snapshot/tests.rs` sidecar unit tests covering canonical hashing, all `Breaking`/`Warning`/`Info` drift rules, threshold evaluation, bounds truncation, JSON round-trip, and plain-ASCII sanitization.
- [x] T004: Implement `src/postgres/snapshot.rs` with PostgreSQL 14-18 compatible `capture_schema_snapshot(client, schema)` reading zero table rows and zero credentials.
- [x] T005: Coordinator (`tea-wire`) wiring in `src/lib.rs`, `src/postgres/mod.rs`, `src/postgres/session.rs`, `src/cli/*`, `src/app/*`, and `src/ui/*`.
- [x] T006: Run `cargo xtask verify` (formatting, clippy, unit tests, CLI contract, and live PostgreSQL integration tests) once wired.
