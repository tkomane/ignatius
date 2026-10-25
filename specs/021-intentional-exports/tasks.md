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

## W10-010 follow-up - 2026-10-25

- [x] T023 Add failing-first complete-row progress and abandonment-flush
  regressions through real encoder/runtime/export boundaries, following the
  pinned contract. Keep injected I/O failures separate from native full-disk
  evidence and use no production trigger.
- [x] T024 Repair both premature counts and introduce typed flushed/unconfirmed
  partial-file reporting without changing successful bytes, exit codes,
  transaction/cancellation semantics or retrying any row/SQL.
- [x] T025 Review the patch and run affected output/export/runtime/live checks,
  inspect partial-file/count evidence, update user/recovery/release docs and
  record the combined full gate with remaining platform boundaries.

T023-T025 evidence on 2026-10-25: six behavioral reds/four controls preceded
the repair; 24 writer faults cover before/mid/terminator failures on first and
later rows across four formats. Final private output 30/export 11/runtime 29
and three SELECT-only live contracts passed (73 cases, zero skips), with
Clippy/fmt. All 33 original moved tests were independently checked unchanged.
Primary retained existing best-effort Drop recovery rather than discarding
accepted buffered data. Source/evidence/patch SHA:
`/private/tmp/ignatius-w10-010-evidence-20261025.txt`.

Primary and a separate native peer reviewed the applied patch without a
remaining blocker. The combined modified `dd69f66` source passed the final
locked verifier: 1,315 harness cases/1,027 library, PostgreSQL 18.6 plain/TLS,
formatting, warnings-denied lints/API docs. Log:
`/private/tmp/ignatius-verify-2026-10-25-help-and-export-final.log`.
macOS 27.0.1 arm64/Homebrew Rust 1.99.1; socket remains a semantic skip.
Injected I/O failures are not native full-disk evidence. The frozen Linux
`871db73` proof excludes this repair; current-source Linux/MSRV/Windows,
hosted and native hand use remain separate.
