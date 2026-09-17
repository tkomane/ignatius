# Research: GUI-grade experience

**Method**: Web research was performed on 2026-09-17, one pass per source
with at most three observations each, reading official documentation and
READMEs only. Every citation below names the URL actually fetched. Gaps are
stated rather than filled: no fetched source documents NULL rendering or
viewport-position wording for a terminal grid, so those remain this
feature's own decisions; harlequin's documentation site rate-limited
(HTTP 429) and only its README was read; television's docs pages returned
404 and its shipped default configuration was read instead; k9s's exact
header composition and the lazygit footer hint count were not confirmed by
any page read. The owner's own environment (starship prompt, Warp settings,
fzf options) was read locally on 2026-09-17 and is cited as "owner
environment".

## Existing presentation boundary

The renderer asks a semantic `Token` enum for meaning and three
compile-time palettes answer with truecolor values; contrast is enforced by
tests, colour-off degrades to modifiers only, and glyphs come in three
tiers with ASCII designed rather than tolerated. The renderer paints
foreground only, `Presentation` is constructed once at startup, mouse
capture is off by default, and bracketed paste is enabled but its events
are dropped. All of this is the foundation the decisions below extend; none
of it is replaced.

## Design decisions

### D-2501: Paint the frame with a base fill, panes as blocks, overlays as Clear-then-block

Ratatui's documented style merge order (base style, then borders, then
titles, then content) makes one base-style background fill cheap and
consistent, with widgets overriding only what differs. Overlays follow the
documented popup pattern: render `Clear` over the target area first, then
the styled block; the first frame uses the terminal clear, which the
runtime already performs. Rejected: painting per-widget without a base
coat, because any missed cell leaks the terminal background, which is the
defect this feature exists to fix.

### D-2502: Exactly three elevation levels

Catppuccin's style guide defines an eight-step ladder (base, mantle, crust,
surface0-2, overlay0-2). Terminal cells cannot carry shadows or blur, so
depth must come from a few unmistakable background steps: base for the
frame, pane surface for pane interiors, overlay surface for floating
layers. Three levels map onto the existing `Surface`/`SurfaceAlt` token
family without inventing a parallel system. Rejected: adopting the full
eight-step ladder, because eight backgrounds in a 24-line terminal reads as
noise and multiplies every contrast obligation.

### D-2503: A colour-depth tier that emits indexed colours below truecolor

Ratatui's own documentation warns that `Color::Rgb` renders correctly only
on truecolor terminals and that Crossterm performs no downsampling, with
glitching as the observed failure. The lipgloss ecosystem solves this with
per-colour tiers chosen by detected profile. Ignatius therefore adds
`ColorDepth { TrueColor, Indexed256, Basic16, None }` symmetric with the
existing `GlyphTier`, and a pure quantizer at the single Rgb conversion
point: nearest xterm-256 cube or greyscale entry for 256, a fixed hand-
checked 16-colour table for basic terminals. Contrast tests keep running on
the truecolor palette; reduced depths are documented approximations that
never carry meaning alone. Rejected: trusting the terminal to approximate,
because the documented behaviour is glitching, not approximation.

### D-2504: Depth detection by convention, overridable, with the existing vetoes

The de-facto standard is `COLORTERM` equal to `truecolor` or `24bit`
(case sensitive), with `TERM` containing `256color` as the lower-tier
fallback; terminfo's RGB capability is too new to rely on alone. Detection
order: `NO_COLOR` and `TERM=dumb` force none, an explicit `--color-depth`
flag or `ui.color-depth` key overrides, then `COLORTERM`, then `TERM`,
then 16. `doctor` reports the value and its source. Rejected: probing with
a truecolor sequence and reading the reply, because the shallow-detection
doctrine prefers an admitted default over a probe that hangs or lies on
some terminals; the one probe this feature does add (OSC 11) is bounded and
optional.

### D-2505: `theme = "auto"` by a hand-rolled OSC 11 exchange

Crossterm 0.29 has no background-query API (verified absence in its event
documentation), so the runtime writes `ESC ] 11 ; ? BEL` before the
alternate screen, reads for at most 100 ms, and accepts a reply of the
documented shape `ESC ] 11 ; rgb:RRRR/GGGG/BBBB` with either BEL or ST
termination, parsing both. Luminance picks dark or light. No reply, or an
unreadable reply, means the configured default with the unanswered state
recorded for `doctor`; a late reply is discarded wherever it arrives and
must never surface as typed input. Rejected: a dependency for one
sequence, and rejected: making `auto` the default, because a truthful
fallback beats a wrong guess on terminals that never answer.

### D-2506: User themes as a validated `[theme]` table with `extends`

