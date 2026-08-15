# Feature Specification: Foundation and proven vertical slice

**Feature Branch**: `001-foundation-vertical-slice`

**Created**: 2026-08-15

**Status**: Implemented, verified on macOS. Cross-platform verification pending.

**Input**: Feature 001 of the product plan: one command starts safely, enters and
restores the terminal, loads validated configuration, connects to a disposable
PostgreSQL instance, executes `SELECT 1`, renders a result, handles an error,
cancels `pg_sleep`, and exits cleanly.

## User Scenarios & Testing

### User Story 1 - Run a query against a local database and see the answer (Priority: P1)

Someone with PostgreSQL running locally wants to check something. They type one
command, get a working interface, run a query, read the result, and leave. The
terminal they return to is exactly as they left it.

**Why this priority**: This is the product. Everything else is in service of it,
and if this is not trustworthy nothing built on top of it can be.

**Independent Test**: Start a disposable PostgreSQL container, run the client
against it, execute `SELECT 1`, observe the rendered result, quit, and confirm
the terminal is usable.

**Acceptance Scenarios**:

1. **Given** a reachable PostgreSQL server, **When** the user opens the client
   and presses F5, **Then** the result appears with its column names, row count
   and elapsed time, and the status line reads `Completed`.
2. **Given** the client is open, **When** the user quits with Ctrl+Q, **Then**
   the alternate screen is left, the cursor is visible, raw mode is off, and the
   shell prompt works normally.
3. **Given** no configuration file exists, **When** the client starts, **Then**
   it uses built-in defaults and writes nothing.
4. **Given** a terminal narrower than 80 columns, **When** the client is open,
   **Then** it shows one pane deliberately rather than a clipped two-pane layout,
   and below 40x8 it shows the minimum size and a line-oriented alternative.

### User Story 2 - Understand a failure well enough to fix it (Priority: P1)

A query fails, or the server is unreachable, or the credentials are wrong. The
user needs to know what happened, why, and what to do next, without reading a
stack trace or a bare error code.

**Why this priority**: Equal to P1 because a client that only works when
everything works is not usable for production-adjacent tasks.

**Independent Test**: Run a statement against a missing relation and confirm the
headline, likely cause, next action and SQLSTATE are all available, and that the
process exit code distinguishes this from a connection failure.

**Acceptance Scenarios**:

1. **Given** a statement referencing a missing relation, **When** it is run,
   **Then** the interface shows a plain headline, what was attempted, the likely
   cause, a next action, and SQLSTATE on request.
2. **Given** the server is unreachable, **When** a connection is attempted,
   **Then** the failure names the host and port and distinguishes itself from an
   authentication failure by both wording and exit code.
3. **Given** a connection string containing a password, **When** anything about
   that connection is displayed or logged, **Then** the password never appears.

### User Story 3 - Stop a query that is taking too long (Priority: P2)

A query is running longer than expected. The user needs it to stop, needs to know
whether it actually stopped, and needs the interface to stay responsive while
they wait.

**Why this priority**: Cancellation is what makes an interactive client safe to
point at a real database.

**Independent Test**: Run `SELECT pg_sleep(30)`, cancel it, and confirm the
server reports SQLSTATE 57014 well before 30 seconds elapse.

**Acceptance Scenarios**:

1. **Given** a long statement is running, **When** the user presses Ctrl+C,
   **Then** the status reads `Cancellation requested` and the interface still
   redraws and accepts input.
2. **Given** cancellation was requested, **When** the server confirms, **Then**
   the status reads `Query cancelled by server` and nothing is re-run.
3. **Given** cancellation was requested twice, **When** the second request is
   made, **Then** no second cancellation is sent and the wording does not change.

### User Story 4 - Use it from a script (Priority: P2)

A script needs to run SQL and act on the outcome, with output it can parse and an
exit code it can branch on.

**Why this priority**: The claim that one tool serves both exploration and
automation is only true if the automation half is a real contract.

**Independent Test**: Pipe SQL into the command, parse the JSON output, and
assert the exit code for both success and failure.

**Acceptance Scenarios**:

1. **Given** stdout is a pipe, **When** a query succeeds, **Then** only data
   appears on stdout, diagnostics go to stderr, and the exit code is 0.
2. **Given** a query fails, **When** the command exits, **Then** stdout carries
   nothing and the exit code is 7.
3. **Given** no SQL is supplied and stdin is a terminal, **When** the command
   runs, **Then** it refuses with guidance instead of waiting.

### Edge Cases

- A value containing terminal escape sequences is displayed without those
  sequences reaching the terminal.
- A result larger than the row cap is bounded in memory while still reporting the
  true row count.
- A buffer of only comments or stray semicolons sends nothing to the server.
- An unterminated quote does not hang, panic, or split into two statements.
- A `sslmode` the build does not implement fails with an explanation rather than
  being approximated.
- The connection drops mid-statement, and the outcome is reported as unknown
  rather than as failure or success.

## Requirements

### Functional

- **FR-001**: The client MUST open a full-screen interface with no subcommand and
  a scriptable command tree otherwise.
- **FR-002**: The client MUST resolve a connection target from, in order,
  explicit arguments, a connection string, PostgreSQL environment variables, and
  built-in defaults.
