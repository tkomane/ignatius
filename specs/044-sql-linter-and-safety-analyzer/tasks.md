# Tasks: SQL Linter and Safety Analyzer (044)

- [ ] T001: Implement `src/lint/mod.rs` rules, masking, splitting, bounds and report model.
- [ ] T002: Add `src/lint/tests.rs` sidecar tests for every rule, protected regions, mapping, ordering and bounds.
- [ ] T003: Add `ExitCode::Findings` with compatibility and README documentation.
- [ ] T004: Add `ignatius lint` CLI (`table|json|sarif`, thresholds, stdin, directories, atomic output).
- [ ] T005: Add `tests/lint_contract.rs` covering exit codes, formats, stdin, directories and file safety.
- [ ] T006: Add `Action::LintBuffer`, model state, discovery, keymap `Ctrl+K !` and reducer navigation.
- [ ] T007: Add the findings overlay renderer with compact/ASCII forms and tests.
- [ ] T008: Document the rule catalogue, keymap and README surfaces; wire the documentation contract.
- [ ] T009: Run focused gates and the full `cargo xtask verify` battery.
- [ ] T010: Update `docs/status.md` and `CHANGELOG.md` with delivery evidence.
