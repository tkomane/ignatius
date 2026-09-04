---

description: "Implementation tasks for explicit retained-result refresh"
---

# Tasks: Explicit retained-result refresh

**Input**: Design documents from `specs/024-retained-result-refresh/`.

**Prerequisites**: `spec.md`, `plan.md`, `research.md`, `data-model.md`,
`contracts/refresh.md`, and `quickstart.md`.

**Tests**: Tests are required by the specification. Reducer and discovery tests
must cover the safe refusal boundary before implementation is marked complete.

## Phase 1: Setup

- [x] T001 Confirm `specs/024-retained-result-refresh/` is active in
  `.specify/feature.json` and that its specification, plan, research,
  data-model, contract and quickstart agree on one explicit read-only refresh.

## Phase 2: Foundational action scaffolding

- [x] T002 Add the `RefreshResult` action variant and compile-safe exhaustive
  arms in `src/app/message.rs`, `src/app/discovery.rs` and `src/ui/keymap.rs`
  so the focused tests can name the new intent without changing behavior yet.

## Phase 3: User Story 1 - Refresh the answer in place (Priority: P1) 🎯 MVP

**Goal**: A user can discover and explicitly refresh one eligible retained
result without changing the SQL editor.

**Independent Test**: Seed a retained result, change the source row, invoke the
refresh action, and observe one execution effect using the retained source while
the editor remains unchanged.

### Tests for User Story 1

- [x] T003 [US1] Add reducer tests in `src/app/update.rs` proving an eligible
  retained read source emits one execution effect from `last_sql`, not the
  current editor buffer, and leaves editor text and revision unchanged.
- [x] T004 [US1] Add reducer tests in `src/app/update.rs` proving a retained
  source with named parameters opens the existing prompt, re-prompts on each
  refresh, accepts an empty value, and emits no effect before acceptance.
- [x] T005 [P] [US1] Add action reachability and configured-key tests in
  `src/app/discovery.rs` and `src/ui/keymap.rs` for the palette entry, portable
  `F6`, the enhanced-terminal `Ctrl+Shift+R` alias, contextual hint and
  printable editor input preservation.

### Implementation for User Story 1

- [x] T006 [US1] Add the default portable `F6` binding, enhanced-terminal
  `Ctrl+Shift+R` alias, action catalog, palette entry and exhaustive user-facing
  descriptions in `src/app/message.rs`, `src/app/discovery.rs` and
  `src/ui/keymap.rs`, including the `refresh-result` configuration name.
- [x] T007 [US1] Add the pure retained-source eligibility predicate and
  prerequisite wording in `src/app/discovery.rs`, requiring one visible result
  and exactly one read-classified retained statement.
- [x] T008 [US1] Route `RefreshResult` through the pure reducer in
  `src/app/update.rs`, using the retained source and existing execution and
  named-parameter paths without editing the SQL buffer.
- [x] T009 [US1] Add explicit refresh-in-flight and outcome wording to
  `src/app/model.rs` and `src/app/update.rs`, then render it in
  `src/ui/layout.rs` so Results says when the retained result is refreshing.
- [x] T010 [US1] Add renderer tests in `src/ui/layout.rs` for refreshing state,
  refusal wording, narrow layout and ASCII/no-colour meaning.

**Checkpoint**: The MVP refresh action is reachable, uses only the retained
source, prompts safely when needed, and produces one ordinary read execution.

## Phase 4: User Story 2 - Know when refresh is not safe or available (Priority: P1)

**Goal**: Unsafe or unavailable refresh attempts explain their boundary and
emit no SQL.

**Independent Test**: Exercise missing result, missing source, busy state,
visible plan, failed transaction, unusable connection, multi-statement source,
write-classified source and unknown source through the reducer and discovery.

- [x] T011 [US2] Add refusal and no-effect tests in `src/app/update.rs` and
  `src/app/discovery.rs` for every unavailable or unsafe source shape, including
  a zero-row retained result that remains eligible.
