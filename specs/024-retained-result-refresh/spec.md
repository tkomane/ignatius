# Feature Specification: Explicit retained-result refresh

**Feature Branch**: `024-retained-result-refresh`

**Created**: 2026-09-04

**Status**: Implemented locally; locked verifier green with plain/TLS PostgreSQL; Unix-socket skip; terminal hand checks pending

**Input**: User description: "Continue the roadmap toward a first-class,
GUI-like PostgreSQL client with an explicit way to refresh a retained result
without silently replaying SQL."

## User Scenarios & Testing

### User Story 1 - Refresh the answer in place (Priority: P1)

Someone has a completed result visible in the Results pane and wants to see
the current database state. They choose a clearly named refresh action from the
command palette or use its documented shortcut. Ignatius runs the exact,
eligible source that produced the retained result, leaves the SQL editor
untouched, shows that a refresh is in progress, and replaces the old snapshot
only when the new execution completes.

**Why this priority**: A result that can be explored but not deliberately
refreshed feels like a report viewer rather than a live database workbench.
Refresh is the smallest interaction that closes that loop while preserving
the client's explicit-action safety model.

**Independent Test**: Seed a disposable table, run one read-only result query,
change the table outside the client, invoke refresh, and assert that the new
value appears while the editor text is byte-for-byte unchanged and the action
is represented as one new execution.

**Acceptance Scenarios**:

1. **Given** a retained result from one eligible read-only statement, **when**
   Results has focus and the user chooses Refresh retained result, **then** the
   retained source is used, the Results surface says it is refreshing, and the
   editor buffer is unchanged.
2. **Given** the refresh completes successfully, **when** the new result is
   shown, **then** it is a new retained snapshot with the ordinary result
   counts, elapsed time and local-view reset rules, and no hidden second run.
3. **Given** the retained source contains named parameters, **when** refresh
   is chosen, **then** Ignatius asks for the values again before sending the
   statement and accepts an empty value as data.

### User Story 2 - Know when refresh is not safe or available (Priority: P1)

Someone tries to refresh when there is no retained result, while a query or
plan is active, inside a failed transaction, without a usable connection, or
from a source that could include writes or several statements. Ignatius makes
the action unavailable or refuses it with a short reason and sends nothing.

**Why this priority**: A refresh control that can replay a write or an entire
buffer is a dangerous convenience. The boundary must be visible before the
user relies on the action.

**Independent Test**: Exercise each unavailable or unsafe source shape through
the reducer and command-palette discovery, and assert that no execution effect
is emitted and the reason names the prerequisite or refusal.

**Acceptance Scenarios**:

1. **Given** no retained result, a running query or plan, a failed transaction,
   a disconnected session, or no retained source, **when** refresh is invoked,
   **then** no SQL is sent and the interface says what must change first.
2. **Given** a retained execution whose source has more than one statement or
   is not read-classified, **when** refresh is invoked, **then** it is refused
   before parameter entry and the current editor and result remain untouched.
3. **Given** a read-only result on a server read-only or production-classified
   connection, **when** refresh is invoked, **then** the read is allowed under
   the existing server and connection controls; refresh does not invent a
   write restriction for a read.
4. **Given** the user changes the editor, filter, sorting or selection before
   refresh is chosen, **when** refresh starts, **then** the retained source is
   still the source used and local result state is not mistaken for SQL input.

### User Story 3 - Keep refresh explicit and auditable (Priority: P2)

Someone wants to understand what caused a new database request. Refresh only
starts from a deliberate user action, reports its outcome, preserves the
existing history and parameter privacy rules, and never reruns because the
window opened, a cell update completed, a filter changed, a connection was
restored, or an earlier refresh failed.

**Why this priority**: GUI-like affordances are trustworthy only when their
side effects are legible. Explicit refresh must add convenience without
changing the product's no-replay promise.

**Independent Test**: Drive each non-refresh state transition after a retained
result and assert that none emits an execution effect; then invoke refresh once
and verify one history outcome with the safe template and no prompted value.

**Acceptance Scenarios**:

1. **Given** a retained result, **when** the user changes sorting, filtering,
   focus, layout, or opens and closes a view-only overlay, **then** no refresh
   request is made.
2. **Given** a successful or failed refresh, **when** the server responds,
   **then** Ignatius reports the actual outcome and does not retry or replay the
   source automatically.
3. **Given** a parameterized retained source, **when** history or diagnostics
   are inspected after refresh, **then** the template is eligible under the
   existing rules and entered values are absent.

## Edge Cases

- A trailing semicolon, comments and surrounding whitespace do not make one
  source statement unsafe; an additional statement does.
- A result from a multi-statement buffer is not refreshable even when every
  visible statement appears to read, because the retained source is the whole
  submitted input and replaying it would be ambiguous.
- An unrecognised or structurally changing statement is refused rather than
  treated as a read. SQL impact classification remains advisory, not a
  privilege boundary.
- Named parameters are prompted again in their existing order. Their values
  never become part of the retained source, editor text, history entry or
  debug representation.
- A result with zero rows is still refreshable when it has a retained result
  set and an eligible source; the refreshed empty state is reported honestly.
