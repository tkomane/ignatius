# Feature Specification: Errors that point at the problem

**Feature Branch**: `013-errors-at-cursor`

**Created**: 2026-09-04

**Status**: Implemented locally; locked verifier green with Unix-socket skip

**Input**: User description: "Continue the roadmap with maximal release maturity and a first-class, GUI-like terminal UX for errors, drawing on Neovim, Codex CLI, Claude Code CLI, and Warp."

## User Scenarios & Testing

### User Story 1 - Put the cursor on the server's answer (Priority: P1)

When a statement fails and PostgreSQL reports a character position, the user
should arrive at the problem rather than having to count characters from an
error message. The editor keeps the submitted SQL unchanged, places its caret
at the reported character, and visibly marks the smallest identifiable token
containing that character. The error surface also states the line and column in
words so the location remains useful without colour.

**Why this priority**: This removes the repeated, high-frequency cost of turning
a server position into an edit. It is the smallest error interaction that makes
the client feel like an editor rather than a query launcher.

**Independent Test**: Run a deliberately malformed statement in the TUI with a
multi-line buffer, confirm that the editor caret and marker move to the server
position, and confirm that the buffer and undo history are unchanged.

**Acceptance Scenarios**:

1. **Given** a submitted statement whose server error has a valid position,
   **When** the execution finishes with that error, **Then** the editor caret
     is at the corresponding character in the failing statement, the containing
   token is marked, the editor is the active editing surface, and the error
   text states the one-based line and column.
2. **Given** a buffer containing more than one statement, **When** the later
   statement fails with a position, **Then** the location is mapped into that
   statement rather than into the beginning of the buffer or an earlier
   statement.
3. **Given** an error position that is absent, out of range, or cannot be
   mapped honestly, **When** the execution finishes, **Then** the existing
   cursor and buffer remain unchanged and the error says that an editor
   location is unavailable instead of highlighting an unrelated character.
4. **Given** the client has positioned the caret for an error, **When** the user
   edits the buffer, **Then** the old marker is removed or explicitly labelled
   as belonging to the previous submission and is never presented as a marker
   for the changed SQL.

### User Story 2 - Understand which database object rejected the write (Priority: P2)

When PostgreSQL reports a constraint failure, the user should see the relevant
constraint, schema, relation, and column context in the primary error surface
when those facts are available. A current schema snapshot may make the object
name and column relationship more useful, but an unavailable or stale snapshot
must not turn into a guessed explanation.

**Why this priority**: Constraint failures are common during data work and the
server already supplies facts that are easy to lose inside collapsed technical
detail. Making the object context prominent shortens the path from failure to
the right edit while preserving PostgreSQL's authority.

**Independent Test**: Trigger unique, foreign-key, check, and not-null failures
against the disposable PostgreSQL fixture and verify that the TUI, plain mode,
and verbose diagnostics expose the server-supplied object context without
leaking credentials or inventing missing fields.

**Acceptance Scenarios**:

1. **Given** a constraint error with structured schema, table, column, or
   constraint fields, **When** the diagnostic is shown, **Then** those facts
   are readable before the expandable technical section and are clearly labelled
   as database object context.
2. **Given** a matching current schema snapshot, **When** the diagnostic names a
   relation and column, **Then** the client presents the safely quoted object
   and column names and does not describe an object that the snapshot cannot
   match.
3. **Given** a missing, stale, unreadable, or incomplete schema snapshot,
   **When** the constraint error is shown, **Then** the server-supplied fields
   remain available, the limitation is stated, and no catalogue-derived claim
   is made.

### User Story 3 - Keep the same answer in every interface (Priority: P3)

A person using plain mode or a script should receive the same actionable error
meaning as a TUI user. The plain diagnostic includes a line-oriented source
excerpt and caret when a location can be mapped, while JSON keeps the stable
machine-readable position and structured technical fields. Retrying remains an
explicit user action; the client never replays a failed statement because it
found a location.

**Why this priority**: A terminal-native tool is used both interactively and in
automation. Consistent, truthful output prevents a polished TUI from creating a
second support and scripting contract.

**Independent Test**: Run the same failing SQL in plain mode and JSON mode,
compare the reported statement, position, and object context with the TUI
diagnostic, and verify that no second execution occurs without an explicit run
request.

**Acceptance Scenarios**:

1. **Given** a position that maps to the submitted statement, **When** plain
   mode renders the failure, **Then** it prints the statement number, line and
   column, and a readable source line with a caret, using no terminal control
   sequences.
2. **Given** a failed statement, **When** JSON output is requested, **Then** the
   numeric server position and existing structured diagnostic fields remain
   present and their meaning is not changed by presentation mode.
3. **Given** any error location, **When** the diagnostic is displayed, **Then**
   no retry, SQL rewrite, connection downgrade, or additional write is started
   automatically.

## Edge Cases

- PostgreSQL positions are one-based characters, not UTF-8 byte offsets; a
  position beside accented, CJK, or other multi-byte characters must never split
  a character or move the caret into invalid text.
- Tabs, blank lines, trailing newlines, and a caret at the end of a line must
  render with a stable line and column.