- [x] T012 [US2] Implement the fail-closed refusal paths in
  `src/app/discovery.rs` and `src/app/update.rs`, preserving the editor and
  retained result and stating the prerequisite or source boundary.
- [x] T013 [US2] Add keymap and discovery assertions in `src/ui/keymap.rs` and
  `src/app/discovery.rs` that the active shortcut and palette never replace the
  existing Run buffer binding or printable editor characters.

**Checkpoint**: No passive or unsafe state can cause refresh to send SQL, and
the direct shortcut remains understandable and non-destructive.

## Phase 5: User Story 3 - Keep refresh explicit and auditable (Priority: P2)

**Goal**: Refresh remains a deliberate, one-outcome action with existing
history, diagnostics, cancellation and secret-parameter guarantees.

**Independent Test**: Drive filtering, sorting, focus, grid controls, inspector,
cell-update completion, reconnect and failed-refresh transitions, then verify
zero automatic refresh requests and one safe history outcome for an explicit
parameterized refresh.

- [x] T014 [US3] Add reducer lifecycle tests in `src/app/update.rs` proving
  view-only actions and passive state transitions emit no refresh, cancellation
  does not retry, and one explicit refresh produces one outcome.
- [x] T015 [US3] Add privacy and history assertions in `src/app/update.rs` and
  `tests/cli_contract.rs` proving the retained template is safe while prompted
  values are absent from debug, diagnostics, history and machine output.
- [x] T016 [US3] Preserve the existing execution completion, cancellation,
  connection-loss and history paths in `src/app/update.rs`, including explicit
  refresh wording and no automatic follow-up.

**Checkpoint**: Refresh is a transparent extension of ordinary execution, not a
timer, retry, replay or second persistence route.

## Phase 6: Polish and cross-cutting concerns

- [x] T017 [P] Update `docs/design/keymap.md`,
  `docs/product/experience-roadmap.md`, `docs/product/roadmap.md`,
  `docs/product/journeys.md`, `docs/support/compatibility.md`,
  `docs/security/data-handling.md`, `docs/security/threat-model.md`,
  `docs/operations/local-development.md` and `CHANGELOG.md` with the supported
  refresh boundary and no-replay wording.
- [x] T018 [P] Add documentation parity assertions and Feature 024 quickstart
  references in `tests/documentation_matches_the_build.rs`.
- [x] T019 Run focused formatting, reducer, discovery, keymap, renderer and
  documentation checks; record focused results and any skipped environment
  evidence.
- [x] T020 Run `cargo --locked xtask verify`, then run `cargo xtask db down` and
  `cargo xtask db status`; record full-suite results separately from semantic
  skips and manual terminal gaps in `docs/status.md`.
- [x] T021 Mark all completed tasks, update the Feature 024 specification and
  roadmap state only after evidence is recorded, and run the normal-hyphen
  prose scan across all changed documentation.

## Dependencies and execution order

- T001 precedes all implementation.
- T002 is the compile-safe action prerequisite. T003-T005 are tests for the
  MVP; T006-T010 implement and verify the same vertical slice.
- T011-T013 extend the MVP with fail-closed availability and refusal behaviour.
- T014-T016 preserve explicit execution, privacy and no-replay contracts.
- T017-T021 follow stable behavior and complete the evidence and documentation
  gates.

## Parallel opportunities

- T005 can run in parallel with T003-T004 after T002 is complete.
- T017 and T018 can run in parallel after user-visible wording is stable.
- T019 is the focused validation checkpoint before T020.

## Implementation strategy

Complete the MVP first: action catalog, one-source eligibility, retained-source
execution, parameter re-prompting and visible refresh state. Then add every
refusal boundary and finally audit passive transitions, privacy, documentation
and full verification. Do not add a refresh-specific runtime effect, timer,
cache or scripted command.
