# Implementation Plan: Living Surfaces and Truthful Motion

**Spec**: [spec.md](spec.md)

## Approach

The feature lands in three waves so that no two writers touch the same files
at the same time, and so that the shared motion core is reviewed and merged
before anything consumes it.

### Wave 1 - Motion core and theme curtain (one owner)

Branch `feat/063-living-surfaces-core`.

- `src/ui/motion.rs` (new) and `src/ui/motion/tests.rs`: easing, `Transition`,
  duration table, the hard-edge curtain helper.
- `src/ui/theme.rs`: depth-aware foreground mixing between two tokens through
  `Theme::mix`.
- `src/app/model.rs`, `src/app/update.rs`, `src/app/message.rs`: motion clock,
  active transitions, motion frame message, settle-on-input, transition start
  on `SwitchTheme` and on a complete, successful statement.
- `src/cli/interactive.rs`: a second ticker at about 33 ms while transitions
  are active, leaving the 90 ms busy ticker unchanged.
- `src/ui/layout.rs`: curtain composite of the outgoing and incoming
  presentations behind a hard edge, with the header and footer rails in the
  incoming theme from the first frame.
- `docs/design/icons-and-motion.md` and
  `tests/documentation_matches_the_build.rs`: the motion grammar replaces
  "exactly three moving things".

### Wave 2 - Consumers (two owners, disjoint files)

Branched from `main` after Wave 1 merges.

- **Workbench owner**: `src/ui/widgets/mod.rs`, `results.rs`, `editor.rs`,
  `objects.rs`, `header.rs`, `footer.rs`, `banner.rs` and their tests, plus
  `specs/025-gui-grade-experience/contracts/banner.md`. Result settle drawing,
  heartbeat meter, identity line. The focus sweep is cut.
- **Overlay owner**: `src/ui/widgets/overlays.rs`, `palette.rs`,
  `src/ui/notices.rs` and their tests. Static background recession behind
  blocking overlays, with no motion. The arrival fade is cut.

Each owner may add the model fields and reducer hooks they need in
`src/app/model.rs` and `src/app/update.rs`, keeping those edits small and
additive so the two branches rebase cleanly.

### Wave 3 - Showcase, README and reconciliation (one owner)

Branched from `main` after Wave 2 merges.

- `src/ui/acceptance/tests.rs` and `scripts/render-showcase.py`: animated
  captures at fixed motion times and refreshed stills.
- `docs/assets/`: refreshed screenshots, banner SVGs with the new identity
  line, animated captures.
- `README.md`, `CHANGELOG.md`, `docs/status.md`, `docs/product/roadmap.md`.

## Shared files

`docs/status.md`, `CHANGELOG.md`, `README.md` and the screenshots are written
only in Wave 3. Wave 1 and Wave 2 pull requests record their status and
changelog text in the pull request body for the Wave 3 owner to reconcile.

## Complexity Tracking

| Addition | Why it is needed | Simpler alternative rejected because |
| --- | --- | --- |
| Separate motion clock and cadence | Smooth transitions need about 30 frames per second, while the busy ticker is 90 ms and drives spinner, banner and mouse timing | Raising the global tick would change banner timing, spinner speed and double-click windows |
| Rendering two presentations during the curtain | A per-column composite behind a hard edge keeps every cell fully readable in one theme or the other | A full-screen crossfade, or even a blended edge, passes through colours near 1:1 contrast between a dark and a light theme |
