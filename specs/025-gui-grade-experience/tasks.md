---

description: "Implementation tasks for the GUI-grade experience"
---

# Tasks: GUI-grade experience

**Input**: Design documents from `specs/025-gui-grade-experience/`.

**Prerequisites**: `spec.md`, `plan.md`, `research.md`, `data-model.md`,
`contracts/presentation.md`, `contracts/mouse.md`,
`contracts/theme-config.md`, `contracts/grid.md`, and `quickstart.md`.

**Tests**: Tests are required by the specification. Each story's tests are
written and watched failing before its implementation tasks are marked
complete.

## Worker rules

This task list is decomposed for the multiagent setup in
`docs/operations/agent-orchestration.md` and
`docs/operations/agent-playbook.md`. These rules are part of the tasks, not
advice:

- At most 3 concurrent workers, and at most one of them in the integrator
  lane at any time.
- Single-writer files: `src/app/model.rs`, `src/app/message.rs`,
  `src/app/update.rs`, `src/cli/interactive.rs`, `src/ui/layout.rs`,
  `Cargo.lock`, CI files, `docs/status.md`. Only the integrator edits
  them, one change at a time.
- `[P]` is legal only when both tasks have disjoint owned-path lists and
  neither list contains a single-writer file. This predicate is checkable;
  the reviewer enforces it.
- No task in Phase 3 or later starts until Phase 2 is implemented,
  independently reviewed and accepted. Before the extraction lands,
  parallelism is fake: everything is still one file.
- Implementers run only the focused commands named in their assignment
  block. They never run `cargo xtask db*`, never run
  `cargo --locked xtask verify`, never commit, and never edit
  `docs/design/*` (those documents are tested against the current build;
  they change in Phase 12 together with the behaviour, with one
  exception below).
- Exception for bindable actions: `tests/documentation_matches_the_build.rs`
  compares `docs/design/keymap.md` against the build in both directions,
  so any task that adds a bindable action (T022, T031, T041) hands the
  matching `docs/design/keymap.md` rows and parity-assertion updates to
  the integrator to land in the same change. This keeps the parity suite
  green between phases; T046 covers the narrative design documents only.
- The contracts' pinned values are decisions. A missing value means stop
  and report, never choose.
- Every substantive slice gets an independent review on a different model
  family before the lead accepts it.

## Phase 1: Setup

- [x] T001 Confirm `specs/025-gui-grade-experience/` is active in
  `.specify/feature.json` and that its specification, plan, research,
  data-model, contracts and quickstart agree on one painted, mouse-capable,
  theme-owning interactive experience with keyboard parity throughout.

## Phase 2: Foundational extraction (serialized, integrator only)

**Goal**: The renderer becomes per-pane widget modules with pure hit
regions, presentation becomes model state, and mouse and paste events
arrive as inert messages. Behaviour is identical before and after.

- [x] T002 Create `src/ui/widgets/mod.rs` and move the header and footer
  renderers from `src/ui/layout.rs` into `src/ui/widgets/header.rs` and
  `src/ui/widgets/footer.rs` wholesale, wiring `layout.rs` to call them;
  no rendered output changes.
- [x] T003 Move the editor and completion renderers into
  `src/ui/widgets/editor.rs` and the object tree renderer into
  `src/ui/widgets/objects.rs`; no rendered output changes.
- [x] T004 Move the grid, inspector, expanded-row and results renderers
  into `src/ui/widgets/results.rs`; no rendered output changes.
- [x] T005 Move the palette and connection-details renderers into
  `src/ui/widgets/palette.rs` and the prompt, confirmation, plan and
  definition renderers into `src/ui/widgets/overlays.rs`, leaving
  `src/ui/layout.rs` as the orchestrator that owns `Presentation`,
  `LayoutMode`, `render`, `main_panes`, `body_area`, `render_to_string`
  and `buffer_to_string`; no rendered output changes.
- [x] T006 Add `Region` and the pure
  `region_at(model, area, col, row) -> Option<Region>` in
  `src/ui/widgets/mod.rs`, derived from the same layout arithmetic the
  renderers use, with unit tests for every region kind, boundaries and
  misses at both layout modes.
- [x] T007 Move `Presentation` into `Model` in `src/app/model.rs`,
  constructing it in `src/cli/interactive.rs` at startup and threading it
  from the model through `src/ui/layout.rs`; no behaviour changes.
- [x] T008 Add `Message::Mouse { kind, column, row, modifiers }` and
  `Message::Pasted(String)` in `src/app/message.rs`, translate
  `Event::Mouse` and `Event::Paste` in `src/cli/interactive.rs`, and add
  exhaustive inert arms in `src/app/update.rs`; no behaviour changes.
