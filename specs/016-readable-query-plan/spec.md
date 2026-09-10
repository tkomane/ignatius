# Feature Specification: A plan you can read

**Feature Branch**: `016-readable-query-plan`

**Created**: 2026-09-04

**Status**: Implemented locally; locked verifier green with Unix-socket skip

**Input**: User description: "Continue the roadmap autonomously with a
first-class, GUI-like terminal experience. Make the query plan readable, with
the quality and discoverability of Neovim, Codex CLI, Claude Code CLI, and
Warp."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - See what PostgreSQL plans to do (Priority: P1)

A person with a SQL statement can ask for its plan from the workbench and see
the answer as a readable tree rather than a wall of database-generated text.
The tree names each operation, its relation or index when known, its estimated
rows, and its estimated cost. A compact summary makes the plan's scope and
estimates legible before the person starts interpreting individual nodes.

**Why this priority**: A graphical client earns its place by putting the answer
where the question was. A readable plan turns a terminal from a query runner
into a tool for understanding why a query behaves as it does.

**Independent Test**: With a connected session and a read-only statement under
the cursor, choose the plan action and render the result at full, compact,
narrow, ASCII, and no-colour sizes. The editor, transaction, result rows, and
history stay unchanged while the plan tree names the server's estimate.

**Acceptance Scenarios**:

1. **Given** a connected session with a valid statement under the cursor,
   **When** the person asks to explain it, **Then** the workbench shows a tree
   with the server's node names, estimated rows, and cost values, plus a
   summary that says it is an estimate rather than an observed execution.
2. **Given** a buffer contains more than one statement, **When** the person
   asks for a plan, **Then** only the statement at the cursor is planned and
   the rest of the buffer remains unchanged.
3. **Given** the session is disconnected, the buffer is empty, or the current
   statement is already an EXPLAIN command, **When** the person searches for a
   plan, **Then** the interface explains the missing prerequisite and sends
   nothing.

---

### User Story 2 - Compare the estimate with reality (Priority: P1)

A person can explicitly ask PostgreSQL to execute the current statement while
measuring it. Before anything runs, the workbench says plainly that analysis
executes the statement, that functions can have side effects, and that a
data-changing statement can change data. After confirmation, the plan shows
actual rows, actual time, and loop counts where PostgreSQL supplied them, puts
the comparison beside the estimate, and marks the most expensive or slowest
node in words as well as styling.

**Why this priority**: An estimate explains the planner's belief; the mismatch
between belief and reality explains most performance surprises. The dangerous
part is that ANALYZE is an execution, so the confirmation and the evidence of
what really happened are part of the feature rather than polish.

**Independent Test**: With a known read-only statement, choose analyze, verify
that no effect exists before confirmation, confirm it, and render a plan with
actual metrics and an estimate-mismatch marker. Repeat with cancellation and a
statement classified as writing to verify the warning and existing protection
remain visible.

**Acceptance Scenarios**:

1. **Given** a statement is ready to analyze, **When** the person chooses
   analyze, **Then** a confirmation names the statement, says that it will
   execute, and waits for an explicit confirmation before sending anything.
2. **Given** the person cancels the confirmation, **When** the screen returns
   to the workbench, **Then** no SQL was sent, the buffer and transaction are
   unchanged, and history contains no analysis entry.
3. **Given** analysis completes, **When** the plan is shown, **Then** actual
   rows, actual time, and loops are shown only where supplied, estimate
   comparisons use the server's documented per-loop values, and a deviation
   of one order of magnitude or more is called out as a mismatch.
4. **Given** analysis loses the connection or fails, **When** the response is
   displayed, **Then** the workbench states that the outcome may be unknown,
   does not retry, and does not show a successful plan as though it were
   complete evidence.

---

### User Story 3 - Work through a plan, not just look at it (Priority: P2)

A person can move through the plan tree with the same visible keyboard model as
the rest of the workbench. The selected node is obvious, children can be
expanded or collapsed without another server request, and the node detail says
what is known, what is estimated, what is observed, and what is unavailable.
The person can leave the plan and return to the prior result view without
rerunning the query.

