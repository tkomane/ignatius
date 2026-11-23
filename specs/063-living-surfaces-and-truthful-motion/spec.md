# Feature Specification: Living Surfaces and Truthful Motion

**Feature Branch**: `feat/063-living-surfaces-core` (core), then per-wave branches
**Created**: 2026-11-23
**Status**: Specified
**Milestone**: M17 - Living Surfaces

## Problem Statement

Feature 062 made every still frame of Ignatius calm and editorial. Between
frames the interface is still abrupt: a theme switch replaces every colour in
one paint, focus moving between panes is visible only after reading the title,
a modal lands on top of a workbench that competes with it for attention, and a
finished query looks exactly like the frame before it finished.

`docs/design/icons-and-motion.md` currently allows "exactly three moving
things" (the spinner, the running meter and the boot reveal). That rule kept
motion honest, but it also leaves state changes that the eye would read faster
if they arrived with a short, bounded transition.

This feature adds a small motion grammar that makes state changes easier to
see without ever making a state harder to read, and records it as the new
rule.

## Principles this feature must keep

- **Principle I (Delight must remain truthful).** A transition decorates a
  state change that has already happened in the model. The words, focus
  markers and safety badges of the new state are present in the very first
  frame. No transition implies progress, a count, or an outcome that is not
  known.
- **Principle VI (Accessible meaning survives styling loss).** Freeze any
  transition at any frame and the screen still reads correctly. ASCII,
  `NO_COLOR`, `Basic16` and plain modes show the settled state immediately.
- **Principle XI (Editorial craft and tactile restraint).** Motion is short,
  eased and quiet. Every colour still flows through `Theme` and `Token`.
- **Architecture constraint.** The reducer reads no clock. Elapsed time
  arrives in a message from the runtime, so every animated frame is
  reproducible in a test.

## Motion grammar

Every animation in this feature follows the same seven rules. A change that
cannot meet one of them does not animate.

1. **State first.** The model reaches its new state in the same reduction that
   starts the transition. The transition only selects how the new state is
   painted for a short time.
2. **Bounded.** No transition lasts longer than 800 ms. Each one has a fixed
   duration in a single table in `src/ui/motion.rs`.
3. **Input wins.** Any key press settles every active transition to its final
   frame and is still delivered to its handler, as the boot reveal already
   does. A transition never delays, swallows or reorders input.
4. **Reduced motion is stillness.** With `ui.reduced-motion = true`, plain
   mode, or non-interactive output, no transition starts. The rendered buffer
   is byte-identical to the settled frame.
5. **Depth-honest colour.** Colour interpolation happens only at `TrueColor`.
   `Indexed256` steps to the final state at the midpoint, and `Basic16` and
   `None` show the final state immediately.
6. **Idle sleeps.** `Model::is_animating()` is true only while a transition or
   an existing busy animation is active, so a settled client wakes nothing.
7. **Safety surfaces never move or recede.** The header and footer rails that
   carry `[PROD]`, `[READ-ONLY]`, `[TX ACT]`, `[TX FAIL]` and TLS badges are
   never dimmed, scrimmed or faded. Reviewed-write, typed-confirmation and
   other confirmation overlays receive no arrival animation, so their
   paint-before-accept behaviour is unchanged.

## User Scenarios & Testing

### User Story 1 - Motion core (Priority: P1)

The runtime can drive short transitions at an animation cadence without a clock
in the reducer, and the client returns to sleep when they settle.

**Acceptance Scenarios**:

1. A `Transition` started at motion time `t0` with duration `d` reports eased
   progress `0.0` at `t0`, `1.0` at `t0 + d` and after, and is reported settled
   once complete.
2. While any transition is active, the runtime delivers motion frames at about
   30 frames per second; once all settle, `is_animating()` returns to its
   previous meaning and the ticker sleeps or returns to the existing busy
   cadence.
3. Spinner, running meter, boot reveal and mouse double-click timing behave
   exactly as before the change.
4. Under reduced motion no transition is ever created, and a test renders the
   same model at two different motion times and asserts byte-identical
   buffers.
5. A key press during a transition settles it and the key's action is
   applied in the same reduction.

### User Story 2 - Theme curtain (Priority: P1)

Switching theme sweeps the new palette across the workbench from left to right
in about 320 ms with an eased, soft leading edge, instead of replacing every
colour in one paint.

**Acceptance Scenarios**:

1. Every cell is painted entirely in the old or the new theme, except a
   leading edge of at most three columns that blends between them at
   `TrueColor`.
