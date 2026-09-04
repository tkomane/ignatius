# Feature Specification: Completion that knows the schema

**Feature Branch**: `012-schema-completion`

**Created**: 2026-08-16

**Status**: Implemented in local ancestor commit `3168e7c`; task list complete.
Recorded focused and PostgreSQL 18.4 verification is in `docs/status.md`.
Unix-socket and cross-platform hand evidence remain separate from that local run.

**Input**: The owner's brief of 2026-08-16 - the least cognitive load and the
least expertise of anything in the market - and
`docs/product/experience-roadmap.md`, which puts this first and says why.

## Scope note

This is the feature that decides whether the product's claim is true. Every
other improvement makes an existing session better; this one changes whether the
session gets started by someone who does not have the schema memorised.

It reads the catalogue and it writes into the buffer. It runs no statement the
user did not ask for, and it never sends anything anywhere.

## User Scenarios & Testing

### User Story 1 - Offer what exists, where it makes sense (Priority: P1)

Someone starts typing a query against a database they do not know well.

**Independent Test**: connect to the container's synthetic schema, type
`SELECT * FROM cus`, and be offered `customers` with its schema named.

**Acceptance Scenarios**:

1. **Given** a partly typed identifier after `FROM` or `JOIN`, **When**
   completion runs, **Then** tables and views are offered, each labelled with
   its schema and what kind of object it is.
2. **Given** a partly typed word where a keyword belongs, **When** completion
   runs, **Then** keywords are offered, and they do not crowd out object names
   where both could fit.
3. **Given** a statement that already names its tables, **When** a column is
   being typed, **Then** the columns offered are the columns of those tables,
   with the type of each shown.
4. **Given** an alias, **When** `o.` is typed after `FROM orders o`, **Then**
   the columns of `orders` are offered and nothing else.
5. **Given** a schema name and a dot, **When** `public.` is typed, **Then** the
   objects of that schema are offered and nothing else.

### User Story 2 - Never insert what was not chosen (Priority: P1)

The failure that makes people switch completion off for ever is having their
text changed for them.

**Acceptance Scenarios**:

1. **Given** an open completion list, **When** an ordinary character is typed,
   **Then** it is inserted as typed and the list narrows. Nothing is accepted.
2. **Given** an open list, **When** the accept key is pressed, **Then** the
   highlighted entry replaces exactly the word being typed and nothing else.
3. **Given** an open list, **When** dismiss is pressed, **Then** the list closes
   and the buffer is exactly what was typed.
4. **Given** an accepted completion, **When** undo is pressed, **Then** it is a
   single undo step and the typed prefix is back.
5. **Given** an identifier that needs quoting - mixed case, a space, a reserved
   word - **When** it is accepted, **Then** it is inserted correctly quoted,
   because an offered name that produces a syntax error is worse than no offer.

### User Story 3 - Say where the list came from (Priority: P1)

Principle I, applied to the one feature most tempted to bluff.

**Acceptance Scenarios**:

1. **Given** a catalogue that has not been read yet, **When** completion is
   asked for, **Then** it offers keywords, says the schema is still loading, and
   does not pretend the database has no tables.
2. **Given** a catalogue read some time ago, **When** the list is shown, **Then**
   the interface can say how old it is, and one key reloads it.
3. **Given** a database the user cannot read the catalogue of, **When**
   completion is asked for, **Then** it says so once and stops asking.

### User Story 4 - It works without colour, without Unicode, and in plain mode (Priority: P2)

**Acceptance Scenarios**:

1. **Given** the ASCII glyph tier, **When** the list is shown, **Then** each
   entry's kind is a word or an ASCII marker, never a glyph alone.
2. **Given** `--plain`, **When** completion is requested explicitly, **Then**
   the candidates are printed as lines and one is chosen by name or number. A
   line-oriented surface gets the capability in a line-oriented shape rather
   than not at all.

### Edge Cases

- A schema with thousands of tables: the list is bounded and says it is.
- A statement with a syntax error before the cursor: completion still offers
  something sensible rather than nothing.
- A CTE: names defined by `WITH` are in scope and are offered.
- A subquery with its own alias.
- Typing inside a string literal or a comment: nothing is offered.
- A table the user has no privilege to read: it is in the catalogue, and whether
  it is offered is decided explicitly rather than by accident.

## Requirements

### Functional

- **FR-1201**: Candidates MUST be derived from the catalogue already read for
  the object tree. No new statement is issued to serve a keystroke.
- **FR-1202**: What is in scope MUST be determined from the statement under the
  cursor, including table aliases and `WITH` names.
- **FR-1203**: Accepting a candidate MUST replace the word being typed and
  nothing else, MUST quote the identifier when PostgreSQL requires it, and MUST
  be one undo step.
- **FR-1204**: Completion MUST be dismissable, and MUST never act on a key it
  was not given.
- **FR-1205**: Every candidate MUST show what kind of thing it is and where it
  came from - schema for an object, table for a column, and the column's type.
- **FR-1206**: Nothing is offered inside a string literal or a comment. The
  lexer already knows the difference and there is one implementation of it.
- **FR-1207**: The list MUST be bounded, and a truncated list MUST say so with
  the true count, in the same words the result pane already uses.
- **FR-1208**: Completion MUST be switchable off in configuration, and the key
  that requests it explicitly MUST work whether or not the automatic offer is on.

### Security

- **SEC-1201**: An object name from the server is untrusted text. It is rendered
  through the existing escaping, and inserted through the existing quoting.
  Neither may be reimplemented for this feature.
- **SEC-1202**: No completion state is written to disk. What someone was about to
  type is not history.

## Success Criteria

- **SC-1201**: Against the container's synthetic schema, a query joining two
  tables can be written without typing any table or column name in full.
- **SC-1202**: A table named to break interpolation is offered, accepted, and
  the resulting statement runs and returns its rows. The hostile-name test that
  exists for the object tree, extended to this path.
- **SC-1203**: Typing a full statement with completion on produces exactly the
  same buffer as typing it with completion off. Proven by a property test over
  generated statements: this is the "never changed my text" claim, and it is the
  one that must be mechanically checked rather than asserted.
- **SC-1204**: With the catalogue unread, completion offers keywords and says so.

## Assumptions

- The statement under the cursor is already identified: the gutter marks it and
  the run key uses it. This feature reuses that boundary rather than finding its
  own.
- Ranking is by prefix match, then by kind, then alphabetically. Frequency-based
  ranking is a later question and needs history that is scoped and erasable, so
  it is not assumed here.
- Fuzzy matching is not in the first version. A prefix that offers the right
  thing beats a fuzzy match that offers four things.
