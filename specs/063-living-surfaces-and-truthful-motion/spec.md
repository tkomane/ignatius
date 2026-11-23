# Feature Specification: Living Surfaces and Truthful Motion

**Feature Branch**: `feat/063-living-surfaces-core` (core), then per-wave branches
**Created**: 2026-11-23
**Status**: Delivered on 2026-11-23 (PRs #131, #133, #134 and the Wave 3 showcase). Benchmark safety found during review continues in #132.
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
4. **Reduced motion is stillness.** With `ui.reduced-motion = true`, ASCII
   glyphs, colour off, 16 colours, plain mode, or non-interactive output, no
   transition starts. The rendered buffer is byte-identical to the settled
   frame.
5. **Depth-honest colour.** Colour interpolation happens only at `TrueColor`.
   `Indexed256` steps to the final state at the midpoint, and `Basic16` and
   `None` show the final state immediately.
6. **Idle sleeps.** `Model::is_animating()` is true only while a transition or
   an existing busy animation is active, so a settled client wakes nothing.
7. **Safety surfaces never move or recede.** The header and footer rails that
   carry `[PROD]`, `[READ ONLY]`, `[TX ACTIVE]` and `[TX FAILED]` are
   never dimmed, scrimmed, faded or split between two themes; they change in a
   single frame. No overlay arrives with motion, and while a reviewed-write,
   typed-confirmation, copy-review, prompt or connection trust surface is open,
   or a confirmation armed inside a dashboard (the statistics reset hazard,
   maintenance acknowledged for production, the import review step), nothing
   moves at all, so their paint-before-accept behaviour is unchanged.

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

Switching theme sweeps the new palette across the pane body from left to right
in about 320 ms, eased out, with a hard edge, instead of replacing every colour
in one paint.

**Acceptance Scenarios**:

1. Every cell is painted entirely in the old or the new theme at every depth.
   There is no blended edge: a blend between a dark and a light theme passes
   through colours near 1:1 contrast.
2. The header and footer rails switch to the new theme atomically in the first
   frame, so a safety capsule is never split across two themes. Only the pane
   body between them sweeps.
3. At `Indexed256` the same hard edge sweeps. At `Basic16` and `None` the new
   theme appears immediately.
4. The text content of every cell is identical in every frame of the curtain;
   only colours differ.
5. The theme name notice and configuration effects are unchanged.

### User Story 3 - Focus sweep (Cut)

**Cut on 2026-11-23 after design review.** Focus changes happen hundreds of
times a day and already carry four instant signals: the heavy border, the
focus colour, the `[focused]` word and the leader glyph. A sweep would leave
part of the newly focused border looking unfocused while it travels, which
makes the state harder to read, not easier. Focus therefore does not move.

### User Story 4 - Result settle (Priority: P2)

When a statement completes successfully and its result was not truncated or
limited, the Results title status word is present immediately in its final
wording, and its foreground fades from `Success` to its resting colour
(`Muted`) over about 450 to 600 ms, eased out.

**Acceptance Scenarios**:

1. At `TrueColor` the foreground interpolates through `Theme::mix`. At
   `Indexed256` it shows `Success` for the first half and the resting colour
   after. At `Basic16`, `None` and in ASCII mode nothing moves.
2. Only the foreground changes; there is never a background flash.
3. Failed, cancelled, truncated or limited, and connection-lost outcomes start
   no transition and keep their static colours. A failure is never faded
   toward calm.
4. Each new job identity restarts the settle, and a settle only ever tints the
   result of the job that started it.
5. A `\watch` loop iteration and a generated cell update start no settle: the
   first repeats unattended, and the second leaves the earlier result on
   screen.

### User Story 5 - Heartbeat meter (Priority: P2)

The running meter becomes a ping-pong comet with a short gradient tail at
`TrueColor`. It still shows time passing, never progress, and keeps the word
`Running` and the elapsed time beside it. Lower depths and ASCII keep a moving
block. Reduced motion keeps the existing static meter. The comet is driven by
the existing 90 ms busy frame counter, not by a transition.

### User Story 6 - Static background recession (Priority: P2)

While a blocking overlay is open, the workbench pane bodies behind it recede
statically by blending their foreground toward the pane surface. Nothing
moves: the recession is a still frame, present from the first frame the
overlay is drawn in. Header and footer rails never recede.

The arrival fade originally planned here was **cut on 2026-11-23 after design
review**: the palette and completion open more than a hundred times a day, so
a fade on every opening would read as a delay rather than as information.

### User Story 7 - Quieter identity line (Priority: P3)

The boot reveal tagline and the README banner replace "the PostgreSQL client
you can point at production" with a calmer identity line chosen in the README
work, kept consistent across `src/ui/widgets/banner.rs`, the banner SVGs and
the spec 025 banner contract.

## Functional Requirements

- **FR-001**: Add a pure `src/ui/motion.rs` module with easing functions, a
  `Transition` value (start, duration, kind), the duration table, and
  depth-aware foreground interpolation between two tokens exposed through
  `Theme` (`Theme::mix`) so widgets never construct `Color::Rgb`. The
  remaining transition kinds are `ThemeCurtain` and `ResultSettle`.
- **FR-002**: Add a monotonic motion clock to the model, advanced only by a
  runtime message carrying elapsed time, plus the set of active transitions.
- **FR-003**: Extend `Model::is_animating()` and the runtime ticker so motion
  frames arrive at about 33 ms while a transition is active, without changing
  the 90 ms busy cadence semantics for the spinner, meter, banner or mouse
  clock.
- **FR-004**: Settle all transitions on any key input before the key is
  reduced; never start transitions under reduced motion or plain mode.
- **FR-005**: Implement the theme curtain by rendering the outgoing and
  incoming presentations and compositing them per column behind a hard edge,
  with the header and footer rails in the incoming theme from the first frame,
  inside the pure UI layer.
- **FR-006**: Implement result settle and the heartbeat meter in the workbench
  widgets (`mod.rs`, `results.rs`, `editor.rs`, `objects.rs`, `header.rs`,
  `footer.rs`). The focus sweep is cut (User Story 3).
- **FR-007**: Implement static background recession behind blocking overlays
  (`overlays.rs`, `palette.rs` and the recession pass in `layout.rs`), with no
  motion and with the header and footer rails excluded. The completion popup
  does not cause recession: it is anchored to the caret, does not block the
  panes, and opens too often to dim the workbench each time. ASCII mode never
  recedes. The overlay arrival fade is cut (User Story 6).
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
  bounded duration. The theme curtain introduces no transitional colour at
  all: every cell is wholly in one palette.
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