Atuin's theming (semantic keys, TOML, parent inheritance) and k9s skins
(named files, per-context selection) both show users expect theme
configuration keyed by meaning, not by widget. Ignatius adds one `[theme]`
table: `extends` names a built-in base, the remaining keys override named
tokens with hex values, and the result is selectable as `custom`.
Validation happens at configuration load, before the terminal is taken:
unknown token, malformed colour, or a contrast failure refuses the whole
theme with exit 3 and the token and rule named. Colour-off remains
modifiers-only whatever the theme source. The meaning-parity test matrix
gains one valid and one deliberately hostile fixture theme, so the
guarantee becomes "any theme that passes validation", not "every shipped
palette". Rejected: a themes directory with named files, because one table
in the existing configuration file is enough for one custom theme and adds
no new file discovery surface. Rejected: a transparent base value in the
k9s style, because inheriting the terminal background is exactly the
defect this feature removes.

### D-2507: Runtime theme switching through model-owned presentation

Presentation moves into the model, so Switch theme is an ordinary palette
action handled by the pure reducer, applied on the next frame, session-
local, never written to configuration. This is foundational work because
every depth- and theme-aware story reads presentation from the model.

### D-2508: The converged mouse verb set, and nothing more

Across fzf, lazygit and yazi the converged verbs are: left-click selects
or focuses, double-click accepts or opens, the wheel scrolls the pane
under the pointer, and a modifier plus wheel handles the second axis.
Drag exists but is fringe (opt-in in yazi, undocumented for lazygit), so
Ignatius binds drag only where a GUI user reaches for it, split bars and
column edges, and keeps every drag supplementary to a keyboard route.
Right-click stays unbound: macOS reports Ctrl+left-click as a right click
in Crossterm, so binding it would make one physical gesture mean two
things. Crossterm's other documented quirk, that Up and Drag events may
substitute the left button, means no verb may depend on which button ends
a drag. Every TUI studied ships a mouse kill switch; `ui.mouse = false`
remains that switch.

### D-2509: Hit regions as pure functions of the same layout arithmetic

Mouse events become `Message::Mouse { kind, column, row, modifiers }` and
the reducer resolves them through one pure `region_at` function computed
from the same rect arithmetic the renderer uses. Hit-testing that cannot
disagree with drawing is the property; a second geometry implementation is
the rejected alternative.

### D-2510: Mouse capture defaults to on, as a declared amendment

Today capture is off by default, with a tested rationale: capture takes
the terminal's own text selection. Lazygit ships capture on by default and
documents the same cost. With first-class verbs the trade flips: capture
is on by default, `ui.mouse = false` is the documented opt-out, help names
Shift-selection as the native-selection route, and OSC 52 remains the
opt-in in-app copy answer. The design document's mode table, its rationale
paragraph and the two tests that pin the old default are amended in the
same change.

### D-2511: Paste through the bracketed-paste event, bounded and honest

Crossterm delivers `Event::Paste(String)` under the bracketed-paste
feature the client already enables; the input thread simply never consumed
it, which is the confirmed defect. Paste inserts through the editor's
existing edit primitives as one undoable step, CRLF and CR normalised to
LF, bounded at 1 MiB with the limit named. A paste while a non-text
surface owns input is acknowledged as ignored, never dropped silently. On
terminals without bracketed paste, characters arrive as typed input
exactly as today; the client does not guess.

### D-2512: Grid wording and verbs from the tool that won

The VS Code PostgreSQL extension's grid documents auto-sizing to visible
content with manual override, an explicit three-state sort (ascending,
descending, clear), a row-number gutter, and status text carrying row
counts and timing. Ignatius already auto-sizes, keeps gutter row numbers
and states counts; this feature adds the missing pieces: viewport position
in words in the documented shape `rows 120-160 of 1,248 retained` (no
fetched source documents a terminal-grid position wording, so the shape is
this feature's own, consistent with the existing count lines), the same
three-state sort cycle on header click and on the existing keys, cell
click and double-click selection mapped to the existing inspector, wheel
scrolling at three rows per notch with Shift for columns, and drag or
keyboard column resizing within documented bounds.

### D-2513: Startup that paints first and names its steps

K9s renders its frame before data and streams rows in as caches fill; the
VS Code extension stages its connection flow, confirms success in place
with a status indicator, and pre-maps failure text to causes and next
steps. Ignatius adopts the same order: the full shell paints before any
connection completes, the connecting state names its step (resolving
profile, acquiring credential, TLS handshake, server handshake, loading
catalogue) with target and elapsed time and an escape route, failures keep
the failing step's name with the existing diagnostic wording, and the
first connected frame lands focus in the editor with the first schema
level expanded and the run hint visible. The unconfigured first frame
states the one next action. Rejected: a splash screen, and rejected: any
connection dialog rework, because the picker from Feature 020 already
covers selection and this feature must not respecify it.

### D-2514: Overlay geometry from the owner's own picker

The owner's fzf configuration is height 80 percent, reverse layout (input
at the top, matches growing downward), a border, and a right-hand preview
at 60 percent; fzf and television default to a 50 percent preview split,
and both ship preview toggling. The palette adopts the owner's geometry:
80 percent height within existing minimums, input at the top, right
preview at 60 percent of the overlay width for purposes with previewable
retained content, full width for the rest, and omission below a documented
minimum width. Previews render retained safe data only.

