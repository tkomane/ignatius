# Feature Specification: Plain line-oriented terminal mode

**Feature Branch**: `007-plain-line-mode`

**Created**: 2026-08-15

**Status**: Implemented in code; formal specification and hand-verification
follow-up are in progress

**Input**: User description: "Provide a plain line-oriented terminal mode for
screen readers and terminals that cannot support a full-screen interface, while
preserving safe query execution and truthful connection state."

## User Scenarios & Testing

### User Story 1 - Use the client without a full-screen terminal (Priority: P1)

A person using a screen reader, a narrow terminal, `TERM=dumb`, or a transcript-
oriented workflow wants to connect and run SQL without an alternate screen,
cursor movement or visual-only status indicators. They should be able to read
the session from top to bottom and keep the result in scrollback.

**Why this priority**: Accessibility and terminal compatibility are release
criteria. A client that only works for a rich visual terminal excludes users and
fails in common remote and automation environments.

**Independent Test**: Start the plain mode against the disposable database,
send a multi-line `SELECT`, and confirm that the result, prompts and status
messages form a readable line-oriented transcript with no terminal control
sequences.

**Acceptance Scenarios**:

1. **Given** a reachable database and a terminal that cannot provide a
   full-screen interface, **When** the user starts plain mode, **Then** the
   client connects without taking an alternate screen or enabling raw mode and
   prints a textual connection summary.
2. **Given** plain mode is running, **When** the user enters a query, **Then**
   result data is written to the data stream and prompts, notices and status
   messages are written to the message stream.
3. **Given** `TERM=dumb`, **When** the user starts plain mode, **Then** the
   session remains usable and does not depend on colour, Unicode decoration or
   cursor addressing.
4. **Given** end of input, **When** the user sends Ctrl+D or closes the input
   stream, **Then** the client exits successfully without waiting for another
   line.

### User Story 2 - Enter PostgreSQL statements naturally (Priority: P1)

A user wants to enter a statement over several lines without accidentally
 sending half of it. They also need a small, discoverable set of session
 commands for help, connection information and leaving the client.

**Why this priority**: Line-oriented input is only useful if it preserves the
same PostgreSQL-aware statement boundaries as the full-screen client.

**Independent Test**: Enter a statement over several lines, include semicolons
inside a string and a dollar-quoted body, then confirm that exactly one complete
statement is sent. Exercise help, connection information and quit commands.

**Acceptance Scenarios**:

1. **Given** an unfinished statement, **When** the user enters another line,
   **Then** the client shows a continuation prompt and does not execute yet.
2. **Given** a semicolon inside a quoted string, quoted identifier or
   dollar-quoted body, **When** the user enters it, **Then** it does not end the
   statement prematurely.
3. **Given** a line beginning with a supported session command, **When** the
   user enters `\?`, `\c` or `\q`, **Then** help, connection information or a
   clean exit is performed without sending SQL.
4. **Given** a backslash appearing inside an SQL statement, **When** the user
   enters it, **Then** it remains SQL input rather than becoming a session
   command.
5. **Given** an unknown session command, **When** the user enters it, **Then**
   the client explains that the command is unknown and points to help.

### User Story 3 - Keep safety and state visible in plain text (Priority: P1)

A user may connect to a production-classified target or encounter an error
inside a transaction. They need the same safeguards and truthful state in plain
mode that they would have in the full-screen client, without relying on colour.

**Why this priority**: Accessibility cannot create a safety downgrade. A plain
mode that hides production posture, cancellation state or transaction recovery
would be unsafe to use for the exact workflows where readable output matters.

**Independent Test**: Run a destructive statement against a production-
classified disposable target, cancel the confirmation, then run a transaction
that fails and confirm the transcript names the failed state and ROLLBACK.

**Acceptance Scenarios**:

1. **Given** a production-classified target and a destructive statement,
   **When** the statement is ready to run, **Then** the client states the impact,
   names the production classification and requires the database name before
   sending it.
2. **Given** a production confirmation is requested, **When** the user presses
   enter or provides the wrong answer, **Then** nothing is sent and the client
   says that it was cancelled.
3. **Given** a statement is cancelled, **When** the server confirms the result,
   **Then** the transcript distinguishes a cancellation from a generic query
   failure and does not claim success before confirmation.
4. **Given** an explicit transaction has failed, **When** the statement ends,
   **Then** the prompt and outcome say that the transaction failed and name
   ROLLBACK as the way out.
5. **Given** a connection summary or diagnostic is printed, **When** it is
   shown on either stream, **Then** it carries the database, environment,
   posture, TLS state, attempted action, likely cause and next action in words,
   and never prints a password.

## Edge Cases

- A blank line at the start of a statement does nothing.
- A multi-line statement ends only at a PostgreSQL-aware boundary.
- A semicolon inside a string, quoted identifier or dollar-quoted function body
  does not end the input.
- A backslash in the middle of SQL is not interpreted as a session command.
- End of input while a statement is incomplete exits without sending a partial
  statement.
- A server notice is kept separate from result data.
- A query returns no rows, multiple result sets, NULL, an empty string or text
  that resembles NULL; each outcome remains distinguishable in the existing
  output contract.
