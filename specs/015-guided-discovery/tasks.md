---

description: "Implementation tasks for guided discovery in the interactive workbench"

---

# Tasks: Guided discovery

**Input**: Design documents from `/specs/015-guided-discovery/`

**Prerequisites**: `plan.md`, `spec.md`, `research.md`, `data-model.md`,
`contracts/discovery.md`, and `quickstart.md`

**Tests**: Included because the feature specification requires independent
acceptance checks and the repository constitution requires explicit evidence for
state transitions, terminal rendering, preserved scripted contracts, and live
database behaviour where relevant.

**Organization**: Tasks are grouped by user story. Pure discovery rules and the
active-keymap boundary come first; each story then adds an independently
testable presentation slice.

## Phase 1: Setup

**Purpose**: Establish the application-owned discovery seam without changing
scripted output, SQL execution, or existing palette effects.

- [x] T001 [P] Add the `src/app/discovery.rs` module declaration and public
  exports in `src/app/mod.rs`, keeping discovery derived and side-effect free
- [x] T002 [P] Add `KeymapSnapshot`, contextual hint data, and discovery test
  fixtures in `src/app/discovery.rs`, with no dependency from the application
  layer on terminal input implementation details

## Phase 2: Foundational

**Purpose**: Make the active configured keymap and the existing model state
available to every discovery surface through one truthful boundary.

**Critical**: Complete this phase before story work so configured-key and
applicability decisions cannot drift between the footer, palette, and empty
states.

- [x] T003 [P] Add active-binding and chord-prefix snapshot generation in
  `src/ui/keymap.rs`, including configured replacements and a test that the
  snapshot resolves every advertised key to its action
- [x] T004 Add the read-only keymap snapshot field to `src/app/model.rs` and
  initialise it at interactive startup in `src/cli/interactive.rs`, without
  storing SQL, credentials, or onboarding state
- [x] T005 [P] Add pure discovery context classification in
  `src/app/discovery.rs` for focus, connection posture, query phase, editor
  content, retained result state, object-tree state, history, and transaction
  recovery
- [x] T006 [P] Add applicability and prerequisite tests in
  `src/app/discovery.rs` for disconnected, connecting, connected, lost, and
  failed connections; empty and non-empty editor; running and cancellation
  states; rows, no rows, filtered-empty results; and failed transactions
- [x] T007 Add `action_is_available`, intent-group mapping, and bounded ordered
  candidate lists in `src/app/discovery.rs`, using existing `Action` values and
  never creating a second reducer or effect path
- [x] T008 Add model and keymap regression tests proving discovery derivation
  has no execution, connection, metadata, file, history, clipboard, or buffer
  side effect

**Checkpoint**: All discovery decisions are pure, the real configured keymap
can be read by the interactive model, and existing scripted paths are unchanged.

## Phase 3: User Story 1 - Start without a manual (Priority: P1)

**Goal**: Make the first frame and the principal empty panes explain the current
state and one safe next action in short plain language.

**Independent Test**: Render fresh models at full, compact, narrow, ASCII, and
no-colour sizes for unusable and usable connections. Confirm the state and next
action are present while editor, connection, and result state remain unchanged.

### Tests for User Story 1

- [x] T009 [P] [US1] Add renderer buffer tests in `src/ui/layout.rs` for fresh
  disconnected, connecting, connected-empty, connected-with-SQL, and existing
  result states at 80x24 and compact dimensions
- [x] T010 [P] [US1] Add degraded-presentation assertions in `src/ui/layout.rs`
  for ASCII, no-colour, narrow, reduced-motion, and plain-adjacent rendering,
  proving state words and recovery actions remain visible
- [x] T011 [P] [US1] Add a reducer no-side-effect test in `src/app/update.rs`
  proving that opening, rendering, and dismissing discovery copy never changes
  the submitted SQL, focus, selection, connection, history, or result data

### Implementation for User Story 1

- [x] T012 [US1] Add state-specific editor empty-state copy in `src/ui/layout.rs`
  for unusable connection, connecting, usable-empty, and non-empty buffer
  contexts, naming a valid next action without a modal
- [x] T013 [US1] Add state-specific results placeholder and failed-transaction
  recovery copy in `src/ui/layout.rs`, distinguishing no result, running,
  no rows, filtered-empty, and rollback-required states
- [x] T014 [US1] Add object-tree empty, loading, failed, and no-filter-match
  recovery copy in `src/ui/layout.rs`, preserving existing reload and filter
  behaviour
- [x] T015 [US1] Thread the active keymap into empty-state rendering where a key
  is shown, and use the snapshot/keymap helper rather than duplicating default
  shortcut strings