- [x] T009 Gate: `cargo fmt --all -- --check`, workspace clippy with
  warnings denied, and the full pre-existing `ui::layout`, `app::update`
  and parity test sets pass unchanged at the new module layout; the
  independent reviewer accepts the extraction before any later phase
  starts.

**Checkpoint**: Structure only. Any rendered-output diff is a defect.

## Phase 3: User Story 1 - Pasted SQL arrives in the editor (Priority: P1) 🎯 MVP

**Goal**: A paste lands at the caret as one undoable edit; no paste is
silently dropped.

**Independent Test**: Deliver paste messages with CRLF text, oversize
text, and while prompts and confirmations are open; assert insertion,
refusal and acknowledgement per spec with zero executions.

### Tests for User Story 1

- [x] T010 [US1] Add failing tests in `src/app/editor.rs` (or its
  `tests.rs`) and `src/app/update/tests.rs` for caret insertion as one
  undo step, CRLF and CR normalisation, the 1 MiB bound with its named
  refusal, revision advance, no execution, paste into a masked prompt
  without echo, and the ignored-paste notice when a confirmation owns
  input.

### Implementation for User Story 1

- [x] T011 [US1] Implement paste insertion through the editor's existing
  edit primitives in `src/app/editor.rs`; the integrator fills the
  `Message::Pasted` arm in `src/app/update.rs` with routing to editor,
  text-accepting prompts, and the acknowledged-ignore path.
- [x] T012 [US1] Render the ignored-paste and refusal notices through the
  existing transient-notice route and assert their wording, narrow layout
  and ASCII output in `src/ui/widgets/footer.rs` tests.

**Checkpoint**: Pasting works everywhere text is accepted and is honest
everywhere it is not.

## Phase 4: User Story 2 - The screen is a surface (Priority: P1)

**Goal**: Every frame is painted on the three-level ladder at a detected
colour depth.

**Independent Test**: Render across depths and themes; assert no
terminal-background cell at truecolor and 256, quantizer determinism, and
unchanged meaning words at every depth.

### Tests for User Story 2

- [ ] T013 [US2] Add failing tests for the detection precedence of
  `contracts/presentation.md` in `src/ui/terminal.rs`, and for the
  quantizer and 16-colour table in `src/ui/theme.rs` (exact cube and grey
  candidates, tie-to-lower-index, no indices 0-15 from the quantizer).

### Implementation for User Story 2

- [ ] T014 [US2] Implement `ColorDepth`, the quantizer, the fixed
  16-colour table and depth-aware emission at the single Rgb conversion
  point in `src/ui/theme.rs`, and detection in `src/ui/terminal.rs`.
- [ ] T015 [P] [US2] Add `ui.color-depth` to `src/config/schema.rs`, the
  template block in `src/config/store.rs`, and the `--color-depth` flag in
  `src/cli/mod.rs`, with refusal tests for invalid values.
- [ ] T016 [US2] Add `SurfacePane` and `SurfaceOverlay` tokens to
  `src/ui/theme.rs` with the contract's starting values in all three
  palettes, passing the existing contrast tests (adjusting hex under the
  tests where required and recording the adjustment).
- [ ] T017 [US2] Paint the base fill in `src/ui/layout.rs` (integrator
  lane) and the pane and overlay surfaces in `src/ui/widgets/*.rs`,
  including Clear-then-block overlays, with layout tests asserting no
  unpainted cell at truecolor and 256 depth and unpainted surfaces at
  16-colour and colour-off.
- [ ] T018 [P] [US2] Report depth, source and override in
  `src/diagnostics/doctor.rs` with tests for the contract's wording.
- [ ] T019 [US2] Extend the meaning-parity matrix in `src/ui/layout.rs`
  tests to all four colour depths.

**Checkpoint**: The product owns its background and degrades honestly.

## Phase 5: User Story 3 - A results grid you can work (Priority: P1)

**Goal**: Position in words, three-state sort, bounded column resizing,
wheel scrolling; keyboard routes complete without a mouse.

**Independent Test**: Drive sort, resize and scrolling by keyboard on a
retained multi-screen result; assert the contract's exact wording shapes,
bounds and zero executions.

### Tests for User Story 3

- [ ] T020 [US3] Add failing tests in `src/app/grid.rs` and
  `src/ui/widgets/results.rs` for the range wording shapes of
  `contracts/grid.md` (plain, truncated, filtered, empty), the sort cycle
  and its wording, and width bounds and steps.

### Implementation for User Story 3

