# Tasks: Synthetic Test Fixture Generator (Feature 040)

- [x] T001: Implement pure domain models and deterministic PRNG fixture generator in `src/seed/mod.rs` (`SeedStrategy`, `ColumnGenerator`, `TableGenerator`, `DeterministicPrng`, `FixtureReport`, and SQL/JSONL/CSV/table formatters).
- [x] T002: Add pure unit tests in sidecar `src/seed/tests.rs` (PRNG determinism, strategy generation, foreign key sampling, formatters).
- [x] T003: Implement catalog inspection in `src/postgres/metadata.rs` and execution methods in `src/postgres/session.rs`.
- [x] T004: Register keymap chord `Ctrl+K +` and configurable action `synthetic-seed` in `src/ui/keymap.rs` and `docs/design/keymap.md`.
- [x] T005: Integrate TEA messages, action dispatch, and state in `src/app/model.rs`, `src/app/message.rs`, and `src/app/update.rs`.
- [x] T006: Build the Field Industrial TUI Synthetic Seeding Cockpit overlay in `src/ui/widgets/overlays.rs` and wire into `src/ui/layout.rs`.
- [x] T007: Implement the `ignatius seed` CLI subcommand in `src/cli/mod.rs`.
- [x] T008: Add CLI contract integration tests in `tests/synthetic_seed_contract.rs`.
- [x] T009: Run documentation contract tests and full verification battery (`cargo xtask verify`).
- [x] T010: Update `docs/status.md` and `CHANGELOG.md` with Wave 32 delivery evidence.
