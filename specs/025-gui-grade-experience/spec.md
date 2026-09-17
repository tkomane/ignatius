# Feature Specification: GUI-grade experience

**Feature Branch**: `025-gui-grade-experience`

**Created**: 2026-09-17

**Status**: Specified; not implemented. Written from the owner's 2026-09-17
observed-friction evidence and a planning session's code reading; no product
behaviour has changed for this feature yet.

**Input**: User description: "Close the gap between the GUI-like promise and
the observed experience. On 2026-09-17 the owner abandoned Ignatius for the
VS Code Azure PostgreSQL extension during real work against an Azure
PostgreSQL database because the extension was cleaner and simpler. Four
pillars: visual coherence, effortless navigation, results-grid ergonomics,
zero-friction startup, plus the confirmed dropped-paste defect. Evidence
sample size is one observation by the owner, recorded as one."

## User Scenarios & Testing

### User Story 1 - Pasted SQL arrives in the editor (Priority: P1)

Someone copies a query from a runbook, a browser or another terminal and
pastes it into the SQL editor. The pasted text appears at the caret as
ordinary typed input: one undo step, Windows line endings normalised, and
nothing executed. Today the full-screen client requests paste events from
the terminal and then discards them, so pasting appears to do nothing.

**Why this priority**: Pasting a query is the single most common way real
work enters a database client. A client that silently drops a paste fails
the first minute of the exact Azure investigation that motivated this
feature.

**Independent Test**: Deliver a paste event containing a multi-line
statement with CRLF line endings and assert the editor contains the text at
the caret as one undoable edit, the buffer is not executed, and the editor
revision advances once.

**Acceptance Scenarios**:

1. **Given** the editor has focus, **when** a paste event delivers text,
   **then** the text is inserted at the caret as a single undoable edit,
   CRLF and CR line endings become LF, and no statement runs.
2. **Given** an overlay or prompt owns input, **when** a paste event
   arrives, **then** the paste goes to that surface if it accepts text
   entry, and otherwise nothing changes and the client says nothing was
   pasted rather than silently dropping it.
3. **Given** a paste larger than 1 MiB, **when** it arrives, **then** the
   client refuses it with a short reason naming the limit, and the editor
   is unchanged.
4. **Given** a paste containing control characters other than tab and
   newline, **when** it is inserted, **then** the stored text is the pasted
   text and the rendered view stays free of raw control sequences under the
   existing display-sanitising rules.

### User Story 2 - The screen is a surface, not a transparency (Priority: P1)

Someone opens Ignatius in any terminal and sees a coherent, deliberately
painted application: the whole frame carries the theme's background, panes
sit on a visible surface, and overlays sit visibly above them. Today the
client paints only foreground text, so the terminal's own background bleeds
through everywhere and the light theme is unreadable on a dark terminal.

**Why this priority**: This is the largest single visual difference between
Ignatius and every GUI tool, and the cheapest to close. Nothing else in the
visual pillar reads as designed until the product owns its background.

**Independent Test**: Render the connected client to a buffer and assert
that every cell in the frame carries the theme background, that pane
interiors and overlay interiors use their designated surface levels, and
that the same render with colour off carries the same words with
modifier-only styling.

**Acceptance Scenarios**:

1. **Given** any built-in theme at full-colour or 256-colour depth,
   **when** any screen is rendered, **then** no cell shows the terminal's
   own background: the base surface, the pane surface and the overlay
   surface are painted from exactly three documented elevation levels.
2. **Given** a terminal advertising only 256-colour support, **when** the
   client renders, **then** colours are quantized from the same palette by
   documented rules; **given** only 16-colour support, **then** foreground
   colours map by the documented table, surfaces are not painted and
   elevation follows the colour-off rules; in both cases every meaning
   remains carried by words exactly as at full colour.
3. **Given** `NO_COLOR`, `TERM=dumb`, `--color never` or colour disabled in
   configuration, **when** the client renders, **then** no colour of any
   depth is emitted and the existing modifier-only degradation applies
   unchanged.
4. **Given** the diagnostics command, **when** capability detection runs,
   **then** the detected colour depth and its source are reported alongside
   the existing detections, and an override is reported as an override.

### User Story 3 - A results grid you can work (Priority: P1)

