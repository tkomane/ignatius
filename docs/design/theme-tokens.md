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

## The five built-in palettes

- **Dark**, tuned for a dark terminal, low-saturation, no pure black background.
- **Light**, designed for a light terminal by darkening and desaturating hues, not
  by inverting the dark palette. An inverted dark theme is why most "light modes"
  are unpleasant.
- **High contrast**, on pure black, where every token clears 7:1 against the
  background.
- **Field Industrial**, inspired by Teenage Engineering and Dieter Rams (Braun ET66):
  matte anodized slate surfaces, warm bone foreground text, signal orange focus,
  phosphor amber warnings, alpine sage success, and graphite framing.
- **Material**, inspired by Google Material Design 3 Tonal Dark: deep neutral tonal
  surfaces (`#111318`, `#191C22`, `#22262E`, `#2B303A`), cool slate-white typography
  (`#E3E8F0` / `#F1F5FB`), Material blue focus and selection (`#8AB4F8`), cyan
  informational accents (`#78D9EC`), sage success (`#81C995`), soft amber warnings
  (`#FDD663`), and coral alerts (`#F28B82`).

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

### The Field Industrial Preset (Built-in & Custom Override)

Enable the built-in Teenage Engineering / Dieter Rams palette directly via
`ui.theme = "field-industrial"`, or customize individual tokens in `[theme]`:

```toml
[ui]
theme = "field-industrial"
```

Or extend any built-in with `[theme]`:

```toml
[ui]
theme = "custom"

[theme]
extends = "field-industrial"
focus = "#FF6A00"                  # Tactile industrial safety orange
warning = "#F59E0B"                # Vintage phosphor amber indicator
success = "#10B981"                # Alpine sage green
border = "#2E3547"                 # Precision graphite hairline
header = "#F4F3EF"                 # Warm bone / cream paper text
syntax-keyword = "#82AAFF"         # Technical drafting cobalt
```

### The Material Tonal Dark Preset (Built-in & Custom Override)

Enable the built-in Google Material Design 3 Tonal Dark palette directly via
`ui.theme = "material"`, or extend and customize individual tokens in `[theme]`:

```toml
[ui]
theme = "material"
```

Or customize it with `[theme]`:

```toml
[ui]
theme = "custom"

[theme]
extends = "material"
focus = "#8AB4F8"                  # Material blue focus and selection
info = "#78D9EC"                   # Material cyan telemetry accent
success = "#81C995"                # Material sage success
warning = "#FDD663"                # Material warm amber warning
danger = "#F28B82"                 # Material coral alert
```

### The Atelier & Paper Gallery Presets (Built-in & Custom Override)

Enable curated obsidian-espresso dark (`atelier`) or archival rag-paper light (`paper`) via `ui.theme`:

```toml
[ui]
theme = "atelier" # or "paper"
```

Key surface and accent tokens:

| Preset | `surface` / `surface-pane` / `surface-alt` / `surface-overlay` | `text` / `header` / `muted` | `focus` / `selection` / `warning` / `danger` |
| --- | --- | --- | --- |
| `atelier` | `#141210` / `#1B1917` / `#24211E` / `#2E2A26` | `#F2ECE1` / `#FAF5EC` / `#ADA396` | `#E6825C` / `#D8BE9A` / `#E6BD6A` / `#EB847D` |
| `paper` | `#EAE3D5` / `#F6F1E7` / `#E4DCCD` / `#DBD2C1` | `#1E1B18` / `#14110F` / `#4B443C` | `#9A3115` / `#25486B` / `#714304` / `#961B21` |

Both presets guarantee `>= 4.5:1` contrast across every non-border foreground token on all four surface tiers (`>= 11.7:1` for primary `text`/`header` on base surfaces). At `TrueColor` and `Indexed256`, `syntax-comment` (`Modifier::DIM | Modifier::ITALIC`) and `null-value` (`Modifier::ITALIC`) render in italic so editorial notes and `[null]` cells stand apart from data strings, while `Theme::surface_badge` strips both `DIM` and `ITALIC` so UI pills stay crisp and upright.

`ui.theme` accepts `dark`, `light`, `high-contrast`, `field-industrial`, `material`, `atelier`, `paper`, `custom` and `auto`. When
`ui.theme = "auto"`, startup probes the terminal background luminance over OSC 11
within a 100 ms deadline before entering the alternate screen, selecting `light`
when WCAG relative luminance is at least 0.5 and falling back to `dark` without
hanging when unanswered. An absent
definition adds no custom palette to the session cycle. An empty `[theme]`
inherits dark and still has Custom identity. `extends` defaults to dark and
accepts any of the built-in palettes (`dark`, `light`, `high-contrast`, `field-industrial`, `material`, `atelier`, `paper`). Partial definitions inherit every omitted
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