### D-2515: One zoom toggle, not screen modes

Lazygit cycles three screen modes; that is a richer model than this
feature needs. One key, `Ctrl+K z`, maximises the focused pane over the
body area and restores the exact prior layout on repeat. Splits move by
`Ctrl+K ,` and `Ctrl+K .` and by dragging the split bar, clamped to
documented minimums, session-local, never persisted.

### D-2516: Quiet chrome, threshold-gated, suppression over stacking

The owner's prompt disables every module that does not change a decision
and gates telemetry by thresholds; Claude Code's status line hides hint
chrome while richer surfaces are open rather than stacking rows. The
header keeps identity and location left and context right, elapsed time
appears only at or above 200 ms, the non-production local default carries
no badge while `PROD` stays loud, and the capsule set extends to the four
badge domains with one fixed colour each and words carrying every meaning.
The footer keeps its existing six-hint cap and contextual rules.

### D-2517: Density as exactly two documented tables

`ui.density = "comfortable" | "compact"` selects between two pinned
spacing tables. A continuous knob, or a third value, multiplies test
surface without a user benefit; two values match the Warp precedent in the
owner environment.

## Alternatives considered

- **Adopting a ready-made theme crate or the Catppuccin palette verbatim**:
  rejected; several upstream pairs fail the repository's 4.5:1 contrast
  tests, and the interaction principles forbid copying another product's
  visual identity. The default dark palette is owner-informed and
  contrast-corrected by the existing tests, which stay authoritative.
- **A background-detection dependency**: rejected; one documented OSC
  exchange does not justify a crate, and the no-new-dependency constraint
  holds.
- **Right-click menus**: rejected for this feature; the macOS modifier
  quirk makes the gesture ambiguous, and context menus need their own
  specification if ever wanted.
- **Persisted layout or theme choice**: rejected; session-local state
  keeps the local-first privacy story and the configured defaults as the
  single source of truth.
- **A transparent-background escape hatch**: rejected as above; users who
  want their terminal's background can set matching colours in `[theme]`.
- **Scroll-margin centering (yazi's scrolloff)**: deferred; it is a
  refinement of grid scrolling that can ride any later grid slice without
  specification changes here.
- **A remote-control picker-in-picker (television)**: deferred; the
  existing palette purposes cover source switching, and a layered picker
  is new interaction vocabulary this feature does not need.

## Sources

All accessed 2026-09-17.

- https://docs.rs/ratatui/latest/ratatui/widgets/struct.Block.html
- https://docs.rs/ratatui/latest/ratatui/widgets/struct.Clear.html
- https://docs.rs/ratatui/latest/ratatui/style/enum.Color.html
- https://github.com/catppuccin/catppuccin/blob/main/docs/style-guide.md
- https://github.com/charmbracelet/lipgloss
- https://atuin.sh and https://docs.atuin.sh/latest/guide/theming/
- https://code.claude.com/docs/en/statusline
- https://github.com/jesseduffield/lazygit/blob/master/docs/Config.md
- https://raw.githubusercontent.com/jesseduffield/lazygit/master/docs/keybindings/Keybindings_en.md
- https://github.com/derailed/k9s
- https://raw.githubusercontent.com/junegunn/fzf/master/man/man1/fzf.1
- https://raw.githubusercontent.com/alexpasmantier/television/main/.config/config.toml
- https://yazi-rs.github.io/docs/configuration/yazi
- https://invisible-island.net/xterm/ctlseqs/ctlseqs.html
- https://docs.rs/crossterm/0.29.0/crossterm/event/index.html
- https://docs.rs/crossterm/0.29.0/crossterm/event/struct.MouseEvent.html
- https://github.com/termstandard/colors
- https://learn.microsoft.com/en-us/azure/postgresql/development/vs-code-extension/query-editor-intellisense
- https://learn.microsoft.com/en-us/azure/postgresql/development/vs-code-extension/quickstart-connect-query
- https://github.com/tconbeer/harlequin (README only; harlequin.sh rate-limited)
- https://github.com/achristmascarl/rainfrog
- https://posting.sh and https://github.com/darrenburns/posting
- Owner environment: starship configuration, Warp settings and
  `FZF_DEFAULT_OPTS`, read locally.
