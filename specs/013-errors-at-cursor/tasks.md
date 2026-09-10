---

description: "Implementation tasks for Feature 013"
---

# Tasks: Errors that point at the problem

**Input**: Design documents from `/specs/013-errors-at-cursor/`

**Prerequisites**: `spec.md`, `plan.md`, `research.md`, `data-model.md`,
`contracts/errors.md`, and `quickstart.md`

**Tests**: Included because the feature specification requires pure, reducer,
renderer, plain-mode, JSON, and live PostgreSQL evidence.

**Organization**: Tasks are grouped by independently testable user story. Tasks
that touch the same Rust module are intentionally sequential.

## Phase 1: Setup

**Purpose**: Establish the feature's pure module and baseline test seams.

- [x] T001 [P] Add the error-location module and feature exports in `src/query/mod.rs` and `src/query/error_location.rs`.
- [x] T002 [P] Record the Feature 013 verification entry points in `specs/013-errors-at-cursor/quickstart.md` and `specs/013-errors-at-cursor/contracts/errors.md`.

**Checkpoint**: The feature has a registered pure query seam and explicit
contracts before behavior changes begin.

## Phase 2: Foundational

**Purpose**: Add shared state primitives used by every error presentation.

- [x] T003 [P] Add focused editor revision and non-undo cursor-position tests in `src/app/editor.rs`.
- [x] T004 Add the editor revision counter and UTF-8-safe cursor setter in `src/app/editor.rs`, preserving undo, modified, and movement invariants.
- [x] T005 [P] Add diagnostic statement-identity, object-context, redaction, and JSON shape tests in `src/diagnostics/diagnostic.rs`.
- [x] T006 Add `statement_number` and redacted structured object context to `src/diagnostics/diagnostic.rs`, and re-export any public types from `src/diagnostics/mod.rs`.

**Checkpoint**: The editor can reject stale text safely and diagnostics can carry
structured PostgreSQL facts without source SQL or credentials.

## Phase 3: User Story 1 - Put the cursor on the server's answer (Priority: P1)

**Goal**: Map a server character position to the correct editor character and
token, then apply it only to the unchanged submitted buffer.

**Independent Test**: Pure mapping tests plus reducer and TUI renderer tests
demonstrate a multi-line, multi-statement UTF-8 failure moving the caret and
marker without changing SQL or undo history; invalid and stale positions leave
the cursor alone.

### Tests for User Story 1

- [x] T007 [P] [US1] Add failing pure mapping tests for statement ordinals, UTF-8 characters, tabs, line/column calculation, token spans, end-of-line positions, comments, quoted strings, dollar-quoted bodies, and invalid positions in `src/query/error_location.rs`.
- [x] T008 [P] [US1] Add failing reducer tests for valid error focus/cursor placement, unchanged buffer and undo history, run-statement source spans, missing/out-of-range positions, and stale editor revisions in `src/app/update.rs`.
- [x] T009 [P] [US1] Add failing renderer tests for the `!` error-line marker, token emphasis, line/column wording, no-colour/ASCII meaning, and compact error-plus-editor layout in `src/ui/layout.rs`.

### Implementation for User Story 1

- [x] T010 [US1] Implement the UTF-8-safe `StatementSource`, `ErrorLocation`, token-span selection, absolute line/column mapping, and source-caret renderer in `src/query/error_location.rs`.
- [x] T011 [US1] Track `running_editor_revision` and the original run-statement source span in `src/app/model.rs`, `src/app/update.rs`, and `src/app/message.rs` without persisting SQL in diagnostic state.
- [x] T012 [US1] Apply valid locations, editor focus, unavailable-location notes, stale-job rejection, and marker invalidation after edits in `src/app/update.rs`.
- [x] T013 [US1] Render the mapped caret, error-line gutter marker, token emphasis, and truthful unavailable/stale wording in full and compact layouts in `src/ui/layout.rs`.

**Checkpoint**: A valid query error points at the correct editor token, while a
changed or unmappable buffer never receives a guessed marker.

## Phase 4: User Story 2 - Understand which database object rejected the write (Priority: P2)

**Goal**: Promote PostgreSQL's structured constraint/object facts and safely use
the existing schema snapshot for matching detail.

**Independent Test**: Live disposable PostgreSQL constraint failures and pure
renderer tests show schema, relation, column, and constraint context when
available, and state missing catalogue context without inventing facts.

### Tests for User Story 2

- [x] T014 [P] [US2] Add failing PostgreSQL error-mapping tests for structured schema, table, column, constraint, and statement ordinal fields in `src/postgres/error.rs`.
- [x] T015 [P] [US2] Add failing TUI object-context tests for matching, unavailable, stale, and non-matching completion catalogues in `src/ui/layout.rs`.
- [x] T016 [P] [US2] Add live unique, foreign-key, not-null, and check constraint assertions in `tests/postgres_integration.rs` using only synthetic fixture data.

