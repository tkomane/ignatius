# Tasks: Transaction & WAL Observability (Feature 036)

- [x] T001: Implement pure domain models in `src/telemetry/mod.rs` (`CheckpointerStats`, `WalStats`, `DatabaseXactStats`, `XidFreezeMetric`, `ReplicationStandby`, `TelemetryReport`, aligned table/JSON/plain text formatting).
- [x] T002: Add pure unit tests in sidecar `src/telemetry/tests.rs` (rollback calculations, checkpointer timed ratios, XID wraparound percentages, JSON serialization, and table renderers).
- [x] T003: Implement catalog extractors in `src/postgres/metadata.rs` and method `Session::wal_and_transaction_telemetry` in `src/postgres/session.rs`.
- [x] T004: Register keymap chord `Ctrl+K &` and configurable action `wal-observability` in `src/ui/keymap.rs` and `docs/design/keymap.md`.
- [x] T005: Integrate TEA messages, action dispatch, and state in `src/app/model.rs`, `src/app/message.rs`, and `src/app/update.rs`.
- [x] T006: Build the Field Industrial TUI Transaction & WAL Observability overlay in `src/ui/widgets/overlays.rs` and wire into `src/ui/layout.rs`.
- [x] T007: Implement the `ignatius wal-stats` CLI subcommand in `src/cli/mod.rs`.
- [x] T008: Add CLI contract integration tests in `tests/wal_observability_contract.rs`.
- [x] T009: Run documentation contract tests and full verification battery (`cargo xtask verify`).
- [x] T010: Update `docs/status.md` and `CHANGELOG.md` with Wave 28 delivery evidence.
