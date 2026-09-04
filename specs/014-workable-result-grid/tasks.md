---

description: "Implementation tasks for the workable interactive result grid"
---

# Tasks: A result grid you can work

**Input**: Design documents from `/specs/014-workable-result-grid/`

**Prerequisites**: `plan.md`, `spec.md`, `research.md`, `data-model.md`,
`contracts/result-grid.md`, and `quickstart.md`

**Tests**: Included because the feature specification requires independent
acceptance checks and the repository constitution requires evidence for state,
terminal, live PostgreSQL, and preserved scripted contracts.

**Organization**: Tasks are grouped by user story. Foundational work preserves
the existing result and session boundaries; each story then adds an independently
testable slice.

## Phase 1: Setup

**Purpose**: Establish the implementation seam without changing the existing
interactive or scripted result behaviour.

- [x] T001 [P] Add the `src/app/grid.rs` module declaration and feature-local test seam in `src/app/mod.rs`, keeping the existing result model and keymap unchanged until the foundational contracts are in place

---

## Phase 2: Foundational

**Purpose**: Add the server-result metadata boundary and application-owned grid
state that all user stories depend on.

**Critical**: Complete this phase before story work so local view state cannot
leak into `ResultSet`, plain output, or session execution semantics.

- [x] T002 [P] Add optional server-described column type labels, length-safe constructors, and metadata accessors in `src/query/result.rs`, with unit tests for empty, matching, partial, and mismatched metadata
- [x] T003 Add a live integration assertion for row-bearing result headers, text values, NULL values, and server type labels in `tests/postgres_integration.rs` before wiring the session sidecar
- [x] T004 Implement the post-execution parse/describe sidecar in `src/postgres/session.rs`, accepting labels only when count and source names match and falling back without changing query success, values, logging, or execution count
- [x] T005 [P] Write invariant tests for `ResultGridState` in `src/app/grid.rs`, covering result reset, session-only preferences, duplicate source columns, last-visible-column protection, bounded widths, and horizontal position bounds
- [x] T006 Implement `ResultGridState`, `SortSpec`, `SortDirection`, `GridCommand`, and bounded column helpers in `src/app/grid.rs` without owning rows or mutating `ResultSet`
- [x] T007 Integrate `ResultGridState` into `src/app/model.rs` and reset result-specific state from `src/app/update.rs` whenever a new execution replaces `last_execution`, while preserving documented type-label and freeze preferences
- [x] T008 [P] Add a focused no-side-effect regression check in `tests/cli_contract.rs` or the nearest existing contract module proving grid state is not part of plain, JSON, streaming export, history, or editor behaviour

**Checkpoint**: The result retains server order and values, type metadata is
optional and truthful, and an empty grid state can be attached and reset safely.

---

## Phase 3: User Story 1 - Read and compare returned rows (Priority: P1) MVP

**Goal**: Sort retained rows locally with deterministic comparison and preserve
the selected source record without executing or rewriting SQL.

**Independent Test**: Use a mixed result containing numeric-looking text,
duplicate values, empty text, NULL, and a retained-row cap. Cycle the selected
column through ascending, descending, and original order, then verify source
row labels, selected-record identity, counts, and no execution effect.

### Tests for User Story 1

- [x] T009 [P] [US1] Add pure ordering tests in `src/app/grid.rs` for decimal-aware integers, fractions, exponents, negative values, large values without floating-point loss, case-folded text fallback, NULL placement, equal-value stability, and original-order restoration
- [x] T010 [P] [US1] Add reducer tests in `src/app/update.rs` for sort cycling, selected source-row preservation through a reorder, filtered-row mapping, retained-row scope wording, and the absence of `Effect::Execute` or connection effects
- [x] T011 [P] [US1] Add result-render tests in `src/ui/layout.rs` for ascending, descending, original-order, filtered, and truncated states with source row gutters and safe hostile cell text

### Implementation for User Story 1

- [x] T012 [US1] Implement the stable cell comparator and `None`/ascending/descending sort cycle in `src/app/grid.rs`, including deterministic source-index tie-breaking and NULL-after-non-NULL behaviour in both directions
- [x] T013 [US1] Derive displayed source-row indices from retained rows, active filters, and grid sort in `src/app/model.rs`, keeping `ResultSet.rows` in server order and keeping selected source-row lookup authoritative
- [x] T014 [US1] Preserve selected source-row identity and update display-position selection during sort and filter transitions in `src/app/update.rs`, including safe behaviour when no filtered row remains
- [x] T015 [US1] Render local-sort direction, retained-row scope, source-row numbering, filtered counts, and cap/truncation wording in `src/ui/layout.rs` without implying PostgreSQL ordering or a page fetch