- [ ] T021 [US3] Implement the range line, the sort-cycle action and
  column-width state in `src/app/grid.rs`, rendered in
  `src/ui/widgets/results.rs`; sorting and resizing remain local view
  state with no execution effect and preserved selection identity.
- [ ] T022 [US3] Add `narrow-column` and `widen-column` bindings
  (`Ctrl+K [`, `Ctrl+K ]`) in `src/ui/keymap.rs`, palette entries in
  `src/app/palette.rs`, hints in `src/app/discovery.rs`, and reachability
  tests; the integrator lands the matching `docs/design/keymap.md` rows
  and parity assertions in the same change.
- [ ] T023 [US3] Assert NULL, empty and literal-NULL distinction on
  painted stripes at every depth in `src/ui/widgets/results.rs` tests.

**Checkpoint**: The grid is workable by keyboard alone; mouse verbs attach
in Phase 8.

## Phase 6: User Story 4 - The first frame is already useful (Priority: P1)

**Goal**: A painted shell from the first frame, named connecting steps,
and a productive first connected frame.

**Independent Test**: Drive launch to connection in the reducer; assert
step naming, shell completeness, post-connect focus and expansion, and the
unconfigured next-action frame.

### Tests for User Story 4

- [ ] T024 [US4] Add failing tests for step wording with target and
  elapsed time, skipped-step honesty, failure naming the failing step,
  first-frame shell completeness, post-connect editor focus with the run
  hint, first-level tree expansion, and the unconfigured next action, in
  `src/app/update/tests.rs`, `src/ui/widgets/header.rs` and
  `src/ui/widgets/objects.rs`.

### Implementation for User Story 4

- [ ] T025 [US4] Report named connecting steps from
  `src/connection/service.rs` as messages; the integrator adds the step
  variant in `src/app/message.rs` and the `ConnectionState::Connecting`
  step field in `src/app/model.rs`.
- [ ] T026 [US4] The integrator routes step transitions, post-connect
  focus and first-level expansion in `src/app/update.rs`, skipping
  inapplicable steps without showing them.
- [ ] T027 [US4] Render steps in `src/ui/widgets/header.rs`, the loading
  and expanded tree states in `src/ui/widgets/objects.rs`, and the
  unconfigured first frame's single next action.

**Checkpoint**: Startup reads as a designed product before the first row
arrives.

## Phase 7: User Story 5 - Themes the user owns (Priority: P2)

**Goal**: Validated `[theme]` configuration, runtime switching, and
truthful `auto`.

**Independent Test**: Load valid, partial and each invalid `[theme]`
table; assert the contract's exact refusal lines and exit 3; switch at
runtime; drive `auto` with an answering and a silent terminal.

### Tests for User Story 5

- [ ] T028 [US5] Add failing tests for `extends` inheritance, each refusal
  wording line of `contracts/theme-config.md`, colour-off neutrality of
  custom themes, and the OSC 11 reply parser (both terminators, component
  scaling, malformed replies) in `src/config/schema.rs` and
  `src/ui/theme.rs`.

### Implementation for User Story 5

- [ ] T029 [US5] Parse and validate `[theme]` in `src/config/schema.rs`
  and build the custom theme in `src/ui/theme.rs`, refusing whole with
  exit 3 before the terminal is taken.
- [ ] T030 [P] [US5] Add the `[theme]` block and new `[ui]` keys to the
  template in `src/config/store.rs` in the same change as the schema.
- [ ] T031 [US5] The integrator adds the `switch-theme` action
  (`src/app/message.rs`, `src/app/update.rs`), its palette entry in
  `src/app/palette.rs`, and the presentation swap on the next frame, with
  the contract's cycle order, landing the matching `docs/design/keymap.md`
  action row and parity assertions in the same change.
- [ ] T032 [US5] The integrator implements the one-shot OSC 11 exchange in
  `src/cli/interactive.rs` before the alternate screen with the 100 ms
  deadline and drain rule, the luminance decision in `src/ui/theme.rs`,
  and the answered and unanswered `doctor` wording in
  `src/diagnostics/doctor.rs`.
- [ ] T033 [US5] Add the valid and hostile fixture themes to the parity
  matrix in `src/ui/layout.rs` tests.

**Checkpoint**: Appearance is owned, validated and truthful.

## Phase 8: User Story 6 - The mouse is a first-class path (Priority: P2)

**Goal**: The contract's verb table dispatches through hit regions to
existing actions; capture defaults on with a documented escape.

**Independent Test**: Deliver every verb, every unbound event and the drag
mechanics as messages; assert state identical to keyboard routes and
no-ops elsewhere; rerun the interaction set with the mouse off.

