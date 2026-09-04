# Feature Specification: Reviewable cell-to-UPDATE

**Feature Branch**: `023-cell-update`

**Created**: 2026-09-04

**Status**: Implemented locally; locked verifier green with Unix-socket skip; terminal hand checks pending

**Input**: The experience roadmap item "Editing a cell to generate an
`UPDATE`".

## User Scenarios & Testing

### User Story 1 - Turn one retained cell into a precise update (Priority: P1)

Someone runs a simple result-producing `SELECT`, moves to one retained cell,
and invokes the cell-update action. Ignatius discovers the live relation's
primary key, asks for the replacement text, and shows the exact generated
statement before anything is sent. A second explicit confirmation is required
to execute it once.

**Why this priority**: A small, reviewable write is the most valuable bridge
between reading data and acting on it, but it must preserve the user's control
over both the target row and the final SQL.

**Independent Test**: With a disposable table containing a primary key, retain
one row in the result grid, generate an update for a non-key direct column,
assert that metadata discovery and value entry produce a review modal with one
key predicate, assert that confirmation alone emits one parameterized execution,
and assert that dismissal emits no execution.

**Acceptance Scenarios**:

1. **Given** a retained result from `SELECT order_id, note FROM public.orders`,
   **when** the selected cell is edited, **then** Ignatius discovers
   `order_id` as the primary key, asks for a replacement value, and leaves the
   original editor SQL and result snapshot unchanged while the prompt is open.
2. **Given** a valid replacement value, **when** the value is accepted,
   **then** the review surface shows the fully bound `UPDATE`, the qualified
   relation, the selected row identity, and the instruction that no database
   work has happened yet.
3. **Given** the review surface, **when** the user presses Enter, **then** one
   parameterized `UPDATE` execution effect is emitted and no automatic rerun of
   the original `SELECT` occurs.
4. **Given** the value prompt or review surface, **when** the user presses Esc,
   **then** no execution effect is emitted and the result selection remains
   available.

### User Story 2 - Refuse guesses and unsafe targets (Priority: P1)

Someone selects a result that came from a join, expression, view, ambiguous
projection, missing key, NULL key, hidden or stale row, read-only session, or
production-classified connection. Ignatius explains the exact boundary and
does not invent a table, predicate, key, cast, or write route.

**Why this priority**: The client invented the write against data it did not
choose. A false positive is more dangerous than a missing convenience.

**Independent Test**: Exercise each unsupported or stale shape through pure
source analysis and reducer tests, then verify production and server
read-only refusals against a disposable PostgreSQL session without an update
being submitted.

**Acceptance Scenarios**:

1. **Given** a query with a join, subquery, CTE, aggregate, expression,
   duplicate output, or multiple statement, **when** cell update is invoked,
   **then** it is refused before key discovery with a readable reason.
2. **Given** a single-table result whose projection does not contain every
   primary-key column, **when** metadata returns, **then** the value prompt is
   not opened and no statement is generated.
3. **Given** a selected row whose primary-key value is NULL, **when** the
   update is prepared, **then** it is refused because the row cannot be
   identified exactly.
4. **Given** a production-classified or server read-only connection, **when**
   the action is invoked, **then** it is refused before a value is requested or
   an execution effect is emitted.
5. **Given** the result, selection, editor source, or connection changes while
   key discovery or review is open, **when** confirmation is attempted,
   **then** the candidate is rejected as stale and nothing is sent.

### User Story 3 - Keep the write auditable and bounded (Priority: P1)

Someone can read what the client intends to change, see that the values are
literal data, and understand that the retained result is not refreshed after
the write. Generated values travel through the existing secret parameter
boundary, are not placed in the effect debug representation or history, and
are bound only at the PostgreSQL execution boundary.

**Why this priority**: Review is only meaningful when the statement shown is the
statement that will run, and when the client can state what it has not verified.

