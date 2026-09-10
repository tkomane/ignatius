# Feature Specification: A result grid you can work

**Feature Branch**: `014-workable-result-grid`

**Created**: 2026-09-04

**Status**: Implemented locally; task list complete and focused/full plain/TLS
PostgreSQL evidence recorded in `docs/status.md`. Unix-socket and platform
hand-terminal evidence remain separate gaps.

**Input**: User description: "Continue the roadmap with maximal release
maturity and a first-class, GUI-like terminal UX, drawing on Neovim, Codex
CLI, Claude Code CLI, and Warp."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Read and compare returned rows (Priority: P1)

When a query returns more rows than fit on screen, the user should be able to
sort the rows already retained by a selected column, move through the ordered
view, and keep enough context to understand that the order is local to this
result. Sorting should feel immediate and should never rewrite or run the SQL
again.

**Why this priority**: A result that can only be read in server order is a
printout, not a work surface. Local ordering is the highest-value interaction
that turns the existing bounded result into something people can investigate.

**Independent Test**: Run a query with mixed text, numeric-looking values and
NULLs, sort by the selected column in both directions, and verify the selected
record, row labels, NULL placement, and retained-row counts remain truthful.

**Acceptance Scenarios**:

1. **Given** a result with retained rows, **When** the user chooses sort for the
   selected column, **Then** the grid orders retained rows locally, labels the
   direction, and does not emit another execution request.
2. **Given** a sorted result, **When** the user chooses sort again, **Then** the
   direction changes predictably and a third choice returns to the original
   server order.
3. **Given** a selected row before sorting, **When** sorting changes its screen
   position, **Then** the same source row remains selected rather than a
   different record being silently chosen.
4. **Given** a result that was truncated by the retention limit, **When** it is
   sorted, **Then** the UI says that only retained rows were ordered and does
   not imply that unseen rows were included.

### User Story 2 - Shape a wide result for the task (Priority: P1)

When a result has many columns, the user should be able to choose which columns
are visible, adjust the width of the selected column, see available PostgreSQL
type labels, and freeze the first visible identity column while moving across
the rest. The underlying rows and values remain unchanged, and the view never
loses the selected column without saying why.

**Why this priority**: Wide tables are where graphical clients feel effortless
and terminal clients usually become a wall of abbreviations. Local view controls
bring the most useful part of a GUI grid into the terminal without adding a
second query path.

**Independent Test**: Run a result with at least six columns, open the grid
controls, hide and restore columns, widen and narrow one, toggle type labels,
freeze the first column, and navigate across the visible window at wide and
narrow terminal sizes.

**Acceptance Scenarios**:

1. **Given** a result with several columns, **When** the column chooser opens,
   **Then** every column is listed with a clear shown or hidden state and its
   available type label, and Enter toggles only that column.
2. **Given** multiple visible columns, **When** the user hides the selected
   column, **Then** selection moves to a visible neighbour and the grid states
   how many columns are shown; the final visible column cannot be hidden.
3. **Given** a selected visible column, **When** the user widens or narrows it,
   **Then** its width changes within visible minimum and maximum bounds, the
   change is local to the view, and other columns remain usable.
4. **Given** type information is available for a result column, **When** type
   labels are enabled, **Then** the label appears with the header; when it is
   unavailable, the UI says so instead of inferring a server type from text.
5. **Given** a wide result and a selected column beyond the first, **When** the
   user enables a frozen first column, **Then** the first column remains in the
   viewport while the selected column and its neighbours scroll horizontally.
6. **Given** a narrow, ASCII, or no-colour presentation, **When** any grid
   control is active, **Then** words, selection, shown or hidden state, sort
   direction, and frozen-column meaning remain explicit without relying on
   colour or Unicode.

### User Story 3 - Discover and recover grid actions (Priority: P2)

A user who does not know the key bindings should be able to discover result
controls from the command palette and a contextual grid-controls surface. A
reset action should return the current result to its readable default view, and
running a new query should not carry view assumptions into an unrelated result.

**Why this priority**: The product cannot be GUI-like if its useful controls are
hidden behind remembered keystrokes. The palette and visible state make the
feature learnable while keeping keyboard speed for repeat users.

**Independent Test**: Focus the result pane, open the controls through the
palette and its advertised chord, apply each control, reset the view, then run a
new query and confirm per-result changes are cleared while stable preferences
remain understandable.

**Acceptance Scenarios**:

1. **Given** the result pane has focus, **When** the user opens the grid
   controls, **Then** the available actions are searchable, named in plain
   language, and show the currently selected column and active state where
   relevant.
2. **Given** a changed grid view, **When** the user chooses reset, **Then** the
   result returns to its automatic widths, original column visibility, and
   server row order without changing the retained data.
3. **Given** a result view has been customised, **When** a new execution
   finishes, **Then** view state tied to the previous result is cleared and the
   new result starts with a readable default layout.
4. **Given** the current result has no rows or no columns, **When** a grid
   control is chosen, **Then** the client keeps the result explanation visible
   and does not open a misleading empty control state.

### Edge Cases

- Sorting a column containing SQL NULL, empty strings, hostile control
  characters, wide characters, duplicate values, or values too large for the
  display must remain deterministic and safe.