- Semicolons inside strings, comments, quoted identifiers, and dollar-quoted
  bodies must not shift the failing statement chosen for a marker.
- A statement can fail after earlier statements in the same buffer have already
  produced real results; the error location must point at the failed statement
  while preserving those results and the execution status.
- A user can edit while an async execution is in flight; a late error must not
  move the caret or mark text in a different buffer.
- PostgreSQL may report an internal or rewritten-query position, or no position
  for a permission, transaction, connection, or cancellation failure. The client
  must report the limitation rather than approximate it.
- A constraint can provide a name without a table or a table without a column;
  each absent fact remains absent in the primary summary and technical detail.
- A schema snapshot can be loading, unavailable, stale, or restricted by the
  connected role. Error display must remain useful in every state.
- Narrow, ASCII, no-colour, high-contrast, and reduced-motion presentations
  must retain the location and constraint meaning in explicit text.

## Requirements

### Functional Requirements

- **FR-001**: The client MUST preserve the exact submitted SQL, including its
  whitespace and UTF-8 text, when it records or presents an error location.
- **FR-002**: The client MUST associate a server-reported position with the
  statement that actually failed, including when the submitted buffer contains
  multiple statements.
- **FR-003**: The client MUST convert a valid one-based character position into
  a safe editor location without treating a character position as a byte
  position.
- **FR-004**: The TUI MUST place the editor caret at a valid mapped location and
  mark the smallest source token that can be identified there without changing
  the buffer or adding an undo step.
- **FR-005**: The TUI MUST state the error's statement number, line, and column
  in words, and the location marker MUST remain distinguishable when colour,
  icons, and Unicode glyphs are unavailable.
- **FR-006**: When a valid location is mapped for a query failure, the TUI MUST
  make the editor the active surface while keeping the error explanation and
  recovery action visible.
- **FR-007**: When a location is missing or cannot be mapped, the client MUST
  leave the existing caret unchanged and MUST state why it cannot point at the
  buffer.
- **FR-008**: Editing the buffer after an error MUST invalidate the old editor
  marker so a diagnostic for an earlier submission cannot be mistaken for a
  diagnostic for the current text.
- **FR-009**: The client MUST NOT retry, rewrite, downgrade transport security,
  or execute another statement automatically as part of locating or displaying
  an error.
- **FR-010**: Plain mode MUST render a mapped error as line-oriented text with a
  statement number, line and column, and a safe source excerpt with a caret when
  the excerpt can be derived honestly.
- **FR-011**: JSON diagnostics MUST retain the numeric server position and the
  existing structured technical fields, with no credential, token, SQL value,
  or result data added solely for presentation.
- **FR-012**: Constraint diagnostics MUST promote available schema, table,
  column, and constraint facts into a clearly labelled primary context without
  inventing fields PostgreSQL did not supply.
- **FR-013**: Catalogue enrichment MUST be used only when a current snapshot
  matches the server-supplied object; otherwise the server fields and the
  snapshot limitation MUST remain the source of truth.
- **FR-014**: All new user-facing and logged diagnostic text MUST pass through
  the existing redaction and terminal-sanitisation boundaries.
- **FR-015**: The feature MUST retain the existing error dismissal, technical
  expansion, cancellation, transaction recovery, and stale-job protections.

### Key Entities

- **Submitted statement context**: The exact text, statement ordinal, and source
  span that were sent to PostgreSQL for one execution.
- **Error location**: A validated editor caret, token span, one-based line and
  column, and the statement ordinal derived from a server position.
- **Constraint context**: Server-supplied schema, relation, column, and
  constraint facts, optionally related to a matching schema snapshot.

## Success Criteria

### Measurable Outcomes

- **SC-001**: 100% of unit cases covering single statements, multi-statement
  buffers, UTF-8 text, tabs, comments, quoted strings, and invalid positions
  either map to the correct character boundary or explicitly return no mapping.
- **SC-002**: A TUI error-location acceptance test demonstrates the caret,
  token marker, line/column wording, unchanged SQL, unchanged undo history, and
  editor focus after a server-style failure.
- **SC-003**: Disposable PostgreSQL integration evidence covers a syntax error
  and at least one constraint failure, with the reported SQLSTATE and structured
  object fields verified separately from renderer tests.
- **SC-004**: Plain, no-colour, ASCII, narrow, and JSON checks preserve the same
  error meaning without terminal control sequences or credential values.
- **SC-005**: The locked repository verifier passes every available gate; any
  unavailable PostgreSQL transport or platform gate is recorded as an explicit
  skip or evidence gap rather than a pass.

## Assumptions

- PostgreSQL remains the authoritative source for SQLSTATE, position, and
  structured error fields; the client does not attempt to become a SQL parser or
  a replacement for server validation.
- The existing statement boundary lexer and schema completion snapshot remain
  the available local context sources.
- A user may edit while an async query is running, so matching the submitted
  text is required before moving or marking the current buffer.
- The current diagnostics, redaction, plain-mode, JSON, TUI, and PostgreSQL
  integration test contracts are extended rather than replaced.
- Repair SQL, automatic query linting, full semantic type checking, and automatic
  retries are out of scope for this feature.