- **FR-003**: The client MUST accept both `postgres://` URIs and libpq
  keyword/value connection strings, including IPv6 literals, percent-encoding and
  Unix socket directories.
- **FR-004**: The client MUST execute the whole buffer or the statement at the
  cursor, splitting statements with PostgreSQL's quoting rules rather than on
  semicolons.
- **FR-005**: The client MUST present column names, values, row counts, command
  outcomes, server notices and elapsed time.
- **FR-006**: The client MUST cancel a running statement through PostgreSQL's
  cancellation request and MUST NOT re-run or retry any statement automatically.
- **FR-007**: The client MUST provide `connect`, `query`, `config`, `doctor`,
  `completion` and `version` commands.
- **FR-008**: `query` MUST support table, CSV, TSV, JSON, NDJSON and Markdown
  output, and MUST refuse a format that cannot represent the result
  unambiguously.
- **FR-009**: The client MUST load, validate and migrate versioned configuration,
  and MUST NOT migrate implicitly.

### User experience

- **UX-001**: Connection, environment classification, read/write posture, TLS
  state, query status, row count, truncation and focus MUST be visible without
  opening a diagnostics panel.
- **UX-002**: Errors MUST be layered: headline, what was attempted, likely cause,
  next action, and expandable technical detail.
- **UX-003**: Status wording MUST match the real state, including
  `Cancellation requested` before confirmation and
  `Connection lost - query outcome unknown` when the outcome is unknown.
- **UX-004**: Every meaning carried by colour MUST also be carried by text.
- **UX-005**: The client MUST open on a useful buffer, not a blank screen, and
  MUST show how to run, cancel, get help and quit.
- **UX-006**: A SQL NULL MUST be distinguishable from an empty string and from
  the text `NULL`.

### Security

- **SEC-001**: No password may appear in any output, log, diagnostic or `Debug`
  representation.
- **SEC-002**: A TLS failure MUST NOT be followed by an unencrypted retry.
- **SEC-003**: The TLS state shown MUST be what the server reports, not what was
  requested, and MUST be reported as unknown when it cannot be confirmed.
- **SEC-004**: A security-relevant connection parameter that this build does not
  implement MUST fail the connection rather than be ignored.
- **SEC-005**: Control characters in server-supplied values and identifiers MUST
  be escaped before display.
- **SEC-006**: Environment classification MUST be explicit and MUST NOT be
  inferred from a host name.

### Reliability

- **REL-001**: The terminal MUST be restored on normal exit, error, panic and
  SIGTERM: alternate screen, cursor visibility, raw mode, bracketed paste and
  mouse capture.
- **REL-002**: Result rows held in memory MUST be bounded by an explicit cap,
  with truncation visible and the true count preserved.
- **REL-003**: A result belonging to a superseded job MUST be discarded.
- **REL-004**: Configuration writes MUST be atomic and MUST take a backup before
  a migration.

### Performance

- **PERF-001**: Startup MUST NOT depend on the network; the interface draws
  before the connection completes.
- **PERF-002**: Input and rendering MUST NOT block on database work.

### Operations

- **OPS-001**: `doctor` MUST report platform, paths, configuration, terminal and
  optional connection checks, in human and JSON form, with a next action for
  every non-passing check.
- **OPS-002**: `version --verbose` MUST report product version, source revision,
  build identity, target triple and compiler as distinct facts.
- **OPS-003**: Logging MUST be off unless requested, and MUST never contain SQL
  text or row values.

### Compatibility

- **COMPAT-001**: Documented exit codes MUST be stable: 0 success, 2 usage,
  3 configuration, 4 connection, 5 authentication, 6 TLS, 7 query, 8 cancelled,
  9 interrupted export, 70 internal.
- **COMPAT-002**: `NO_COLOR`, `TERM=dumb` and `--plain` MUST suppress colour
  regardless of configuration.
- **COMPAT-003**: Unsupported libpq parameters MUST be reported, and which ones
  are unsupported MUST be documented.

## Success Criteria

- **SC-001**: A new user with a local PostgreSQL server reaches a rendered query
  result in under two minutes using only the documented steps.
- **SC-002**: `SELECT pg_sleep(30)` is cancelled and confirmed by the server in
  under 10 seconds, with the interface responsive throughout.
- **SC-003**: A 5000-row result held at a cap of 10 keeps 10 rows in memory and
  still reports 5000.
- **SC-004**: No test, output stream or log contains a password, including when
  the connection fails.
- **SC-005**: Every documented exit code is produced by a real invocation in the
  test suite.
- **SC-006**: The interface renders correct meaning in dark, light,
  high-contrast, no-colour and ASCII presentations at 100x30, 80x24, 60x20 and
  30x6.

## Assumptions

- PostgreSQL 14 through 18 are the supported server versions, matching the
  upstream support window as of 2026-08-15. Version 18.4 is what has been tested
  so far; the rest is CI work.
- `standard_conforming_strings` is `on`, the default since PostgreSQL 9.1.
- Connection profiles, the OS credential store, `.pgpass`, service files, query
  history and the object explorer are out of scope here and belong to Features
  002, 003 and 005.
- The user has a terminal that supports the alternate screen for the interactive
  client; `query` covers the cases where they do not.
