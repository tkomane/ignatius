# Feature Specification: Copy a result value out

**Feature Branch**: `018-clipboard-copy`

**Created**: 2026-09-04

**Status**: Implemented locally; locked verifier green with Unix-socket skip;
manual terminal and clipboard-acceptance checks pending

**Input**: User description: "Add an opt-in OSC 52 copy action for the selected
PostgreSQL result value, with explicit size confirmation, safe terminal
transport, truthful disabled and unconfirmed status wording, and full keyboard
and palette discoverability."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Copy the value I am looking at (Priority: P1)

As a database operator reviewing a retained result, I can select a text cell,
see exactly which cell and how many UTF-8 bytes and characters will leave the
client, and explicitly confirm the copy.

**Why this priority**: Moving one known result value into another terminal
application is a high-frequency GUI-like workflow, but it must remain explicit
and truthful because the value may contain sensitive data.

**Independent Test**: With a retained result containing ordinary, empty, and
Unicode text cells, invoke Copy selected value, inspect the confirmation, press
Enter for one cell and Escape for another, and verify the first sends exactly
one copy effect while the second sends none.

**Acceptance Scenarios**:

1. **Given** Results is focused on a text cell, **When** the user invokes Copy
   selected value, **Then** a confirmation identifies the result row and
   column and shows UTF-8 byte and character counts without showing the value.
2. **Given** a valid copy confirmation, **When** the user presses Enter, **Then**
   the exact selected text bytes are sent once through the configured terminal
   copy transport and the UI says terminal acceptance is unconfirmed.
3. **Given** a valid copy confirmation, **When** the user presses Escape or
   Cancel, **Then** no copy transport is written and the result and selection
   remain available.
4. **Given** the selected text is empty, **When** the user confirms, **Then** a
   zero-byte text value is sent and is distinguished from SQL NULL.

### User Story 2 - Decide whether data may leave through this terminal (Priority: P1)

As a security-conscious operator, I can keep terminal clipboard transport off
by default, enable it deliberately, and understand that the client writes a
sequence but cannot verify or clear the receiving clipboard.

**Why this priority**: The safety boundary is part of the feature, not an
implementation detail. Operators need an honest choice before a result value
can cross a terminal, SSH connection, or multiplexer.

**Independent Test**: Omit the clipboard setting and invoke the action, then set
`[clipboard] osc52 = true` and repeat. Verify the first path sends nothing and
explains the setting, while the second presents the exposure warning and only
writes after confirmation.

**Acceptance Scenarios**:

1. **Given** the setting is omitted or `osc52 = false`, **When** the user
   invokes Copy selected value, **Then** nothing is written and the UI explains
   that `[clipboard] osc52 = true` is required.
2. **Given** OSC 52 is enabled, **When** the confirmation is shown, **Then** it
   explains that the value travels through the terminal path and that the client
   never reads the clipboard.
3. **Given** the terminal ignores or does not support OSC 52, **When** the
   client writes the sequence successfully, **Then** the UI reports that the
   sequence was sent while keeping clipboard acceptance unconfirmed.

### User Story 3 - Copy remains a local, discoverable result action (Priority: P2)

As an operator learning the interface, I can find the copy action from the
Results context or command palette, use it from the inspector, and trust that
it never changes SQL, history, exports, or the editor.

**Why this priority**: Discoverability and non-interference make the action
usable in a terminal interface without taking printable letters away from
editing or creating surprising database side effects.

**Independent Test**: Reach the action from its named key chord and palette in
the grid and inspector, including a filtered or sorted result, then verify the
same source cell is offered and all ordinary query and export routes behave as
before.

**Acceptance Scenarios**:

1. **Given** a text cell is selected in the grid or inspector, **When** the user
   opens contextual help or the palette, **Then** Copy selected value and its
   prerequisite are discoverable without assigning printable `c` as an editor
   command.
2. **Given** a filtered or sorted displayed row, **When** the user confirms a
   copy, **Then** the value is resolved from the retained source row and column.
3. **Given** a copy confirmation is open and the retained result changes or
   the candidate becomes invalid, **When** the user confirms, **Then** nothing
   is sent and the UI explains that the candidate is stale.
4. **Given** the user uses plain output, JSON, NDJSON, export, history, or SQL
   editing, **When** those routes run, **Then** no OSC 52 sequence is emitted
   and their existing behavior is unchanged.

### Edge Cases

- SQL NULL has no text value to copy; the action explains that nothing was sent
  rather than silently converting it to an empty string or the literal `NULL`.
- ESC, BEL, OSC-looking text, C0/C1 controls, bidirectional text, newlines,
  tabs, wide characters, and other Unicode are sent as their original UTF-8
  bytes inside a safe encoded payload; the raw value is never terminal syntax.
- A value over 1 MiB of raw UTF-8 bytes is refused in full. No partial payload
  is sent, and the UI names the exact client limit and suggests export.
- A terminal write or flush failure produces a safe actionable diagnostic that
  does not include the result value.
- A terminal, SSH path, or multiplexer that ignores OSC 52 cannot be detected by
  this write-only route; the UI must not claim that the operating-system
  clipboard contains the value.
