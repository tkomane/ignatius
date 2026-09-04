# Tasks: Format the SQL buffer

**Input**: Design documents from `/specs/019-format-buffer/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md,
contracts/format-buffer.md, quickstart.md

**Tests**: Included because the specification requires deterministic formatting,
protected-byte preservation, cursor and undo recovery, accessible discovery,
plain-mode parity, and no-side-effect evidence.

## Phase 1: Setup

**Purpose**: Establish the feature traceability surfaces without changing query
execution.

- [x] T001 [P] Register the pure formatter module in `src/query/mod.rs` and re-export only the documented formatter types.
- [x] T002 [P] Add Feature019 references to the documentation/build contract test inventory in `tests/documentation_matches_the_build.rs`.

---

## Phase 2: Foundational formatter

**Purpose**: Build the bounded lexical and layout core before integrating user
actions.

**CRITICAL**: No user-facing action work begins until this phase is complete.

### Tests first

- [x] T003 [P] Add pure formatter tests in `src/query/format.rs` for clause layout, two-space lists, multiple statements, operator spacing, and idempotence.
- [x] T004 [P] Add protected-region tests in `src/query/format.rs` for ordinary and escape strings, quoted identifiers, dollar bodies, line comments, nested block comments, Unicode, and control-looking text.
- [x] T005 [P] Add refusal and bound tests in `src/query/format.rs` for empty/comment-only input, every unterminated region kind, invalid cursor boundaries, the inclusive 1 MiB limit, and over-limit input.
- [x] T006 [P] Add cursor-map tests in `src/query/format.rs` proving token-relative mapping and UTF-8-safe output positions.

### Implementation

- [x] T007 Implement the bounded PostgreSQL-aware lexical scanner, protected-region recognition, safe `FormatError`, and `FormattedSql` result in `src/query/format.rs`.
- [x] T008 Implement deterministic whitespace, clause, predicate, list, operator, comment, parenthesis, semicolon, and two-space indentation rules in `src/query/format.rs`.
- [x] T009 Implement source-to-output cursor mapping and idempotent finalization in `src/query/format.rs`.

**Checkpoint**: The pure formatter is bounded, protected bytes are unchanged,
malformed input is untouched, output is idempotent, and every returned cursor is
a valid UTF-8 boundary.

---

## Phase 3: User Story 1 - Make a query readable in one action (Priority: P1)

**Goal**: Apply the pure formatter to the interactive editor without any
database or file effect.

### Tests first

- [x] T010 [P] [US1] Add editor tests for a whole-buffer format replacement, one undo/redo step, modified-state preservation, no-op revision preservation, and mapped cursor recovery in `src/app/editor.rs`.
- [x] T011 [P] [US1] Add reducer tests for `FormatBuffer` success, already-formatted, empty, and no-effect transitions in `src/app/update.rs`.
- [x] T012 [P] [US1] Add keymap tests for `Ctrl+Shift+F`, `Ctrl+K q`, the configurable `format-buffer` action, and printable `f` preservation in `src/ui/keymap.rs`.
- [x] T013 [P] [US1] Add discovery and palette tests for Format SQL availability, intent wording, editor grouping, and the SQL prerequisite in `src/app/discovery.rs` and `src/app/update.rs`.

### Implementation

- [x] T014 Add `FormatBuffer` to `Action`, exhaustive action lists, and the reducer's local format transition in `src/app/message.rs` and `src/app/update.rs`.
- [x] T015 Add value-free `FormatNotice` state, messages, and clearing behavior to `src/app/model.rs` and the shared editor mutation path.
- [x] T016 Add the default `Ctrl+Shift+F` binding, `Ctrl+K q` chord, configurable action name, short label, and help metadata in `src/ui/keymap.rs`.
- [x] T017 Add Format SQL to palette entries, context availability, prerequisites, contextual hints, and the Editor palette group in `src/app/discovery.rs` and `src/app/update.rs`.
- [x] T018 Render applied, already-formatted, empty, and refused outcomes as readable footer/editor status text in `src/ui/layout.rs`.

**Checkpoint**: A connected or disconnected local editor can format and undo the
whole buffer through the shortcut, chord, or palette, with no effect returned
other than the local editor state transition.

---

## Phase 4: User Story 2 - Keep my place and my escape hatch (Priority: P1)

**Goal**: Preserve logical cursor position and stale-error honesty.

### Tests first

- [x] T019 [P] [US2] Add reducer coverage proving a changed format invalidates an old error location while an unchanged format does not invent a new marker in `src/app/update.rs`.
- [x] T020 [P] [US2] Add editor property-style fixture coverage for ASCII and Unicode cursor positions staying on character boundaries after formatting in `src/app/editor.rs`.

### Implementation

- [x] T021 [US2] Apply the mapped cursor after the single editor replacement without creating a second undo entry in `src/app/update.rs`.
- [x] T022 [US2] Preserve existing modal precedence so formatting cannot edit the hidden buffer while confirmations, palettes, completion, filters, inspector, definitions, or trust details are active in `src/app/update.rs`.

**Checkpoint**: Formatting is one reversible edit and never leaves a stale
server position presented as current.

---

## Phase 5: User Story 3 - Preserve meaning in every terminal tier (Priority: P1)

**Goal**: Provide plain-mode parity and explicit accessible refusal wording.

### Tests first

- [x] T023 [P] [US3] Add `Reader` and help-text tests for the out-of-band `\\format` command while a SQL buffer is pending and its non-interception inside protected text in `src/cli/plain.rs`.
- [x] T024 [P] [US3] Add plain-mode formatting tests proving the pending buffer changes only on success, the preview uses the message stream, and no result stdout is produced in `src/cli/plain.rs`.
- [x] T025 [P] [US3] Add layout tests for all format outcomes in full, compact, narrow, ASCII, no-colour, and reduced-motion presentations in `src/ui/layout.rs`.
- [x] T026 [P] [US3] Add CLI/documentation regression assertions proving formatting adds no execution, history, export, JSON, NDJSON, or machine-output behavior in `tests/cli_contract.rs` and `tests/documentation_matches_the_build.rs`.

### Implementation

- [x] T027 [US3] Add the `Format` plain command, out-of-band command recognition, safe preview output, and pending-buffer replacement in `src/cli/plain.rs`.
- [x] T028 [US3] Add explicit refusal wording with kind, location, size, and next action while excluding raw SQL and terminal controls in `src/query/format.rs` and `src/cli/plain.rs`.
- [x] T029 [US3] Update keymap, interaction, accessibility, compatibility, operations, product journey, roadmap, and changelog documentation for the format contract.

**Checkpoint**: Full-screen and plain workflows expose the same local formatting
capability, protected contents remain exact, and result stdout remains data-only.

---

## Phase 6: Polish and evidence

- [x] T030 [P] Complete the Feature019 documentation/build contract assertions and verify no new em dash or secret is introduced.
- [x] T031 [P] Update `docs/status.md` with focused evidence, the Feature019 state, and explicit manual terminal/screen-reader evidence boundaries.
- [x] T032 Run the focused checks from `specs/019-format-buffer/quickstart.md` and resolve formatter, reducer, UI, plain, or documentation drift.
- [x] T033 Run `cargo fmt --all -- --check`, `cargo check --locked`, and clippy with warnings denied.
- [x] T034 Start disposable PostgreSQL, run `cargo --locked xtask verify`, record every pass and skip, tear it down, and verify `cargo xtask db status` reports `Not running.`
- [x] T035 Review final dirty-worktree scope, task/spec/plan consistency, protected-byte tests, no-side-effect boundaries, and release-readiness separation without committing or publishing.

## Dependencies and execution order

- Setup tasks precede the formatter foundation.
- T003-T006 are tests-first and can be prepared together; T007-T009 implement
  their shared pure core sequentially.
- User Story 1 depends on the foundation; its tests precede T014-T018.
- User Story 2 depends on the editor integration and shares `src/app/update.rs`,
  so T019-T022 follow User Story 1 sequentially.
- User Story 3 depends on the action and renderer; its tests precede T027-T029.
- Polish and evidence follow all desired story behavior.

## Notes

- `[P]` means tasks use different files or have no dependency on another task in
  the same phase.
- Tasks do not authorize commit, push, tag, signing, release, or publication.
- Manual Warp, Windows Terminal, Linux, SSH, multiplexer, and screen-reader use
  remain evidence tasks and must not be inferred from automated renderer tests.
