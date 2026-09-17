# Contract: Theme configuration, validation and auto detection

Pinned grammar, wording and protocol facts. An implementer who needs a
value that is not here stops and reports.

## Configuration surface

New and changed keys:

```toml
[ui]
theme = "dark"          # "dark" | "light" | "high-contrast" | "custom" | "auto"
color-depth = "auto"    # "auto" | "truecolor" | "256" | "16"
density = "comfortable" # "comfortable" | "compact"
mouse = true            # changed default; false disables capture

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
  Nothing else parses.
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

Colour off (any route) renders every theme, custom included,
modifier-only. A custom theme cannot reintroduce colour under `NO_COLOR`.

## Runtime switching

- Palette action name: `Switch theme`, palette group Session, configurable
  action name `switch-theme`, no default direct key.
- Cycle order when invoked without a choice: `dark`, `light`,
  `high-contrast`, then `custom` when defined, then back to `dark`.
- The switch replaces `Model::presentation`'s theme on the next frame.
  Session-local; configuration is never written.

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