## Painted surfaces and elevation

The interface organizes visual hierarchy into four painted elevation levels:

- **Surface**: Base outer screen frame background (`#11131d` on Dark, `#e6e9ef` on Light, `#111318` on Material, `#141210` on Atelier, `#eae3d5` on Paper).
- **SurfacePane**: Pane interior elevation for the Objects tree, SQL editor, and Results grid (`#1b1e2e` on Dark, `#f4f6f9` on Light, `#191c22` on Material, `#1b1917` on Atelier, `#f6f1e7` on Paper).
- **SurfaceAlt**: Elevated bar and stripe surface used for the full-width top header statusbar, bottom footer statusline, active editor `CursorLine`, elevated Results column header bar, recessed modal badges, and alternating zebra stripes (`#22263a` on Dark, `#e2e6f0` on Light, `#22262e` on Material, `#24211e` on Atelier, `#e4dccd` on Paper).
- **SurfaceOverlay**: Floating overlay elevation for the command palette, completion menu, definitions, plan/notice readers, confirmation prompts, and elevated rail keycaps (`#292e42` on Dark, `#dce0e8` on Light, `#2b303a` on Material, `#2e2a26` on Atelier, `#dbd2c1` on Paper).

Overlays employ Clear-then-block painting: every overlay paints a `Clear` widget
over its bounding rectangle before rendering its block surface and contents. This
prevents underlying text, borders, or cursor artifacts from covered panes from
bleeding through into the overlay.

At `Basic16` and `None` colour depths, surface background fills are left
unpainted. The terminal emulator's native background shows through, and visual
elevation is conveyed cleanly through borders, glyphs, and explicit wording.

## Fills

These styles use a token as a background rather than a foreground:

- **Capsules**, for the production marker. The token becomes the background and
  the surface becomes the text. Contrast is the same pair the palette tests
  already hold to a threshold, so a capsule cannot drift into illegibility.
- **Pills (`Theme::pill(token)`)**, **Keycaps (`Theme::keycap(token)`)**, and **Recessed Modal Pills (`Theme::recessed_pill(token)`)**: `pill` provides `SurfaceAlt`-backed foreground emphasis; `keycap` elevates header brand badges, footer key hints, and empty-state starter keys onto `SurfaceOverlay` over `SurfaceAlt` rails (`Basic16` bold foreground, `None` modifier-only without inversion); `recessed_pill` places bold, non-dimmed tokens onto `SurfaceAlt` inside `SurfaceOverlay` modals so secondary metadata badges remain crisp across both dark and light palettes.
- **Stripes**, for alternating result rows. Text on a striped row is held to the
  same contrast thresholds as text on the main surface, by its own test.
- **Selection**, with Surface ink on Selection fill at truecolor and `Indexed256`
  depth. Full-width selection bars highlight the active row across the focused
  Objects tree, command palette list, completion popup, and plan explorer tree.
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

The interface supports a 4-tier colour depth model:

- **TrueColor**: 24-bit direct RGB colour emitted through virtual terminal
  sequences. Used when `COLORTERM` reports `truecolor` or `24bit`, or when
  explicitly configured.
- **Indexed256**: The 256-colour palette, selected when `TERM` contains
  `256color` or configured explicitly. RGB colours pass through a deterministic
  quantizer using the standard 6x6x6 colour cube (indices 16-231) and the 24-step
  greyscale ramp (indices 232-255). The quantizer never emits indices 0-15,
  preserving user-configured ANSI palette colours.
- **Basic16**: The 16 basic ANSI colours, chosen as the portable fallback or by
  configuration. Token colours map through a fixed 16-colour lookup table.
- **None**: No colour sequences emitted; tokens resolve to font styling modifiers
  (bold, dim, reversed) and text words. Selected when `NO_COLOR`, `TERM=dumb`,
  `--plain`, or `ui.color = "never"` is active.

Contrast thresholds (4.5:1 for readable text, 3:1 for semantic and graphical
elements, and 7:1 for the high-contrast theme) are mathematically verified in
the test suite against WCAG relative luminance at `TrueColor`. At reduced depths
(`Indexed256` and `Basic16`), quantized approximations maintain readability,
while `None` conveys all meaning through text labels and styling modifiers.