**Independent Test**: Inspect model, message, effect, tracing, history and
renderer representations for a synthetic value marker, then execute a live
generated update and verify exactly one row changes only after confirmation.

**Acceptance Scenarios**:

1. **Given** a value containing quotes, backslashes, newlines, Unicode or SQL
   punctuation, **when** the review is rendered, **then** the displayed SQL
   shows one escaped literal and the execution effect carries the template and
   secret bindings separately.
2. **Given** a value containing NUL or a value that exceeds the client bound,
   **when** it is accepted, **then** preparation is refused before any
   execution effect and the value is absent from the diagnostic.
3. **Given** a successful generated update, **when** it completes, **then** the
   status reports the affected-row outcome and says the prior result remains a
   snapshot; Ignatius does not rerun the original query.
4. **Given** a generated update, **when** history is enabled, **then** the safe
   parameter template is recorded under the existing history rules and the
   entered or retained cell values are not written there.

## Edge Cases

- Only one statement is eligible, and it must be a conservative single-table
  `SELECT` with direct column projections. Joins, subqueries, CTEs, set
  operations, aggregates, expressions, computed aliases and duplicate output
  columns are refused rather than approximated.
- A bare relation name is resolved by PostgreSQL using the current session's
  search path. The resolved schema and relation are shown before confirmation.
- A schema-qualified relation is accepted and is resolved exactly.
- `SELECT *` is accepted only when the returned result columns uniquely match
  the live relation metadata. Explicit direct projections may use a simple
  output alias, but the selected result column must still map to one source
  column.
- Only ordinary and partitioned tables are eligible. Views, materialized
  views, foreign tables, sequences and functions are refused.
- The relation must expose a primary key, every primary-key column must be in
  the result projection, and every selected-row key value must be non-NULL.
  Primary-key cells themselves are not editable in this first slice.
- A blank replacement is an empty text literal. The feature does not invent a
  NULL shorthand; database NULL can be expressed by editing SQL directly.
- Values are bound as escaped text literals and PostgreSQL performs its normal
  assignment cast. No client-side type inference or expression entry is
  allowed.
- The generated statement is bounded at 16 KiB. Oversized values are refused
  before a review state is created.
- The result is a retained snapshot. A successful update does not rerun the
  source `SELECT`, page for more rows, or silently move the selection.
- If the connection is lost while metadata, value entry or review is pending,
  the candidate is discarded without retrying the update; a retained result can
  still be copied locally.
- This first slice is interactive-only. Plain mode, non-interactive query,
  bulk editing, insert/delete generation, multi-row updates, and an editable
  SQL buffer route remain outside this feature.

## Requirements

### Functional Requirements

- **FR-2301**: The interactive client MUST expose a discoverable cell-update
  action through the command palette and `Ctrl+K u`; printable `u` MUST remain
  editor input.
- **FR-2302**: The action MUST require focus on one retained, visible result
  cell and MUST preserve the result's source-row identity through local filter
  and sort state.
- **FR-2303**: The client MUST retain the original result SQL separately from
  the editable editor buffer and MUST refuse a stale or unavailable source.
- **FR-2304**: Source analysis MUST accept only one conservative single-table
  `SELECT` with direct source-column projections and MUST refuse ambiguous
  shapes without guessing.
- **FR-2305**: Relation discovery MUST resolve the source relation against the
  live PostgreSQL session and MUST return the resolved schema, relation kind,
  read privilege, update privilege and primary-key membership.
- **FR-2306**: The client MUST refuse non-table targets, unavailable update
  privilege, missing primary keys, missing projected key columns, NULL key
  values, duplicate mappings and primary-key cell edits.
- **FR-2307**: The replacement prompt MUST support insertion, backspace, empty
  text, Enter to accept and Esc or Cancel to abandon without changing the
  editor or sending SQL.
- **FR-2308**: The review surface MUST show the qualified target, selected row
  identity, generated statement, literal-data semantics, and the fact that no
  statement has been sent yet.
