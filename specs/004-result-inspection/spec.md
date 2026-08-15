# Feature Specification: Reading a result

**Feature Branch**: `004-result-inspection`

**Created**: 2026-08-15

**Status**: Implemented, verified on macOS. The reducer rules and both views
are covered by unit and layout tests, including hostile values, wide characters
and a selection that outlived its result.

**Scope note**: the roadmap places four things under result work: an expanded
row view, a cell inspector, result filtering, and copying to the clipboard.
This specification covers the first two. Filtering and copying are deferred and
recorded in `docs/status.md`. Copying is deferred deliberately rather than for
time: putting database contents on the system clipboard, or writing them into
the terminal with OSC 52, moves data out of this process, and that decision
belongs in an ADR rather than in a keybinding.

## User Scenarios & Testing

### User Story 1 - Read one row when it has too many columns to fit (Priority: P1)

A row with thirty columns is unreadable in a grid on an eighty-column terminal.
The user wants that one row laid out down the screen, one column per line, the
way `psql`'s `\x` does it.

**Why this priority**: it is the single most common reason someone leaves a
terminal client for a GUI. A wide row is not an edge case; it is what real
tables look like.

**Independent Test**: run a query returning a row wider than the terminal,
toggle the expanded view, and read every column name and value without
scrolling sideways.

**Acceptance Scenarios**:

1. **Given** a result with more columns than fit, **When** the expanded view is
   toggled, **Then** the selected row is shown as one `column: value` line per
   column, with the names aligned.
2. **Given** the expanded view is on, **When** the selection moves to another
   row, **Then** the view follows the selection.
3. **Given** the expanded view is on, **When** a new query runs, **Then** the
   view stays on, because it is a preference and not a state of the result.
4. **Given** a result with no rows, **When** the expanded view is toggled,
   **Then** it says there is no row to show rather than rendering an empty frame.

### User Story 2 - See a value in full, exactly as it is (Priority: P1)

A cell holds a 4 KB JSON document, or a value with a trailing space, or a NULL.
The grid can only ever show a fragment. The user wants the whole value, and
wants to know what kind of nothing they are looking at.

**Why this priority**: the grid truncates, and a truncated value that looks
complete is the failure this whole project is written against. The inspector is
where the client stops abbreviating.

**Independent Test**: select a cell holding a value longer than the pane, open
the inspector, and scroll to the end of it.

**Acceptance Scenarios**:

1. **Given** a selected cell, **When** the inspector is opened, **Then** it shows
   the column name, what the value is, and the value in full, wrapped to the
   pane rather than cut off.
2. **Given** the inspector is open on a value taller than the pane, **When** the
   user moves down, **Then** the value scrolls and the position is stated.
3. **Given** the inspector is open, **When** the user moves left or right,
   **Then** it moves to the neighbouring column of the same row and stays open,
   so a row can be read one column at a time.
4. **Given** the inspector is open, **When** a new execution finishes, **Then**
   the inspector closes, because the cell it was showing no longer exists.
5. **Given** a cell holding SQL NULL, an empty string, or the four-letter text
   `NULL`, **When** each is inspected, **Then** the description distinguishes
   them in words.

### Edge Cases

- A value containing escape sequences, which must never reach the terminal.
- A value of wide characters, which must wrap without splitting one.
- A value containing real newlines, whose line count is a fact about the data
  and not about how the pane wrapped it.
- A pane one column wide.
- Opening the inspector on a result that has no rows, or no columns.
- A selection that survives from a larger previous result.

## Requirements

### Functional

- **FR-401**: The result pane MUST offer an expanded view showing one row as one
  line per column.
- **FR-402**: The expanded view MUST follow the selected row.
- **FR-403**: The inspector MUST show the selected cell's value in full.
- **FR-404**: The inspector MUST derive its cell from the current selection and
  the current result rather than holding a copy of either.
- **FR-405**: The inspector MUST close when the result it was reading is
  replaced.
- **FR-406**: The inspector MUST allow movement between columns of the same row
  without closing.
- **FR-407**: The inspector MUST state its scroll position when the value does
  not fit.

### User experience

- **UX-401**: The description MUST distinguish SQL NULL, an empty string, and
  text that reads `NULL`, in words.
- **UX-402**: Character counts MUST describe the value as stored. The position
  line describes the pane, and MUST be worded so it cannot be read as a property
  of the value.
- **UX-403**: Both views MUST read correctly with colour off and in ASCII.
- **UX-404**: The inspector MUST be a mode that is peeled in the documented
  order, and typing in it MUST NOT reach the editor.

### Security

- **SEC-401**: Values MUST be escaped for display before wrapping, so no control
  sequence can reach the terminal from either view.
- **SEC-402**: Neither view may write a value anywhere outside this process.

## Success Criteria

- **SC-401**: A thirty-column row is readable on an eighty-column terminal
  without horizontal scrolling.
- **SC-402**: A value of 4 KB can be read to its end.
- **SC-403**: A value crafted to clear the screen is displayed as text in both
  views.
- **SC-404**: NULL, empty string and the text `NULL` are told apart by reading
  the inspector, not by knowing the convention.

## Assumptions

- The inspector reads what is already in memory. It never re-queries, so a value
  beyond the row cap is not inspectable; the cap is stated where it applies.
- A newline in a value becomes a line break in the inspector rather than the
  text `\n` the grid shows, because a document read as one endless line is not
  read at all. Every other control character is still escaped, and the split
  happens before escaping, so no newline survives into what is drawn.
- Wrapping is by display width, not by word. A word-wrapped SQL value would be
  a lie about where its line breaks are.
