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

## Editor horizontal view - 2026-10-25

FR-2529 / T058 corrects the observed long-line clipping independently of mouse
dispatch. The existing line-number, space and statement/error-marker gutter
stays fixed. The remaining inner width is text capacity. Keep the existing
derived vertical window; derive one horizontal cell offset from the current
caret line and use it for every visible line. Do not add stored viewport state,
soft wrapping or changes to SQL, scalar cursor movement, revision or undo.

- Apply the existing display sanitiser first. Map rendered graphemes and each
  complete escaped-control representation back to their original source-byte
  ranges and one-based scalar columns. Use the locked renderer's grapheme
  semantics without adding a segmentation dependency. A scalar cursor inside
  a combining/ZWJ cluster highlights that whole displayed cluster; this does
  not introduce grapheme-based editing or change the title's scalar column.
- When text capacity is positive, the focused caret has at least one visible
  Selection cell. Fit its whole displayed unit when possible; an end-of-line
  or zero-width-only caret uses a selected blank. If a caret unit is wider
  than the entire text capacity, use a selected blank in that capacity instead
  of drawing a partial wide glyph or partial control escape. With no text
  capacity there is no visibility claim and no completion popup.
- Clip non-caret units by display cells. A wide grapheme or escaped-control
  unit intersecting an edge is replaced by the intersecting number of blank
  cells, preserving following cell positions. Never emit a raw control or
  split a UTF-8 scalar. Keep syntax/error styles, statement/error gutter
  markers and theme/depth/glyph degradation; the caret retains Selection
  precedence. An unfocused editor keeps the same derived view without a caret.
- Completion anchors to this actual visible caret coordinate. Editor hit
  mapping uses the same offset and units: any cell of a visible wide/escaped
  unit maps to its first source scalar; trailing blank cells map to the line's
  end position. The fixed gutter and existing vertical mapping retain their
  meanings. This mapping alone does not enable mouse capture or dispatch.
- Home/buffer-start, cursor movement and width changes recompute the view.
  Reaching a prefix that fits returns its offset to zero. Do not clamp a long
  line's source offset to a `u16` scroll limit or confuse display cells with
  source scalars. Painting/geometry are pure and perform no I/O or effects.

Acceptance retains the failing actual-Buffer baseline, then covers long ASCII,
wide and combining/ZWJ text, escaped controls, end-of-line/interior carets,
very long lines beyond 65,535 cells, tiny capacities, Home/resize, completion
and hit alignment, both densities and colour/glyph degradation. Inspect actual
rendered cells and repeat the owned 100x30 PTY journey. These proofs establish
rendering and captured input, not native emulator/font appearance or whole W09.

### Bounded display-unit projection - 2026-10-25

FR-2530 / T059 follows a source-bound W10 diagnostic: the initial T058 view
constructs a complete per-unit vector for caret discovery and another for
rendering, although only a small window is visible; this does not assert
simultaneous lifetimes. The 16 KiB/1 MiB three-sample local test-profile comparison
found a material increase; it is not release-build p95 acceptance.

Production caret discovery, rendering and hit mapping must project only the
units needed by their requested cell window plus the caret. Retained unit
metadata is bounded by positive visible-cell capacity plus one caret unit;
zero-width units need not be retained unless they own that caret. Computing
whole-line width/source columns may still scan the source line linearly. Keep
ordinary source clusters borrowed, avoid rescanning whole prefixes per unit,
and reuse the existing display sanitiser. A whole-line sanitisation fast path
may avoid per-scalar allocation for unchanged safe text; do not duplicate the
sanitiser's control/bidi rules or bypass sanitisation on a guessed character set.
After the full sanitiser confirms unchanged text, an ASCII run may use one
width-one unit per byte; control validation still happens before that path.

