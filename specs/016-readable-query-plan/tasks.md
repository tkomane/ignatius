---

description: "Implementation tasks for the readable query-plan workflow"
---

# Tasks: A plan you can read

**Input**: Design documents from `/specs/016-readable-query-plan/`

**Prerequisites**: `spec.md`, `plan.md`, `research.md`, `data-model.md`,
`contracts/plan.md`, and `quickstart.md`.

**Tests**: Tests are required by the feature specification and constitution.
Every story has test-first tasks before its implementation tasks.

**Strategy**: Build the pure bounded plan model first, then connect the
reducer and PostgreSQL boundary, then render and document the experience. Keep
plan state separate from ordinary result, history, export, and machine-output
state throughout.

## Phase 1: Setup

**Purpose**: Confirm the implementation surface and avoid unnecessary
dependencies or unrelated worktree changes.

- [x] T001 Verify `specs/016-readable-query-plan/spec.md`, `plan.md`, `research.md`, `data-model.md`, `contracts/plan.md`, and `quickstart.md` against the current repository APIs and constitution
- [x] T002 [P] Confirm the existing `serde_json` dependency and lockfile are sufficient; do not add a dependency in `Cargo.toml`
- [x] T003 [P] Inventory the existing Feature013-015 worktree changes and confirm the Feature016 source paths in `plan.md` do not overwrite unrelated work

## Phase 2: Foundational

**Purpose**: Establish pure plan types and application state before any SQL
effect or terminal rendering is connected.

**Blocking rule**: User-story work depends on the bounded parser and plan
session state being available.

- [x] T004 [P] [US1] Add bounded `PlanDocument`, `PlanNode`, `PlanFact`, parser errors, metric helpers, and resource-limit constants in `src/query/plan.rs`
- [x] T005 [P] [US1] Register the plan query module in `src/query/mod.rs` and keep it independent of terminal, database, and filesystem code
- [x] T006 [P] [US3] Add `PlanView`, `PlanStatus`, path selection, visible-node flattening, collapse state, and local navigation helpers in `src/app/plan.rs`
- [x] T007 [P] [US3] Register and re-export plan view state from `src/app/mod.rs`
- [x] T008 [US1] Add parser fixtures and unit tests for valid JSON trees, missing optional metrics, unknown fields, nested child order, malformed JSON, and explicit size/depth/node bounds in `src/query/plan.rs`
- [x] T009 [US3] Add plan-view unit tests for selection movement, collapse/expand, hidden descendants, path recovery, and bounded visible-node lists in `src/app/plan.rs`

**Checkpoint**: Pure plan parsing and local plan navigation are independently
testable with no PostgreSQL connection and no terminal I/O.

## Phase 3: User Story 1 - See a readable plan tree (Priority: P1)

**Goal**: Ask for a non-executing plan for the statement under the cursor and
read it as an honest hierarchical tree in the Results pane.

**Independent test**: A reducer test proves the plain-plan action emits only a
plan-specific effect for the current statement; parser and renderer tests prove
the returned tree and estimate fields are readable in supported modes.

### Tests for User Story 1

- [x] T010 [P] [US1] Add reducer tests in `src/app/update.rs` proving Explain plan uses only the current statement, clears no ordinary result state, and emits no history or machine-output effect
- [x] T011 [P] [US1] Add PostgreSQL session/integration coverage in `tests/postgres_integration.rs` for plain structured EXPLAIN on the disposable PostgreSQL fixture and assert the target statement is not executed
- [x] T012 [P] [US1] Add CLI and documentation contract assertions in `tests/cli_contract.rs` and `tests/documentation_matches_the_build.rs` for the plan boundary, key discovery, and unchanged plain/JSON/NDJSON/history paths

### Implementation for User Story 1

