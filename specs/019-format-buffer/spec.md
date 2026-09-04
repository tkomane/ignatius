# Feature Specification: Format the SQL buffer

**Feature Branch**: `019-format-buffer`

**Created**: 2026-09-04

**Status**: Implemented locally; locked verifier green with Unix-socket skip;
manual terminal checks pending

**Input**: User description: "Continue the roadmap toward maximal release maturity and make the terminal UX/UI as first class and GUI-like as possible."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Make a query readable in one action (Priority: P1)

Someone has a valid or partly edited SQL buffer that is difficult to scan. They
choose Format SQL from the command palette, press the visible format shortcut,
or use the format chord. The whole buffer becomes a deterministic, readable
layout without contacting PostgreSQL or changing what the SQL means.

**Why this priority**: A graphical editor makes structure visible while a
person is writing. Formatting is the smallest local feature that gives the
terminal editor that same immediate feedback without requiring a database or a
remembered style guide.

**Independent Test**: Put a compact multi-clause query in the editor, invoke
format, and verify that clauses and lists are laid out, the buffer is still
editable, and no execution or other effect is produced. Formatting the result
again produces exactly the same text.

**Acceptance Scenarios**:

1. **Given** a non-empty SQL buffer, **When** Format SQL is chosen, **Then** the
   buffer is replaced by the deterministic formatted text and the footer states
   what changed.
2. **Given** an already formatted buffer, **When** Format SQL is chosen,
   **Then** the text, cursor, revision, and modified state remain unchanged and
   the interface says that it is already formatted.
3. **Given** a formatting request, **When** it is applied, **Then** no query,
   metadata load, history write, file write, or network operation occurs.

### User Story 2 - Keep my place and my escape hatch (Priority: P1)

Someone formats while editing a long statement and continues from the same
logical SQL token. They can undo the complete formatting change once and redo it
once, without losing earlier edits or receiving a stale server error marker.

**Why this priority**: Formatting is only helpful when it feels reversible. A
cursor that jumps to an unrelated part of the query or an undo stack that needs
many presses makes a local convenience feel unsafe.

**Independent Test**: Place the cursor inside a quoted and an unquoted token,
format a multi-line buffer, and verify that the cursor remains inside the same
token. Undo restores the exact original text and cursor in one step; redo
restores the formatted text.

**Acceptance Scenarios**:

1. **Given** a cursor inside a token or on whitespace adjacent to one, **When**
   formatting changes surrounding layout, **Then** the cursor maps to the same
   token or nearest logical position without splitting UTF-8.
2. **Given** a formatted buffer, **When** Undo is pressed once, **Then** the
   exact pre-format text, cursor, and editor history position return.
3. **Given** a visible error location, **When** formatting changes the buffer,
   **Then** the old location is cleared or labelled as belonging to the previous
   submission rather than being silently applied to new text.

### User Story 3 - Preserve meaning in every terminal tier (Priority: P1)

Someone uses a narrow, ASCII-only, no-colour, reduced-motion, screen-reader, or
plain terminal workflow. Formatting retains literal, quoted-identifier, dollar
body, and comment contents exactly, explains refusal when the input is unsafe to
reformat, and exposes the same capability through plain mode as `\\format`.

**Why this priority**: The product promise is not a pretty full-screen view. It
is dependable meaning and discoverability in the environments where a terminal
client is actually used.

**Independent Test**: Format SQL containing hostile-looking control text inside
protected regions, Unicode, comments, nested queries, and malformed quotes in
full and plain modes. Compare protected bytes, visible wording, and no-effect
outcomes.

**Acceptance Scenarios**:

1. **Given** strings, quoted identifiers, dollar-quoted bodies, and comments,
   **When** formatting runs, **Then** each protected region is byte-for-byte
   unchanged and only outside whitespace/layout changes.
2. **Given** an unterminated quote, comment, or dollar body, **When** formatting
   runs, **Then** no text changes and the message names the kind and location
   without echoing the SQL buffer.
3. **Given** a plain-mode SQL buffer, **When** `\\format` is entered, **Then**
   formatted SQL is printed on the message stream, the SQL remains pending for
   later execution, and result stdout stays data-only.
4. **Given** a buffer larger than the formatter bound, **When** formatting is
   requested, **Then** no text changes and the message points to an explicit
   saved-query or export route.

### Edge Cases

- Empty or comment-only input reports that there is no SQL to format and does
  not create an undo step.
- Multiple statements retain their semicolons and order, with each statement
  separated predictably.
- Nested parentheses and subqueries receive bounded indentation without
  changing function arguments or protected text.
