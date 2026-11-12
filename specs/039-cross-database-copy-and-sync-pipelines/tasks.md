# Tasks: Cross-Database Copy and Sync Pipelines (Feature 039)

- [x] T001: Implement pure domain models and topological sorting in `src/sync/mod.rs` (`DependencyNode`, `DependencyGraph`, `SyncPlan`, `SyncTableProgress`, `SyncReport`, and tabular/JSON/plain formatters).
- [x] T002: Add pure unit tests in sidecar `src/sync/tests.rs` (topological DAG ordering, cycle detection, formatters).
- [x] T003: Implement catalog extractors and streaming execution in `src/postgres/metadata.rs` and methods in `src/postgres/session.rs`.
- [x] T004: Register keymap chord `Ctrl+K |` and configurable action `data-sync` in `src/ui/keymap.rs` and `docs/design/keymap.md`.
- [x] T005: Integrate TEA messages, action dispatch, and state in `src/app/model.rs`, `src/app/message.rs`, and `src/app/update.rs`.
- [x] T006: Build the Field Industrial TUI Data Sync Cockpit overlay in `src/ui/widgets/overlays.rs` and wire into `src/ui/layout.rs`.
- [x] T007: Implement the `ignatius sync` CLI subcommand in `src/cli/mod.rs`.
- [x] T008: Add CLI contract integration tests in `tests/sync_pipeline_contract.rs`.
- [x] T009: Run documentation contract tests and full verification battery (`cargo xtask verify`).
- [x] T010: Update `docs/status.md` and `CHANGELOG.md` with Wave 31 delivery evidence.
