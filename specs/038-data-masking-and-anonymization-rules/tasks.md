# Tasks: Data Masking and Anonymization Rules (Feature 038)

- [x] T001: Implement pure domain models in `src/mask/mod.rs` (`MaskRule`, `PiiType`, `PiiCandidate`, `TableMaskPolicy`, `MaskReport`, deterministic SQL expression synthesis, and formatters).
- [x] T002: Add pure unit tests in sidecar `src/mask/tests.rs` (heuristic PII detection, deterministic expression generation, view SQL synthesis, and JSON serialization).
- [x] T003: Implement catalog extractors in `src/postgres/metadata.rs` and method `Session::analyze_data_masking` in `src/postgres/session.rs`.
- [x] T004: Register keymap chord `Ctrl+K )` and configurable action `data-masking` in `src/ui/keymap.rs` and `docs/design/keymap.md`.
- [x] T005: Integrate TEA messages, action dispatch, and state in `src/app/model.rs`, `src/app/message.rs`, and `src/app/update.rs`.
- [x] T006: Build the Field Industrial TUI Data Masking & Anonymization overlay in `src/ui/widgets/overlays.rs` and wire into `src/ui/layout.rs`.
- [x] T007: Implement the `ignatius mask-rules` CLI subcommand in `src/cli/mod.rs`.
- [x] T008: Add CLI contract integration tests in `tests/mask_rules_contract.rs`.
- [x] T009: Run documentation contract tests and full verification battery (`cargo xtask verify`).
- [x] T010: Update `docs/status.md` and `CHANGELOG.md` with Wave 30 delivery evidence.
