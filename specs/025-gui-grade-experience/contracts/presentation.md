# Contract: Presentation - surfaces, depth and density

Pinned values for painted surfaces, colour depth and density. An
implementer who needs a value that is not here stops and reports; nothing
in this contract is a suggestion. Final hex values live in
`src/ui/theme.rs` under its contrast tests, which remain the authority: a
starting value below that fails a contrast threshold is adjusted there and
the adjustment is recorded in the pull request, not silently.

## Elevation ladder

Exactly three levels, expressed as three surface tokens. `SurfaceAlt`
remains the stripe fill and is not an elevation level.

| Level | Token | Painted where |
| --- | --- | --- |
| Base | `Surface` | the whole frame, header and footer bands |
| Pane | `SurfacePane` (new) | every pane interior |
| Overlay | `SurfaceOverlay` (new) | every overlay, prompt, completion menu |

Starting values (owner-palette-informed):

| Token | Dark | Light | High contrast |
| --- | --- | --- | --- |
| `Surface` | `#181825` | `#E6E9EF` | `#000000` |
| `SurfacePane` | `#1E1E2E` | `#EFF1F5` | `#000000` |
| `SurfaceOverlay` | `#313244` | `#DCE0E8` | `#101010` |

- High contrast keeps its near-black ladder; overlay separation there is
  carried by borders and words, and every token must clear 7:1.
- The light palette stays designed, not inverted: overlays are slightly
  darker than panes, never a grey wash.
- Row stripes render as `SurfaceAlt` over `SurfacePane`; with colour off,
  stripes are dropped exactly as today.

## Accent hue families

Hex is chosen in `theme.rs` under the contrast tests; the hue family per
semantic role is fixed here:

| Role | Family (dark reference) |
| --- | --- |
| Danger, production environment, failed transaction | red `#F38BA8` |
| Warning, active transaction | yellow `#F9E2AF` |
| Success | green `#A6E3A1` |
| Info, transport | sky `#89DCEB` |
| Focus, selection accent | mauve `#CBA6F7` |
| Read-only posture | teal `#94E2D5` |
| Muted | grey-blue `#A6ADC8` |
| Text | `#CDD6F4` |

## Colour depth

`ColorDepth { TrueColor, Indexed256, Basic16, None }`, held by
`Presentation`, detected into `TerminalFacts`.

Detection precedence, first match wins:

1. `NO_COLOR` set, `TERM=dumb`, `--plain`, or colour resolved off:
   `None`.
2. `--color-depth` flag, then `ui.color-depth` key: `truecolor`, `256`,
   `16` (value `auto` falls through to detection).
3. `COLORTERM` equal to `truecolor` or `24bit` (case sensitive):
   `TrueColor`.
4. `TERM` value containing `256color`: `Indexed256`.
5. Otherwise: `Basic16`.

Emission rules:

- `TrueColor`: emit `Color::Rgb` exactly as today.
- `Indexed256`: emit `Color::Indexed`, chosen by the quantizer below.
  Surfaces are painted.
- `Basic16`: emit `Color::Indexed(0..=15)` for foregrounds by the table
  below. Surfaces are NOT painted; elevation follows the colour-off rules.
- `None`: no colour of any kind; the existing modifier-only rules apply.

Contrast tests run on the truecolor palette only. Reduced depths are
documented approximations and never carry meaning alone.

## Quantizer

A pure function `fn quantize_256(rgb: Rgb) -> u8` in `src/ui/theme.rs`:

- Candidates are the 6x6x6 colour cube, indices 16-231 with component
  levels `[0, 95, 135, 175, 215, 255]`, and the grey ramp, indices
  232-255 with values `8, 18, 28, .. 238`.
- The result is the candidate with the smallest squared RGB distance;
  ties resolve to the lower index. Indices 0-15 are never produced (they
  are terminal-redefinable and unpredictable).

`fn basic_16(token, theme) -> u8` is a fixed lookup, not a computation:

| Family | Normal | Bright |
| --- | --- | --- |
| red | 1 | 9 |
| green | 2 | 10 |
| yellow | 3 | 11 |
| blue/mauve | 4 | 12 |
| magenta/pink | 5 | 13 |
| cyan/sky/teal | 6 | 14 |
| text/grey | 7 | 15 |
| muted | 8 | - |

Bright is used when the token's truecolor relative luminance is 0.5 or
greater. The complete 24-token-per-theme table is written in the
implementation beside its test; families come from the accent table above.

## Density

`ui.density = "comfortable"` (default) or `"compact"`. One spacing table,
consumed by widgets as data:

| Knob | Comfortable | Compact |
| --- | --- | --- |
| Pane title padding (cells each side) | 1 | 0 |
| Grid cell padding (cells each side) | 1 | 1 |
| Blank line under the grid toolbar | 1 | 0 |
| Footer hint cap | 6 | 4 |
| Overlay outer margin (cells) | 2 | 1 |

No other spacing varies by density. Unknown values are refused by the
existing configuration rule (report before the terminal is taken, exit 3).

### Density implementation clarification - 2026-10-25

- The 6/4 hint bound supersedes Feature 015's original five-action bound.
  Prioritisation, availability, active keys and complete-pair width fitting
  remain unchanged. Apply the cap after filtering, using the presentation
  actually supplied to the renderer. Live paging/hit testing uses the model's
  presentation snapshot; explicit rendering overrides control their own
  spacing.
- Title padding applies to the bordered pane and overlay title helpers,
  including Plan, error, failed-transaction and empty titles. It does not alter
  capsule padding, tree indentation or the safety caption's existing corner
  inset. Remove only caller-owned decorative edge padding; preserve whitespace
  belonging to an object name or other title content, without blanket trimming.
- Compact removes only an empty toolbar breathing row. A cell-update,
  clipboard, refresh or grid-state notice retains one visible row in either
  density. Grid prelude height is `4 + type_row + max(notice_row, blank_rows)`;
  rendering, hit mapping, viewport wording and paging share that calculation.
- Overlay margin is preferred clearance outside the outer border on each
  axis, within existing family maximum dimensions and anchoring. Reduce that
  clearance toward zero only when necessary to preserve a family's existing
  usable interaction rows; never hide a previously visible password/input row
  merely to satisfy the preferred margin. At 40x8, a short password prompt
  still shows its masked field and accept/cancel guidance in both densities.
  Completion keeps its caret anchor and existing five-row minimum when space
  permits. Existing too-small behavior remains; report any newly exposed
  confirmation-disclosure gap rather than silently changing authorization.
- Plan occupies Results and receives no overlay margin. All actual overlay
  families use shared boxes for drawing and hit/viewport calculations. Column
  geometry retains its existing one-cell padding without adding another pair.
- Only the optional `ui.density` setting and its template default are added.
  Existing constructors remain comfortable by default; configuration selects
  density at startup. There is no new toggle action, CLI flag, doctor field,
  theme value, third density or mouse-capture change in T040.

## Doctor reporting

`ignatius doctor` reports, in this order, beside the existing detections:
the colour depth in use, its source (`flag`, `configuration`, `COLORTERM`,
`TERM`, `default`, or `forced off`), and, when `ui.theme = "auto"` was
configured, whether the terminal answered the background query
(`answered rgb(...)` or `no answer within 100 ms; using dark`).
