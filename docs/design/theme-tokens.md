# Theme tokens

Widgets ask for meaning, never for a colour. `Token::Danger`, not "red". Swapping
a theme therefore cannot change what a widget means, and a token a theme forgot
would be a compile error rather than an invisible element.

The authority is `src/ui/theme.rs`.

## The tokens

| Token | Meaning |
| --- | --- |
| `Surface`, `SurfaceAlt` | Ordinary and raised backgrounds |
| `Text`, `Muted` | Primary text; secondary text, hints and units |
| `Border`, `Focus` | Pane edges; the focused pane |
| `Success`, `Warning`, `Danger`, `Info` | Outcome and severity |
| `EnvironmentProduction`, `EnvironmentNonProduction` | Connection classification |
| `TransactionActive`, `TransactionFailed` | Transaction state |
| `NullValue` | SQL NULL, distinct from an empty string |
| `Selection`, `Header` | Selected row; column headers |
| `SyntaxKeyword`, `SyntaxLiteral`, `SyntaxNumber`, `SyntaxComment`, `SyntaxIdentifier` | SQL colouring in the editor |

The syntax tokens hold to the same contrast thresholds as everything else, which
is why they are muted rather than the saturated colours a screenshot would
flatter. With colour off, keywords keep a bold weight and comments a dim one and
the rest is plain text: a buffer where every second word is emphasised is harder
to read, not easier. Nothing about the meaning of the SQL depends on any of it.

## The three palettes

- **Dark**, tuned for a dark terminal, low-saturation, no pure black background.
- **Light**, designed for a light terminal by darkening and desaturating hues, not
  by inverting the dark palette. An inverted dark theme is why most "light modes"
  are unpleasant.
- **High contrast**, on pure black, where every token clears 7:1 against the
  background.

## Contrast is tested, not asserted

Tests compute WCAG relative luminance from the palette values and fail the build
if a token falls below its threshold: 4.5:1 for primary text on its surface, 3:1
for every semantic token, 7:1 for every token in the high-contrast theme. The
formula is checked against the known value of black on white, 21:1.

A token that resolves to its own background colour also fails, because that is an
invisible element.

## Fills

Two styles use a token as a background rather than a foreground:

- **Capsules**, for the production marker. The token becomes the background and
  the surface becomes the text. Contrast is the same pair the palette tests
  already hold to a threshold, so a capsule cannot drift into illegibility.
- **Stripes**, for alternating result rows. Text on a striped row is held to the
  same contrast thresholds as text on the main surface, by its own test.

With colour off, a capsule becomes reversed text and striping is dropped rather
than faked. A modifier applied to every second row would be noise, not help.

## No colour at all

With colour disabled, tokens resolve to modifiers only: focus and headers become
bold, selection becomes reversed, muted becomes dim. No foreground or background
is emitted at all, which a test enforces token by token.

This is the mode `NO_COLOR`, `TERM=dumb` and `--plain` select, and it is a
first-class presentation rather than a degraded one: every state is still legible
because every state is also a word.

## Colour depth

Tokens are RGB and emitted as truecolor sequences, which Warp and Windows
Terminal both support. On a 16-colour terminal the emulator approximates them,
which affects appearance only, never meaning. Overriding this is Feature 007.
