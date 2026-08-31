# Tasks: Writing the SQL

**Input**: Design documents from `specs/005-sql-editing/`

**Prerequisites**: [spec.md](./spec.md), [plan.md](./plan.md),
[research.md](./research.md), [data-model.md](./data-model.md),
[quickstart.md](./quickstart.md)

**Tests**: Included because the feature specification defines independent
acceptance scenarios and the editor is a recoverability boundary.

**Status note**: Checked tasks are backed by the committed source, the active
acceptance contracts and the 2026-08-31 verifier.

**Scope reconciliation (2026-08-31)**: `spec.md` is the current feature
authority and includes colouring requirements FR-508 through FR-510, UX-505 and
SC-504 through SC-505. The narrower exclusions in `plan.md` and `research.md`
record the earlier design boundary and are superseded by the implemented spec.

## Phase 1: Setup and design package

- [x] T001 Record the editor scope, exclusions and open acceptance boundary in `specs/005-sql-editing/spec.md`
- [x] T002 Create the implementation plan and constitution check in `specs/005-sql-editing/plan.md`
- [x] T003 [P] Record editor decisions and unresolved evidence in `specs/005-sql-editing/research.md`
- [x] T004 [P] Define editor state, snapshots and cursor invariants in `specs/005-sql-editing/data-model.md`
- [x] T005 [P] Add the requirements-quality checklist in `specs/005-sql-editing/checklists/requirements.md`
- [x] T006 [P] Add the runnable validation guide in `specs/005-sql-editing/quickstart.md`

## Phase 2: Foundational editor model

**Purpose**: Establish safe text and history behavior before relying on the
terminal keymap.

- [x] T007 [P] [US1] Keep cursor offsets on UTF-8 character boundaries in `src/app/editor.rs`
- [x] T008 [P] [US1] Implement vertical goal-column, line, buffer, word and page movement in `src/app/editor.rs`
- [x] T009 [P] [US2] Implement bounded snapshot undo and redo in `src/app/editor.rs`
- [x] T010 [P] [US3] Implement forward, backward and word deletion without splitting characters in `src/app/editor.rs`
- [x] T011 [US1] Preserve line indentation in the editor's newline operation in `src/app/editor.rs`

**Checkpoint**: The pure editor model has focused unit coverage; reducer wiring
and real terminal acceptance remain separate gates.

## Phase 3: User Story 1 - Write a statement longer than one line (Priority: P1)

**Goal**: Move through a multi-line buffer, correct a word and preserve the
cursor-following window.

**Independent test**: Use the editor and reducer tests, then type and correct an
eight-line statement in a real pty.

- [x] T012 [US1] Route editor movement and page actions through the focus-aware reducer in `src/app/update.rs`
- [x] T013 [US1] Bind movement and line-break actions with readable descriptions in `src/ui/keymap.rs`
- [x] T014 [US1] Render line numbers, cursor position and a cursor-following window in `src/ui/layout.rs`
- [x] T015 [US1] Route Enter through `Editor::insert_newline` and add reducer coverage for indented input in `src/app/update.rs`
- [x] T016 [US1] Add the eight-line type, correction and run acceptance scenario in `tests/editor_contract.rs`

**Checkpoint**: The P1 editing journey is complete only when T015 and T016
pass, not merely when the model unit tests pass.

## Phase 4: User Story 2 - Take back a mistake (Priority: P1)

**Goal**: Recover a deletion or replacement without leaving the editor in an
ambiguous history state.

**Independent test**: Type, delete, move, type again, undo and redo through the
real action path.

- [x] T017 [US2] Route undo and redo actions only when the editor has focus in `src/app/update.rs`
- [x] T018 [US2] End the undo coalescing run at every cursor movement in `src/app/editor.rs`
- [x] T019 [US2] Add reducer coverage for undo grouping across a cursor movement in `src/app/update.rs`

**Checkpoint**: A movement cannot silently merge edits on opposite sides of the
cursor.

## Phase 5: User Story 3 - Move and delete by word (Priority: P2)

**Goal**: Treat identifier parts and punctuation as separate movement stops and
delete a previous word as one recoverable change.

**Independent test**: Exercise Ctrl and Alt word movement, word deletion and
undo from the editor focus.

- [x] T020 [US3] Bind Ctrl and Alt word movement plus word deletion in `src/ui/keymap.rs`
- [x] T021 [US3] Cover identifier and punctuation word boundaries in `src/app/editor.rs`
- [x] T022 [US3] Add focused keymap tests for Home, End, PageUp, PageDown, Delete, undo, redo and Ctrl/Alt word movement in `tests/keymap_contract.rs`

**Checkpoint**: Every advertised editing key has a tested action and description.

## Phase 6: Polish and evidence

- [x] T023 [P] Record the committed Feature 005 implementation and its evidence boundary in `docs/status.md`
- [x] T024 [P] Record 402 library, 31 CLI and 33 integration tests in `docs/operations/verification.md`
- [x] T025 Run the full verifier after the shared source work is stable and record the resulting counts in `docs/operations/verification.md`

Evidence on 2026-08-31: the active editor contract passes three tests. They
cover inherited indentation and execution through the reducer, undo grouping
across movement, and the eight-line correction journey through a real Unix pty
at 100 by 30. The keymap contract passes two tests covering Home, End, PageUp,
PageDown, Delete, undo, redo and both Ctrl/Alt word-movement directions. The
full verifier passes formatting, lints, 513 unit/layout tests, 38 CLI contract
tests and 38 PostgreSQL integration tests.

- [x] T026 Update the Feature 005 status only after T015, T016, T018, T019 and T022 are complete in `specs/005-sql-editing/spec.md`

## Dependencies and execution order

- Phase 1 establishes the requirements and evidence boundary.
- Phase 2 precedes reducer and presentation work.
- User Story 1 and User Story 2 depend on the model; User Story 3 reuses the
  movement and deletion primitives.
- T015 and T018 precede the reducer and terminal acceptance checks.
- T016 and T022 preserve the user-facing behaviour as integration contracts.
- T025 follows all source work and records the complete local gate.

## Parallel opportunities

- T003 through T006 are documentation-only and can run in parallel after the
  spec is stable.
- T007 through T011 are separate model concerns but share `src/app/editor.rs`
  and should be integrated serially by its owner.
- T016 and T022 are separate test files and can run in parallel.
- T023 and T024 are evidence documentation and can be updated alongside source
  review, but must not overstate focused or platform evidence.

## Current boundary

- Feature 005 is complete in the current spec and committed source.
- The acceptance contracts remain separate from Windows ConPTY and screen-reader
  hand evidence, which are tracked by their platform features.
- No result here upgrades a Unix pty run into Windows terminal evidence.