Someone investigates a wide, tall result the way they would in a GUI: they
always know where they are in the data, sort by clicking a header or using
the existing keys, adjust a column that is too narrow, scroll with the
wheel, and open a cell in detail with a double click. Nothing they do
re-runs the query, and every count stays honest.

**Why this priority**: The results grid is where the VS Code extension most
clearly beat Ignatius in the owner's session. It is the surface where users
spend the longest continuous time.

**Independent Test**: Retain a result larger than one screen, then drive
sorting, scrolling, column resizing and cell selection through both the
keyboard and synthetic mouse input, asserting identical resulting state for
both routes, position wording that always matches the viewport, and zero
execution effects.

**Acceptance Scenarios**:

1. **Given** a retained result taller than the pane, **when** the grid is
   rendered, **then** the Results surface states the visible row range and
   totals in words, in the shape `rows 120-160 of 1,248 retained`, and the
   wording stays consistent with the existing truncation and filter counts.
2. **Given** a rendered header row, **when** the user clicks a column
   heading or uses the existing sort keys, **then** the sort cycles
   ascending, descending, then original server order, the state is stated
   in words, and the query is not re-executed.
3. **Given** a rendered grid, **when** the user clicks a cell, **then**
   that cell becomes the selection with its source row and column identity
   preserved; **when** the user double-clicks a cell, **then** the existing
   inspector opens on that cell, exactly as the keyboard route does.
4. **Given** a grid with more columns or rows than fit, **when** the user
   scrolls the wheel, **then** the rows move three per notch; **when** the
   user scrolls with Shift held, **then** the visible columns move; neither
   changes the selection's identity.
5. **Given** a column edge, **when** the user drags it, or presses the
   documented narrower and wider keys `Ctrl+K [` and `Ctrl+K ]` on the
   selected column, **then** the column width changes within documented
   bounds and the layout states remain reversible local view state.
6. **Given** painted row stripes at any colour depth, **when** NULL, empty
   text and the literal text NULL appear, **then** the three remain
   distinguishable by words and markers exactly as the existing rules
   require, with colour and striping as reinforcement only.

### User Story 4 - The first frame is already useful (Priority: P1)

Someone starts Ignatius and immediately sees the real application: the full
shell is painted before any connection completes, connecting progress is
told as named steps rather than one word, and when the connection is ready
the first keystroke is productive because the object tree shows the first
level of the schema and focus is in the editor with the run hint visible.
Someone with nothing configured sees the one next action instead of empty
panes.

**Why this priority**: The abandonment evidence begins at startup: in the
VS Code extension, getting connected and oriented takes no thought. First
impressions of every session are made here.

**Independent Test**: Drive the model from launch through a successful
connection and assert each named connecting step renders with the target
and elapsed time, the shell is fully painted in every pre-connection frame,
and the first post-connection frame has the editor focused, the run hint
visible and the first schema level expanded. Separately assert the
unconfigured state names its single next action.

**Acceptance Scenarios**:

1. **Given** a launch with a resolvable target, **when** the connection is
   being established, **then** the header, pane frames and footer are fully
   painted, and the connecting state names the current step from resolving
   profile, acquiring credential, TLS handshake, server handshake, and
   loading catalogue, together with the target and elapsed time and a
   visible way to cancel or quit.
2. **Given** a connection step fails, **when** the failure is shown,
   **then** the step that failed is named with the existing diagnostic
   wording, and no step is described as complete unless it completed.
3. **Given** the connection succeeds, **when** the first connected frame
   renders, **then** the object tree shows the first schema level already
   expanded, focus is in the editor, and the footer names the run action.
4. **Given** no target and no profiles, **when** the client starts,
   **then** the first frame states the one next action for getting
   connected instead of empty panes, and the connection picker remains the
   route when profiles exist.
5. **Given** catalogue loading is still in progress after connection,
   **when** the tree renders, **then** the loading state is stated in words
   and the editor is usable without waiting.

### User Story 5 - Themes the user owns (Priority: P2)

Someone chooses how Ignatius looks: they pick a built-in theme, follow the
terminal's own background automatically, or define their own theme in
configuration by overriding tokens over a built-in base. They switch themes
at runtime from the command palette and the change applies immediately. A
broken theme is refused before the terminal is taken, with the exact
problem named.

**Why this priority**: Ownership of appearance is a defining GUI property,
and the revised painted palette needs a configuration surface. It depends
on the painted-surface story landing first.