**Why this priority**: A plan that cannot be navigated is only a prettier
printout. Local selection and expansion give the terminal the directness of a
graphical inspector while retaining the speed and keyboard completeness of the
workbench.

**Independent Test**: Load a nested plan, move selection through parent and
child nodes, collapse and reopen a branch, inspect a node with missing actual
fields, and dismiss the plan. Confirm all changes are local presentation state
and the prior result remains intact.

**Acceptance Scenarios**:

1. **Given** a nested plan is visible, **When** the person moves and activates
   a node, **Then** selection and expansion change locally and the selected
   node's details remain readable in the available space.
2. **Given** a plan contains an operation or field this build does not
   recognise, **When** the node is selected, **Then** the known fields remain
   visible and the unknown or unavailable part is labelled rather than guessed.
3. **Given** a plan is open over an existing result, **When** the person
   dismisses it, **Then** the existing result, filter, selected row, and
   transaction state return without a second execution.

### Edge Cases

- A statement has no plan row, returns malformed or unexpectedly shaped JSON,
  or contains no plan root. The interface reports that the plan could not be
  read and keeps the source and prior result available.
- PostgreSQL supplies a plan with no actual metrics because it was not
  analyzed, or a server version omits a field. The view says `not measured` or
  `not supplied`; it never invents zeroes.
- A plan is very deep or large. The view stays bounded, indicates that the
  display is truncated, and keeps the selected node and exit action usable.
- A statement starts with comments, has multiple statements, or contains
  quoted semicolons. The existing statement boundary rules choose the current
  statement without changing SQL.
- EXPLAIN ANALYZE is requested inside an open transaction or for a statement
  that appears to write, change structure, or destroy data. The confirmation
  says that execution occurs in the current session and preserves the existing
  production and destructive-action safeguards; no automatic rollback is
  implied.
- A node, relation, index, expression, or server diagnostic contains control
  characters or terminal escape sequences. The displayed text remains safe and
  readable in every presentation tier.
- The terminal is narrow, compact, ASCII, no-colour, reduced-motion, or too
  small. The plan retains its node names, selected state, safety wording, and
  way out, or uses the existing too-small recovery message.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The workbench MUST let a person ask for a plan for exactly the
  statement at the cursor, and MUST leave all other editor text unchanged.
- **FR-002**: A plain plan request MUST ask PostgreSQL for planning information
  without executing the statement, and MUST NOT add the generated plan command
  to user statement history.
- **FR-003**: The plan view MUST render a hierarchical tree with every
  supported node from the server response, preserving parent-child order.
- **FR-004**: Each rendered node MUST show its operation name and, when
  supplied, relation or index context, estimated rows, startup and total cost,
  and row width. Missing fields MUST be labelled as unavailable.
- **FR-005**: The plan view MUST show a summary stating whether it is an
  estimate or an observed execution, and MUST show planning time and total
  execution time when supplied.
- **FR-006**: An analyze request MUST display an explicit pre-execution warning
  that the statement will run, that side effects are possible, and that no
  automatic rollback is promised. It MUST wait for an explicit confirmation.
- **FR-007**: Cancelling an analyze confirmation MUST send no SQL, write no
  history entry, change no transaction state, and preserve the editor and
  existing result view.
- **FR-008**: After analysis, the view MUST show actual rows, actual startup
  and total time, and loop counts only when supplied by PostgreSQL, and MUST
  compare actual per-loop rows with estimated rows without presenting cost
  units as milliseconds.
- **FR-009**: The view MUST identify the most expensive node for a plain plan
  or the slowest measured node for an analyzed plan, with a text label that
  names which basis was used. It MUST call out estimate deviations of at least
  10x as an estimate mismatch.
- **FR-010**: The person MUST be able to move the selected node and expand or
  collapse its children locally, with no query, catalogue reload, filesystem
  operation, clipboard transfer, telemetry, or history write.
