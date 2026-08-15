# Feature Specification: What an object is

**Feature Branch**: `007-object-definitions`

**Created**: 2026-08-16

**Status**: Implemented, verified on macOS against PostgreSQL 18.4, including a
table named to break a client that interpolates identifiers.

**Numbering note**: files are numbered in build order. This is the DDL half of
roadmap Feature 005, *PostgreSQL object explorer*. Dependency navigation,
indexes and extensions as tree nodes, and a dedicated metadata connection are
still to come.

## User Scenarios & Testing

### User Story 1 - What is this view actually doing? (Priority: P1)

Someone finds a view in the tree. The name says what it is for; the definition
says what it does.

**Why this priority**: it is the question the object tree provokes and cannot
answer. Without it, the answer is another tool or a `psql` meta-command.

**Independent Test**: select a view, press the definition key, and read the
`SELECT` that defines it.

**Acceptance Scenarios**:

1. **Given** a view, an index or a function, **When** its definition is shown,
   **Then** it is the text PostgreSQL itself renders, so what is shown is what
   will run.
2. **Given** a table, **When** its definition is shown, **Then** its columns,
   constraints and indexes are shown in the shape of the statement that would
   create it, and the panel says the text was assembled rather than rendered.
3. **Given** a definition, **When** it is drawn, **Then** it is coloured by the
   same lexer that colours the editor.
4. **Given** a definition longer than the panel, **When** the user scrolls,
   **Then** the position is stated.

### User Story 2 - Nothing quietly wrong (Priority: P1)

The catalogue can refuse, and objects disappear between one query and the next.

**Acceptance Scenarios**:

1. **Given** an object the role may not read, **When** its definition is asked
   for, **Then** the reason appears where the answer would have been.
2. **Given** a definition asked for and then dismissed, **When** the answer
   arrives, **Then** it is discarded rather than filling a panel that asked
   something else.
3. **Given** a schema or a group rather than an object, **When** the definition
   key is pressed, **Then** nothing opens, because there is nothing to show.

### Edge Cases

- An object named `we"ird "; DROP TABLE orders; --`.
- An object dropped between the tree being drawn and the definition being asked
  for.
- A definition of several hundred lines.
- A function with several overloads.

## Requirements

### Functional

- **FR-701**: Views, materialized views, indexes and functions MUST be rendered
  by PostgreSQL's own `pg_get_*def` functions.
- **FR-702**: A relation without such a renderer MUST be described from its
  columns, constraints and indexes.
- **FR-703**: The panel MUST state which of the two it is showing.
- **FR-704**: A definition that arrives for a request the panel is no longer
  waiting for MUST be discarded.
- **FR-705**: A failure MUST be shown in the panel with what to do next.
- **FR-706**: The definition MUST scroll, and MUST state its position when it
  does not fit.

### Security

- **SEC-701**: Every catalogue lookup MUST bind the schema and the name as
  parameters. No object name may be interpolated into SQL.
- **SEC-702**: Identifiers in the text shown MUST be quoted, because it will be
  copied.
- **SEC-703**: The text MUST be escaped for display, as any server-controlled
  text is.

### User experience

- **UX-701**: The panel MUST open while it waits rather than after, and say that
  it is waiting.
- **UX-702**: An assembled description MUST NOT be presented as a script that
  recreates the object.

## Success Criteria

- **SC-701**: A view's definition can be read without leaving the client.
- **SC-702**: A table's description names its constraints.
- **SC-703**: An object named to break an interpolating client is described, and
  the table that name was written to destroy still exists afterwards.

## Assumptions

- Only the first overload of a function is shown, ordered by OID. Showing every
  overload in one panel would need a chooser, and the tree lists them separately
  already.
- A definition is a snapshot of what the catalogue said when it was asked. It is
  not live, and nothing refreshes it behind the user.
