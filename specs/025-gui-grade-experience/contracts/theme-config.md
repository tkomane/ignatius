# Contract: Theme configuration, validation and auto detection

Pinned grammar, wording and protocol facts. An implementer who needs a
value that is not here stops and reports.

## Manual delivery clarification - 2026-10-25

The next complete journey is FR-2513/2515/2516: custom configuration, explicit
startup selection and a session-local runtime cycle. Automatic background
detection/FR-2514/T032 remains open. Until that separate task is implemented,
`auto` is explicitly refused as specified below. Mouse capture remains false
until the independent pointer story is complete; this slice does not change it.
The later OSC protocol section remains the requirement for that future task.

## Configuration surface

New and changed keys:

```toml
[ui]
theme = "dark"          # "dark" | "light" | "high-contrast" | "custom" | "auto"
color-depth = "auto"    # "auto" | "truecolor" | "256" | "16"
density = "comfortable" # "comfortable" | "compact"
mouse = false           # interim default until the complete pointer story

[theme]                 # optional; defines the "custom" theme
extends = "dark"        # "dark" | "light" | "high-contrast"; default "dark"
# any token name = "#RRGGBB", for example:
danger = "#F38BA8"
surface-pane = "#20203A"
```

- Token keys are the kebab-case names of the token enum, as listed in the
  theme design document (for example `text`, `muted`, `surface`,
  `surface-pane`, `surface-overlay`, `danger`, `environment-production`,
  `syntax-keyword`).
- Colour values are exactly seven characters: `#` plus six hex digits.
  Digits are ASCII, either case. Nothing else parses; no trimming or aliases.
- Manual selection is exactly `dark`, `light`, `high-contrast` or `custom`.
  Missing `[theme]` means no custom definition; an empty table defines custom
  extending dark. Missing `extends` also defaults to dark. A custom definition
  is validated completely even when inactive or colour is disabled.
- The template in `src/config/store.rs` gains every new key with its
  default in the same change as the schema (a test compares the template
  against the defaults).
- `--color-depth <auto|truecolor|256|16>` is the flag form and follows the
  existing flag-over-configuration precedence. `NO_COLOR` and `TERM=dumb`
  beat everything.

## Load-time validation

Validation of `[theme]` runs during configuration load, before the
terminal is taken. The whole theme is refused on the first failure found;
process exit code is 3 (the existing configuration exit). Exact wording,
one line each, on standard error:

- Unknown token:
  `configuration error: [theme] has no token named "danger2". Token names are listed in docs/design/theme-tokens.md.`
- Malformed colour:
  `configuration error: [theme] danger = "red" is not a colour. Use "#RRGGBB" hex, for example "#F38BA8".`
- Unknown base:
  `configuration error: [theme] extends = "solarized" is not a built-in theme. Use "dark", "light" or "high-contrast".`
- Contrast failure:
  `configuration error: [theme] muted on surface-pane is 2.1:1; this pair must reach 4.5:1.`

Thresholds are the existing documented ones: 4.5:1 for text-carrying
tokens, 3:1 for semantic accents, 7:1 for every pair when
`extends = "high-contrast"`. The measured ratio in the message is computed
by the same relative-luminance code the tests use.

### Complete resolved-palette matrix

Use the existing relative-luminance implementation and unrounded RGB ratios.
The four backgrounds, in order, are Surface, SurfaceAlt, SurfacePane and
SurfaceOverlay. Canonical token names and order come from Token::ALL.

| Pair | Dark/light base | High-contrast base |
| --- | ---: | ---: |
| Text, Muted, Header, NullValue and all five Syntax tokens against every background | 4.5 | 7.0 |
| Focus, Success, Warning, Danger, Info, EnvironmentProduction, EnvironmentNonProduction, TransactionActive, TransactionFailed against every background | 3.0 | 7.0 |
| Border against every background | decorative; must still differ from Surface | 7.0 |
| Surface ink on Selection fill | 4.5 | 7.0 |
| Surface ink on EnvironmentProduction capsule fill | 4.5 | 7.0 |