- **FR-011**: The plan action MUST be unavailable or clearly explain its
  prerequisite when there is no usable connection, no current statement, a
  busy query, a failed transaction, or an already-wrapped EXPLAIN statement.
- **FR-012**: Failed, malformed, truncated, or connection-lost plan responses
  MUST be labelled as incomplete or unavailable and MUST NOT be rendered as a
  successful plan.
- **FR-013**: Plan text MUST remain meaningful in full, compact, narrow, ASCII,
  no-colour, reduced-motion, and low-height presentations, including a visible
  selection and a keyboard path out.
- **FR-014**: The plan view MUST not alter normal table, JSON, NDJSON, streaming
  export, history, terminal-restoration, or plain-mode output contracts.
- **FR-015**: Plan input, server fields, and diagnostics MUST pass through the
  existing terminal-safe display boundary; no credential, token, or secret may
  appear in the plan model, history, logs, or rendered output.
- **FR-016**: The plan tree MUST have a bounded resource policy for response
  size and node depth or count, and MUST state when the bound prevents a full
  view.

### Key Entities *(include if feature involves data)*

- **Plan document**: The server's structured plan plus its estimate or observed
  execution summary, planning and execution timings, and a bounded tree of
  plan nodes.
- **Plan node**: One operation in the tree, with parent-child order, operation
  context, estimate fields, optional actual fields, additional safe facts, and
  derived warnings or expense markers.
- **Plan session**: The local presentation state for one request, including
  whether it is an estimate or analysis, loading or failure status, selected
  node, expanded branches, and the source statement identity.
- **Analysis confirmation**: A one-shot user decision that records no SQL or
  credential and exists only until the analyzed request is accepted or
  cancelled.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A person who has a connected session and a valid statement can
  open a readable plan within two deliberate actions from the workbench, without
  consulting the keymap documentation.
- **SC-002**: Plans with up to 500 nodes render with every node available for
  navigation; larger plans remain responsive and state exactly what was
  truncated.
- **SC-003**: For a fixed plan fixture, the tree preserves 100% of supported
  parent-child relationships and displays the correct estimate and actual
  values without changing the source SQL.
- **SC-004**: In every analyzed-plan test, zero database effects occur before
  confirmation, and a cancelled confirmation produces zero SQL and zero
  history entries.
- **SC-005**: A known 10x estimate deviation is labelled as a mismatch, while a
  missing actual field is labelled unavailable rather than treated as zero.
- **SC-006**: Full, compact, narrow, ASCII, and no-colour renderings each retain
  the plan identity, selected operation, safety state, and an explicit exit
  path in automated buffer checks.
- **SC-007**: Existing plain, JSON, NDJSON, streaming-export, history, and
  terminal-restoration contract tests remain green with no plan-specific output
  added to those routes.
- **SC-008**: Opening, navigating, expanding, searching, and dismissing a plan
  produces no network, filesystem, clipboard, telemetry, or persistence effect
  beyond the explicitly selected database plan request.

## Assumptions

- PostgreSQL 14 through 18 supplies the documented structured EXPLAIN shape;
  unknown fields and newer node attributes may be omitted from the first view
  but must not make known plan data unreadable.
- The existing statement-boundary lexer is sufficient to identify the
  statement under the cursor. A full SQL parser is outside this slice.
- A plain EXPLAIN request is a read-only planning request, but permission
  errors, RLS effects, and planner settings remain server-owned facts.
- EXPLAIN ANALYZE is an execution request. PostgreSQL permissions and the
  existing advisory impact classification remain the real controls; a SELECT
  that invokes a side-effecting function is not made safe by its first keyword.
- Analysis runs in the current session and transaction. The client does not
  invent a BEGIN or ROLLBACK, because changing transaction semantics would be
  less truthful than showing the current boundary.
- Plan persistence, plan diffing, plan export, visual diagrams, and automatic
  analysis after an ordinary query are outside this slice.
- The current result pane is the natural place to read a plan, so returning to
  an earlier retained result does not require a second query.
