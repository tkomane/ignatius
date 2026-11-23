# Tasks: Living Surfaces and Truthful Motion

**Input**: Design documents from `/specs/063-living-surfaces-and-truthful-motion/`
**Prerequisites**: `spec.md`, `plan.md`

## Phase 1: Specification

- [x] T001 Author Feature 063 `spec.md`, `plan.md` and `tasks.md`.

## Phase 2: Wave 1 - Motion core and theme curtain

- [x] T002 Add `src/ui/motion.rs` with easing, `Transition`, the duration table and the hard-edge curtain helper, with sidecar tests.
- [x] T003 Add depth-aware foreground mixing between two tokens to `Theme` (`TrueColor` interpolates, `Indexed256` steps at the midpoint, `Basic16` shows the final colour and `None` emits none), with tests.
- [x] T004 Add the motion clock, active transitions (`ThemeCurtain`, `ResultSettle`) and motion frame message to the model and reducer; settle transitions on key input before reducing the key; never start transitions under reduced motion, ASCII, colour off, 16 colours or outside the interactive runtime; hold still while a confirmation, review or prompt is open.
- [x] T005 Drive motion frames at about 33 ms from the runtime only while a transition is active, keeping spinner, meter, banner and mouse timing unchanged, with tests.
- [x] T006 Implement the theme curtain composite in `src/ui/layout.rs` behind a hard edge with the rails in the new theme from the first frame, with tests for cell text invariance, whole-cell paint, rails, depth behaviour and settled determinism.
- [x] T007 Amend `docs/design/icons-and-motion.md` with the motion grammar and update `tests/documentation_matches_the_build.rs`.

## Phase 3: Wave 2 - Consumers

- [x] T008 Cut on 2026-11-23: no focus sweep (spec User Story 3). No work remains.
- [x] T009 Result settle: fade the Results title status word foreground from `Success` to `Muted` for a complete, successful result, reading `model.motion().result_settle()`.
- [x] T010 Heartbeat comet meter at `TrueColor` with lower-depth and reduced-motion fallbacks.
- [x] T011 Replace the boot reveal identity line and update the spec 025 banner contract and tests.
- [x] T012 Static workbench recession behind blocking overlays, with no motion and with header and footer rails excluded. The arrival fade is cut (spec User Story 6).

## Phase 4: Wave 3 - Showcase, README and reconciliation

- [x] T013 Deterministic animated captures at fixed motion times in the showcase pipeline.
- [x] T014 Refresh every still screenshot and the banner SVGs.
- [x] T015 Rewrite `README.md` and reconcile `CHANGELOG.md`, `docs/status.md` and `docs/product/roadmap.md`.
- [ ] T016 Full verification, hosted CI green, merge, and clean branches and worktrees.
