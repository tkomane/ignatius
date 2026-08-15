# Feature Specification: The statement history

**Feature Branch**: `006-statement-history`

**Created**: 2026-08-16

**Status**: Implemented, verified on macOS against PostgreSQL 18.4, including
subprocess evidence that a credential-bearing statement never reaches the file.

**Numbering note**: files are numbered in build order. This completes roadmap
Feature 003, *Delightful query loop*.

## The tension this feature is

A history is the difference between retyping a query and pressing a key. It is
also a file of SQL, written against real databases, sitting on a disk for as
long as nobody deletes it. Both are true at once, and the design is the
resolution rather than a preference for one of them.

The resolution: record by default, because retyping is the most common small
misery of a terminal client; refuse to record anything that looks like a
credential; make every control explicit and reachable; and never let a session
keep less than the user thinks without saying so.

## User Scenarios & Testing

### User Story 1 - Run that query again (Priority: P1)

Someone wrote a good query yesterday. Today they want it back without
remembering it.

**Independent Test**: run a statement, restart the client, search for part of
it, and press Enter.

**Acceptance Scenarios**:

1. **Given** statements have run, **When** the history search is opened, **Then**
   they are listed newest first with when they ran, on which database, and how
   they ended.
2. **Given** the search is open, **When** the user types part of a statement,
   **Then** matching statements rank best-first.
3. **Given** a match is selected, **When** the user confirms, **Then** the
   statement is placed in the editor and nothing runs on its own.
4. **Given** a fresh client, **When** it opens, **Then** the history is read from
   disk without the interface waiting on it.

### User Story 2 - Do not keep this (Priority: P1)

Someone is about to work on a database where they would rather leave no trace,
or is running a statement that carries a secret.

**Why this priority**: a tool that records by default and cannot be told to stop
is a tool people stop trusting, and trust is the product.

**Independent Test**: pause recording, run a statement, and confirm it is not in
the file.

**Acceptance Scenarios**:

1. **Given** a statement that mentions a credential, **When** it runs, **Then**
   it is not written, and the session is told: a sentence on the message stream
   in plain mode, and a marker in the header of the full-screen client that
   lasts until the next statement is kept.
2. **Given** `--no-history` or the in-client pause, **When** statements run,
   **Then** nothing is written and the interface says the session is not
   recording.
3. **Given** `history.enabled = false`, **When** anything runs, **Then** nothing
   is written and no session can turn it back on.
4. **Given** a history file, **When** the user clears it, **Then** it is gone,
   and clearing requires saying so explicitly.

### User Story 3 - See exactly what is kept (Priority: P2)

Someone wants to know what is on their disk.

**Acceptance Scenarios**:

1. **Given** a history, **When** `history list` runs, **Then** the entries are
   printed with their times, outcomes and databases.
2. **Given** a history, **When** `history path` runs, **Then** the file's
   location is printed, and `config paths` lists it too.
3. **Given** a listing, **When** it is piped, **Then** the entries are the only
   thing on the data stream.

### Edge Cases

- A history file damaged by a crash or a hand edit.
- A history longer than its configured length.
- A statement containing an escape sequence, listed back into a terminal.
- A connection string with a password in it, as the recorded target.
- A history directory that cannot be written.

## Requirements

### Functional

- **FR-601**: Interactive sessions MUST record each statement once it has an
  outcome, with the time, redacted target, database, environment, statement,
  outcome and elapsed time.
- **FR-602**: Scripted `query` runs MUST NOT be recorded.
- **FR-603**: The history MUST be searchable from the client, and choosing an
  entry MUST place it in the editor without running it.
- **FR-604**: The file MUST be trimmed to its configured length, newest kept.
- **FR-605**: A damaged line MUST NOT lose the rest of the history.
- **FR-606**: `history list`, `history path` and `history clear --yes` MUST
  exist, and clearing MUST require the explicit flag.

### Security and privacy

- **SEC-601**: A statement mentioning a credential MUST NOT be written. The
  check is a documented heuristic that over-refuses.
- **SEC-602**: The connection target MUST be redacted with the single shared
  implementation before it is written.
- **SEC-603**: No result row may ever be written to the history.
- **SEC-604**: The file MUST be created `0600` on Unix, and trimming MUST NOT
  widen it.
- **SEC-605**: Recording MUST be stoppable in configuration, for a run, and for
  a session, and configuration MUST win over the session.
- **SEC-606**: A session that is not recording MUST say so where the user is
  already looking.
- **SEC-607**: Recorded statements MUST be escaped before being displayed.

### User experience

- **UX-601**: Only what reached the file may appear in the interface as history.
- **UX-602**: A statement refused for mentioning a credential MUST be reported
  where the user is already looking, because a silently missing entry is a
  mystery. The rule itself MUST also be stated in the history search, which is
  where someone goes looking for a statement that is not there.
- **UX-603**: The reason nothing is listed MUST distinguish "nothing recorded
  yet" from "history is switched off".
- **UX-604**: The number of statements a session holds in memory MUST be bounded
  whether they arrive at load or one at a time.

## Success Criteria

- **SC-601**: A statement run yesterday can be found and reused in three
  keystrokes and a few characters.
- **SC-602**: `ALTER ROLE app PASSWORD 'x'` never appears in the file.
- **SC-603**: A password in a connection URI never appears in the file.
- **SC-604**: A paused session leaves the file unchanged, and says it is paused.

## Assumptions

- The history is per machine, not per profile or per database. A single file is
  what people expect from a shell, and splitting it would make "what is on my
  disk" a harder question to answer, not an easier one.
- The search is over the most recent entries held in memory, not the whole file.
  Nobody scrolls a thousand statements, and the bound is real rather than a
  display limit.
- Times are stored with an offset and displayed without one. The file is
  unambiguous; the listing is readable.