**Checkpoint**: User Story 1 is independently usable. Sorting is local,
stable, bounded, reversible, and does not issue another execution request.

---

## Phase 4: User Story 2 - Shape a wide result for the task (Priority: P1)

**Goal**: Make wide results readable through searchable column visibility,
bounded widths, truthful type labels, and a responsive frozen-column viewport.

**Independent Test**: Use at least six columns, including duplicate labels and
partial type metadata. Open the chooser, hide and restore columns, resize the
selected column, toggle type labels, freeze the first visible column, and move
across wide, narrow, ASCII, and no-colour layouts.

### Tests for User Story 2

- [x] T016 [P] [US2] Add grid-state tests in `src/app/grid.rs` for duplicate-label column toggles by source index, selection migration after hiding, last-column rejection, width minimum and maximum, type/freeze preference transitions, and reset
- [x] T017 [P] [US2] Add palette and keymap contract tests in `src/app/palette.rs` and `src/ui/keymap.rs` for searchable controls, searchable columns, shown/hidden/type state, duplicate labels, the `Ctrl+K g` entry point, and printable editor key preservation
- [x] T018 [P] [US2] Add renderer buffer tests in `src/ui/layout.rs` for type rows, unavailable and partial metadata, hidden-column counts, selected-column visibility, frozen first-column layout, horizontal range wording, narrow terminals, ASCII, no-colour, missing cells, and hostile labels or type names

### Implementation for User Story 2

- [x] T019 [US2] Implement column visibility, bounded width, type-label preference, freeze preference, selected-column recovery, and horizontal-window calculations in `src/app/grid.rs`, guaranteeing that the selected visible column remains reachable
- [x] T020 [US2] Add result-controls intent and grid command handling in `src/app/message.rs` and `src/app/update.rs`, applying sort, chooser, width, type, freeze, and reset transitions without I/O effects
- [x] T021 [US2] Add searchable grid-control and column-chooser palette commands in `src/app/palette.rs`, listing each source column once with source position, shown/hidden state, and available or unavailable type label
- [x] T022 [US2] Add the grid-controls action, chord metadata, labels, and discoverability entries in `src/ui/keymap.rs`, while retaining printable keys for the SQL editor and existing focus modes
- [x] T023 [US2] Update result navigation in `src/app/update.rs` and source-column helpers in `src/app/model.rs` so left and right movement follows visible source columns and hiding the selected column selects a visible neighbour
- [x] T024 [US2] Render responsive visible-column windows, bounded explicit widths, type labels, selected-column identity, hidden counts, frozen-first-column state, and narrow-terminal fallback wording in `src/ui/layout.rs`
- [x] T025 [US2] Adjust expanded-row and cell-inspector lookup in `src/app/inspect.rs` and `src/ui/layout.rs` to follow the selected source row and column while leaving source values, missing cells, and the existing inspection contract intact

**Checkpoint**: User Stories 1 and 2 are independently usable. The grid is a
safe, readable local view over the retained result, including wide and degraded
terminal presentations.

---

## Phase 5: User Story 3 - Discover and recover grid actions (Priority: P2)

**Goal**: Make the result controls learnable, stateful in plain language, and
recoverable through reset and new-result boundaries.

**Independent Test**: Focus Results, open controls through the advertised chord
and ordinary command palette, search each action, apply changes, reset, and run
a new query. Confirm empty results remain explanatory and only documented
session preferences survive.

### Tests for User Story 3

- [x] T026 [P] [US3] Add palette and reducer tests in `src/app/palette.rs` and `src/app/update.rs` for contextual control entry, action search, Escape dismissal, reset recovery, new-execution reset, empty-result safety, and no accidental SQL execution
- [x] T027 [P] [US3] Add layout tests in `src/ui/layout.rs` for contextual result-grid hints and explicit active-state wording when colour or Unicode styling is unavailable

### Implementation for User Story 3

- [x] T028 [US3] Wire the command palette and `Ctrl+K g` contextual result-controls surface through `src/app/update.rs`, including focus and no-result guards and plain-language current-state details
- [x] T029 [US3] Add reset, notice, and new-execution recovery wording in `src/app/grid.rs`, `src/app/model.rs`, and `src/ui/layout.rs`, making blocked last-column actions and unavailable metadata explicit
- [x] T030 [US3] Preserve palette dismissal, editor input, inspection, filter, and empty-result behaviour across the new commands in `src/app/update.rs` and `src/app/palette.rs`

**Checkpoint**: All user stories are independently discoverable and recoverable
without changing the submitted SQL or retained result data.

---

## Phase 6: Polish and cross-cutting verification

**Purpose**: Close documentation, security, accessibility, compatibility, and
release-evidence gaps without claiming unavailable platform evidence.

