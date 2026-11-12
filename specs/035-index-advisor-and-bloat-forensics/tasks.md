# Tasks: Index Advisor & Bloat Forensics (Feature 035)

- [x] T001: Implement pure domain data models and calculations in `src/advisor/mod.rs` (`AdvisorReport`, `RedundantIndex`, `IndexBloatMetric`, `UnindexedForeignKey`, `HypoIndexTestResult`, formatting).
- [x] T002: Add unit tests in sidecar `src/advisor/tests.rs` (duplicate detection, prefix-redundancy checks, bloat estimation formulas, JSON and table rendering).
- [x] T003: Implement catalog extractors in `src/postgres/metadata.rs` and methods in `src/postgres/session.rs` (`Session::analyze_indexes`, `Session::test_hypothetical_index`).
- [x] T004: Register keymap chord `Ctrl+K ^` and configurable action `index-advisor` in `src/ui/keymap.rs` and `docs/design/keymap.md`.
- [x] T005: Integrate TEA messages, action dispatch, and state in `src/app/model.rs`, `src/app/message.rs`, and `src/app/update.rs`.
- [x] T006: Build the Field Industrial TUI Index Advisor overlay widget in `src/ui/widgets/overlays/advisor.rs`.
- [x] T007: Implement the `ignatius index-advisor` CLI subcommand in `src/cli/mod.rs`.
- [x] T008: Add CLI contract integration tests in `tests/index_advisor_contract.rs`.
- [x] T009: Run documentation contract tests and full verification battery (`cargo xtask verify`).
- [x] T010: Update `docs/status.md` and `CHANGELOG.md` with Wave 27 delivery evidence.