2. At `Indexed256` the edge is hard (no blend). At `Basic16` and `None` the new
   theme appears immediately.
3. The text content of every cell is identical in every frame of the curtain;
   only colours differ.
4. The theme name notice and configuration effects are unchanged.

### User Story 3 - Focus sweep (Priority: P2)

When focus moves to a pane, the `[focused]` word and heavy border appear
immediately, and the focus accent colour sweeps along the new pane's border
from the title outward in about 220 ms.

### User Story 4 - Result settle (Priority: P2)

When a statement completes, fails or is cancelled, the Results title status
words are present immediately and their capsule carries a brief tint
(`Success`, `Danger` or `Warning`) that fades to rest over about 700 ms.

### User Story 5 - Heartbeat meter (Priority: P2)

The running meter becomes a ping-pong comet with a short gradient tail at
`TrueColor`. It still shows time passing, never progress, and keeps the word
`Running` and the elapsed time beside it. Lower depths and ASCII keep a moving
block. Reduced motion keeps the existing static meter.

### User Story 6 - Overlay arrival and depth (Priority: P2)

Non-confirmation overlays (command palette, help, completion, inspectors and
dashboards) arrive with a short surface and border fade (about 160 ms) while
the workbench pane bodies behind them recede by blending their foreground
toward the pane surface. Header and footer rails never recede.

### User Story 7 - Quieter identity line (Priority: P3)

The boot reveal tagline and the README banner replace "the PostgreSQL client
you can point at production" with a calmer identity line chosen in the README
work, kept consistent across `src/ui/widgets/banner.rs`, the banner SVGs and
the spec 025 banner contract.

## Functional Requirements

- **FR-001**: Add a pure `src/ui/motion.rs` module with easing functions, a
  `Transition` value (start, duration, kind), the duration table, and
  depth-aware colour interpolation helpers exposed through `Theme`
  (`Theme::mix` or equivalent) so widgets never construct `Color::Rgb`.
- **FR-002**: Add a monotonic motion clock to the model, advanced only by a
  runtime message carrying elapsed time, plus the set of active transitions.
- **FR-003**: Extend `Model::is_animating()` and the runtime ticker so motion
  frames arrive at about 33 ms while a transition is active, without changing
  the 90 ms busy cadence semantics for the spinner, meter, banner or mouse
  clock.
- **FR-004**: Settle all transitions on any key input before the key is
  reduced; never start transitions under reduced motion or plain mode.
- **FR-005**: Implement the theme curtain by rendering the outgoing and
  incoming presentations and compositing them per column, inside the pure UI
  layer.
- **FR-006**: Implement focus sweep, result settle and heartbeat meter in the
  workbench widgets (`mod.rs`, `results.rs`, `editor.rs`, `objects.rs`,
  `header.rs`, `footer.rs`).
- **FR-007**: Implement overlay arrival and background recession in
  `overlays.rs`, `palette.rs` and the completion popup, excluding every
  confirmation and reviewed-write overlay.
- **FR-008**: Replace the boot reveal tagline and update the spec 025 banner
  contract and banner tests in the same change.
- **FR-009**: Amend `docs/design/icons-and-motion.md` to replace "exactly three
  moving things" with the motion grammar above, and update
  `tests/documentation_matches_the_build.rs` in the same change.
- **FR-010**: Extend the showcase pipeline so animated captures can be
  rendered deterministically from real widget buffers at fixed motion times.

## Non-Functional Requirements

- **NFR-001 (Determinism)**: Freshly constructed models render settled. Every
  animated frame used in a test or showcase is selected by an explicit motion
  time, never by wall-clock time.
- **NFR-002 (Performance)**: The curtain doubles render work only while it
  runs. No transition allocates per cell per frame beyond the buffers it
  composites.
- **NFR-003 (Contrast)**: Settled frames keep every existing contrast test.
  Transitional colours exist only on cells that are changing and only for the
  bounded duration.
- **NFR-004 (Tests and hygiene)**: Unit tests live in sidecar `tests.rs`
  files. No em or en dashes. `AGENTS.md` and `CLAUDE.md` stay identical.
- **NFR-005 (Toolchain)**: Compiles and passes Clippy with warnings denied on
  MSRV 1.90 and CI Rust 1.97.1.

## Out of Scope

- Blinking or breathing cursors, idle ambient animation, or any motion while
  nothing is changing.
- Count-up numbers, progress percentages, or animated row arrival, because
  each would show a value that is not yet true.
- Any change to safety, credential, transport or confirmation semantics.
