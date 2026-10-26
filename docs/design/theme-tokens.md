# Theme tokens

Widgets ask for meaning, never for a colour. `Token::Danger`, not "red". Swapping
a theme therefore cannot change what a widget means, and a token a theme forgot
would be a compile error rather than an invisible element.

The authority is `src/ui/theme.rs`.

## The tokens

| Token | Meaning |
| --- | --- |
| `Surface`, `SurfaceAlt`, `SurfacePane`, `SurfaceOverlay` | Frame, stripe, pane and overlay backgrounds |
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

Custom definitions use the complete matrix over all four surfaces: text, muted,
header, NULL and the five syntax roles require 4.5:1; semantic roles require
3:1. A normal border must differ from Surface. A high-contrast base requires
7:1 for every non-background role, including border and selection. Surface ink
on Selection and the actual production capsule fill requires 4.5:1, or 7:1 for
a high-contrast base. These are resolved RGB checks; terminal quantization,
DIM rendering, font and physical display contrast remain separate facts.

## Manual custom configuration

Keep the definition in the existing `config.toml`:

```toml
[ui]
theme = "custom"

[theme]
extends = "dark"
syntax-keyword = "#C99CF0"
```

`ui.theme` accepts `dark`, `light`, `high-contrast` and `custom`. Automatic
background detection remains pending; `auto` is refused explicitly. An absent
definition adds no custom palette to the session cycle. An empty `[theme]`
inherits dark and still has Custom identity. `extends` defaults to dark and
accepts only the three built-ins. Partial definitions inherit every omitted
token. An inactive definition is validated too.

Keys are exact and case-sensitive. Colours are `#` followed by six ASCII hex
digits, with either hex case accepted. Supported keys are:

```text
surface surface-alt surface-pane surface-overlay text muted border focus
success warning danger info environment-production environment-non-production
transaction-active transaction-failed null-value selection header
syntax-keyword syntax-literal syntax-number syntax-comment syntax-identifier
```

Unknown keys, invalid colours or failing contrast refuse the whole definition
with exit 3 before interactive terminal acquisition. Load, show, validate,
migrate and save share validation. Reflected input is redacted, escaped to one
bounded ASCII line and never printed as terminal control sequences. Use
`ignatius config validate` before starting. The command palette's **Switch
theme** entry cycles the validated palettes without saving the session choice;
an optional `[keys] switch-theme = "alt+f12"` binding uses the same action.

## Fills

These styles use a token as a background rather than a foreground:

- **Capsules**, for the production marker. The token becomes the background and
  the surface becomes the text. Contrast is the same pair the palette tests
  already hold to a threshold, so a capsule cannot drift into illegibility.
- **Stripes**, for alternating result rows. Text on a striped row is held to the
  same contrast thresholds as text on the main surface, by its own test.
- **Selection**, with Surface ink on Selection fill at truecolor/256 depth.
  Expanded Results applies that ink after the field's Header, Focus and value
  token styles, preserving their bold/dim emphasis. Actual rendered-buffer tests
  check the composed styles; palette arithmetic alone cannot establish them.
  Selected command-palette group, label and detail spans apply Selection after
  their semantic styles too, retaining modifiers and the unselected styles.

With colour off, a capsule becomes reversed text and striping is dropped rather
than faked. A modifier applied to every second row would be noise, not help.

## No colour at all

With colour disabled, tokens resolve to modifiers only: focus and headers become
bold, selection becomes reversed, muted becomes dim. No foreground or background
is emitted at all, which a test enforces token by token.

`[ui] color = "never"` and `--color never` select this mode. An explicit
`--color` choice wins over configuration; an explicit `--color auto` uses terminal
facts. `NO_COLOR`, `TERM=dumb` and `--plain` force colour off even against an
explicit or configured `always` choice. Startup and doctor share this resolution
before selecting colour depth and the validated palette.

This is a first-class presentation rather than a degraded one: every state is still legible
because every state is also a word.

## Colour depth

Tokens are RGB and emitted as truecolor sequences, which Warp and Windows
Terminal both support. On a 16-colour terminal the emulator approximates them,
which affects appearance only, never meaning. Overriding this is Feature 007.
