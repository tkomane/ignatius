# Feature Specification: Asking, and keeping

**Feature Branch**: `010-asking-and-keeping`

**Created**: 2026-08-16

**Status**: Implemented, and specified after the fact. Written down because the
constitution requires specification-led work and four things were built without
one: the password prompt, the starter configuration file, saved queries, and an
export of what is on screen. Recording the drift is part of the correction.

**Why one spec for four things**: they are the same shape. Each is a moment
where the client asks the person in front of it for something, or keeps
something on their behalf, and each one's honesty problem is the same - saying
exactly what will happen before it happens, and exactly what happened after.

## User Scenarios & Testing

### User Story 1 - The server wants a password (Priority: P1)

Someone connects to a database whose password is not in a file, not in the
environment, and not in the connection string. The server refuses. They know the
password.

**Independent Test**: connect to the container with the password removed from
the URI, type it at the prompt, and get a session.

**Acceptance Scenarios**:

1. **Given** a refusal for want of a password, **When** there is a terminal to
   ask on, **Then** the client asks and tries again with what is typed.
2. **Given** the prompt, **When** characters are typed, **Then** they are not
   echoed anywhere: the full-screen client shows a count of dots and plain mode
   shows nothing at all.
3. **Given** no terminal at one end or the other, **When** the server refuses,
   **Then** nothing is asked and the refusal is the outcome, so a script fails
   instead of hanging.
4. **Given** an empty answer or a cancelled prompt, **When** it is submitted,
   **Then** the server's own refusal is what remains.
5. **Given** any of this, **When** it is over, **Then** the password is in no
   file, no configuration, no history and no log, and no `Debug` anywhere prints
   it.

### User Story 2 - A configuration file worth reading (Priority: P2)

Someone new runs the client and wants to change something.

**Acceptance Scenarios**:

1. **Given** no configuration file, **When** `config init` runs, **Then** a file
   is written holding this build's defaults, with the parts worth knowing about
   as comments and the examples commented out.
2. **Given** a file that already exists, **When** `config init` runs, **Then** it
   is refused unless `--force` is given, and the existing file is untouched.
3. **Given** the written file, **When** `config validate` runs, **Then** it
   passes: what the product writes, the product accepts.

### User Story 3 - A query worth keeping (Priority: P2)

Someone writes a query they will want next week.

**Acceptance Scenarios**:

1. **Given** a buffer, **When** it is saved under a name, **Then** an ordinary
   `.sql` file appears in the saved-queries directory holding exactly that SQL.
2. **Given** saved queries, **When** they are searched, **Then** each is listed
   by name and by what it does, skipping a leading comment block.
3. **Given** a chosen query, **When** it is opened, **Then** it replaces the
   buffer, and the replacement can be undone.
4. **Given** any name, **When** it is used, **Then** it is a name and never a
   path: separators, `..`, drive letters, control characters and names Windows
   reserves for devices are all refused before anything is written.

### User Story 4 - Keeping what is on screen (Priority: P2)

Someone has found the rows they wanted and needs them in a file.

**Acceptance Scenarios**:

1. **Given** rows on screen, **When** an export is asked for, **Then** the
   prompt says how many rows will be written before anything is written.
2. **Given** a truncated result or a filter, **When** the prompt appears,
   **Then** it says the file will hold what is on screen rather than what the
   query returned, gives the number the server returned, and names the way to
   get all of it.
3. **Given** a destination that exists, **When** the export runs, **Then** it is
   refused, and the advice names something the reader can actually do in the
   surface they are in.
4. **Given** a path beginning with `~`, **When** it is used, **Then** it means
   the home directory, because there is no shell behind the prompt to expand it.

### Edge Cases

- A password containing characters the terminal treats specially.
- A saved query whose file was edited by hand, or is unreadable.
- An export interrupted part way.
- A name that is only whitespace.

## Requirements

### Functional

- **FR-1001**: A refusal for want of a password MUST produce a prompt in both
  surfaces when there is a terminal at both ends, and MUST NOT otherwise.
- **FR-1002**: A typed password MUST reach exactly one connection attempt and be
  dropped with it.
- **FR-1003**: `config init` MUST write this build's defaults and MUST refuse to
  replace an existing file without `--force`.
- **FR-1004**: Saved queries MUST be plain `.sql` files in the directory
  `config paths` names.
- **FR-1005**: Opening a saved query MUST be undoable.
- **FR-1006**: An export from the client MUST write what the pane shows, through
  the same partial-file machinery a scripted export uses.
- **FR-1007**: An export MUST state its row count, and any difference from what
  the query returned, before writing.

### Security

- **SEC-1001**: No password may be echoed, stored, logged, or printed by any
  `Debug` implementation.
- **SEC-1002**: A saved query's name may not escape its directory.
- **SEC-1003**: A refusal's advice MUST name something the reader can do in the
  surface they are in. Advice naming a flag that does not exist there is the
  same failure as naming a status bar that is not there.

## Success Criteria

- **SC-1001**: A password typed at the prompt opens a session, and does not
  appear in the terminal transcript afterwards. Proven under a real
  pseudo-terminal against the disposable container.
- **SC-1002**: A file written by `config init` passes `config validate`.
- **SC-1003**: A name containing a path separator never writes a file.
- **SC-1004**: An export of a filtered view of a truncated result says three
  things: how many will be written, how many the server returned, and how to get
  all of them.

## Assumptions

- The prompt does not offer to remember a password. Remembering one is
  ADR-0011's question and is not answered here.
- Saved queries are files and nothing more: no index, no metadata, no format of
  ours. Anything else can read and write them, which is the point.
- An export from the client is comma-separated. A scripted export chooses its
  format; someone keeping what is on screen wants the obvious one.