Retain the existing high-contrast rule that every non-background token,
including Border and Selection, reaches 7.0 against all four backgrounds.
EnvironmentProduction is the only current production capsule fill; do not
pre-implement US8's future capsule set. This contract concerns resolved RGB,
not a new physical-display, DIM, 256-colour or basic 16-colour contrast guarantee.
Do not exempt inherited, empty, inactive or colour-off definitions. The four
measured dark/light NullValue/SyntaxComment RGB corrections are admitted only
after actual-library failing threshold tests, followed by styled inspection.
Keep their palette identity and all other RGB entries unchanged unless another
matrix failure is first demonstrated and reviewed.

### Parsing, precedence and safe refusal

Preserve one strict configuration schema. Defer only `ui.theme` and the top-level
theme value while the original TOML deserializer retains syntax/duplicate-key,
unknown non-theme key, non-theme type and original-source span behavior. Do not
strip/rewrite a parsed document or add a second field list/catch-all schema.
An unvalidated pending representation cannot become successful configuration.
The expected-key list may add exactly the newly recognised top-level `theme`
key. The rejected key, wrapper, class, order and span remain unchanged. Generic
implementation type names must not replace `Config`/`UiConfig` in diagnostics.

- Load: existing read/parse failures and ordinary Config::validate issue order/
  aggregation win first, then theme lowering and full resolution.
- Migrate: preserve its existing parse wrapper and newer-schema refusal first;
  validate theme before current-version no-op, dry-run, backup or write.
- Save: validate the supplied typed theme before serialization/write.
- Migrate/save currently do not run ordinary range validation. Preserve that
  distinction; adding theme checks does not authorize broadening old behavior.
- Show/validate retain their existing load/keymap path. Doctor retains its
  report/JSON envelope and failed-check behavior. No new terminal query occurs.

Theme errors use a typed theme-only presentation tag, never a headline-prefix
test or global Config formatter change. Commands propagating that diagnostic
emit its exact human line plus one newline, exit 3, before terminal acquisition.
Existing structured envelopes and all non-theme diagnostic behavior remain.

Theme first-error order: selection/missing definition; table shape; extends
type/name; unknown keys lexically; malformed overrides in Token::ALL order;
contrast pairs in token/background order, then Selection and production fill.
For normal Border equality use the fixed wording below. Compare ratios
unrounded, but floor their displayed ratio to one decimal; thresholds always
display one decimal. A rejected ratio below 4.5 must not print 4.5 as its measure.

Additional exact lines (quoted placeholders are already safe):

- `configuration error: ui.theme = "custom" requires a [theme] table.`
- `configuration error: ui.theme = "{selection}" is not a theme. Use "dark", "light", "high-contrast" or "custom".`
- `configuration error: ui.theme = "auto" is not available in this build. Use "dark", "light", "high-contrast" or "custom".`
- `configuration error: ui.theme must be a string. Use "dark", "light", "high-contrast" or "custom".`
- `configuration error: theme must be a table.`
- `configuration error: [theme] extends must be a string. Use "dark", "light" or "high-contrast".`
- `configuration error: [theme] {token} must be a colour string. Use "#RRGGBB" hex, for example "#F38BA8".`
- `configuration error: [theme] border must differ from surface.`

For every reflected string, call the existing redact_text first. Then escape
double quote/backslash as `\"`/`\\`, LF/CR/tab as `\n`/`\r`/`\t`, and every other
control or non-ASCII scalar as `\u{XXXX}` with uppercase hex and at least four
digits. Preserve other printable ASCII. Admit at most 80 escaped ASCII characters,
never splitting an escape/scalar; append exactly `... [truncated]` outside that
budget when shortened. Never echo a non-string value. No raw newline, ESC or
bidi scalar may reach the physical line; this is formatting after the one
existing redactor, not a second secret matcher. Preserve the original four
short-safe example lines verbatim.

Colour off (any route) renders every theme, custom included,
modifier-only. A custom theme cannot reintroduce colour under `NO_COLOR`.

### Configured colour route - observed integration gap, 2026-10-25

