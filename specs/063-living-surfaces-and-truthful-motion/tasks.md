# Tasks: Living Surfaces and Truthful Motion

**Input**: Design documents from `/specs/063-living-surfaces-and-truthful-motion/`
**Prerequisites**: `spec.md`, `plan.md`

## Phase 1: Specification

- [x] T001 Author Feature 063 `spec.md`, `plan.md` and `tasks.md`.

## Phase 2: Wave 1 - Motion core and theme curtain

- [ ] T002 Add `src/ui/motion.rs` with easing, `Transition`, the duration table and curtain compositing helpers, with sidecar tests.
- [ ] T003 Add depth-aware colour mixing to `Theme` (`TrueColor` interpolates, `Indexed256` steps at the midpoint, `Basic16` and `None` settle immediately), with tests.
- [ ] T004 Add the motion clock, active transitions and motion frame message to the model and reducer; settle transitions on key input before reducing the key; never start transitions under reduced motion or plain mode.
- [ ] T005 Drive motion frames at about 33 ms from the runtime only while a transition is active, keeping spinner, meter, banner and mouse timing unchanged, with tests.
- [ ] T006 Implement the theme curtain composite in `src/ui/layout.rs`, with tests for cell text invariance, edge width, depth behaviour and settled determinism.
- [ ] T007 Amend `docs/design/icons-and-motion.md` with the motion grammar and update `tests/documentation_matches_the_build.rs`.

## Phase 3: Wave 2 - Consumers

- [ ] T008 Focus sweep along the newly focused pane border.
- [ ] T009 Result settle tint on the Results title status capsule.
- [ ] T010 Heartbeat comet meter at `TrueColor` with lower-depth and reduced-motion fallbacks.
- [ ] T011 Replace the boot reveal identity line and update the spec 025 banner contract and tests.
- [ ] T012 Overlay arrival fade and workbench recession for non-confirmation overlays, with header and footer rails excluded.

## Phase 4: Wave 3 - Showcase, README and reconciliation

- [ ] T013 Deterministic animated captures at fixed motion times in the showcase pipeline.
- [ ] T014 Refresh every still screenshot and the banner SVGs.
- [ ] T015 Rewrite `README.md` and reconcile `CHANGELOG.md`, `docs/status.md` and `docs/product/roadmap.md`.
- [ ] T016 Full verification, hosted CI green, merge, and clean branches and worktrees.