- [x] T013 [US1] Add `ExplainPlan` and `AnalyzePlan` actions, plan completion messages, plan-specific effects, and plan execution payloads in `src/app/message.rs`
- [x] T014 [US1] Add plan job/pending-confirmation fields and lifecycle helpers to `src/app/model.rs`, preserving ordinary `last_execution`, result-grid, filter, and history state
- [x] T015 [US1] Add `Session::explain` in `src/postgres/session.rs` using structured JSON EXPLAIN with the bounded response cap, safe diagnostics, and no raw SQL logging
- [x] T016 [US1] Add the interactive runtime plan effect and completion dispatch in `src/cli/interactive.rs`, including parser failure, cancellation, and connection-loss outcomes
- [x] T017 [US1] Implement plain-plan target selection, preconditions, job identity, stale-completion rejection, and state transitions in `src/app/update.rs`
- [x] T018 [US1] Add plan actions to palette availability, prerequisites, grouping, contextual hints, and keymap reachability in `src/app/discovery.rs` and `src/ui/keymap.rs`
- [x] T019 [US1] Bind discoverable plan chords and labels in `src/ui/keymap.rs` without changing existing bindings; cover them in keymap contract tests
- [x] T020 [US1] Render loading, ready, failed, truncated, selected, and estimate-only plan states in the Results pane of `src/ui/layout.rs`
- [x] T021 [US1] Add renderer tests in `src/ui/layout.rs` for full, compact, narrow, ASCII, no-colour, reduced-motion, and unavailable-field wording

**Checkpoint**: Plain Explain plan works end to end, does not execute the
target statement, remains palette/chord discoverable, and restores the prior
ordinary result when dismissed.

## Phase 4: User Story 2 - Compare estimate with reality safely (Priority: P1)

**Goal**: Make EXPLAIN ANALYZE explicit, one-shot confirmed, truthful about
side effects, and useful for comparing estimated and observed metrics.

**Independent test**: Reducer tests prove no database effect occurs before
confirmation and that cancellation has no effect; parser/renderer/integration
tests prove actual rows, time, loops, per-loop comparison, 10x mismatch, and
expense-basis wording.

### Tests for User Story 2

- [x] T022 [P] [US2] Add reducer tests in `src/app/update.rs` for Analyze plan confirmation, Enter/type-name requirements, cancel/dismiss behavior, production/destructive safeguards, and no pre-confirmation effect
- [x] T023 [P] [US2] Extend `src/query/plan.rs` tests for actual rows/time/loops, per-loop ratios, missing/zero values, 10x mismatch thresholds, and estimated-cost versus measured-time attention basis
- [x] T024 [P] [US2] Add disposable PostgreSQL integration coverage in `tests/postgres_integration.rs` for analyzed SELECT plans and actual metrics, recording server version and separating plain/TLS evidence
- [x] T025 [P] [US2] Add renderer contract tests in `src/ui/layout.rs` for the analysis warning, observed fields, loop wording, side-effect wording, slow-node marker, and no fabricated metrics

### Implementation for User Story 2

- [x] T026 [US2] Implement the one-shot Analyze plan confirmation model and action handling in `src/app/model.rs` and `src/app/update.rs`, reusing existing impact classification and production protection
- [x] T027 [US2] Generate the analyzed structured EXPLAIN request in `src/postgres/session.rs` with timing enabled and preserve the current transaction semantics without automatic rollback
- [x] T028 [US2] Compute and expose per-loop estimate ratios, mismatch labels, and the largest estimated-cost or slowest measured-time attention path in `src/query/plan.rs`
- [x] T029 [US2] Render analysis confirmation and observed plan metrics with explicit `not supplied`, `not measured`, planner-cost, per-loop, and side-effect wording in `src/ui/layout.rs`
- [x] T030 [US2] Handle analyzed completion, cancellation, failed transaction, and connection-lost state without recording history or overwriting ordinary results in `src/app/update.rs`

**Checkpoint**: Analyze plan cannot start silently, clearly states that the
statement executes and may have side effects, and reports observed metrics only
when PostgreSQL supplied them.

## Phase 5: User Story 3 - Navigate the plan locally (Priority: P2)

**Goal**: Explore large plans with keyboard-first movement and local
expand/collapse while retaining the prior ordinary result.

**Independent test**: View-state, reducer, and renderer tests prove navigation
does not perform I/O, collapsed descendants are hidden, and dismissal restores
the retained result without rerunning SQL.

### Tests for User Story 3