**Independent Test**: Load a configuration with a partial theme override
table and assert the resulting theme applies over its declared base; load
known-bad tables (unknown token, malformed colour, failing contrast) and
assert each is refused before the terminal is taken with exit code 3;
switch themes at runtime and assert the next frame uses the new theme with
state preserved.

**Acceptance Scenarios**:

1. **Given** a `[theme]` table naming a base with `extends` and overriding
   a subset of tokens, **when** the client starts, **then** the custom
   theme is selectable as `custom`, unset tokens come from the base, and
   `ui.theme = "custom"` selects it.
2. **Given** a `[theme]` table with an unknown token name, a malformed
   colour value, or a token pair that fails the documented contrast
   thresholds, **when** the client starts, **then** the problem is reported
   before the terminal is taken, the message names the token and the rule,
   and the exit code is 3.
3. **Given** `ui.theme = "auto"`, **when** the terminal answers the
   standard background query within 100 milliseconds, **then** the dark or
   light theme is chosen by the reported background luminance; **when** it
   does not answer or answers unreadably, **then** the client falls back to
   the dark theme and the diagnostics command says the query went
   unanswered rather than claiming detection.
4. **Given** a connected session, **when** the user chooses Switch theme in
   the command palette, **then** the next frame renders in the chosen theme
   with editor, results and selection state unchanged, and the choice lasts
   for the session without being written to configuration.
5. **Given** colour is disabled by any route, **when** any theme including
   a custom theme is active, **then** the render is modifier-only: a user
   theme cannot reintroduce colour under `NO_COLOR`.

### User Story 6 - The mouse is a first-class path (Priority: P2)

Someone uses the mouse the way they would in a GUI: clicking focuses a pane
and selects the item under the pointer, clicking a header sorts, a double
click opens detail, the wheel scrolls the pane under the pointer, and
dragging a split moves it. Every one of these has a keyboard equivalent,
and turning the mouse off removes no capability. The client says plainly
that terminal-native text selection moves behind Shift while capture is on.

**Why this priority**: "GUI-like" without a working pointer is a metaphor.
The verbs land after the grid and startup stories define the actions they
dispatch to.

**Independent Test**: Deliver synthetic mouse events for every documented
verb and assert each produces exactly the state the documented keyboard
equivalent produces, that unbound events change nothing, and that with
mouse disabled in configuration every capability remains reachable by
keyboard.

**Acceptance Scenarios**:

1. **Given** the default configuration, **when** the interactive client
   starts, **then** mouse capture is on, `ui.mouse = false` turns it off,
   and the help surface states that the terminal's own text selection is
   available with Shift while capture is on and that in-app copy is the
   existing opt-in route.
2. **Given** two visible panes, **when** the user left-clicks inside the
   unfocused one, **then** focus moves to it and the item under the pointer
   becomes the selection where the pane has selectable items.
3. **Given** any documented mouse verb, **when** it is performed, **then**
   the resulting model state is identical to its documented keyboard
   equivalent, and no mouse-only capability exists.
4. **Given** a right-button click anywhere, **when** it is received,
   **then** nothing changes: right-click is unbound in this feature and no
   context menu exists.
5. **Given** an overlay is open, **when** the user clicks inside it,
   **then** the click acts on the overlay's items; **when** the user clicks
   outside it, **then** nothing is dismissed or activated implicitly.
6. **Given** the wheel is scrolled over a pane without pointer focus,
   **then** the pane under the pointer scrolls and keyboard focus does not
   move.

### User Story 7 - Panes that move, and room to breathe (Priority: P2)

Someone shapes the layout to the task: they widen the sidebar, give the
results more height, temporarily zoom one pane to the whole screen and
restore it, and choose between a comfortable and a compact density. The
shape they set lasts for the session and never silently persists.

**Why this priority**: Fixed splits are why wide results feel cramped
today. Zoom is the cheapest fix for "the grid is too small" and both
lazygit and k9s ship it as a core verb.

**Independent Test**: Adjust each split by keyboard and by drag and assert
the clamped bounds and that the reducer's paging matches the resized panes;
zoom and restore a pane and assert every other pane returns to its prior
geometry; switch density and assert the documented spacing table applies.

**Acceptance Scenarios**:

1. **Given** the full layout, **when** the user presses the documented
   shrink and grow keys `Ctrl+K ,` and `Ctrl+K .`, **then** the focused
   pane's primary split moves by a documented step within documented
   minimums, and paging keys keep matching what is visible.
2. **Given** a pane border, **when** the user drags it, **then** the split
   follows within the same bounds as the keyboard route.
3. **Given** any focused pane, **when** the user presses the zoom key
   `Ctrl+K z`, **then** the pane fills the body area with its title stating
   the zoom; pressing it again restores the previous layout exactly.
4. **Given** `ui.density = "compact"`, **when** the client renders, **then**
   the documented compact spacing table applies everywhere; `"comfortable"`
   remains the default, and no third value exists.
5. **Given** any split or zoom state, **when** the client exits and starts
   again, **then** the layout is the configured default: session layout is
   never persisted.
6. **Given** the compact one-pane layout on a small terminal, **when**
   split or zoom keys are pressed, **then** the client explains that the
   layout is already single-pane rather than doing nothing.

### User Story 8 - Quiet chrome that speaks when it matters (Priority: P3)

Someone glances at the header and footer and reads exactly what matters
now: identity and location on the left, context on the right, badges only
where a fact deserves attention. Production stays loud. Local defaults stay
silent. Elapsed time appears when it is worth knowing, not always.

**Why this priority**: Polish that follows the owner's own prompt design.
It refines surfaces the P1 stories already repaint, so it comes after them.

**Independent Test**: Render sessions across environment classifications
and timing profiles, asserting the badge set matches the documented
domain-to-colour map, the sub-threshold renders omit the gated telemetry,
and every badge's meaning survives colour-off as reversed text with the
same words.

**Acceptance Scenarios**:

1. **Given** a production-classified connection, **when** the header
   renders, **then** the `PROD` capsule is present and loud as today;
   **given** a non-production local default, **then** no environment badge
   competes for attention.
2. **Given** the documented badge domains for environment, transaction
   state, transport and posture, **when** any badge renders, **then** its
   colour comes from the fixed domain-to-colour map, its meaning is carried
   by its word, and with colour off it renders as reversed text.
3. **Given** a statement that completed in under 200 milliseconds, **when**
   the header renders, **then** the elapsed figure is omitted; at or above
   the threshold it is shown as today.
4. **Given** any of these changes, **when** the footer renders, **then**
   the existing contextual hint rules and the six-hint cap are unchanged.

### User Story 9 - Overlays with a preview (Priority: P3)

Someone opens the command palette, history, saved queries or the connection
picker and works in a taller overlay with the input at the top and, for
entries that have real content behind them, a live preview pane on the
right: the definition behind an object, the SQL behind a saved query, the
safe summary behind a profile. Purposes with nothing to preview use the
full width.

**Why this priority**: A refinement of an overlay engine that already
works, modelled on the owner's own picker configuration. It layers onto the
painted-surface and palette infrastructure from the earlier stories.

**Independent Test**: Open each overlay purpose and assert the documented
geometry, that preview-bearing purposes render the selected entry's
preview from already-retained safe data, that selection movement updates
the preview without executing anything, and that no-preview purposes use
the full width.

**Acceptance Scenarios**:

1. **Given** any palette purpose, **when** it opens, **then** the overlay
   is 80 percent of the screen height bounded by the existing minimums, the
   input line sits at the top, and matches grow downward from it.
2. **Given** a purpose with previewable content, **when** an entry is
   selected, **then** a right-hand preview occupying 60 percent of the
   overlay width shows that entry's content from retained safe data,
   moving the selection updates it, and nothing is executed or fetched to
   render it.
3. **Given** a purpose without previewable content, **when** it opens,
   **then** the list uses the full overlay width and no empty preview box
   is drawn.
4. **Given** a narrow terminal below the documented preview minimum,
   **when** a preview-bearing purpose opens, **then** the preview is
   omitted, the list remains complete, and the entry's detail remains
   reachable through the existing routes.

## Edge Cases

- A paste while a masked prompt is open goes to that prompt as hidden input
  and never echoes; a paste while a confirmation is open is ignored with a
  notice, so pasted text cannot answer a confirmation by accident.
- A paste whose size is exactly the limit is accepted; one byte more is
  refused with the limit named.