Do not change T058 geometry, grapheme/escape boundaries, selected blank and
focus rules, byte/scalar provenance, syntax/error styles, source text, undo or
effects. Preserve every original case and compare the retained styled buffers
exactly. Add a failing allocation-shape regression for a very long off-screen
line, then repeat the same hashed diagnostic harness/profile/workloads with
fresh processes and measured renderer time/RSS. Report observed values and
small-sample limits; no fabricated absolute ceiling, 100-action p95 result or
native terminal claim follows. Root owns independent review/full verification.

## Quiet chrome (US8 / T042-T043) - 2026-10-26

- **Header layout (one row)**:
  - Left side carries identity and posture: product name (`ignatius` or `ignatius:` when a connection target follows), sanitised connection target (`user@host:port/db`), and the safety posture badge capsule (`[PRODUCTION]`, `[STAGING]`, `[LOCAL]`, `[UNCLASSIFIED]`, `[READ ONLY]`) beside any active provider badge.
  - Right side carries active session context: active transaction capsule (`[TX ACTIVE]` or `[TX FAILED]`), running spinner + live elapsed time while a query is executing, or the completed statement elapsed time when `last_elapsed >= 200 ms`, plus `[MUTED]` when motion is off.
  - When the header row cannot fit both groups cleanly, the left-hand identity and safety posture take priority and the right-hand context group truncates or yields space first so `ignatius`, the target, and the safety words remain readable at 80x24 and 40x8.
- **Routine suppression**:
  - `tx: idle` is routine and MUST NOT appear in the header.
  - `tls: verified`, `tls: verification skipped`, and `tls: plaintext` are routine in the steady-state header and MUST NOT appear there (non-verified TLS remains visible in the connection picker and write-confirmation overlays, and `Session Summary` continues to report full transport facts on demand).
  - Completed statement elapsed time (`last_elapsed`) is rendered in the header ONLY when `last_elapsed >= 200 ms` (`Duration::from_millis(200)`). Live `running_for` elapsed time while a query is executing remains visible at all durations so the connecting and running wait indicators stay truthful.
- **Filled badge capsules**:
  - At `ColorDepth::TrueColor` and `ColorDepth::Indexed256` with colour enabled, non-routine posture and transaction capsules (`[PRODUCTION]`, `[STAGING]`, `[READ ONLY]`, `[TX ACTIVE]`, `[TX FAILED]`) render as filled capsules (semantic token colour as background, `Surface` as foreground text, `BOLD`) whenever the token's foreground/background pair clears the active theme's contrast threshold (`4.5:1` on `Dark`/`Light`, `7.0:1` on `HighContrast`). If a custom theme's token against `Surface` does not clear that threshold, fall back to the existing foreground-on-`Surface` `BOLD` badge style so custom-theme contrast admission is never violated.
  - Routine environment badges (`[LOCAL]`, `[UNCLASSIFIED]`) remain quiet muted badges on `Surface` so they do not compete with `[PRODUCTION]`, `[READ ONLY]`, or `[TX FAILED]` while still preserving their words in layout and safety tests.
  - At `ColorDepth::Basic16` and `ColorDepth::None` (or with colour off), badges use the existing foreground/modifier rules with no painted background fill, and keep their brackets (`[PRODUCTION]`, `[READ ONLY]`, `[TX ACTIVE]`, `[TX FAILED]`) so words and brackets carry the meaning without colour.
- **Footer layout (one row)**:
  - Left side carries the current status / notice / diagnostic message; when idle, shows `Ready` in muted style.
  - Right side carries context-filtered key hints from the single keymap authority, capped at **6** hints in `Density::Comfortable` and **4** hints in `Density::Compact` (`Presentation::footer_hint_cap()`), dropping lower-priority hints from the right when narrow so no key/label pair is ever half-rendered.

## Doctor reporting

`ignatius doctor` reports, in this order, beside the existing detections:
the colour depth in use, its source (`flag`, `configuration`, `COLORTERM`,
`TERM`, `default`, or `forced off`), and, when `ui.theme = "auto"` was
configured, whether the terminal answered the background query
(`answered rgb(...)` or `no answer within 100 ms; using dark`).