### Tests for User Story 6

- [ ] T034 [US6] Add failing tests in `src/app/update/tests.rs` for every
  row of the `contracts/mouse.md` verb table (state equality with the
  keyboard route), unbound events as no-ops, Ctrl-click ignored, drag end
  on any Up, resize-cancels-drag, and the 400 ms double-click window.

### Implementation for User Story 6

- [ ] T035 [US6] The integrator implements mouse dispatch in
  `src/app/update.rs`: `region_at`, the verb table as data, wheel steps
  and drag clamps per the contracts.
- [ ] T036 [US6] Flip the capture default in `src/config/schema.rs` and
  `src/ui/terminal.rs`, amending the two tests that pin the old default,
  and add the help wording for Shift-selection and the opt-out.
- [ ] T037 [US6] Add a mouse-off test run proving every capability in the
  interaction test set is reachable with `ui.mouse = false`.

**Checkpoint**: The pointer works everywhere it should and nowhere it
should not.

## Phase 9: User Story 7 - Panes that move (Priority: P2)

**Goal**: Adjustable splits, exact-restore zoom, two-value density.

**Independent Test**: Drive split keys, drags, zoom and density through
the reducer and renderer; assert bounds, steps, exact restore, spacing
tables and the compact-layout explanation.

### Tests for User Story 7

- [ ] T038 [US7] Add failing tests for split bounds and steps, zoom
  save-and-exact-restore, paging that follows resized panes, density
  spacing tables, session-locality and the single-pane explanation, in
  `src/app/update/tests.rs` and `src/ui/layout.rs` tests.

### Implementation for User Story 7

- [ ] T039 [US7] The integrator adds `Model::splits` in
  `src/app/model.rs`, the `shrink-pane`, `grow-pane` and `zoom-pane`
  actions in `src/app/message.rs` and `src/app/update.rs`, and split-aware
  geometry in `src/ui/layout.rs` (`main_panes`, `body_area`, paging
  helpers).
- [ ] T040 [P] [US7] Add `ui.density` to `src/config/schema.rs` and the
  template in `src/config/store.rs`, and consume the spacing table in
  `src/ui/widgets/*.rs`.
- [ ] T041 [US7] Add the key defaults (`Ctrl+K ,`, `Ctrl+K .`,
  `Ctrl+K z`) in `src/ui/keymap.rs`, palette entries in
  `src/app/palette.rs`, hints in `src/app/discovery.rs`, and reachability
  tests; the integrator lands the matching `docs/design/keymap.md` rows
  and parity assertions in the same change.

**Checkpoint**: Layout serves the task, and a new session starts clean.

## Phase 10: User Story 8 - Quiet chrome (Priority: P3)

**Goal**: Threshold-gated telemetry and the fixed-domain capsule set.

**Independent Test**: Render across environments and timings; assert the
domain map, gating at 200 ms, loud production, silent local default and
reversed-text degradation.

- [ ] T042 [US8] Add failing tests for the badge domain map, elapsed
  gating, the left-identity and right-context split, the unchanged
  six-hint footer cap, and colour-off reversed text, in
  `src/ui/widgets/header.rs` and `src/ui/widgets/footer.rs`.
- [ ] T043 [US8] Extend the capsule helpers in `src/ui/theme.rs` and
  implement the header and footer arrangement in
  `src/ui/widgets/header.rs` and `src/ui/widgets/footer.rs`.

**Checkpoint**: Chrome is earned, never ambient noise.

## Phase 11: User Story 9 - Overlays with a preview (Priority: P3)

**Goal**: Owner-geometry palettes with retained-data previews.

**Independent Test**: Open each purpose; assert geometry, preview sources,
full-width fallback, narrow omission and zero fetching.

- [ ] T044 [US9] Add failing tests for the 80 percent height, input at the
  top, the 60 percent preview for definition, saved-query, profile and
  dependency purposes, full width elsewhere, narrow omission and
  no-execution previews, in `src/app/palette.rs` and
  `src/ui/widgets/palette.rs`.
- [ ] T045 [US9] Implement preview sources in `src/app/palette.rs` and the
  geometry in `src/ui/widgets/palette.rs`, sanitising previewed text
  through the existing display rules.

**Checkpoint**: Choosing something shows what it is before Enter.

## Phase 11b: User Story 10 - The name materializes (Priority: P3)

**Goal**: An original ASCII identity mark in the README and a bounded boot
reveal in the interactive client, over the connecting wait.

