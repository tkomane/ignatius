# Data model: GUI-grade experience

The feature adds no persisted entity beyond configuration keys. It moves
presentation into the interactive model, adds session-local layout state,
and derives everything else from state the model already holds.

## Presentation

- **Owner**: `Model::presentation` (new field; today `Presentation` is built
  once in `cli/interactive.rs` and passed by reference)
- **Purpose**: make the active theme, glyph tier, colour depth, density and
  reduced-motion facts part of reducible state so a theme switch is an
  ordinary message with an ordinary next frame
- **Rules**:
  - constructed once at startup from configuration, flags and detection;
  - replaced only by an explicit switch action or a detection answer at
    startup, never by a passive transition;
  - the renderer reads it from the model and holds no copy;
  - colour off forces modifier-only styling regardless of theme source.

## Colour depth

- **Owner**: `Presentation` (active value); `TerminalFacts` in
  `src/ui/terminal.rs` (detected value and its source)
- **Purpose**: one fact naming how colour is emitted: truecolor, 256, 16,
  or none
- **Rules**:
  - detection order: `NO_COLOR` or `TERM=dumb` force none; explicit flag or
    `ui.color-depth` overrides; `COLORTERM` in `truecolor`/`24bit` means
    truecolor; `TERM` containing `256color` means 256; otherwise 16;
  - quantization is a pure function from the truecolor palette, applied at
    the single Rgb conversion point in `src/ui/theme.rs`;
  - contrast rules are enforced on the truecolor palette only; reduced
    depths are documented approximations;
  - `doctor` reports value, source and any override.

## Theme

- **Owner**: built-in palettes in `src/ui/theme.rs`; custom theme derived
  at load from the `[theme]` configuration table
- **Purpose**: token-to-colour resolution for exactly three elevation
  levels and the existing semantic tokens
- **Rules**:
  - a custom theme is base (`extends`, default `dark`) plus overrides;
  - validation happens at configuration load, before the terminal is
    taken: unknown token, malformed colour or contrast failure is refused
    whole with exit 3 and the token and rule named;
  - the parity guarantee is per built-in theme plus one valid and one
    hostile fixture theme in tests; user themes are guaranteed by the
    load-time validation rule, not by enumeration;
  - `ui.theme` accepts `dark`, `light`, `high-contrast`, `custom`, `auto`;
  - `auto` resolves once at startup from the terminal background answer
    (100 ms deadline) to `dark` or `light`, falling back to `dark` with the
    unanswered state recorded for `doctor`.

## Split state

- **Owner**: `Model::splits` (new field)
- **Purpose**: session-local layout shape: sidebar width, editor share,
  zoom
- **Rules**:
  - adjusted only by explicit keys or an explicit drag;
  - clamped to documented minimums at every change and on every resize;
  - zoom stores the pre-zoom shape and restores it exactly;
  - never persisted; a new session starts from the configured default;
  - `editor_page`/`results_page` style paging derives from the same values
    the renderer uses, so paging always matches what is visible.

## Density

- **Owner**: `Presentation`, from `ui.density`
- **Purpose**: select one of exactly two documented spacing tables
- **Rules**: `comfortable` (default) or `compact`; unknown values are
  refused by the existing configuration rule; the table is data consumed by
  widgets, not per-widget improvisation.

## Connecting step

- **Owner**: the existing `ConnectionState::Connecting` variant, extended
  with a step
- **Purpose**: name the current stage of establishing a session
- **Rules**:
  - steps: resolving profile, acquiring credential, TLS handshake, server
    handshake, loading catalogue;
  - a step that does not apply is skipped, never shown as done;
  - reported by the connection service as messages; the reducer never
    guesses a step;
  - failure keeps the failing step's name in the existing diagnostic.

## Hit region

Derived, not stored. A pure function of the model and the frame area:

| Input | Output | Why |
| --- | --- | --- |
| model, area, column, row | `Option<Region>` | one place decides what is under the pointer |

- Regions: header, footer, sidebar row, editor text position, editor
  gutter, results header cell, results cell, results scrollbar, vertical
  split, horizontal split, overlay item, column edge.
- Computed from the same layout arithmetic the renderer uses, so a hit
  region can never disagree with what was drawn.
- Every mouse verb resolves through this function to the same `Action` the
  keyboard emits; a position outside every region maps to nothing.

## Mouse and paste messages

- **Owner**: `Message::Mouse { kind, column, row, modifiers }` and
  `Message::Pasted(text)` (new variants)
- **Purpose**: input events become ordinary reducible messages
- **Rules**:
  - the runtime translates terminal events; the reducer stays pure;
  - unbound kinds and out-of-region positions produce no state change;
  - paste inserts through the editor's existing edit primitives as one
    undoable step, CRLF and CR normalised to LF, bounded at 1 MiB;
  - a paste while a non-text surface owns input follows the spec's
    accept-or-state-ignored rule; nothing is silently dropped.

## Badge domain

Derived, not stored. Existing model facts render through a fixed
domain-to-colour map:

| Domain | Source fact | Loud when |
| --- | --- | --- |
| Environment | connection classification | production |
| Transaction | server-reported transaction state | active or failed |
| Transport | observed TLS state | not encrypted or unknown |
| Posture | read-only / read-write | read-only |

Elapsed time renders only at or above 200 ms. Words carry every meaning;
colour off renders badges as reversed text.

## Overlay preview

Derived, not stored. A palette purpose declares its preview source from
already-retained safe data:

| Purpose | Preview source |
| --- | --- |
| Object definitions | retained definition text |
| Saved queries | the file's SQL text through display sanitising |
| Connection picker | the existing safe profile summary |
| Dependencies | the retained dependency explanation |
| Others | none; full-width list |

Moving the selection changes only which retained content is shown. Nothing
is executed, fetched or written to render a preview.
