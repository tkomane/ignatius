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
The product mark is an elevated keycap, `Theme::keycap(Token::Header)`, the
database icon uses `\u{25c9}` (`◉`) in the `unicode` tier to avoid CJK trigram
font fallback on macOS terminals, and the target string (`user@host:port/db`) is
split into semantic spans (`user` in `Token::Info`, `@host:port/` in
`Token::Muted`, `db` in `Token::Header`). The header carries three kinds of
capsule, each a full-contrast `Theme::capsule` with the word inside its
brackets:

- **Environment**: `[PROD]` in `Token::EnvironmentProduction` and `[STAGING]` in
  `Token::Warning`. `[LOCAL]` and `[UNCLASSIFIED]` stay quiet muted text, and
  other classifications such as `[DEV]` are plain text in
  `Token::EnvironmentNonProduction`.
- **Posture**: `[READ ONLY]` in `Token::Info` when the session is restricted to
  read-only statements; the ordinary read-write posture is not badged.
- **Transaction**: `[TX ACTIVE]` in `Token::TransactionActive` while a
  transaction is open, and `[TX FAILED]` in `Token::TransactionFailed` when it
  has failed and needs a rollback; an idle session is not badged.

No safety capsule ever yields to a short row. When the header cannot fit
everything, things give way in a fixed order: the history or catalogue notice
and `[MUTED]`, then the completed time, then the provider badge, then the live
clock, then a failed connection's stage; then the target shortens to its
database and then with a visible ellipsis; then the product badge goes; then a
quiet environment label shortens; and a running `\watch` loop, capsule and stop
hint together, goes last, because it is a side effect in progress. The
environment, `[READ ONLY]` and transaction capsules stay at every width. A
quiet environment label (`[LOCAL]`, `[UNCLASSIFIED]`, `[DEV]` or a custom name)
is bounded to a readable prefix with an ellipsis, so a long custom name never
pushes a capsule off the row. The compact header used below 80x24 leads with
the safety capsules, then the quiet label, the database and the focused pane,
each whole or shortened, never clipped mid-word.

Transport state is not in the steady-state header. Once connected, `verified`,
`verification skipped` and `plaintext` are routine there by the spec 025
presentation contract; the full transport facts are in the connection picker,
in the write-confirmation overlays where they matter, and in Connection
details on demand.

The completed elapsed time appears only when a statement took at least
`200 ms`, preventing flicker on fast queries, and uses the Results summary's
formatter (`240 ms`, `1250 ms`), so one duration reads the same in both places.
Live clocks, in the header, the footer and the Results pane, read in tenths of
a second with the same spacing (`1.2 s`). With colour off, and at 16 colours, every capsule degrades
to reversed text with unchanged words and brackets (`[PROD]`, `[READ ONLY]`,
`[TX ACTIVE]`, `[TX FAILED]`), so a monochrome screen reads the same.

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
   carry `[PROD]`, `[READ ONLY]`, `[TX ACTIVE]` and `[TX FAILED]` are never
   dimmed, faded or split between two themes. While a confirmation, a
   review, a prompt or the connection trust surface is open, including a
   confirmation armed inside a dashboard (the statistics reset hazard,
   maintenance acknowledged for production, the import review step, and a
   held benchmark run), nothing moves, so its paint-before-accept behaviour is
   unchanged.

Three moving things answer "is this still working":

- A braille spinner beside the word `Running` or `Cancellation requested`.
- A heartbeat that travels back and forth while a statement runs.
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
  news that is not calm. A `\watch` loop and a generated cell update do not
  settle either. Only the word's ink moves, never a background, and the
  resting word is muted ink without dimming, so the last frame of the fade
  is exactly the resting frame and nothing changes when it ends. A custom
  theme whose success colour reads below 4.5:1 on the pane gets no settle,
  because a word is never painted in an ink it cannot be read in.

Focus changes and overlays do not move. Focus already has its heavier border,
its colour and the `[focused]` word in the same frame, and the palette and
completion open too often for a fade to be anything but a delay.

**The heartbeat shows time passing, not progress.** PostgreSQL reports no
progress for a running statement, so nothing in the interface may imply a
percentage, and a bar that fills from left to right and starts again reads as
one. The heartbeat is a comet instead: a heavy head on a 24-cell hairline
track, moving one cell per busy tick (90 ms) at a constant pace and turning at
each end, so it never fills, never arrives and never completes. Beside it the
word `Running` and the elapsed time say what is known.

- **Truecolour**: the head in the information colour with a three-cell tail
  fading into the track colour behind it.
- **256 colours**: the same head and tail shapes, with the tail in the track
  colour rather than a blend.
- **16 colours**: a bold head alone on the track.
- **No colour**: the heavy head alone on the light track, shape only.
- **ASCII**: an `=` head on a `-` track.

While `Cancellation requested` is shown the heartbeat keeps moving, because
the statement may still be running, and takes the warning colour. Under reduced
motion there is no heartbeat at all; the word and the elapsed time remain.

**The boot reveal wordmark** provides immediate feedback while a connection is
being established. It operates under strict bounds: a `1.4 s` cap (it takes
twelve ticks of the 90 ms busy clock, about 1.1 s), a left-to-right sweep with
easing and a decode leading edge, an any-key skip that still delivers the typed
key to its handler, immediate truncation on
any connection outcome (success or failure), runs at most once per process,
renders a static final frame from tick 0 under reduced motion, falls back to a
compact monogram under 64 columns, is omitted entirely on too-small layouts
below 40x8, and is never emitted in plain mode or non-interactive machine output.
Its final frame sets the identity line, `a quiet instrument for PostgreSQL`,
centred under the wordmark.

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
window. At most one motion frame is ever waiting: while one is still being
drawn the ticker sends nothing and keeps counting, so a slow terminal gets
fewer frames that each carry all the time that passed. A key never waits
behind a queue of frames, and a transition still ends on time. An idle client
wakes nothing, which is the difference between a tool you leave open all day
and one you close.