**Independent Test**: Drive the connecting frames through the reducer at
the existing tick and assert the pinned sweep and easing, any key or
connection outcome jumping to the final frame, reduced motion rendering
the final frame from the first tick, and plain mode and machine output
never carrying banner bytes.

### Tests for User Story 10

- [ ] T052 [US10] Add failing tests first: reducer tests in
  `src/app/update/tests.rs` for sweep progression (the easing values at
  ticks 0, 6 and 12), skip-on-key with the key still delivered to its
  normal handler, truncate on any connection outcome, once per process,
  and reduced-motion final-first; and renderer tests in
  `src/ui/widgets/banner/tests.rs` for the three pinned variants, the
  per-tier decode charsets, no banner on the too-small layout, and no
  banner bytes in plain mode or machine output.

### Implementation for User Story 10

- [ ] T053 [US10] Implement the banner art and the pure sweep functions in
  `src/ui/widgets/banner.rs` (new file, not single-writer) and render it
  from the connecting shell; the integrator routes the small reducer arm
  and the once-per-process fact through `src/app/model.rs` and
  `src/app/update.rs`; add the gradient anchors in `src/ui/theme.rs`
  under the existing contrast tests. No new dependency.
- [ ] T054 [US10] Add the ASCII-tier art to `README.md` under the CI
  badge, replacing the current tagline paragraph, which moves below the
  block; monochrome only, no screenshots or GIFs. `README.md` has no
  parity test and is not `docs/design/`.

**Checkpoint**: The name appears while the wait is real, and disappears
when it is not.

## Phase 12: Polish and cross-cutting concerns

- [ ] T046 Amend the narrative design documents in the same change as the
  final behaviour: `docs/design/theme-tokens.md` (painted surfaces,
  elevation, colour depth, the superseded Feature 007 override note),
  `docs/design/terminal-compatibility.md` (mouse default and rationale,
  OSC 11, `COLORTERM` detection), `docs/design/icons-and-motion.md`
  (capsule set; the motion section changes from two moving things to
  three, naming the boot reveal with its bounds and its reduced-motion
  parity), `docs/design/interaction-principles.md` (paste and mouse
  paragraphs; the avoided list entry "ASCII art on launch" becomes "ASCII
  art on launch, except the bounded US10 identity reveal: connecting
  frames only, 1.4 s cap, any-key skip, reduced-motion static, never in
  plain mode"), `docs/design/information-architecture.md` (first frame,
  results range line, splits, overlay geometry) and the
  `docs/design/keymap.md` mouse verb table (its per-action key rows will
  already have landed with T022, T031 and T041).
- [ ] T047 [P] Update `docs/support/compatibility.md`,
  `docs/product/experience-roadmap.md`, `docs/product/journeys.md`,
  `docs/security/data-handling.md` (paste bound and preview sanitising)
  and `CHANGELOG.md`.
- [ ] T048 Update the parity assertions in
  `tests/documentation_matches_the_build.rs` for every amended document,
  key and action in the same change as T046.
- [ ] T049 Run the focused checks from `quickstart.md` and record focused
  results and any skipped environment evidence.
- [ ] T050 Run `cargo --locked xtask verify` with the disposable database,
  then `cargo xtask db down` and `cargo xtask db status`; record full-suite
  results separately from semantic skips and manual terminal gaps in
  `docs/status.md`.
- [ ] T051 Mark completed tasks only after evidence is recorded, and run
  the normal-hyphen prose scan across all changed documentation.

## Dependencies and execution order

- T001 precedes everything. T002-T009 are strictly serialized in the
  integrator lane and gate every later phase.
- After T009: Phases 3, 4, 5 and 6 may run concurrently within the worker
  cap, subject to the `[P]` predicate and one-integrator rule; their
  integrator-lane tasks (T011, T017, T025, T026) queue through the single
  integrator.
- Phase 7 follows Phase 4 (it swaps model-owned presentation built there).
  Phase 8 follows Phases 5 and 6 (its verbs dispatch to grid and layout
  actions). Phase 9 follows Phase 2 only, but its T039 queues in the
  integrator lane. Phases 10 and 11 follow Phase 4.
- T046-T048 land together after all behaviour is stable; T049 precedes
  T050; T051 is last. US10 (T052-T054) follows Phases 4 and 6, the
  colour-depth machinery and the connecting shell it builds on, and
  precedes Polish; its design-document additions ride T046.

## Parallel opportunities

- Legal `[P]` pairs after T009 include: T015 with T020-T023; T018 with
  anything outside `src/diagnostics/`; T030 with T040; T042-T043 with
  T044-T045; T047 with T046 (distinct documents).
