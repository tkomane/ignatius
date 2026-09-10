# Tasks: Intentional export shapes

**Input**: Design documents from `/specs/021-intentional-exports/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md,
contracts/exports.md, quickstart.md

**Tests**: Included because the specification requires byte-stable output,
explicit SQL safety, deterministic interactive choice and no early file effect.

## Phase 1: Setup

- [x] T001 Register the Feature 021 output contract and update `.specify/feature.json`.
- [x] T002 Record the existing output and atomic-export boundaries without changing current formats.

## Phase 2: Interactive format choice

- [x] T003 [P] Add an application-level `ExportFormat` enum for CSV, TSV, JSON, NDJSON and Markdown.
- [x] T004 [P] Add the format-purpose palette and typed `ChooseExportFormat` command.
- [x] T005 Add the selected format to the path prompt and `Effect::ExportRows` boundary.
- [x] T006 Route `ExportRows` through the format palette while preserving retained/filter row scope.
- [x] T007 [P] Add reducer tests proving opening/searching/choosing a format performs no I/O effect before path acceptance.
- [x] T008 [P] Add layout tests for format choice, retained-row scope, compact, ASCII, no-colour and reduced-motion wording.

## Phase 3: CLI INSERT output

- [x] T009 Add `Format::Insert`, `insert_table` output options and the `--insert-table` CLI flag.
- [x] T010 Add pre-connection validation for missing/empty table, duplicate columns and `--no-header`.
- [x] T011 [P] Implement quoted identifier and SQL literal encoding, including NULL, empty text, controls, Unicode and NUL refusal.
- [x] T012 [P] Add buffered INSERT output for one or multiple result sets with deterministic boundaries.
- [x] T013 Add streaming INSERT output through the existing partial-file writer.
- [x] T014 Add buffered/streamed byte-equivalence and adversarial literal tests.
- [x] T015 Add machine-output and existing-format regression tests proving no decoration or drift.

**Checkpoint**: A script can request explicit INSERT data safely, and an
interactive user can choose a file shape before naming its destination.

## Phase 4: Integration and documentation

- [x] T016 [P] Add CLI contract coverage for INSERT validation, output and unchanged plain/machine routes.
- [x] T017 [P] Add documentation contract assertions for new format names and boundaries.
- [x] T018 Update compatibility, interaction, keymap/journey, local-development, roadmap, experience-roadmap and changelog docs.
- [x] T019 Review output value redaction and changed-file hyphen rules.
- [x] T020 Run focused checks and record each result separately.
- [x] T021 Start disposable PostgreSQL, run `cargo --locked xtask verify`, record versions and every skip, tear down and verify status.
- [x] T022 Review `git diff --check`, task/spec consistency and dirty-worktree scope; tick only evidenced tasks.

## Dependencies

- Phase 2 depends on the existing palette and name-prompt boundaries.
- Phase 3 depends on the existing `Format`, `OutputOptions`, `Cell` and `Export` contracts.
- Phase 4 depends on both interactive and CLI paths being complete.

## Notes

- `[P]` marks work that can proceed on separate files or boundaries.
- Existing Features 013 through 020 working-tree changes are preserved.
- No task authorizes commit, push, tag, signing, release or publication.