- Bracketed paste unavailable in the terminal means pasted characters
  arrive as rapid key input exactly as today; the client does not guess.
- Quantization to 256 or 16 colours may make two distinct token colours
  coincide; every distinction still carried only by those colours must
  already be carried by words, so no meaning is lost. The contrast rules
  are enforced on the full-colour palette and documented as approximate
  below it.
- A terminal that answers the background query slowly answers into a
  discarded window: after the 100 millisecond deadline the reply is
  ignored wherever it arrives and must never leak into the input stream as
  typed characters.
- A custom theme that extends `high-contrast` inherits its stricter
  documented thresholds for the tokens it overrides.
- Theme switching while a query runs recolours the next frame without
  touching the running request, prompts or retained results.
- A click lands on a pane border, a blank cell or a decoration: the
  documented hit regions decide, and anything outside them does nothing.
- A drag that leaves the terminal window or ends off-pane completes at the
  last in-bounds position; a resize during a drag cancels the drag and
  re-clamps every split to the new geometry.
- Mouse events arriving with capture configured off are ignored entirely.
- A double click on two different cells is two single clicks; double-click
  timing is a documented fixed window, not a terminal guess.
- Splits clamp so no pane can be made smaller than its documented minimum,
  and the last visible column of the grid keeps its existing protection.
- Zoom on the compact single-pane layout, and split keys there, explain the
  state instead of silently doing nothing.
- A results range line for zero rows keeps the existing empty-result
  wording rather than inventing `rows 0-0`.
- Sorting by click on a header of the frozen column region behaves exactly
  like sorting that column by keyboard.
- The unconfigured first frame with no profiles names creating one or
  passing a target as the next action; it does not open an empty picker.
- A connecting step that legitimately does not apply, such as no credential
  step for trust-only connection routes, is skipped without being shown as
  completed work.

## Requirements

### Functional Requirements

- **FR-2501**: The interactive client MUST insert delivered paste events
  into the focused text surface at the caret as one undoable edit,
  normalising CRLF and CR to LF, without executing anything, and MUST
  refuse a paste above 1 MiB with the limit named.
- **FR-2502**: A paste while a non-text surface owns input MUST NOT be
  silently discarded: text-accepting prompts receive it, and every other
  surface states that the paste was ignored.
- **FR-2503**: At full-colour and 256-colour depths every rendered frame
  MUST paint the theme's background over the whole terminal area, with
  pane interiors and overlay interiors drawn from exactly three documented
  elevation levels of the active theme; at 16-colour depth and below,
  surfaces MUST remain unpainted and elevation MUST follow the colour-off
  rules.
- **FR-2504**: The client MUST render at four colour capability levels:
  full colour, 256-colour, 16-colour and no colour. Detection MUST use the
  standard truecolor environment convention with a documented fallback,
  MUST be overridable by flag and configuration, and `NO_COLOR` and
  `TERM=dumb` MUST continue to defeat every other setting.
- **FR-2505**: Colour quantization MUST be a documented, deterministic
  mapping from the full-colour palette; the contrast rules MUST be
  enforced on the full-colour palette, and reduced depths MUST be
  described as approximations that never carry meaning alone.
- **FR-2506**: The diagnostics command MUST report the detected colour
  depth, its detection source, and whether an override is in effect.
- **FR-2507**: The Results surface MUST state the visible row range and
  retained total in words in the documented shape, consistent with the
  existing retained, filtered and truncated counts.
- **FR-2508**: Grid sorting, cell selection, inspector opening, column
  scrolling and column resizing MUST each be reachable by both the
  documented keyboard route and the documented mouse verb, producing
  identical state; column width changes MUST respect documented bounds and
  remain local, reversible view state with no execution effect.
- **FR-2509**: The wheel MUST scroll the grid three rows per notch and
  MUST scroll columns with Shift held; wheel input MUST never change the
  selection's source identity.
- **FR-2510**: While a connection is being established the client MUST
  paint the complete shell and MUST name the current connecting step from
  the documented step list, with target, elapsed time and a visible cancel
  or quit route; failed steps MUST be named without overstating progress.
- **FR-2511**: The first connected frame MUST show the object tree's first
  schema level expanded, place focus in the editor, and show the run hint
  in the footer; catalogue loading MUST NOT block editor use.