- The integrator lane is the bottleneck by design: T011, T017, T025, T026,
  T031, T032, T035, T039 and T046 serialize through it in that preferred
  order.

## Implementation strategy

Land the extraction first and prove it changed nothing. Then ship the P1
stories as the visible win: paste, painted surfaces, the workable grid and
the useful first frame. P2 adds ownership and the pointer; P3 refines
chrome and overlays; if scope pressure appears, Phases 10 and 11 detach
cleanly into a follow-up feature because they touch no single-writer file.
Do not add persistence, animation, context menus, network calls or a
second SQL source of truth anywhere in this feature.

## Worker assignment blocks

Pre-filled per the playbook shape. The lead fills `Source revision` at
assignment time. External actions already authorized: the owner granted
the lead branch, commit, push, pull-request and auto-merge authority for
this feature on 2026-09-18; implementers perform no git actions and every
block below records that lead-only grant.

### Foundational (integrator)

```text
Package / full feature slug / outcome IDs: W09 / 025-gui-grade-experience / O2, O3
Problem and expected user-visible result: extract widgets, hit regions,
  model-owned presentation, inert Mouse/Pasted arms; zero visible change.
Owned paths and shared-file coordinator: src/ui/layout.rs, src/ui/widgets/**,
  src/app/model.rs, src/app/message.rs, src/app/update.rs,
  src/cli/interactive.rs; coordinator: self (integrator).
Dependencies and explicit non-goals: none before it; no behaviour change,
  no styling change, no new bindings.
Acceptance scenarios / existing requirement and task IDs: T002-T009;
  plan.md Constitution Check IX (extraction proven by unchanged tests).
Commands and environment needed: cargo fmt --all -- --check;
  cargo clippy --locked --workspace --all-targets --all-features -- -D warnings;
  cargo test --locked --lib ui::layout; cargo test --locked --lib app::update;
  cargo test --locked --lib ui::widgets.
External actions already authorized: lead-only branch/commit/push/PR/auto-merge grant dated 2026-09-18; implementers perform no git actions.
Stopping condition: all listed commands green with pre-existing tests
  unchanged; do NOT run xtask verify, db fixtures, git commit, or edit
  docs/design/*.
```

### US1 paste (worker + integrator for T011's update.rs arm)

```text
Package / full feature slug / outcome IDs: W09 / 025-gui-grade-experience / O2
Problem and expected user-visible result: pasted SQL lands at the caret as
  one undo step; no paste silently dropped.
Owned paths and shared-file coordinator: src/app/editor.rs,
  src/ui/widgets/footer.rs; coordinator: integrator for src/app/update.rs.
Dependencies and explicit non-goals: after T009; no plain-mode change, no
  execution on paste, no clipboard read.
Acceptance scenarios / existing requirement and task IDs: FR-2501, FR-2502,
  SC-2501; T010-T012.
Commands and environment needed:
  cargo test --locked --lib app::editor;
  cargo test --locked --lib app::update::tests::paste;
  cargo test --locked --lib ui::widgets::footer.
External actions already authorized: lead-only branch/commit/push/PR/auto-merge grant dated 2026-09-18; implementers perform no git actions.
Stopping condition: named tests green after being watched failing; the
  standing prohibitions apply.
```

### US2 surfaces and depth (worker; integrator for T017's layout.rs fill)

```text
Package / full feature slug / outcome IDs: W09 / 025-gui-grade-experience / O2, O3
Problem and expected user-visible result: painted three-level surfaces at
  truecolor/256; deterministic quantization; honest 16-colour and off.
Owned paths and shared-file coordinator: src/ui/theme.rs,
  src/ui/terminal.rs, src/ui/widgets/**, src/diagnostics/doctor.rs,
  src/config/schema.rs, src/config/store.rs, src/cli/mod.rs; coordinator:
  integrator for src/ui/layout.rs.
Dependencies and explicit non-goals: after T009; contrast tests stay
  authoritative; no fourth elevation level; no indices 0-15 from the
  quantizer.
Acceptance scenarios / existing requirement and task IDs: FR-2503..2506,
  SC-2502, SC-2503; T013-T019; contracts/presentation.md.
Commands and environment needed: cargo test --locked --lib ui::theme;
  cargo test --locked --lib ui::terminal; cargo test --locked --lib config;
  cargo test --locked --lib ui::layout; cargo test --locked --lib diagnostics.
External actions already authorized: lead-only branch/commit/push/PR/auto-merge grant dated 2026-09-18; implementers perform no git actions.
Stopping condition: named tests green after being watched failing; the
  standing prohibitions apply.
```

### US3 grid (worker)