Full-screen startup and doctor must resolve colour as an explicit `--color`
choice, then `[ui] color`, then terminal facts for `auto`. An explicit
`--color auto` overrides configured `never`/`always` with actual auto behavior.
`--plain`, `NO_COLOR` and `TERM=dumb` remain absolute colour-off routes.
Carry the explicit flag/absence fact through CLI presentation until the loaded
configuration is available; a pre-resolved boolean cannot retain precedence.
Bind this result before resolving depth or selecting the validated theme cache.
Doctor must use the same configured route and keep its existing envelope.
Keep raw terminal facts truthful. Doctor's non-terminal capability wording must
name automatic colour, because an explicit setting can override auto; its
TERM=dumb wording must reflect that absolute off gate. Active depth remains a
separate resolved fact.
T057 closes the observed ignored `[ui] color` startup/doctor route; it adds no
flag, configuration key, terminal query or scripted-output schema change.

## Runtime switching

- Palette action name: `Switch theme`, palette group Session, configurable
  action name `switch-theme`, no default direct key.
- Cycle order when invoked without a choice: `dark`, `light`,
  `high-contrast`, then `custom` when defined, then back to `dark`.
- The switch replaces `Model::presentation`'s theme on the next frame.
  Session-local; configuration is never written.
- Retain selected Custom identity inseparably from its validated 24-colour RGB
  palette. Keep the built-in-only Theme::new constructor; missing custom data
  cannot silently select a built-in. Resolve/cache once per load, not per paint.
- One explicit Session palette row is present even with no direct key; aliases
  use the actual keymap snapshot and never duplicate that row.
  Selected palette group, label and detail spans all receive Selection ink/fill
  after their existing individual styles. Preserve their modifiers, words,
  geometry, unselected styling and existing basic16/colour-off reversed selection.
  The selected label already uses Selection: retain its exact basic16 Reset ink;
  do not introduce Text ink where Selection intentionally emits none. The
  25 October styled review observed HighContrast's selected Session/detail
  spans retaining Muted ink on the white Selection fill; T056 repairs that
  composition without changing any palette RGB values.
- Direct Switch theme is a no-op against any existing input/overlay owner,
  including a pending prefix, Help or Notice details. Guard it before clearing
  owner/layout state. Palette cancellation/empty choice retains its parent and
  offset; actual choice keeps existing explicit parent-close behavior and never
  acknowledges notice loss. Password/review/input handlers keep precedence.
- In ordinary or running/cancelling session state, a switch has zero effects and
  changes only active theme. Preserve job/session/generation/transaction/editor/
  result/selection/snapshot state and depth/glyph/density/motion. It performs no
  database, file, clipboard, history, cancellation or retry action.
  Theme input has no backend-size prerequisite; a size lookup must not reset
  covered owner offsets or prevent cycling. Separate resize events retain their
  existing geometry and review-admission behavior.
- A programmatic catalog/active-custom mismatch retains the active theme and
  reports an ordinary internal diagnostic; it must not choose a built-in.
  Preserve an existing query diagnostic and attach at most one fixed theme
  technical note rather than replacing its PostgreSQL/error facts.

## `ui.theme = "auto"`

- Runs once at startup, before the alternate screen is entered and before
  the input thread starts.
- Request bytes: `ESC ] 1 1 ; ? BEL` (`\x1b]11;?\x07`).
- Wait at most 100 ms for a reply of the shape
  `ESC ] 1 1 ; r g b : RRRR / GGGG / BBBB` terminated by BEL (`\x07`) or
  by ST (`\x1b\\`). Parse both terminators; components are 1-4 hex digits
  each, scaled to 8 bits by their own width.
- Decision: WCAG relative luminance of the reported background at or above
  0.5 selects `light`; below selects `dark`.
- No reply, or an unparseable reply, selects `dark` and records the fact
  for `doctor` as `no answer within 100 ms; using dark`. This is a
  fallback and is never described as detection.
- Bytes read after the deadline are discarded; a late reply must never
  surface as typed input. The read happens before raw-mode input handling
  begins, and anything unconsumed is drained before the event loop starts.
- `auto` composes with `[theme]` only through `extends`; `auto` never
  selects `custom`.

## Parity obligation

The meaning-parity test matrix becomes: three built-in themes, plus one
valid fixture custom theme, plus one deliberately hostile fixture theme
(extreme but validation-passing values), across colour on and off and all
three glyph tiers, asserting the same meaning words in every combination.
The two fixture themes live in the test code, not in shipped
configuration.