- **FR-2512**: With no target and no profiles the first frame MUST state
  the single next action for getting connected; the existing picker rules
  for configured profiles MUST be unchanged.
- **FR-2513**: A `[theme]` configuration table MUST define a custom theme
  by overriding named tokens over a built-in base declared with `extends`,
  selectable as `custom`; unknown tokens, malformed colours and contrast
  failures MUST be refused before the terminal is taken, naming the token
  and rule, with exit code 3.
- **FR-2514**: `ui.theme` MUST accept `auto`, choosing dark or light from
  the terminal's answered background luminance within a 100 millisecond
  deadline and falling back to dark with the unanswered query reported by
  diagnostics, never presented as detection.
- **FR-2515**: The user MUST be able to switch the active theme at runtime
  from the command palette, applying from the next frame with session
  state preserved and without writing configuration.
- **FR-2516**: With colour disabled by any route, every theme including
  custom themes MUST render modifier-only, exactly as the existing
  colour-off rules require.
- **FR-2517**: Mouse capture MUST default to on for the interactive
  client, with `ui.mouse = false` as the documented opt-out; the help
  surface MUST state the Shift route to terminal-native selection and name
  the existing opt-in in-app copy route.
- **FR-2518**: Every documented mouse verb MUST dispatch to the same
  action its documented keyboard equivalent dispatches to; no capability
  may exist only through the mouse, and disabling the mouse MUST remove no
  capability.
- **FR-2519**: The documented mouse verb set for this feature is: click to
  focus and select, click a header to sort, double click to inspect, wheel
  to scroll the pane under the pointer, drag a split to resize panes, and
  drag a column edge to resize a column. Right-button input MUST remain
  unbound and every undocumented event MUST change nothing.
- **FR-2520**: Pane splits MUST be adjustable by the documented keys and
  by dragging within documented minimums; `Ctrl+K z` MUST zoom the focused
  pane to the body area and restore the exact prior layout on repeat; split
  and zoom state MUST be session-local and never persisted.
- **FR-2521**: `ui.density` MUST accept `comfortable` and `compact` only,
  selecting between two documented spacing tables applied consistently
  across every surface, defaulting to `comfortable`.
- **FR-2522**: Header badges MUST come from the documented badge domains
  with a fixed domain-to-colour map, always carrying their meaning in
  words; production MUST remain loud, the non-production local default
  MUST add no badge, and elapsed time MUST be shown only at or above
  200 milliseconds.
- **FR-2523**: Palette overlays MUST use the documented geometry: 80
  percent of screen height within existing minimums, input at the top,
  matches growing downward; purposes with previewable retained content MUST
  show a right-hand preview at 60 percent of the overlay width that
  updates with the selection without executing or fetching anything, and
  purposes without previewable content MUST use the full width.
- **FR-2524**: On terminals below the documented preview minimum the
  preview MUST be omitted with the list complete and the content reachable
  through existing routes; every new surface in this feature MUST keep the
  existing narrow, compact and too-small layout guarantees.

### Security and Compatibility Requirements

- **SEC-2501**: No new behaviour in this feature may execute SQL, write a
  file, perform network I/O other than the existing database connection,
  or record telemetry. Theme detection speaks only to the local terminal
  and its unanswered state is reported honestly.
- **SEC-2502**: Pasted text, previewed content and theme values MUST pass
  through the existing display-sanitising and redaction boundaries;
  connection previews MUST use the existing safe summaries and never a
  credential, and a paste into a masked prompt MUST never echo.
- **SEC-2503**: The existing accessible-meaning rules are unchanged and
  extended to every new surface: every colour, glyph, badge, stripe and
  elevation introduced here is reinforcement for words that carry the
  meaning alone in ASCII, no-colour, narrow and plain modes.
- **SEC-2504**: The keyboard-first rule is unchanged: every mouse verb has
  a keyboard route, focus stays visible, and no workflow assumes modal
  editor knowledge.
- **SEC-2505**: Plain mode, machine output, scripted `query`, history,
  export, parameter and cancellation contracts are outside this feature
  and MUST be byte-for-byte unchanged by it.
- **SEC-2506**: The terminal MUST be restored on every supported exit path
  with mouse capture and bracketed paste disabled in the existing reverse
  restoration order, on the existing platform routes.
