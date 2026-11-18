# Tasks: Logical Replication and Slot Safety Manager (051)

- [x] T001: Implement `src/replication/mod.rs` pure domain model, risk assessment, redaction, bounds, renderers, and reviewed SQL generators.
- [x] T002: Add `src/replication/tests.rs` sidecar unit tests for conninfo redaction, slot risk thresholds, truncation reporting, renderers, and SQL quoting safety.
- [x] T003: Implement `src/postgres/replication.rs` bounded catalog extractor for publications, subscriptions (with SQLSTATE `42501` fallback), and replication slots (PG 14-18 & recovery-safe).
- [x] T004: Wire `src/replication` and `src/postgres/replication.rs` in `src/lib.rs`, `src/postgres/mod.rs`, and `src/postgres/session.rs` (coordinator).
- [x] T005: Add `ignatius replication` CLI commands and interactive TUI/overlay integration in `src/cli/` and `src/app/` (coordinator).
- [x] T006: Run focused gates, live PostgreSQL contract tests, `cargo xtask verify`, and update `docs/status.md`, roadmap, and changelog (coordinator).
