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

**Header.** The product mark, target, and the header capsule set across four
discrete badge domains:

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

**Results.** Row numbers in a gutter, alternating row backgrounds, a rule under
the headers, a column rule between columns, and a scroll indicator when there are
rows below the fold. Values that read as numbers are right-aligned. That is a
heuristic on the shape of the value, not knowledge of its type, and it is never
described as type-aware: the simple query protocol reports no type information.

**Editor.** Line numbers and a block cursor drawn by the renderer, because the
terminal's own cursor is hidden while the alternate screen is in use. Before
this, the editor had no visible caret at all.

**Focus.** Three signals at once: the pane title says `[focused]`, the border
becomes heavier, and the border colour changes. The first two survive with colour
off.

## Object symbolism

Every kind of database object has its own icon: schema, table, view, materialized
view, sequence, function, index, extension, column, and a key for a column in the
primary key. They exist so the tree can be read by shape rather than by reading
every word.

Two rules keep them honest:

- **The kind is always in the words too.** A group is labelled `tables (18)`, not
  just an icon and a number. An object's kind appears in the palette's detail
  line. Drop to the ASCII tier and the tree is still completely readable.
- **The chevron is the exception that proves the rule.** Open and closed nodes
  must be distinguishable in every tier, so ASCII gets `v` and `>` rather than
  nothing. A tree where open and closed look the same is not a tree.

## Motion

There are exactly three moving things, and all exist to answer "is this still
working":

- A braille spinner beside the word `Running` or `Cancellation requested`.
- A meter that sweeps while a statement runs.
- The bounded US10 boot reveal wordmark on connecting frames.

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
zero animation frames.

## How motion stays testable

The reducer reads no clock. The runtime measures how long the current statement
has been running and passes it in as a message; the model holds a frame counter
that only ever chooses which frame to draw. So an animated state is as
reproducible as any other, and a paused animation can never hide a real state.

The ticker sleeps entirely when nothing is happening. An idle client wakes
nothing, which is the difference between a tool you leave open all day and one
you close.