**Checkpoint**: A new user can identify connection posture, where to type, how
to run when safe, and where results or object recovery actions will appear.

## Phase 4: User Story 2 - Know what matters at the current focus (Priority: P1)

**Goal**: Replace the global footer hint list with a bounded, focus-aware rail
whose keys and action labels come from the active keymap.

**Independent Test**: Render Editor, Results, and Objects with empty, populated,
filtered, and busy states. Confirm no more than five hints are shown, every key
resolves to the adjacent action, and focus changes the priorities.

### Tests for User Story 2

- [x] T016 [P] [US2] Add keymap tests in `src/ui/keymap.rs` for primary keys,
  grouped movement labels, chord-prefix labels, configured replacement keys,
  and missing/unavailable action lookups
- [x] T017 [P] [US2] Add discovery ordering tests in `src/app/discovery.rs`
  for editor, results, and objects focus, including busy cancellation,
  filtered results, hidden sidebar, empty tree, and no-result states
- [x] T018 [P] [US2] Add footer renderer tests in `src/ui/layout.rs` proving the
  five-action bound, focus-specific action words, actual configured keys, and
  readable ASCII/no-colour output

### Implementation for User Story 2

- [x] T019 [US2] Add contextual hint lookup helpers to `src/ui/keymap.rs` for
  direct bindings, grouped movement, and chord continuations using the active
  prefix key
- [x] T020 [US2] Implement bounded contextual hint selection in
  `src/app/discovery.rs`, prioritising the useful actions for each focus and
  state while omitting impossible or misleading actions
- [x] T021 [US2] Update `render_footer` in `src/ui/layout.rs` to render the
  contextual rail, retain status and transaction truth, and fit long configured
  keys and labels within narrow widths
- [x] T022 [US2] Update keymap and interaction documentation contracts in
  `docs/design/keymap.md` and `docs/design/interaction-principles.md` for the
  bounded contextual rail and active-key authority

**Checkpoint**: Focus is visible and actionable. Editor, Results, and Objects
each teach their next useful actions without advertising another pane's no-op.

## Phase 5: User Story 3 - Ask what can be done (Priority: P2)

**Goal**: Make the general palette a contextual command palette that searches by
intent, groups useful actions, includes actual keys, and explains prerequisites.

**Independent Test**: Open the palette in each focus with and without a
connection, search `run`, `inspect`, `help`, `objects`, `save`, and `result`,
then dismiss and choose an applicable action. Confirm search/open/dismiss emit
no effect and chosen actions retain their existing reducer semantics.

### Tests for User Story 3

- [x] T023 [P] [US3] Add palette model tests in `src/app/palette.rs` for the
  `Command palette` purpose, search guidance, intent groups, contextual notes,
  no-match recovery, and unchanged Escape semantics
- [x] T024 [P] [US3] Add palette derivation and reducer tests in `src/app/update.rs`
  for applicable run/help/inspect/object/save/result entries, prerequisite
  wording when unavailable, configured key details, and no I/O on open/search
- [x] T025 [P] [US3] Add renderer tests in `src/ui/layout.rs` for the command
  palette title, query instructions, group labels, key/prerequisite details,
  no-match recovery, narrow layout, ASCII, and no-colour output

### Implementation for User Story 3

- [x] T026 [US3] Add a general command-palette purpose, contextual note, and
  search/leave guidance to `src/app/palette.rs` without changing specialised
  history, dependency, saved-query, or result-grid palette contracts
- [x] T027 [US3] Rework `palette_entries` in `src/app/update.rs` to derive
  plain-language intent groups and actual active key details from discovery,
  while retaining object entries and existing palette commands
- [x] T028 [US3] Add prerequisite-aware empty search wording in
  `src/app/discovery.rs` and `src/ui/layout.rs` for connection, SQL, result,
  object, saved-query, and history requirements
- [x] T029 [US3] Wire the general palette opening paths to the new purpose and
  active key snapshot, preserving connection-details nesting and selected-entry
  effect behaviour

**Checkpoint**: The palette answers what can be done now, finds actions by
intent, and delegates chosen work to the existing safe reducer paths.

## Phase 6: User Story 4 - Recover from an empty or blocked state (Priority: P2)

**Goal**: Ensure every named empty or blocked state has a specific, safe,
readable recovery path.

**Independent Test**: Drive empty results, filtered-empty results, empty trees,
failed connection, failed transaction, no saved queries, and no-match palette
queries through the reducer and renderer. Confirm each state says what is known
and what can be done next, with no automatic work.

### Tests for User Story 4

- [x] T030 [P] [US4] Add reducer and renderer tests for result-filter clear,
  failed-transaction rollback wording, failed/lost connection wording,
  object reload/filter recovery, and no saved-query guidance