- [x] T031 [P] [US3] Add reducer tests in `src/app/update.rs` for plan focus, movement, Activate collapse/expand, Dismiss, stale plan completion, and no-round-trip local navigation
- [x] T032 [P] [US3] Add layout tests in `src/ui/layout.rs` for selected-node detail, nested indentation, collapsed branches, deep-plan truncation, and text selection cues in ASCII/no-colour output
- [x] T033 [P] [US3] Add documentation and keymap assertions in `tests/documentation_matches_the_build.rs` for movement, expansion, dismissal, bounds, and manual platform evidence boundaries

### Implementation for User Story 3

- [x] T034 [US3] Route Results focus movement and activation to `PlanView` in `src/app/update.rs` while preserving existing grid and inspector behavior
- [x] T035 [US3] Add plan-specific contextual footer hints, palette notes, and recovery wording in `src/app/discovery.rs`, `src/app/palette.rs`, and `src/ui/layout.rs`
- [x] T036 [US3] Finish narrow/compact/full layout adaptation, bounded tree indentation, sanitised server text, and selected-node detail rendering in `src/ui/layout.rs`
- [x] T037 [US3] Verify plan dismissal restores the retained ordinary result and that plan actions remain absent from history, export, JSON, NDJSON, and non-interactive output paths in `src/app/update.rs`, `src/cli/interactive.rs`, and `tests/cli_contract.rs`

**Checkpoint**: A ready plan is a local, keyboard-first Results view with
truthful bounds and recovery; moving or collapsing nodes never contacts the
database.

## Phase 6: Polish, documentation, and evidence

**Purpose**: Close cross-cutting contracts and gather evidence without
claiming unavailable platform or socket coverage.

- [x] T038 [P] Update `docs/design/interaction-principles.md` with plan truth, analysis confirmation, local navigation, and styling-loss behavior
- [x] T039 [P] Update `docs/design/keymap.md` with the plan chords, palette discovery, and contextual hints
- [x] T040 [P] Update `docs/product/experience-roadmap.md` and `docs/product/roadmap.md` to record Feature016 implementation status and the next roadmap item
- [x] T041 [P] Update `docs/support/diagnostics.md` and `docs/support/compatibility.md` with plan failure recovery, PostgreSQL support assumptions, cost units, and ANALYZE safety
- [x] T042 [P] Update `CHANGELOG.md` with the user-visible Feature016 plan workflow and safety boundary
- [x] T043 Run the focused validation in `specs/016-readable-query-plan/quickstart.md`, including formatting, locked check, focused tests, documentation/CLI contracts, and diff checks
- [x] T044 Run `cargo xtask db up`, the plan PostgreSQL integration tests, and `cargo xtask db status`; record disposable server version, plain/TLS results, and teardown status separately
- [x] T045 Run `cargo --locked xtask verify`, report every executed gate, identify the Unix-socket skip if unconfigured, and distinguish automated evidence from manual terminal/platform checks
- [x] T046 Mark completed implementation and evidence tasks in this file only after their checks pass; do not commit, push, tag, release, or publish without owner confirmation

## Dependencies and execution order

### Phase dependencies

- Setup has no code dependency and can begin immediately.
- Foundational depends on repository orientation and blocks all story work.
- User Story 1 depends on the pure parser and plan view foundations.
- User Story 2 depends on the plan effect/completion boundary from User Story 1.
- User Story 3 depends on the ready plan state and Results rendering from User Story 1; it uses the analysis state from User Story 2 where applicable.
- Polish and evidence depend on all in-scope stories being implemented.

### Parallel opportunities

- T004-T007 can be split by file after the API shape is agreed.
- T010-T012 are independent test surfaces for User Story 1.
- T022-T025 are independent test surfaces for User Story 2.
- T031-T033 are independent test surfaces for User Story 3.
- T038-T042 are independent documentation files and can be updated in parallel after behavior is settled.

### Within each story

1. Write the story tests and verify they fail for the missing behavior.
2. Implement the pure model or boundary needed by the tests.
3. Connect reducer/runtime/UI behavior.
4. Run the story checkpoint and preserve prior feature tests.

## Implementation strategy

1. Complete the parser and local view foundation.
2. Deliver plain Explain as the first usable increment.
3. Add explicit Analyze confirmation and observed metrics.
4. Add local navigation and result restoration.
5. Close documentation, focused checks, disposable PostgreSQL evidence, and the full verifier.

No task authorizes publication. Publication, tagging, release, or remote
mutation remains a separate owner-confirmed action.