- With no rows, no columns, an empty filtered view, a visible plan, or a query
  running, the action is unavailable or performs a truthful no-op.
- If the retained result changes while confirmation is open, the stale
  candidate is rejected without a write.
- Repeated copy requests are independent confirmations; each confirmed request
  produces at most one sequence.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The system MUST offer a named Copy selected value action whenever
  Results retains a selectable text cell.
- **FR-002**: The action MUST be reachable from a discoverable key chord and the
  command palette, while printable letters remain available for SQL editing.
- **FR-003**: The system MUST resolve the value from the current retained
  result, source row, and source column at confirmation time rather than
  maintaining a second editable copy of the value.
- **FR-004**: Every copy request MUST show a confirmation containing the result
  row, column, UTF-8 byte count, character count, terminal destination, and an
  acceptance-unconfirmed statement before sending.
- **FR-005**: Enter on a valid confirmation MUST emit exactly one effect carrying
  the exact selected text bytes and MUST NOT execute SQL, alter history, write a
  result export, or change the database.
- **FR-006**: Escape or Cancel MUST dismiss the confirmation without emitting a
  terminal copy sequence or changing result data and selection.
- **FR-007**: SQL NULL MUST remain distinct from an empty text value and MUST
  never be silently converted for copying; an empty text value MUST be allowed
  as a zero-byte payload.
- **FR-008**: OSC 52 transport MUST be disabled by default and enabled only by
  `[clipboard] osc52 = true`; disabled requests MUST send nothing and explain
  how to enable the setting.
- **FR-009**: The transport MUST encode the value so its raw bytes cannot become
  terminal control syntax, MUST write only the requested sequence, and MUST
  never read, poll, clear, upload, persist, or telemeter clipboard contents.
- **FR-010**: Values over the 1 MiB raw UTF-8 client bound MUST be refused before
  transport and the UI MUST suggest an explicit export route.
- **FR-011**: Terminal write and flush failures MUST produce a safe diagnostic
  with an actionable next step and no result value.
- **FR-012**: A confirmation whose retained result or source cell is stale MUST
  be rejected without transport and explained to the user.
- **FR-013**: Copy MUST remain outside ordinary result rendering, SQL editing,
  query execution, history recording, export, and machine-readable output.
- **FR-014**: The preference and starter configuration MUST be backward
  compatible with existing configuration files and MUST introduce no secret,
  credential, or raw-result persistence.

### Security Requirements

- **SEC-001**: Raw result values MUST NOT appear in Debug output, diagnostics,
  logs, status text, or the confirmation overlay.
- **SEC-002**: Only one confirmed terminal write MAY occur per request; the
  client MUST NOT read or clear the destination clipboard.
- **SEC-003**: The encoded payload MUST contain no raw result bytes and MUST NOT
  introduce raw ESC or BEL bytes beyond fixed protocol framing.
- **SEC-004**: The setting and documentation MUST state that the terminal,
  local applications, SSH path, and multiplexer may observe or retain the value,
  and that automatic clearing is not guaranteed.

### Key Entities

- **Clipboard preference**: A non-secret, opt-in OSC 52 setting, disabled by
  default.
- **Copy candidate**: Temporary identity of the retained result job, source row,
  source column, byte count, and character count. It never stores the value.
- **Clipboard outcome**: A user-visible disabled, cancelled, stale, too-large,
  sent-but-unconfirmed, or write-failed state without the value.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Tests demonstrate zero terminal writes before explicit confirmation
  and exactly one write after each valid confirmation.
- **SC-002**: Transport tests demonstrate byte-for-byte round-tripping for
  control-containing and Unicode text while containing no raw value bytes or
  unframed ESC/BEL bytes.
- **SC-003**: Configuration tests demonstrate that an omitted setting disables
  transport and an explicit `true` value round-trips through config load/show.
- **SC-004**: The action is reachable in Results and inspector contexts through
  the named palette/chord and its wording renders truthfully in compact, narrow,
  ASCII, no-colour, and reduced-motion modes.
- **SC-005**: Tests demonstrate distinct no-payload outcomes for stale, NULL,
  no-row, too-large, write-failed, disabled, and cancelled requests.
- **SC-006**: Existing plain, JSON, NDJSON, export, history, SQL, and ordinary
  result tests remain green with no OSC 52 output.

## Assumptions

- ADR-0013 is accepted and defines the v1 transport as OSC 52 write-only with
  no new dependency.
- The client can know the bytes written and flushed but cannot know whether a
  terminal accepted or displayed the value.
- The 1 MiB limit is an Ignatius client boundary, not a guarantee about every
  terminal's OSC 52 capacity; larger values use explicit export.
- The retained result owns the text and source-row identity, so copying does
  not requery PostgreSQL or create a second result store.
- Enabling OSC 52 accepts exposure through the terminal path; Ignatius does not
  read, clear, or promise retention behavior for the receiving clipboard.
- Configuration schema version 1 remains backward compatible because omitted
  clipboard settings default to disabled.