### Implementation for User Story 2

- [x] T017 [US2] Populate statement identity and object context from `tokio-postgres` error fields in `src/postgres/error.rs` while preserving existing SQLSTATE and redaction behavior.
- [x] T018 [US2] Add catalogue-aware, safely quoted object and column context rendering with explicit snapshot limitations in `src/ui/layout.rs`.
- [x] T019 [US2] Update the shared plain diagnostic wording for object context and stable statement/character position in `src/diagnostics/diagnostic.rs`.

**Checkpoint**: Constraint failures identify the server-supplied database object
before technical detail and never turn catalogue uncertainty into a claim.

## Phase 5: User Story 3 - Keep the same answer in every interface (Priority: P3)

**Goal**: Keep error meaning aligned across interactive plain output, direct CLI
diagnostics, JSON, and the TUI without implicit execution.

**Independent Test**: The same failing statement produces a line-oriented plain
excerpt and stable JSON fields, with no terminal control sequences or second
execution.

### Tests for User Story 3

- [x] T020 [P] [US3] Add failing plain-mode tests for mapped statement, line, column, source excerpt, caret, unavailable location, and control-sequence absence in `src/cli/plain.rs`.
- [x] T021 [P] [US3] Add failing CLI contract tests for stable numeric position, statement/object JSON fields, failed-query stdout behavior, and no automatic retry in `tests/cli_contract.rs`.

### Implementation for User Story 3

- [x] T022 [US3] Render the shared mapped source excerpt and caret from the submitted SQL in `src/cli/plain.rs`, preserving line-oriented and no-control-sequence behavior.
- [x] T023 [US3] Preserve and document the direct CLI diagnostic fallback for statement/character position and JSON object context in `src/cli/mod.rs`, `src/diagnostics/diagnostic.rs`, and `docs/support/diagnostics.md`.

**Checkpoint**: Plain and machine-readable interfaces retain the same facts and
never execute or rewrite SQL because a location exists.

## Phase 6: Polish and Cross-Cutting Verification

**Purpose**: Complete product documentation, release history, and evidence.

- [x] T024 [P] Update user journeys, interaction guidance, product roadmaps, and compatibility/support wording for Feature 013 in `docs/product/journeys.md`, `docs/product/experience-roadmap.md`, `docs/product/roadmap.md`, `docs/design/interaction-principles.md`, and `docs/support/diagnostics.md`.
- [x] T025 [P] Add the Feature 013 Unreleased entry describing cursor targeting, constraint context, stale-buffer truthfulness, and plain-mode parity in `CHANGELOG.md`.
- [x] T026 Run `cargo fmt --all -- --check`, focused tests, `git diff --check`, and inspect all changed paths for secret or SQL logging regressions.
- [x] T027 Run `cargo --locked xtask verify` with disposable PostgreSQL where available, then record focused, full, live, skipped, and unverified evidence in `docs/status.md`.

## Dependencies and Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: T001-T002 have no code dependencies and establish the
  feature seam and contracts.
- **Foundational (Phase 2)**: T003-T006 depend on the setup seam; T004 depends
  on T003 and T006 depends on T005.
- **User Story 1 (Phase 3)**: T007-T009 are tests first and can run in parallel;
  T010 depends on T007, T011 depends on T004/T010, T012 depends on T008/T011,
  and T013 depends on T009/T012.
- **User Story 2 (Phase 4)**: T014-T016 are independent tests after foundation;
  T017 depends on T005/T014, T018 depends on T015/T017 and T010, and T019
  depends on T006/T017.
- **User Story 3 (Phase 5)**: T020-T021 are independent tests after the shared
  mapper; T022 depends on T020/T010, and T023 depends on T021/T006.
- **Polish (Phase 6)**: T024-T025 can run in parallel after behavior is stable;
  T026 depends on all implementation tasks, and T027 depends on T026.

### Parallel Opportunities

- T001-T002, T003/T005, T007-T009, T014-T016, T020-T021, and T024-T025 can
  run in parallel because they use separate files or are read-only documentation
  changes.
- The MVP is T001-T013 plus the focused tests in T007-T009. It delivers the
  high-frequency cursor and token answer before constraint enrichment or plain
  parity is added.

## Implementation Strategy

1. Complete setup and foundational state primitives.
2. Write and watch the pure, reducer, and renderer tests fail for User Story 1.
3. Implement User Story 1 and stop at its checkpoint for focused validation.
4. Add structured constraint context and live PostgreSQL evidence.
5. Add plain/JSON parity, then update documentation and run the locked verifier.
6. Mark only tasks backed by completed evidence as `[x]`; keep platform and
   transport skips explicit in status rather than treating them as passes.

## Notes

- `[P]` means the task can run in parallel with other tasks in that group.
- Every user story task includes an exact repository path.
- No task authorizes pushing, publishing, signing, tagging, or releasing.
