# Feature Specification: Guided discovery

**Feature Branch**: `015-guided-discovery`

**Created**: 2026-09-04

**Status**: Implemented locally; task list complete and locked-verifier evidence
recorded in `docs/status.md`, with a Unix-socket skip. Manual terminal,
screen-reader and first-use observation remain separate evidence gates.

**Input**: User description: "Continue the roadmap autonomously, with a first-class,
GUI-like terminal experience inspired by Neovim, Codex CLI, Claude Code CLI, and
Warp."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Start without a manual (Priority: P1)

A person opening Ignatius for the first time can tell where to begin from the
screen itself. The empty editor, result pane, and connection state explain the
next useful action in short, direct language without opening a modal or
requiring prior knowledge of the keymap.

**Why this priority**: The product promises to demand less remembering than a
traditional terminal client. The first frame is the first and most important
proof of that promise.

**Independent Test**: Start with an empty buffer in an unconnected session and
in a connected session, at full and compact sizes. The screen names the current
state, gives one valid next action, and leaves the editor and connection state
unchanged.

**Acceptance Scenarios**:

1. **Given** a fresh session with no connection, **When** the first frame is
   shown, **Then** it identifies that a connection is required and explains
   that results will appear after a query, without suggesting that a query can
   run yet.
2. **Given** a connected session with an empty editor and no result, **When**
   the first frame is shown, **Then** it labels a clear starting point, names
   the run action, and explains how to discover more actions.
3. **Given** the editor contains SQL or a result already exists, **When** the
   user returns to an empty pane, **Then** the guidance reflects the current
   state rather than falsely presenting the session as new.

---

### User Story 2 - Know what matters at the current focus (Priority: P1)

A person moving between Editor, Results, and Objects sees a compact hint rail
that changes with focus. It shows the most useful actions for that pane and
state, uses the actual active key bindings, and remains readable without
colour, Unicode, or a wide terminal.

**Why this priority**: Visible focus is only useful when the next action is also
visible. Contextual hints turn the three-pane layout into something that can be
learned by doing rather than memorised in advance.

**Independent Test**: Render each focus with an empty state, a populated state,
and a busy or filtered state where relevant. Confirm that the hint rail changes,
contains only applicable or clearly explained actions, stays within its bound,
and never advertises an action that the active keymap cannot perform.

**Acceptance Scenarios**:

1. **Given** Editor has focus, **When** the footer is rendered, **Then** it
   prioritises run, completion, focus movement, and the command palette over
   result-only actions.
2. **Given** Results has focus with retained rows, **When** the footer is
   rendered, **Then** it prioritises movement, inspection, filtering, grid
   controls, and focus movement; while a query runs it instead explains
   cancellation.
3. **Given** Objects has focus, **When** the footer is rendered, **Then** it
   prioritises opening, filtering, definition, and focus movement, and does not
   imply that typing edits SQL.
4. **Given** a configured replacement key, **When** that action is advertised,
   **Then** the hint uses the configured key rather than a stale default.

---

### User Story 3 - Ask what can be done (Priority: P2)

A person can open a command palette and search by intent, not by remembering an
internal command name. The palette identifies itself as the command palette,
groups actions by purpose, exposes the current focus and connection state, and
explains prerequisites when an action is not currently useful.

**Why this priority**: Hints help with the next step; a searchable palette is
the escape hatch for everything else. It is the terminal equivalent of a
graphical client's visible menus without requiring a mouse.

**Independent Test**: Open the palette in each focus with and without a
connection, search for common intents such as run, inspect, help, objects, and
save, and choose or dismiss entries. Confirm that relevant entries are found,
their effects match their descriptions, and no database or file effect occurs
merely from opening or searching.

**Acceptance Scenarios**:

1. **Given** the user presses the palette key, **When** the overlay opens,
   **Then** its title says `Command palette`, its guidance says how to search and
   leave, and its first entries are useful in the current state.
2. **Given** the user searches for `run`, **When** the session is connected and
   the editor contains a statement, **Then** the matching run actions appear
   with their actual keys and statement scope.
3. **Given** the user searches for an unavailable action, **When** an action
   requires a connection or a result that does not exist, **Then** the palette
   states the prerequisite instead of silently doing nothing.
4. **Given** the user presses Escape, **When** the palette closes, **Then** the
   buffer, focus, selection, and connection remain unchanged.

---

### User Story 4 - Recover from an empty or blocked state (Priority: P2)

A person who reaches an empty result, an empty object tree, a filter with no
matches, no saved queries, a failed connection, or a failed transaction sees a
specific safe next action. The guidance describes what is known and never
pretends that an unavailable capability succeeded.

**Why this priority**: Empty states are where a terminal interface most often
feels like it has stopped responding. Recovery wording keeps the experience
calm and prevents repeated or unsafe guesses.

**Independent Test**: Drive each named empty or blocked state through the pure
application reducer and renderer, then verify that the screen provides a
state-specific recovery action and emits no unrequested execution or network
effect.

**Acceptance Scenarios**:

1. **Given** a result filter matches no retained rows, **When** Results is
   rendered, **Then** it says what was searched and how to clear the filter.
2. **Given** the transaction is failed, **When** Results is rendered, **Then**
   it names rollback as the recovery action and does not offer another query as
   though it could run.
3. **Given** no saved query exists, **When** saved-query search is opened,
   **Then** it explains how to save the current buffer rather than showing a
   blank overlay.