```text
Package / full feature slug / outcome IDs: W09 / 025-gui-grade-experience / O3
Problem and expected user-visible result: range wording, three-state sort,
  bounded column resizing, all by keyboard; zero executions.
Owned paths and shared-file coordinator: src/app/grid.rs,
  src/ui/widgets/results.rs, src/ui/keymap.rs, src/app/palette.rs,
  src/app/discovery.rs; coordinator: none.
Dependencies and explicit non-goals: after T009; mouse dispatch is US6;
  no server-side sort; no execution effects.
Acceptance scenarios / existing requirement and task IDs: FR-2507..2509,
  SC-2507; T020-T023; contracts/grid.md.
Commands and environment needed: cargo test --locked --lib app::grid;
  cargo test --locked --lib ui::widgets::results;
  cargo test --locked --lib ui::keymap.
External actions already authorized: lead-only branch/commit/push/PR/auto-merge grant dated 2026-09-18; implementers perform no git actions.
Stopping condition: named tests green after being watched failing; the
  standing prohibitions apply.
```

### US4 startup (worker + integrator for T025/T026)

```text
Package / full feature slug / outcome IDs: W09 / 025-gui-grade-experience / O2
Problem and expected user-visible result: painted shell from frame one,
  named connecting steps, productive first connected frame.
Owned paths and shared-file coordinator: src/connection/service.rs,
  src/ui/widgets/header.rs, src/ui/widgets/objects.rs; coordinator:
  integrator for src/app/model.rs, src/app/message.rs, src/app/update.rs.
Dependencies and explicit non-goals: after T009; do not respecify the
  Feature 020 picker; no persisted state; no splash.
Acceptance scenarios / existing requirement and task IDs: FR-2510..2512,
  SC-2505; T024-T027.
Commands and environment needed:
  cargo test --locked --lib connection::service;
  cargo test --locked --lib app::update::tests::connecting;
  cargo test --locked --lib ui::widgets::header;
  cargo test --locked --lib ui::widgets::objects.
External actions already authorized: lead-only branch/commit/push/PR/auto-merge grant dated 2026-09-18; implementers perform no git actions.
Stopping condition: named tests green after being watched failing; the
  standing prohibitions apply.
```

### US5 themes (worker + integrator for T031/T032)

```text
Package / full feature slug / outcome IDs: W09 / 025-gui-grade-experience / O2
Problem and expected user-visible result: validated [theme], runtime
  switching, truthful auto.
Owned paths and shared-file coordinator: src/config/schema.rs,
  src/config/store.rs, src/ui/theme.rs, src/diagnostics/doctor.rs;
  coordinator: integrator for src/app/message.rs, src/app/update.rs,
  src/app/palette.rs, src/cli/interactive.rs.
Dependencies and explicit non-goals: after Phase 4; exact refusal wording
  from contracts/theme-config.md; no theme files or directories; auto
  never selects custom.
Acceptance scenarios / existing requirement and task IDs: FR-2513..2516,
  SC-2503, SC-2506; T028-T033.
Commands and environment needed: cargo test --locked --lib config;
  cargo test --locked --lib ui::theme;
  cargo test --locked --lib app::update::tests::theme;
  cargo test --locked --lib diagnostics; cargo test --locked --lib ui::layout.
External actions already authorized: lead-only branch/commit/push/PR/auto-merge grant dated 2026-09-18; implementers perform no git actions.
Stopping condition: named tests green after being watched failing; the
  standing prohibitions apply.
```

### US6 mouse (integrator-heavy)

```text
Package / full feature slug / outcome IDs: W09 / 025-gui-grade-experience / O2, O3
Problem and expected user-visible result: the contract's verb table works;
  capture defaults on with a documented escape; keyboard parity proven.
Owned paths and shared-file coordinator: src/ui/terminal.rs,
  src/config/schema.rs, src/ui/widgets/mod.rs; coordinator: integrator for
  src/app/update.rs.
Dependencies and explicit non-goals: after Phases 5 and 6 (grid and layout
  actions exist); no right-click or context menus; no Ctrl-click binding;
  no verb outside contracts/mouse.md.
Acceptance scenarios / existing requirement and task IDs: FR-2517..2519,
  SC-2504; T034-T037.
Commands and environment needed:
  cargo test --locked --lib app::update::tests::mouse;
  cargo test --locked --lib ui::terminal; cargo test --locked --lib config.
External actions already authorized: lead-only branch/commit/push/PR/auto-merge grant dated 2026-09-18; implementers perform no git actions.
Stopping condition: named tests green after being watched failing,
  including the mouse-off run; the standing prohibitions apply.
```