- The database rejects a statement after a prior failure; the client keeps the
  failed-transaction recovery instruction visible.
- The connection drops while a statement is running; the outcome remains
  unknown rather than being described as success.
- An unknown session command does not alter the SQL buffer.

## Requirements

### Functional Requirements

- **FR-701**: The client MUST provide an explicit plain mode that runs the
  connection, query and quit workflow without requiring a full-screen terminal.
- **FR-702**: Plain mode MUST present prompts, status and diagnostics as
  line-oriented text and MUST NOT emit alternate-screen, raw-mode, cursor-
  addressing or other terminal-control sequences.
- **FR-703**: Plain mode MUST keep result data separate from prompts, notices and
  diagnostics so the data stream remains usable in a pipeline.
- **FR-704**: Plain mode MUST accumulate input until the existing PostgreSQL-aware
  statement boundary says a statement is complete.
- **FR-705**: Plain mode MUST provide discoverable commands for help, connection
  information and quitting, and MUST explain unknown commands.
- **FR-706**: Plain mode MUST use the existing connection target resolution,
  configuration validation, TLS policy, result formatting and secret redaction
  contracts.
- **FR-707**: Plain mode MUST use the existing cancellation request and MUST
  report the server-confirmed outcome without replaying the statement.
- **FR-708**: Plain mode MUST apply production write confirmation before sending
  a statement and MUST send nothing when the confirmation is not accepted.
- **FR-709**: Plain mode MUST report the server-reported transaction state after
  execution and MUST name the recovery action for a failed transaction.
- **FR-710**: Plain mode MUST return the same documented exit-code meaning as the
  corresponding connection, query, cancellation and internal failures in the
  existing command contract.

### User Experience Requirements

- **UX-701**: The first message MUST identify the product, mode, connection and
  available help and quit commands.
- **UX-702**: A continuation prompt MUST make it clear that the client is still
  collecting the current statement.
- **UX-703**: The prompt MUST carry the database, production classification when
  applicable, and the current transaction state as words rather than colour or
  icons.
- **UX-704**: The plain transcript MUST remain understandable when read from
  top to bottom by a screen reader or a person reading scrollback.
- **UX-705**: The plain presentation MUST not hide an error's headline, likely
  cause, next action or failed-transaction recovery instruction behind a visual
  panel.

### Security Requirements

- **SEC-701**: Passwords MUST NOT appear in plain prompts, results, diagnostics,
  logs or either output stream.
- **SEC-702**: A production write MUST NOT be sent before its required textual
  confirmation is accepted.
- **SEC-703**: Plain mode MUST preserve the existing rule that a TLS failure is
  never followed by an unencrypted retry.
- **SEC-704**: Server-supplied control characters MUST remain escaped as text and
  MUST NOT become terminal instructions.

### Compatibility Requirements

- **COMPAT-701**: Plain mode MUST work when `TERM=dumb` and when stdin is a
  pipe, subject to the existing connection and query prerequisites.
- **COMPAT-702**: The default invocation MUST retain the full-screen client; the
  plain mode is opt-in.
- **COMPAT-703**: The plain mode MUST use an ASCII presentation for its own
  decoration and status markers, while preserving the meaning of server values.

## Key Entities

- **Input transcript**: The ordered lines entered by the user, including the
  current incomplete statement and supported session commands.
- **Connection summary**: The database, user, server, environment, posture and
  TLS facts shown before input begins.
- **Plain execution outcome**: The result data, notices, diagnostics, elapsed
  time, exit meaning and server-reported transaction state for one execution.
- **Production confirmation**: The pending impact, required answer and accepted
  or cancelled decision before a protected write is sent.

## Success Criteria

### Measurable Outcomes

- **SC-701**: In representative connect, query, error, transaction and quit
  transcripts, zero terminal-control sequences reach either output stream.
- **SC-702**: A user can enter and execute a three-line query, including a quoted
  semicolon, without the client sending an incomplete statement.
- **SC-703**: A user can discover help, connection information and quit from the
  first transcript without consulting a separate manual.
- **SC-704**: Every production-classified destructive statement in the plain-mode
  contract tests is either confirmed with the required database name or results
  in zero SQL sent to the server.
- **SC-705**: A failed transaction transcript contains both the failed-state
  wording and the ROLLBACK recovery instruction in full and compact layouts.
- **SC-706**: Plain mode preserves the documented separation of data and message
  streams for successful results, notices and failures.
- **SC-707**: Plain mode passes the full repository formatting, lint, unit,
  command-contract and PostgreSQL integration gates on the supported development
  server.

## Assumptions

- Plain mode is opt-in and does not replace the default full-screen interface.
- SQL input is considered complete when the existing PostgreSQL-aware statement
  parser identifies a terminating boundary.
- The initial command set is intentionally small: help, connection information
  and quit are supported; compatibility with the full `psql` meta-command set is
  out of scope.
- Existing connection resolution, configuration, diagnostics, output formats,
  cancellation and production classification remain the authorities for those
  concerns.
- Hand verification with VoiceOver, NVDA, Windows Terminal and Linux terminals
  remains a release follow-up even after automated plain-mode checks pass.
- Query history, configurable keymaps, credential profiles and a richer editor
  remain outside this feature.