- A result can have fewer cells in a retained row than headers due to an
  unusual server or test shape; missing cells remain visibly absent and never
  panic the renderer.
- A user can hide all but one column, attempt to hide the last one, and then
  restore columns through the chooser.
- A result can have duplicate column labels; controls must address the source
  column position, not accidentally toggle every same-named column.
- A result can have no server type metadata, partial type metadata, or a type
  name containing terminal controls; the UI must state availability and
  sanitize it.
- A user can sort, resize, or hide columns while a filter is active; the
  matching count and retained-row count remain correct.
- A narrow terminal may not fit the frozen column together with another column;
  the UI must keep the selected column reachable and state the horizontal view.
- A result with more columns than the configured row cap is still bounded by
  the existing row cap; no control may trigger a page query or automatic rerun.
- The grid preference state must not affect plain output, JSON output, exports,
  history contents, or the SQL editor buffer.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The result grid MUST provide a local sort action for the selected
  column with explicit ascending, descending, and original-order states.
- **FR-002**: Sorting MUST operate only on retained rows, preserve stable order
  for equal values, keep SQL NULL placement deterministic, and state the scope
  when the result is truncated.
- **FR-003**: Sorting MUST preserve the selected source record when its screen
  position changes and MUST NOT execute, retry, rewrite, or alter the submitted
  SQL.
- **FR-004**: The result grid MUST provide a searchable column chooser that
  lists every source column exactly once, including duplicate labels by source
  position, with shown or hidden state.
- **FR-005**: The grid MUST prevent hiding the last visible column and MUST move
  selection to a visible column when the selected column is hidden.
- **FR-006**: The grid MUST provide bounded local width controls for the
  selected column and a reset action that restores automatic widths.
- **FR-007**: The grid MUST preserve source column order unless the user is
  navigating a horizontal viewport; hiding a column MUST NOT reorder the
  remaining columns.
- **FR-008**: The grid MUST display available PostgreSQL type labels separately
  from values, and MUST label missing or partial type information as
  unavailable rather than guessing from a rendered value.
- **FR-009**: The grid MUST support freezing the first visible column while
  horizontally navigating other visible columns, with explicit state wording.
- **FR-010**: Selection, inspection, expanded-row display, filtering, and
  visible-row counts MUST continue to refer to the current source result even
  when sorting or hiding columns changes the presentation.
- **FR-011**: Grid controls MUST be reachable through the command palette and a
  visible contextual controls surface when the result pane is focused.
- **FR-012**: A new execution MUST clear view state tied to the previous result;
  reset MUST restore the current result without discarding rows.
- **FR-013**: Grid state MUST remain in memory only and MUST NOT alter plain,
  JSON, streaming export, history, diagnostic, or SQL-editor behaviour.
- **FR-014**: All server-derived column labels, type labels, and cell text MUST
  pass through the existing terminal-sanitisation boundary before rendering.
- **FR-015**: Empty, truncated, failed, cancelled, connection-lost, narrow,
  ASCII, no-colour, and reduced-motion states MUST retain explicit result
  status and recovery meaning.

### Key Entities *(include if feature involves data)*

- **Result column descriptor**: The source position, server column label,
  optional PostgreSQL type label, visibility state, and local width used by the
  result view.
- **Result view state**: The selected source row and column, local sort
  direction, visible-column set, width adjustments, type-label preference,
  frozen-column preference, and horizontal viewport for one retained result.
- **Retained row order**: The mapping between displayed row positions and the
  original retained row indices, preserving source row identity through filter
  and sort operations.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A user can sort, resize, hide, restore, and reset a six-column
  result without a second database execution and without changing the SQL,
  retained values, or export payload.
- **SC-002**: On an 80-column terminal, a selected column remains reachable and
  its header, type state, and at least one value remain readable while the
  frozen first column stays visible when enabled.
- **SC-003**: 100% of unit and reducer cases for ascending, descending,
  original-order, NULL, duplicate, filtered, truncated, hidden-column, and
  selection-preservation states produce deterministic results or an explicit
  unavailable state.
- **SC-004**: Every grid control is discoverable by name through the palette or
  contextual controls, and every state remains understandable with colour and
  Unicode disabled.
- **SC-005**: Hostile column names, type labels, and values produce no terminal
  control sequences in full, compact, narrow, plain, or no-colour rendering.
- **SC-006**: The locked repository verifier passes all available gates, while
  unavailable transport and platform checks remain separately recorded as
  skips or evidence gaps.

## Assumptions

- The existing bounded result retention remains the only source of rows; this
  feature does not add pagination, server-side sorting, or a rerun path.
- Sorting is a presentation operation over retained values. It uses stable,
  deterministic comparison and does not claim to reproduce a PostgreSQL
  `ORDER BY` collation or query semantics.
- PostgreSQL type metadata is shown only when the execution path exposes it;
  missing metadata is a visible limitation rather than a client-side type
  inference.
- View state is intentionally ephemeral and scoped to the current retained
  result. The type-label and frozen-column preferences may persist for the
  session, while sort, hidden columns, widths, and horizontal position reset
  for a new result.
- Plain and scripted interfaces retain their existing contracts. Grid controls
  improve only the interactive TUI and never move data out of the process.
- No clipboard, OSC 52, cell editing, query rewriting, or automatic refresh is
  introduced by this feature.