### US7 panes and density (integrator for T039)

```text
Package / full feature slug / outcome IDs: W09 / 025-gui-grade-experience / O3
Problem and expected user-visible result: movable splits, exact-restore
  zoom, two-value density.
Owned paths and shared-file coordinator: src/config/schema.rs,
  src/config/store.rs, src/ui/widgets/**, src/ui/keymap.rs,
  src/app/palette.rs, src/app/discovery.rs; coordinator: integrator for
  src/app/model.rs, src/app/message.rs, src/app/update.rs,
  src/ui/layout.rs.
Dependencies and explicit non-goals: after T009; no persistence; no third
  density value; clamps from contracts/grid.md.
Acceptance scenarios / existing requirement and task IDs: FR-2520, FR-2521,
  SC-2507; T038-T041.
Commands and environment needed:
  cargo test --locked --lib app::update::tests::split;
  cargo test --locked --lib ui::layout; cargo test --locked --lib config;
  cargo test --locked --lib ui::keymap.
External actions already authorized: lead-only branch/commit/push/PR/auto-merge grant dated 2026-09-18; implementers perform no git actions.
Stopping condition: named tests green after being watched failing; the
  standing prohibitions apply.
```

### US8 chrome and US9 palette preview (one worker each; no integrator)

```text
Package / full feature slug / outcome IDs: W09 / 025-gui-grade-experience / O2
Problem and expected user-visible result: US8 threshold-gated capsule
  chrome; US9 owner-geometry palette previews.
Owned paths and shared-file coordinator: US8: src/ui/widgets/header.rs,
  src/ui/widgets/footer.rs, src/ui/theme.rs. US9: src/app/palette.rs,
  src/ui/widgets/palette.rs. Coordinator: none for either.
Dependencies and explicit non-goals: after Phase 4; US8 keeps the six-hint
  cap; US9 previews retained data only, nothing fetched or executed.
Acceptance scenarios / existing requirement and task IDs: FR-2522..2524;
  T042-T045.
Commands and environment needed: cargo test --locked --lib ui::widgets;
  cargo test --locked --lib app::palette; cargo test --locked --lib ui::theme.
External actions already authorized: lead-only branch/commit/push/PR/auto-merge grant dated 2026-09-18; implementers perform no git actions.
Stopping condition: named tests green after being watched failing; the
  standing prohibitions apply.
```

### US10 banner (worker + integrator for T053's model and update arms)

```text
Package / full feature slug / outcome IDs: W09 / 025-gui-grade-experience / O2, O3
Problem and expected user-visible result: an original ASCII identity mark in
  the README and a bounded boot reveal in the interactive client.
Owned paths and shared-file coordinator: src/ui/widgets/banner.rs,
  src/ui/theme.rs, README.md; coordinator: integrator for src/app/model.rs,
  src/app/update.rs.
Dependencies and explicit non-goals: after Phases 4 and 6; no new dependency
  (no tachyonfx), no replay, no bare-command banner, no colour in the README
  block, no new bindable action.
Acceptance scenarios / existing requirement and task IDs: FR-2525..FR-2528,
  SC-2509; T052-T054; contracts/banner.md.
Commands and environment needed:
  cargo test --locked --lib ui::widgets::banner;
  cargo test --locked --lib app::update::tests::banner;
  cargo test --locked --lib ui::theme.
External actions already authorized: lead-only branch/commit/push/PR/auto-merge grant dated 2026-09-18; implementers perform no git actions.
Stopping condition: named tests green after being watched failing; the
  standing prohibitions apply.
```

### Polish (integrator)

```text
Package / full feature slug / outcome IDs: W09 / 025-gui-grade-experience / O2, O3
Problem and expected user-visible result: design documents amended with
  the behaviour, parity assertions updated, full evidence recorded.
Owned paths and shared-file coordinator: docs/design/**,
  docs/support/compatibility.md, docs/product/**, docs/security/**,
  CHANGELOG.md, tests/documentation_matches_the_build.rs, docs/status.md;
  coordinator: self (integrator).
Dependencies and explicit non-goals: after every behaviour phase; no
  behaviour change in this phase.
Acceptance scenarios / existing requirement and task IDs: SEC-2508;
  T046-T051.
Commands and environment needed: the quickstart focused set;
  cargo --locked xtask db up; cargo --locked xtask verify;
  cargo xtask db down; cargo xtask db status.
External actions already authorized: lead-only branch/commit/push/PR/auto-merge grant dated 2026-09-18; implementers perform no git actions.
Stopping condition: verifier green with skips reported as skips; evidence
  recorded in docs/status.md before boxes are checked.
```
