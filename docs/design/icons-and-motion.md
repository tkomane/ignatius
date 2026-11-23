# Icons and motion

The interface is meant to be a pleasure to sit in front of for hours. That is a
product requirement, not a decoration budget. What follows is how it is allowed
to be decorative without becoming untrustworthy.

## The rule everything else follows

**An icon never carries a meaning by itself, and neither does motion.** Every
icon sits beside the word it decorates. Every animated indicator has a word next
to it saying what is happening. Turn the icons off and the screen reads
identically; freeze the animation and nothing is lost.

That is what makes it safe to use them liberally. The layout tests enforce it
directly: the same model is rendered in three themes, with and without colour,
across all three glyph tiers, and `[PROD]`, `[null]`, `Ready` and `Results` must
appear in every one of the eighteen combinations.

## Three tiers

| Tier | What it draws | How it is chosen |
| --- | --- | --- |
| `ascii` | No icons at all, `+-\|` borders | `--plain`, `--ascii`, or a terminal that cannot do better |
| `unicode` | Box drawing and widely supported symbols | The default when the environment looks UTF-8 capable |
| `nerd-font` | Icons from a patched Nerd Font | **Only when asked for.** Never inferred |

The Nerd tier is opt-in because whether the terminal's font carries the private
use icon range cannot be detected from inside the terminal, and a confident wrong
answer fills the screen with replacement characters. Turn it on with
`--glyphs nerd-font`, or permanently:

```toml
[ui]
glyphs = "nerd-font"
```

`--plain` and `--ascii` override configuration in both directions, because they
are what someone reaches for when the terminal cannot cope.

## What is drawn

**Header.** The header and footer render on full-width `SurfaceAlt` statusbars.
The product mark is styled via `Theme::pill(Token::Focus)`, the database icon
uses `\u{25c9}` (`◉`) in the `unicode` tier to avoid CJK trigram font fallback on
macOS terminals, and the target string (`user@host:port/db`) is split into
semantic spans (`user` in `Token::Info`, `@host:port/` in `Token::Muted`, `db` in
`Token::Header`). The header capsule set spans four discrete badge domains:

- **Environment**: `PROD` (danger/red capsule); non-production local environments
  remain unbadged by default.
- **Transaction**: `TX ACT` (warning/yellow capsule) when a transaction is open,
  and `TX FAIL` (danger/red capsule) when a transaction has failed and requires
  rollback; idle state is unbadged.
- **Transport**: `TLS verify-full` (info/sky capsule) when encrypted and identity
  is verified, `TLS unverified` (warning/yellow capsule) when encrypted without
  host identity verification, and `TLS NONE` (danger/red capsule) when unencrypted.
- **Posture**: `READ-ONLY` (teal/posture accent capsule) when the session is
  restricted to read-only queries; normal read-write posture is plain text.

The query elapsed-time badge is gated and displayed only when query execution
takes at least `200 ms`, preventing visual flicker on fast queries. With colour
off, all capsules degrade to reversed text with unchanged words and brackets
(`[PROD]`, `[READ-ONLY]`, `[TX ACT]`, `[TX FAIL]`, `[TLS verify-full]`), ensuring
complete readability on monochrome screens.

**Results.** Row numbers in a gutter, an elevated `SurfaceAlt` column header bar,
alternating row backgrounds, a rule under the headers, a column rule between
columns, and a scroll indicator when there are rows below the fold. The grid
features a multi-span semantic toolbar with `Token::Border` pipe delimiters and
`Token::Focus` selected column accents. When query errors occur, diagnostic labels
are structured into multi-span coloured prefixes (`While:`, `Object context:`,
`Catalogue:`, `Location:`, `Likely cause:`, `Next:`). Values that read as numbers
are right-aligned. That is a heuristic on the shape of the value, not knowledge of
its type, and it is never described as type-aware: the simple query protocol
reports no type information.

**Editor.** Line numbers and a block cursor drawn by the renderer, because the
terminal's own cursor is hidden while the alternate screen is in use. When
focused, the active row is highlighted with a full-width `SurfaceAlt`
`CursorLine`, and the active line number is emphasised as `CursorLineNr`
(`Token::Focus` bold when focused, `Token::Header` bold when unfocused). The
editor also features a Blink.cmp-style completion popup with a full-width
selection bar and kind-based semantic colouring (keywords in `Token::SyntaxKeyword`,
functions in `Token::SyntaxNumber`, tables and views in `Token::Info`, columns in
`Token::Text`, and CTEs in `Token::Focus`). Before this, the editor had no visible
caret at all.

**Focus.** Three signals at once: the pane title says `[focused]`, the border
becomes heavier, and the border colour changes. The first two survive with colour
off.

## Object symbolism

Every kind of database object has its own icon: schema, table, view, materialized
view, sequence, function, index, extension, column, and a key for a column in the
primary key. In `--glyphs nerd-font`, Ignatius uses the PostgreSQL Devicon brand
mark alongside crisp single-character Codicon and Material Design icons verified
across standard monospace Nerd Fonts (`CommitMono Nerd Font`, `JetBrainsMono Nerd Font`),
while keeping unselected Objects tree indentation guides in `Token::Border` so the
hierarchy whispers rather than competes with object ink. They exist so the tree can
be read by shape rather than by reading every word.