- Commas in a top-level list may create continuation lines; commas inside a
  function call or protected region are not treated as list boundaries.
- Operators, casts, dots, array brackets, parameters, and unary signs remain
  lexically separated so tokens cannot be accidentally joined.
- Unicode cursor positions remain on character boundaries, and control text in
  protected regions is never copied into diagnostics or status wording.
- A formatting request received while another modal surface is active is handled
  by that surface's existing precedence and cannot edit hidden state.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-1901**: The interactive client MUST provide a discoverable Format SQL
  action through the command palette, a visible shortcut, and a chord.
- **FR-1902**: Format SQL MUST operate on the complete editor buffer as a local
  operation and MUST NOT execute SQL, contact PostgreSQL, write history, or
  write a file.
- **FR-1903**: The formatter MUST produce deterministic output and MUST be
  idempotent: formatting its own output MUST produce identical text.
- **FR-1904**: The formatter MUST preserve the exact bytes of string literals,
  quoted identifiers, dollar-quoted bodies, and comments.
- **FR-1905**: The formatter MUST make major PostgreSQL clauses, boolean
  predicates, and eligible top-level lists readable with bounded indentation,
  while leaving keyword spelling and SQL values unchanged.
- **FR-1906**: Applying a changed format MUST be one undoable editor edit, and
  Undo and Redo MUST restore the exact text and cursor state at their respective
  points.
- **FR-1907**: The formatter MUST map the cursor to the same retained token when
  a token survives formatting and MUST never place it inside a UTF-8 code point.
- **FR-1908**: Empty, comment-only, already formatted, malformed, and
  over-limit inputs MUST have explicit value-free outcomes and MUST NOT create
  an edit when no formatting is applied.
- **FR-1909**: A formatting edit MUST invalidate an old server error location
  using the existing stale-buffer wording rather than leaving a misleading
  marker.
- **FR-1910**: Plain mode MUST expose the same local operation through `\\format`
  without mixing formatted interaction text into result stdout or executing the
  pending buffer.
- **FR-1911**: Refusal messages MUST state a concrete next action and MUST NOT
  include raw SQL, literal contents, credentials, or terminal control bytes.
- **FR-1912**: Full, compact, narrow, ASCII, no-colour, reduced-motion, and
  screen-reader-readable presentation MUST retain the action's words and
  outcome distinction.

### Security and Compatibility Requirements

- **SEC-1901**: Formatting MUST be a pure local transformation with no new
  network, persistence, telemetry, or credential route.
- **SEC-1902**: Unterminated or ambiguous protected regions MUST fail closed by
  leaving the buffer unchanged.
- **SEC-1903**: The formatter MUST use bounded work and refuse input above the
  documented limit before allocating an unbounded derived result.
- **SEC-1904**: Existing run, completion, history, export, result, and machine
  output contracts MUST remain unchanged.

### Key Entities *(include if feature involves data)*

- **Formatted buffer**: Deterministic editor text plus a UTF-8-safe mapped cursor
  position; it is not persisted separately.
- **Format outcome**: Applied, already formatted, empty, refused, or malformed
  result with safe counts and next-action wording.
- **Protected region**: A literal, quoted identifier, dollar-quoted body, or
  comment whose exact source bytes are retained.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-1901**: A compact multi-clause buffer can be formatted through one
  shortcut, one chord, or one palette selection without a database round trip.
- **SC-1902**: Formatting the same buffer twice produces byte-identical text on
  the second invocation for all supported fixture classes.
- **SC-1903**: One Undo restores the exact pre-format buffer and cursor for
  every changed fixture in the formatter test suite.
- **SC-1904**: Protected-region fixtures retain 100% of their source bytes,
  including Unicode, control-looking text, nested comments, and dollar bodies.
- **SC-1905**: Every malformed or over-limit fixture produces zero text edits,
  zero side effects, and a visible next action in interactive and plain modes.
- **SC-1906**: Automated full-layout and plain-output checks retain the same
  outcome words in full, compact, narrow, ASCII, no-colour, and reduced-motion
  presentations.

## Assumptions

- The formatter is intentionally conservative and lexical rather than a full
  PostgreSQL parser or query rewriter; unsupported ambiguity is refused.
- A fixed two-space continuation indentation is sufficient for the first
  formatter slice and is documented as a style, not a PostgreSQL semantic.
- The existing editor, reducer, command palette, keymap, sanitisation, and
  plain-mode stream split remain the shared interaction primitives.
- The first release does not add external formatter dependencies, formatter
  configuration, automatic format-on-save, or database-provided formatting.
- The formatter bound is 1 MiB of UTF-8 source bytes, chosen to keep local
  editor work predictable and recoverable.