- [x] T031 [P] Add terminal-sanitisation and hostile-header/type/cell regression coverage in `src/app/grid.rs` and `src/ui/layout.rs`, proving server content cannot emit control sequences or create grid commands
- [x] T032 [P] Update `docs/design/keymap.md`, `docs/design/interaction-principles.md`, `docs/product/experience-roadmap.md`, `docs/product/roadmap.md`, and `docs/product/journeys.md` with the shipped controls, limits, reset model, and keyboard-first rationale
- [x] T033 [P] Update `docs/support/diagnostics.md`, `docs/support/compatibility.md`, and `CHANGELOG.md` with type-description fallback, retained-row sorting scope, terminal degradation, and unchanged plain/JSON/export contracts
- [x] T034 Run the focused quickstart checks from `specs/014-workable-result-grid/quickstart.md`, including `cargo test app::grid`, `cargo test query::result`, `cargo test postgres::session`, `cargo test app::update`, `cargo test ui::keymap`, `cargo test ui::layout`, and `git diff --check`
- [x] T035 Run disposable PostgreSQL plain and TLS live checks using the repository workflow and record their version, type-description, text-value, NULL, and no-rerun evidence in `docs/status.md`; record Unix-socket or platform checks as skipped when unavailable
- [x] T036 Run `cargo --locked xtask verify` in the authorized host context, preserve the full failure summary if any gate is unavailable, and update `docs/status.md` with separate focused, full-suite, live, skipped, and unverified evidence
- [x] T037 Review the final diff against `specs/014-workable-result-grid/contracts/result-grid.md`, `docs/status.md`, and the existing worktree, confirming no unrelated changes, secrets, SQL/value logging, clipboard path, automatic rerun, publish, or release action was introduced

---

## Dependencies and execution order

### Phase dependencies

- **Setup (Phase 1)**: T001 establishes the module seam.
- **Foundational (Phase 2)**: T002-T008 depend on the existing result/session
  boundaries and block all story work.
- **User Story 1 (Phase 3)**: T009-T015 depend on T005-T007; this is the MVP
  increment and should be validated before the wide-grid work.
- **User Story 2 (Phase 4)**: T016-T025 depend on the foundational state and
  the displayed-row mapping from T013-T014; it also consumes US1 selection
  semantics.
- **User Story 3 (Phase 5)**: T026-T030 depend on the controls implemented for
  US2 and the reset behaviour from T007.
- **Polish (Phase 6)**: T031-T037 depend on all desired story checkpoints.

### User story dependencies

- **US1**: Starts after Foundational and is independently testable.
- **US2**: Starts after Foundational and uses US1's source-row and selection
  mapping so sorting, filtering, and hiding cannot diverge.
- **US3**: Starts after US2's controls exist because it exposes and recovers the
  same commands through discoverable surfaces.

### Parallel opportunities

- T002 and T005 can proceed in parallel after T001; T008 can proceed in parallel
  once the existing output contracts are inspected.
- T009, T010, and T011 are independent test additions and can be prepared in
  parallel before the US1 implementation tasks.
- T016, T017, and T018 are independent test additions across grid, palette, and
  layout seams and can be prepared in parallel.
- T026 and T027 can be prepared in parallel after the controls API is stable.
- T031, T032, and T033 touch separate evidence surfaces and can proceed in
  parallel after the implementation is complete.

## Parallel examples

### User Story 1

```text
Task T009: ordering tests in src/app/grid.rs
Task T010: reducer tests in src/app/update.rs
Task T011: renderer tests in src/ui/layout.rs
```

### User Story 2

```text
Task T016: state tests in src/app/grid.rs
Task T017: palette and keymap tests in src/app/palette.rs and src/ui/keymap.rs
Task T018: renderer buffer tests in src/ui/layout.rs
```

### User Story 3

```text
Task T026: palette and reducer tests in src/app/palette.rs and src/app/update.rs
Task T027: degraded-layout tests in src/ui/layout.rs
```

## Implementation strategy

### MVP first

1. Complete T001-T008 to establish truthful metadata and isolated state.
2. Complete T009-T015 for the local retained-row sort MVP.
3. Stop at the US1 checkpoint and run its focused tests plus the existing
   verifier gates before adding wide-result controls.

### Incremental delivery

1. Add US1 and validate source-row selection and no-rerun behaviour.
2. Add US2 and validate responsive rendering at full and degraded terminal
   sizes.
3. Add US3 and validate discoverability and reset/new-result recovery.
4. Complete T031-T037 and report unavailable live or platform evidence
   separately.

## Notes

- Every task is executable from the repository root and names its target file.
- `[P]` marks only work that can proceed without an incomplete dependency or
  same-file race.
- Grid state is view-only and in memory; no task authorises commit, push, tag,
  release, clipboard transfer, query replay, or destructive cleanup.