- **SEC-2507**: Existing documented refusal conventions apply to every new
  configuration key: an invalid value is reported before the terminal is
  taken with exit code 3, and configuration keys added by this feature are
  refused by the same rule when malformed.
- **SEC-2508**: This feature amends documented design contracts rather
  than contradicting them: the mouse-capture default, the colour-depth
  override note, the painted-surface sections and the visual-identity
  wording are updated in the same change as the behaviour they describe,
  and the documentation parity checks continue to gate both directions.

## Key Entities

- **Elevation level**: One of three documented surface steps of the active
  theme: base, pane and overlay, painted rather than implied.
- **Colour depth**: The active colour capability level: full colour,
  256-colour, 16-colour or none; detected, overridable and reported.
- **Custom theme**: A validated set of token colours produced by overriding
  a built-in base; selectable, switchable and refused whole when invalid.
- **Hit region**: The documented mapping from a screen position to the
  interactive thing under it; the sole basis for every mouse verb.
- **Connecting step**: One named stage of establishing a session, shown
  with target and elapsed time, skipped silently when inapplicable.
- **Split state**: The session-local sidebar width, editor share, column
  widths and zoom fact that shape the layout without persisting.
- **Badge domain**: A documented fact family, environment, transaction,
  transport or posture, with one fixed colour and a word per state.
- **Overlay preview**: The retained, safe content shown beside a palette
  selection, rendered without execution or fetching.

## Success Criteria

### Measurable Outcomes

- **SC-2501**: One hundred percent of tested paste deliveries either
  insert exactly the pasted text as one undo step or state a refusal
  reason; zero are silently dropped.
- **SC-2502**: In every tested frame across all built-in themes, screen
  sizes and overlay states, zero cells render without a theme background
  while colour is available, and the three elevation levels appear only in
  their documented roles.
- **SC-2503**: Rendering the same model at all four colour depths and all
  three glyph tiers yields the same meaning-carrying words in every
  combination, extending the existing parity guarantee to colour depth,
  custom themes included via a valid and a hostile fixture theme.
- **SC-2504**: For every documented mouse verb, the mouse route and the
  keyboard route produce identical model state in one hundred percent of
  tested cases, and the full test set passes with the mouse disabled.
- **SC-2505**: In tested startup runs, every pre-connection frame paints
  the complete shell, every connecting frame names a documented step, and
  the first connected frame accepts productive editor input immediately.
- **SC-2506**: One hundred percent of tested invalid theme tables are
  refused before the terminal is taken with the token and rule named and
  exit code 3; zero invalid themes reach a rendered frame.
- **SC-2507**: Grid position wording matches the actual viewport in every
  tested scroll, sort, filter and resize state, and zero grid, layout,
  theme or overlay interactions in this feature emit an execution effect.
- **SC-2508**: A user can go from launch to their first productive
  keystroke in a configured session with zero orientation inputs, and the
  owner's originating friction journey, connect to a cloud database,
  orient, and work a result, is repeatable in Ignatius without reaching
  for another tool; the acceptance protocol records the observed sample
  size honestly.

## Assumptions

- The observed-friction evidence is one owner session on 2026-09-17; it is
  recorded as sample size one and this specification does not claim user
  research beyond it.
- This feature is interactive-only. Plain mode already has its own
  paste-free, line-oriented contract, and machine output is unaffected.
- The existing three built-in themes remain the base set; their palettes
  may be revised for painted surfaces under the existing contrast tests,
  informed by the owner's palette without copying another product's
  identity, and the light theme remains designed rather than inverted.
- `ui.theme` keeps `dark` as its default; `auto` is opt-in because a
  truthful fallback is preferred over a wrong guess on terminals that do
  not answer the background query.
- Mouse capture defaulting to on is a deliberate amendment of the current
  documented default, justified by first-class mouse support; the existing
  design rationale, native text selection, is preserved through the Shift
  route and the documented opt-out.
- Right-click context menus, session-persisted layout, theme plugins,
  images, animation beyond the existing spinner and meter, and any
  non-database network call are explicitly out of scope.
- Double-click timing, drag thresholds, spacing tables, split minimums,
  the domain-to-colour map and the quantization tables are pinned in this
  feature's contracts during planning so implementation never guesses.
- Windows mouse and paste input arrive by the platform's console route
  rather than terminal escape sequences; the existing cross-platform
  restoration and input rules are the foundation and remain a release
  criterion.