### Edge Cases

- A disconnected, connecting, lost, or failed connection must use different
  wording and must not advertise a runnable query when the session is unusable.
- A connected session with an empty buffer, a non-empty buffer, an empty result,
  and a truncated result must not share one misleading generic hint.
- Results with no rows, filtered-out rows, or no result set must remain
  distinguishable from a query that is still running.
- The command palette must remain useful when the query matches nothing, when
  the object tree has not loaded, and when there are no saved queries.
- A custom keymap, disabled colour, ASCII glyphs, a narrow terminal, reduced
  motion, and a screen reader must receive the same meaning in different forms.
- Opening, searching, dismissing, or rendering discovery surfaces must not
  execute SQL, reload metadata, write files, record history, or change the
  submitted buffer.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The initial interactive view MUST identify the current connection
  state and provide one concrete, valid next action without requiring a help
  document.
- **FR-002**: Empty editor and result states MUST use state-specific wording
  that distinguishes not connected, ready with no query, running, no rows,
  filtered-empty, and failed-transaction states.
- **FR-003**: The interface MUST show a contextual hint rail whose actions are
  selected from the actual active key bindings and the current focus.
- **FR-004**: The contextual hint rail MUST prioritise no more than five actions
  at a time, remain readable within the available footer width, and retain the
  full action meaning in ASCII and no-colour modes.
- **FR-005**: Editor hints MUST expose a discoverable path to run, complete,
  move focus, open the command palette, and get help, subject to the current
  state.
- **FR-006**: Results hints MUST expose a discoverable path to move, inspect,
  filter, open result-grid controls, move focus, and cancel while busy, subject
  to the current state.
- **FR-007**: Objects hints MUST expose a discoverable path to open, filter,
  inspect definitions, move focus, and return to the editor, subject to the
  current state.
- **FR-008**: A configured replacement key MUST be reflected in every hint and
  palette detail that advertises that action; no discovery surface may promise a
  stale default key.
- **FR-009**: The general command palette MUST identify itself, explain how to
  search and leave, group entries by user intent, and include current focus or
  prerequisite details where those facts affect the action.
- **FR-010**: Searching the command palette MUST match action labels and useful
  details by intent, including common words such as run, inspect, help, objects,
  save, and result.
- **FR-011**: An action that is not currently applicable MUST either be omitted
  from the contextual list or state its prerequisite plainly; selecting it MUST
  not silently claim that work occurred.
- **FR-012**: Every empty or blocked discovery state MUST name a safe recovery
  action, or explicitly say when no action is available yet.
- **FR-013**: Discovery surfaces MUST be non-modal where possible, dismissible
  with Escape, and must not change SQL, selection, connection state, history,
  result data, or transaction state merely by being opened, searched, or
  rendered.
- **FR-014**: Discovery wording MUST preserve meaning in colour-disabled,
  ASCII, narrow-terminal, reduced-motion, and plain presentation modes.
- **FR-015**: The feature MUST NOT add telemetry, cloud calls, automatic query
  execution, automatic metadata reloads, clipboard transfers, or new persisted
  onboarding state.
- **FR-016**: Existing script output, export, history, error, completion,
  object-navigation, and terminal-restoration contracts MUST remain unchanged.

### Key Entities *(include if feature involves data)*

- **Discovery context**: The current focus, connection posture, query phase,
  result state, filter state, and visible capabilities that determine which
  guidance is useful.
- **Contextual hint**: A short action label paired with the actual key that
  invokes it, ordered by usefulness for a discovery context.
- **Palette entry**: A searchable action or object with a plain-language label,
  intent group, key or prerequisite detail, and a safe outcome.
- **Empty-state recovery**: A bounded, state-specific explanation that tells a
  person what is missing and what they can do next.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: In a fresh 80-column by 24-row session, the first rendered frame
  names the current state and one valid next action without opening help.
- **SC-002**: In every supported focus and tested state, the footer shows no
  more than five contextual actions, and every shown key resolves to the action
  named beside it.
- **SC-003**: Searching the command palette for each of `run`, `inspect`,
  `help`, `objects`, `save`, and `result` returns at least one relevant entry
  whenever that capability is applicable, or states its prerequisite when it is
  not.
- **SC-004**: All defined empty and blocked states render a non-blank recovery
  explanation in full, compact, narrow, ASCII, and no-colour presentations.
- **SC-005**: Discovery actions produce no execution, connection, metadata,
  file, history, clipboard, or query-buffer side effect in reducer tests.
- **SC-006**: Existing scripted and export outputs are byte-for-byte unchanged
  when discovery-only state is present.
- **SC-007**: A new user can identify how to run a first query, move focus, open
  help, and open the palette from the first frame without consulting external
  documentation.

## Assumptions

- The existing keyboard model, command palette, focus cycle, error states,
  completion, result grid, and plain mode remain the product foundations.
- Discovery is session-local presentation state; the feature does not persist
  a tour, dismissal flag, analytics event, or user profile.
- The active configured keymap is the only authority for advertised keys.
- Mouse support is not required for this slice; keyboard operation remains
  complete and visible.
- Existing database, file, and history effects are invoked only after a user
  chooses an action whose prerequisite is satisfied.
- Manual screen-reader, Warp, Windows, and Linux terminal checks remain
  separate release evidence even when automated rendering checks pass.