Two rules keep them honest:

- **The kind is always in the words too.** A group is labelled `tables (18)`, not
  just an icon and a number. An object's kind appears in the palette's detail
  line. Drop to the ASCII tier and the tree is still completely readable.
- **The chevron is the exception that proves the rule.** Open and closed nodes
  must be distinguishable in every tier, so ASCII gets `v` and `>` rather than
  nothing. A tree where open and closed look the same is not a tree.

## Motion

Motion exists to make a state change easier to see, never to make a state
harder to read. Every moving thing follows the same seven rules, and a change
that cannot meet one of them does not move:

1. **State first.** The model reaches its new state in the same reduction that
   starts a transition. The transition only chooses how that state is painted
   for a short time, so every word, focus marker and safety badge of the new
   state is in the very first frame.
2. **Bounded.** No transition lasts longer than 800 ms. Each one has a fixed
   duration in a single table in `src/ui/motion.rs`.
3. **Input wins.** Any key press, paste or click settles every transition to
   its final frame and is still delivered to its handler in the same step, as
   the boot reveal already does. A transition never delays, swallows or
   reorders input.
4. **Reduced motion is stillness.** With `ui.reduced-motion = true`, ASCII
   glyphs, colour off, 16 colours, plain mode or non-interactive output, no
   transition starts, and the frame is byte-identical to the settled one.
5. **Depth-honest colour.** Colour interpolation happens only at truecolour.
   256 colours step to the final state at the midpoint, and 16 colours and no
   colour show the final state immediately.
6. **Idle sleeps.** The client animates only while a transition or a busy
   indicator is active, so a settled client wakes nothing.
7. **Safety surfaces never move or recede.** The header and footer rails that
   carry `[PROD]`, `[READ-ONLY]`, `[TX ACT]`, `[TX FAIL]` and the TLS badges are
   never dimmed, faded or split between two themes. While a confirmation, a
   review, a prompt or the connection trust surface is open, nothing moves, so
   its paint-before-accept behaviour is unchanged.

Three moving things answer "is this still working":

- A braille spinner beside the word `Running` or `Cancellation requested`.
- A meter that sweeps while a statement runs.
- The bounded US10 boot reveal wordmark on connecting frames.

Two transitions decorate a change that has already happened:

- **The theme curtain.** Switching theme sweeps the new palette across the
  pane body from left to right in 320 ms, eased out. The edge is hard: every
  cell is wholly in the old theme or the new one, because a blend between a
  dark and a light palette passes through colours where text has no contrast.
  The header and footer rails take the new theme in the first frame, and the
  words on screen are identical in every frame. At 16 colours and below the
  new theme appears at once.
- **The result settle.** When a statement completes successfully and its
  result is complete, the Results status word starts in the success colour and
  fades to its resting colour over about half a second. A failure, a
  cancellation, a lost connection or a limited result gets no transition and
  keeps its static colours, because fading any of them toward calm would soften
  news that is not calm. The motion core starts the settle now; the Results
  title draws it from Feature 063 Wave 2.

Focus changes and overlays do not move. Focus already has its heavier border,
its colour and the `[focused]` word in the same frame, and the palette and
completion open too often for a fade to be anything but a delay.

**The meter shows time passing, not progress.** PostgreSQL reports no progress
for a running statement, so nothing in the interface may imply a percentage. It
is a heartbeat, and it is drawn as one.

**The boot reveal wordmark** provides immediate feedback while a connection is
being established. It operates under strict bounds: a `1.4 s` cap (twelve ticks
at 60 Hz), a left-to-right sweep with easing and a decode leading edge, an any-key
skip that still delivers the typed key to its handler, immediate truncation on
any connection outcome (success or failure), runs at most once per process,
renders a static final frame from tick 0 under reduced motion, falls back to a
compact monogram under 64 columns, is omitted entirely on too-small layouts
below 40x8, and is never emitted in plain mode or non-interactive machine output.

Motion stops entirely with `ui.reduced-motion = true`, and the same words and the
same elapsed time remain. A test renders a running query at two different frame
counters under reduced motion and asserts the output is byte-identical. Under
reduced motion, the boot reveal displays directly at its static final frame with
zero animation frames, and no transition starts: a test switches theme under
reduced motion and asserts that frames at two different motion times are
byte-identical.

## How motion stays testable

The reducer reads no clock. The runtime measures how long the current statement
has been running and passes it in as a message; the model holds a frame counter
that only ever chooses which frame to draw. Transitions are measured on a
separate motion clock that only a motion frame from the runtime advances, so a
test chooses any frame of any transition by choosing a motion time. An animated
state is as reproducible as any other, and a paused animation can never hide a
real state.

Both tickers sleep entirely when nothing is happening: the 90 ms busy ticker
while nothing runs or connects, and the motion ticker, at about thirty frames a
second, while no transition is active. They are separate so that a transition
never changes the spinner's speed, the boot reveal's timing or the double-click
window. An idle client wakes nothing, which is the difference between a tool
you leave open all day and one you close.
