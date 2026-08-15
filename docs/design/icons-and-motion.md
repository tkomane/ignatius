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

**Header.** The product mark, the environment classification, read/write posture,
the target, the transport state and the last elapsed time. Production is a filled
capsule; everything else is plain text. The bracketed word stays in every mode,
so `[PROD]` is legible with no colour and no icons at all.

**Transport.** Three states, three icons, because they mean three different
things: encrypted and verified, encrypted without an identity check, and not
encrypted. Nothing draws the second as if it were the first.

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

## Motion

There are exactly two moving things, and both exist to answer "is this still
working":

- A braille spinner beside the word `Running` or `Cancellation requested`.
- A meter that sweeps while a statement runs.

**The meter shows time passing, not progress.** PostgreSQL reports no progress
for a running statement, so nothing in the interface may imply a percentage. It
is a heartbeat, and it is drawn as one.

Motion stops entirely with `ui.reduced-motion = true`, and the same words and the
same elapsed time remain. A test renders a running query at two different frame
counters under reduced motion and asserts the output is byte-identical.

## How motion stays testable

The reducer reads no clock. The runtime measures how long the current statement
has been running and passes it in as a message; the model holds a frame counter
that only ever chooses which frame to draw. So an animated state is as
reproducible as any other, and a paused animation can never hide a real state.

The ticker sleeps entirely when nothing is happening. An idle client wakes
nothing, which is the difference between a tool you leave open all day and one
you close.