- [x] T031 [P] [US4] Add hostile-text and sanitisation assertions for discovery
  copy, object names, filter text, diagnostics, and palette query text across
  ASCII and no-colour modes

### Implementation for User Story 4

- [x] T032 [US4] Refine existing result filter and transaction recovery lines
  in `src/ui/layout.rs` so searched text, clear-filter action, and rollback
  prerequisite remain distinct from no-result and running states
- [x] T033 [US4] Refine saved-query, object-tree, connection-failure, and
  unavailable-action copy in `src/app/palette.rs` and `src/ui/layout.rs`,
  retaining specialised palette safety notes
- [x] T034 [US4] Verify discovery action selection delegates only to existing
  reducer actions and cannot execute, reload, save, export, record history, or
  change transaction state merely by opening, searching, or rendering

**Checkpoint**: Empty and blocked surfaces explain their boundary and recovery
without inviting an unsafe or impossible retry.

## Phase 7: Polish and cross-cutting verification

**Purpose**: Close documentation, compatibility, accessibility, and evidence
gaps without claiming unavailable platform or live-server evidence.

- [x] T035 [P] Update `docs/product/journeys.md`,
  `docs/product/experience-roadmap.md`, and `docs/product/roadmap.md` with
  first-frame guidance, contextual hints, command-palette intent search, and
  empty-state recovery
- [x] T036 [P] Update `docs/support/diagnostics.md`,
  `docs/support/compatibility.md`, and `CHANGELOG.md` with discovery scope,
  configured-key authority, terminal degradation, and unchanged scripted
  contracts
- [x] T037 [P] Add or update documentation contract assertions in
  `tests/documentation_matches_the_build.rs` and `tests/cli_contract.rs` to
  prove Feature015 does not change plain, JSON, streaming, export, history, or
  terminal-restoration output
- [x] T038 Run the focused checks from
  `specs/015-guided-discovery/quickstart.md`, including discovery, palette,
  update, keymap, layout, CLI-contract, and `git diff --check` checks
- [x] T039 Run `cargo --locked xtask verify` in the authorized host context,
  preserving the full failure summary and reporting any integration or
  platform checks that remain skipped
- [x] T040 Start disposable PostgreSQL only if the full verifier requires it,
  record live version and unchanged query evidence separately, then tear down
  only the disposable resources with `cargo xtask db down`
- [x] T041 Review the final diff against the feature contracts, constitution,
  `docs/status.md`, and the existing worktree, confirming no secrets, I/O,
  telemetry, clipboard path, automatic query, unrelated file change, commit,
  push, tag, release, or publication was introduced

## Dependencies and execution order

### Phase dependencies

- **Setup (Phase 1)**: T001-T002 establish the pure discovery seam.
- **Foundational (Phase 2)**: T003-T008 depend on the existing model and keymap
  contracts and block user-story work.
- **User Story 1 (Phase 3)**: T009-T015 depend on the foundational context and
  establish the first-frame MVP.
- **User Story 2 (Phase 4)**: T016-T022 depend on T003-T007 and consume the
  first-frame rendering seam.
- **User Story 3 (Phase 5)**: T023-T029 depend on active-key and applicability
  derivation from the foundational phase.
- **User Story 4 (Phase 6)**: T030-T034 depend on the story rendering and
  palette paths.
- **Polish (Phase 7)**: T035-T041 depend on all desired story checkpoints.

### Parallel opportunities

- T003 and T005 can proceed in parallel after T001-T002.
- T009-T011 are independent renderer/reducer evidence tasks.
- T016-T018 are independent keymap, pure-rule, and renderer tests.
- T023-T025 are independent palette model, reducer, and renderer tests.
- T030-T031 can proceed in parallel before the recovery copy changes.
- T035-T037 touch separate documentation and contract surfaces.

## Implementation strategy

### MVP first

1. Complete T001-T008 and prove the pure context and active-key boundary.
2. Complete T009-T015 for first-frame and empty-state guidance.
3. Stop at the US1 checkpoint and run focused checks before changing the
   footer or general palette.

### Incremental delivery

1. Add first-frame recovery copy and validate degraded presentations.
2. Add contextual hints and validate configured-key resolution.
3. Add intent-search palette entries and prerequisite notes.
4. Close blocked-state recovery, documentation, and evidence gates.

## Notes

- Every task is executable from the repository root and names its target file.
- `[P]` marks only work that can proceed without an incomplete dependency or
  same-file race.
- Discovery is derived in memory. No task authorises commit, push, tag, release,
  clipboard transfer, query replay, telemetry, or destructive cleanup.
