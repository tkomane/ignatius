# Tasks: SQL Linter and Safety Analyzer (044)

- [x] T001: Implement `src/lint/mod.rs` rules, masking, splitting, bounds and report model.
- [x] T002: Add `src/lint/tests.rs` sidecar tests for every rule, protected regions, mapping, ordering and bounds.
- [x] T003: Add `ExitCode::Findings` with compatibility and README documentation.
- [x] T004: Add `ignatius lint` CLI (`table|json|sarif`, thresholds, stdin, directories, atomic output).
- [x] T005: Add `tests/lint_contract.rs` covering exit codes, formats, stdin, directories and file safety.
- [x] T006: Add `Action::LintBuffer`, model state, discovery, keymap `Ctrl+K !` and reducer navigation.
- [x] T007: Add the findings overlay renderer with compact/ASCII forms and tests.
- [x] T008: Document the rule catalogue, keymap and README surfaces; wire the documentation contract.
- [x] T009: Run focused gates and the full `cargo xtask verify` battery.
- [x] T010: Update `docs/status.md` and `CHANGELOG.md` with delivery evidence.