- **FR-2309**: The client MUST require one explicit confirmation after review
  before emitting an execution effect and MUST emit no effect on dismissal.
- **FR-2310**: Generated updates MUST use a distinct parameterized effect whose
  template is reviewable and whose values use `SecretString`; the bound SQL
  MUST exist only at the existing PostgreSQL simple-query boundary.
- **FR-2311**: Confirmation MUST revalidate result job, source row, selected
  column, source SQL and connection posture before sending.
- **FR-2312**: Production-classified and server read-only connections MUST be
  refused before replacement input is requested or a write effect is emitted.
- **FR-2313**: A successful generated update MUST preserve the existing result,
  history, transaction, cancellation, row-count and no-automatic-rerun
  contracts, with a truthful stale-snapshot notice.
- **FR-2314**: Generated SQL and replacement input MUST obey bounded sizes and
  reject NUL before the statement is sent.
- **FR-2315**: The generated template, not bound values, MUST be eligible for
  statement history and tracing; values MUST not appear in diagnostics, debug
  output or machine-output routes.

### Security and Compatibility Requirements

- **SEC-2301**: Key values retained from the result and replacement values
  entered by the user MUST use the existing secret-value boundary while held
  for review or execution.
- **SEC-2302**: Identifiers MUST be quoted by the one PostgreSQL identifier
  helper, and values MUST be bound as literal data; no user value may become an
  identifier, operator, keyword or clause.
- **SEC-2303**: The client MUST not treat SQL classification as a privilege
  boundary. PostgreSQL privileges and the server's read-only posture remain the
  real controls.
- **SEC-2304**: Existing ordinary SQL, parameter prompts, result sorting and
  filtering, production confirmation, history controls, plain output and
  connection lifecycle MUST remain unchanged.

## Key Entities

- **Update source**: A value-free description of the eligible single-table
  projection, including the parsed relation reference and result-to-source
  column mapping.
- **Update relation metadata**: Live PostgreSQL facts about the resolved table,
  its kind, permissions and ordered columns with primary-key membership.
- **Update candidate identity**: Result job, source row, selected result column
  and source SQL captured before metadata or value work begins.
- **Update value prompt**: An ephemeral secret text field for one replacement
  value, never copied into the editor or history.
- **Pending update**: A reviewable generated template plus secret bindings and
  candidate identity, awaiting one confirmation.

## Success Criteria

### Measurable Outcomes

- **SC-2301**: A valid single-table result can reach a review modal in one
  discoverable action, and exactly one confirmation emits exactly one execution
  effect.
- **SC-2302**: Every refused source, missing key, NULL identity, stale
  candidate, production target and read-only target emits zero update effects.
- **SC-2303**: Live disposable PostgreSQL evidence proves one confirmed update
  changes one intended row, a cancelled review changes none, and the original
  result is not automatically rerun.
- **SC-2304**: Automated privacy checks find no synthetic replacement or key
  value in model/effect debug output, history, diagnostics or logging output.
- **SC-2305**: Generated templates and bound values remain within the existing
  parameterized execution and cancellation contracts, with no new dependency,
  storage route or non-interactive mutation surface.
- **SC-2306**: The locked repository verifier remains green, with PostgreSQL,
  Unix-socket, terminal, screen-reader and cross-platform hand checks recorded
  separately.

## Assumptions

- Existing primary-key metadata is sufficient for the first safe vertical slice;
  unique-but-not-primary indexes are not treated as row identity.
- The source query is retained as the template SQL from the completed
  execution, so changing the editor after a query cannot change the target
  silently.
- A generated update is a normal PostgreSQL statement for cancellation,
  transaction state, affected-row reporting and history, but its values are
  supplied through the existing parameter binding boundary.
- The database role and server privileges remain authoritative. The client-side
  production refusal is an additional product guardrail, not a security claim.
