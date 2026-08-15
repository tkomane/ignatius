# Feature Specification: Writing the SQL

**Feature Branch**: `005-sql-editing`

**Created**: 2026-08-16

**Status**: Implemented, verified on macOS by unit and layout tests.

**Numbering note**: files are numbered in build order. This delivers the "real
editor" half of roadmap Feature 003, *Delightful query loop*; the history half,
with its privacy controls, is still to come.

**Scope note**: this is editing and movement, not syntax highlighting. Colouring
SQL is a separate slice with its own risks, and an editor that highlights
beautifully but cannot move the cursor up a line is not an editor.

## User Scenarios & Testing

### User Story 1 - Write a statement longer than one line (Priority: P1)

Someone writes a query over eight lines, notices a mistake on line two, and
wants to go back to it.

**Why this priority**: until now the cursor could only move left and right. A
buffer you cannot navigate is a text field, and nobody writes real SQL in one.

**Independent Test**: type a multi-line statement, move up two lines, fix a
word, and return to the end.

**Acceptance Scenarios**:

1. **Given** a multi-line buffer, **When** the user moves up or down, **Then**
   the cursor changes line and keeps the column it started from where the line
   is long enough.
2. **Given** a cursor on the first line, **When** the user moves up, **Then** it
   goes to the start of the buffer rather than doing nothing.
3. **Given** a buffer longer than the pane, **When** the cursor moves beyond the
   visible lines, **Then** the window follows it and the line numbers keep
   counting from the buffer.
4. **Given** any position, **When** the user presses the page keys, **Then** the
   cursor moves by what the pane can actually show.

### User Story 2 - Take back a mistake (Priority: P1)

Someone deletes a word, or loads a file over their work, and wants it back.

**Why this priority**: recoverability is a principle of this product, and a
buffer with no undo is the one place it was not honoured.

**Independent Test**: type a sentence, delete a word, undo twice, redo once.

**Acceptance Scenarios**:

1. **Given** typed text, **When** the user undoes, **Then** a word and the space
   after it come back as one step, not one character at a time.
2. **Given** an undo, **When** the user redoes, **Then** the change returns.
3. **Given** an undo followed by new typing, **When** the user tries to redo,
   **Then** nothing happens, because that future no longer exists.
4. **Given** a buffer replaced by loaded text, **When** the user undoes, **Then**
   the previous buffer returns.

### User Story 3 - Move and delete by word (Priority: P2)

Editing `orders.customer_id` means moving over parts of it, not characters.

**Acceptance Scenarios**:

1. **Given** an identifier with punctuation in it, **When** the user moves by
   word, **Then** the identifier parts and the punctuation are separate stops.
2. **Given** a cursor after a word, **When** the user deletes by word, **Then**
   the whole word goes in one step, undoable as one step.

### Edge Cases

- A buffer with multi-byte characters, where no movement may split one.
- An empty buffer, where every movement key must be a no-op rather than a panic.
- A long session, where the undo history must not grow without bound.
- A line of only whitespace, and a new line taken from the middle of an indent.

## Requirements

### Functional

- **FR-501**: The editor MUST support vertical movement with a remembered goal
  column.
- **FR-502**: The editor MUST support line start and end, buffer start and end,
  word movement in both directions, and movement by a screenful.
- **FR-503**: The editor MUST support deleting forwards, backwards, and by word.
- **FR-504**: The editor MUST support undo and redo, with a bounded history.
- **FR-505**: A new change MUST discard the redo path.
- **FR-506**: A line break MUST keep the indentation of the line it left, taking
  only the indentation before the cursor.
- **FR-507**: The visible window MUST follow the cursor and MUST NOT be stored,
  so there is one source of truth for where the cursor is.

### User experience

- **UX-501**: One undo step is a word and the space that follows it, so undo
  never leaves a stranded space or takes back a whole paragraph.
- **UX-502**: Word movement MUST treat identifier characters and punctuation as
  separate words.
- **UX-503**: Editing keys MUST do nothing in panes that are not the editor,
  rather than doing something that looks like an edit somewhere else.
- **UX-504**: Word movement MUST be bound with both `Ctrl` and `Alt`, because
  terminals disagree about which one that gesture sends.

### Security

- **SEC-501**: No editing operation may split a character or index outside the
  buffer.

## Success Criteria

- **SC-501**: An eight-line statement can be written, corrected and run without
  the mouse and without retyping a line.
- **SC-502**: A deleted word can be recovered in one keystroke.
- **SC-503**: A two-hundred-line buffer can be navigated end to end, and the
  line numbers are right at both ends.

## Assumptions

- Undo keeps whole snapshots. The buffer is a statement someone typed, so the
  memory is small and the implementation is obviously correct, which matters
  more here than the saving a diff would give.
- There is no selection and no clipboard yet. Both belong with the copy decision
  recorded in `specs/004-result-inspection/spec.md`.