- A locally filtered, sorted or column-shaped result refreshes from the source
  query. The new result follows the existing new-execution rules instead of
  pretending the prior view is the latest data.
- A refresh attempted after a cell update uses the retained source preserved by
  that workflow; the update itself is never repeated implicitly.
- A result plan is not a query result and is not refreshable through this
  action.
- A refresh that fails because the database changed reports the server error
  and does not retry or silently restore an older claim of freshness.
- If the connection is lost while refresh parameters are being entered, the
  prompt is discarded, the outcome is unknown, and no value or retry is sent.

## Requirements

### Functional Requirements

- **FR-2401**: The interactive client MUST expose a clearly named Refresh
  retained result action in the command palette and on a documented direct
  shortcut; the action MUST remain reachable without a mouse or modal-editor
  knowledge.
- **FR-2402**: The action MUST be available only when Results has focus, the
  client is idle, a retained result set exists, the connection is usable, and
  the transaction is not failed.
- **FR-2403**: Refresh MUST use the exact retained source template, separately
  from the editable SQL buffer, and MUST leave the editor unchanged.
- **FR-2404**: The retained source MUST contain exactly one statement and MUST
  be read-classified before refresh is allowed. Multi-statement, write,
  structural, destructive and unrecognised sources MUST be refused.
- **FR-2405**: The interface MUST show an explicit refreshing state at the
  Results surface and MUST preserve the normal running, cancellation and
  outcome wording while the request is in flight.
- **FR-2406**: Choosing refresh MUST be the only event that starts the new
  request. It MUST emit at most one execution request for that action, and a
  named-parameter source MUST prompt again before emission.
- **FR-2407**: A successful refresh MUST replace the retained result using the
  existing result, row-cap, selection, filter, grid, timing and notice rules.
- **FR-2408**: An unavailable or refused refresh MUST emit no execution effect,
  preserve the editor and retained result, and show a concise reason or
  prerequisite.
- **FR-2409**: Refresh MUST NOT be triggered by startup, reconnect, resizing,
  sorting, filtering, view-only controls, cell-update completion, a failed
  refresh or any other passive state transition.
- **FR-2410**: Refresh outcomes MUST use the existing history, diagnostics and
  cancellation contracts, including one outcome per explicit execution and no
  automatic retry.

### Security and Compatibility Requirements

- **SEC-2401**: The retained source may contain safe parameter placeholders,
  but prompted values MUST remain under the existing secret-value boundary and
  MUST NOT appear in history, logs, diagnostics or debug output.
- **SEC-2402**: The read-classification check MUST remain advisory. PostgreSQL
  permissions, transaction state and server read-only posture remain the real
  controls, and refresh MUST never be presented as a privilege bypass.
- **SEC-2403**: Existing ordinary execution, parameter prompts, production
  confirmation, read-only sessions, result exploration, plain output, JSON
  output, history controls and connection lifecycle MUST remain unchanged.
- **SEC-2404**: Refresh MUST not create a second SQL source of truth. The
  retained execution source remains the only source for this action, while the
  editor remains independently editable.

## Key Entities

- **Retained result source**: The safe, value-free source template associated
  with the displayed completed result.
- **Refresh eligibility**: The set of live and retained facts that determine
  whether the action can start without replaying an ambiguous or mutating
  input.
- **Refresh execution**: One explicitly requested read that follows the normal
  running, cancellation, result, history and diagnostic lifecycle.
- **Refresh parameter prompt**: The existing one-shot masked prompt when the
  retained template contains named placeholders.

## Success Criteria

### Measurable Outcomes

- **SC-2401**: A user with an eligible retained result can discover and start a
  refresh from the Results surface in no more than two deliberate inputs, with
  the action name and shortcut visible in help or contextual guidance.
- **SC-2402**: One hundred percent of tested multi-statement, write,
  structural, destructive, unknown, unavailable and failed-transaction cases
  emit zero execution requests.
- **SC-2403**: In all focused refresh tests, the editor text and revision are
  unchanged from before the action, including when the refresh fails.
- **SC-2404**: Every successful refresh test produces exactly one new result
  outcome and zero automatic follow-up requests; every passive view-only test
  produces zero refresh requests.
- **SC-2405**: Parameterized refresh tests show no entered value in the
  retained source, history entry, diagnostic, effect debug representation or
  machine-readable output.
- **SC-2406**: Existing locked verification remains green for all available
  gates, with any semantic environment skip reported separately rather than
  counted as a pass.

## Assumptions

- Refresh is an interactive-only action because only the full-screen client has
  a retained, navigable result surface; scripted users can explicitly run the
  source query they own.
- The existing command palette, keymap, result lifecycle, named-parameter
  prompt, history and diagnostics are the product surfaces to extend.
- A read-classified statement may still invoke a PostgreSQL function with side
  effects; the classification limitation is shown in the existing advisory
  wording and is not presented as a security guarantee.
- A new result follows ordinary new-execution view rules, so local filtering
  and sorting do not silently carry a stale interpretation onto fresh rows.
- The direct shortcut is a configurable `F6` binding that does not displace the
  existing Run buffer shortcut or printable editor input. An enhanced
  `Ctrl+Shift+R` alias may be available where the terminal preserves shifted
  control modifiers, but it is not the portable discovery key.
