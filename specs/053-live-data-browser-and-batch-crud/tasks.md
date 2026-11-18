# Tasks: Live Data Browser and Batch CRUD (053)

- [x] T001: Author `specs/053-live-data-browser-and-batch-crud/spec.md`, `plan.md`, and `tasks.md`.
- [x] T002: Implement `src/crud/mod.rs` pure keyset pagination SQL builder, filter predicate validator, guarded batch CRUD SQL synthesizer, and plain-ASCII `BrowsePage` renderers.
- [x] T003: Add `src/crud/tests.rs` sidecar unit tests covering pagination, order flipping, SQL injection refusal, batch CRUD PK guards, literal escaping, and renderers.
- [x] T004: Implement `src/postgres/crud.rs` async `inspect_table_columns_and_pk` and `browse_table_page` extractors returning `ResultSet`, `BrowsePage`, and derived keyset cursors.
- [x] T005: Wire `crud` and `postgres::crud` in `src/lib.rs`, `src/postgres/mod.rs`, and `src/postgres/session.rs` (coordinator).
- [x] T006: Integrate TUI live data browser / batch CRUD editor and CLI subcommand in `src/cli/`, `src/app/`, and `src/ui/` (coordinator).
- [x] T007: Run `cargo xtask verify` and live PostgreSQL contract tests, and update `docs/status.md` (coordinator).
