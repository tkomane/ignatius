# Research: Plain line-oriented terminal mode

**Date**: 2026-08-15

## Decision 1: Reuse PostgreSQL-aware statement boundaries

**Decision**: Complete a line-oriented statement only when the existing SQL
statement parser reports a terminating boundary.

**Rationale**: Splitting on a newline or the first semicolon would break quoted
strings, quoted identifiers and dollar-quoted function bodies. The repository
already has the required PostgreSQL-aware behaviour and its tests cover those
cases.

**Alternatives considered**:

- Execute every input line: rejected because it sends incomplete statements.
- Split on a semicolon: rejected because semicolons can be part of SQL values
  and function bodies.
- Adopt the full `psql` command language: rejected as unnecessary scope for an
  accessible first slice.

## Decision 2: Keep data and messages on separate streams

**Decision**: Results remain on stdout; prompts, connection summaries, notices,
diagnostics and outcome lines remain on stderr.

**Rationale**: A user can read the complete transcript while a script can pipe
result data without parsing prompts or status. This preserves the existing CLI
contract that failed queries do not contaminate the data stream.

**Alternatives considered**:

- Write everything to one stream: rejected because it makes machine use and
  screen-reader reading less predictable.
- Add a new output format: rejected because plain mode is a presentation mode,
  not a new result contract.

## Decision 3: Make plain mode explicit

**Decision**: `--plain` selects the line-oriented client. The default remains the
full-screen client, and `TERM=dumb` can use the explicit mode.

**Rationale**: Automatic replacement based on terminal detection could surprise
users and would make the behaviour depend on environment inference. An explicit
choice is visible, testable and compatible with existing invocations.

**Alternatives considered**:

- Automatically switch for every narrow or dumb terminal: rejected because the
  user should see and control the presentation choice.
- Remove the full-screen client: rejected because plain mode is an accessibility
  and compatibility path, not a replacement for keyboard-first visual use.

## Decision 4: Reuse existing execution and safety contracts

**Decision**: Plain mode uses the existing target resolution, session,
cancellation request, result writer, diagnostic redaction, environment
classification, transaction state and documented exit codes.

**Rationale**: A second implementation of any of these concerns could silently
  diverge in TLS, secrets, cancellation or production safety. The repository
  constitution makes truthful shared behaviour more important than local
  convenience.

**Alternatives considered**:

- Create a separate plain-mode database path: rejected because it would duplicate
  connection and security logic.
- Use a new output renderer: rejected because it would duplicate result and NULL
  semantics.

## Decision 5: Keep the command set small and explicit

**Decision**: Support help (`\?`), connection information (`\c`) and quit
(`\q`), and explain unknown commands.

**Rationale**: These commands complete discovery, orientation and exit without
  pretending to implement the entire `psql` meta-command language. The command
  set is visible in help and can grow through a future specified slice.

**Alternatives considered**:

- Accept every backslash command as a pass-through: rejected because it would
  create an undocumented and unsafe compatibility promise.
- Provide no commands: rejected because a line-oriented mode must be
  discoverable without a manual.

## Verification evidence and open research

The combined local verification on 2026-08-15 passed formatting, lints, 358
library tests, 29 CLI-contract tests and 33 PostgreSQL integration tests against
the disposable PostgreSQL 18.4 plain and TLS services. Automated evidence does
not prove hand use with VoiceOver, NVDA, Windows Terminal or a Linux terminal;
those remain explicit release follow-up work.
