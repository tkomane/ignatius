# Feature Specification: Object navigation

**Feature Branch**: `002-ide-navigation`

**Created**: 2026-08-15

**Status**: Implemented, verified on macOS against PostgreSQL 18.4 and in CI
against 14, 16 and 18.

**Numbering note**: this is the second feature built, and it takes work the
roadmap had placed in 003 (command palette) and 005 (object explorer). The
owner asked for navigation ahead of the libpq migration in ADR-0009. Files are
numbered in build order; the roadmap records the intent.

**Why the reorder is safe**: this feature touches no credential route, no
connection resolution and no TLS policy, which is the surface ADR-0009 changes.
It reads the catalogue over an existing session. The libpq migration and this
feature do not overlap.

## User Scenarios & Testing

### User Story 1 - See what is in the database without typing SQL (Priority: P1)

Someone connects to a database they do not know. They want to see its shape:
which schemas exist, what is in them, what the columns are called, before
writing a single statement.

**Why this priority**: it is the difference between a query runner and a
workbench. Without it the user needs another tool, or `psql` meta-commands, to
answer the first question they always have.

**Independent Test**: connect to the demo database, open the tree, expand a
schema and a group, and read the columns of a table.

**Acceptance Scenarios**:

1. **Given** a connection, **When** the client opens, **Then** the tree lists the
   schemas with a count of each kind of object they hold.
2. **Given** a schema in the tree, **When** it is opened, **Then** its object
   groups appear without a round trip, because the counts arrived with the list.
3. **Given** a group, **When** it is opened, **Then** the objects load
   asynchronously and the row shows that it is waiting.
4. **Given** a table, **When** it is opened, **Then** its columns appear with
   their types, nullability and primary key membership.

### User Story 2 - Reach anything by typing part of its name (Priority: P1)

The user knows roughly what they want: a command, or a table whose name they
half remember. They want one key, a few characters, and the thing itself.

**Why this priority**: it is the interaction that makes a keyboard interface
faster than a mouse one, and it is what people mean by "IDE-like".

**Independent Test**: open the palette, type three characters that are not a
prefix of the target, and confirm the target ranks first.

**Acceptance Scenarios**:

1. **Given** the palette is open, **When** the user types a subsequence,
   **Then** matching commands and objects are listed best-first.
2. **Given** a match is selected, **When** the user confirms, **Then** a command
   runs, or an object's quoted name is inserted where SQL is written.
3. **Given** the palette is open, **When** the user types, **Then** nothing is
   typed into the editor underneath.

### User Story 3 - Discover what the keyboard can do (Priority: P2)

A new user does not know the bindings and should not have to read a manual.

**Independent Test**: press the chord prefix and read the list of continuations.

**Acceptance Scenarios**:

1. **Given** the chord prefix is pressed, **When** the popup appears, **Then** it
   lists every key that could follow and what each does.
2. **Given** the popup is open, **When** any other key is pressed, **Then** the
   chord ends rather than leaving the interface in a state the user cannot see.

### Edge Cases

- An object named to break a client that interpolates identifiers.
- A schema the role may not use, and an object it may not read.
- A load that arrives after the user collapsed and reopened the node.
- A tree filtered to nothing.
- A schema with hundreds of objects.
- The connection dropping while the tree is open.

## Requirements

### Functional

- **FR-101**: The client MUST show schemas, their object groups, the objects in
  them, and the columns of relations.
- **FR-102**: Counts MUST be gathered in a constant number of round trips
  regardless of how many schemas exist.
- **FR-103**: Children MUST load only when a node is opened.
- **FR-104**: A load whose identity is not the one the node awaits MUST be
  discarded.
- **FR-105**: The palette MUST search commands and loaded objects by
  subsequence, ranking contiguous and word-boundary matches higher.
- **FR-106**: Choosing an object MUST insert its schema-qualified, quoted name.
- **FR-107**: The chord popup MUST list every continuation and MUST end on any
  key.

### User experience

- **UX-101**: The tree MUST show the path to the selected object.
- **UX-102**: Every row MUST carry its meaning in words. Icons are supplementary
  and absent in ASCII.
- **UX-103**: Open and closed nodes MUST be distinguishable in every glyph tier,
  including ASCII.
- **UX-104**: A node waiting for a load MUST show that it is waiting.
- **UX-105**: Input modes MUST be peeled in a fixed order, and typing in one MUST
  NOT leak into another.

### Security

- **SEC-101**: Catalogue queries MUST bind every name as a parameter. No object
  name may be interpolated into SQL.
- **SEC-102**: A name inserted into the editor MUST be quoted so it cannot alter
  the statement it is pasted into.
- **SEC-103**: Object names and schema names MUST be escaped before display.
- **SEC-104**: Objects the role cannot read MUST be listed and marked, not hidden
  and not fatal.

### Performance

- **PERF-101**: Opening a schema MUST require no round trip.
- **PERF-102**: The tree MUST render only the rows that fit on screen.

## Success Criteria

- **SC-101**: A user can find a table in a database they have never seen, without
  writing SQL, in under thirty seconds.
- **SC-102**: A table named `we"ird"; DROP TABLE orders; --` is listed, displayed
  and selected from without altering any other object.
- **SC-103**: Typing three characters that are not a prefix ranks the intended
  object first.
- **SC-104**: The tree reads correctly in all three glyph tiers with colour off.

## Assumptions

- The tree shares the session's connection. A load issued while a long statement
  runs waits behind it on the server; the row shows it is waiting and input stays
  responsive. A dedicated metadata connection is later hardening.
- System schemas are excluded. Showing them by default would bury the user's own
  objects.
- Indexes, extensions and roles are read by the metadata layer but not yet shown
  in the tree.
