# Data model: Guided discovery

The feature adds no persisted or server-owned data. All values below are pure
views derived from the existing application model and active keymap.

## Discovery context

| Field | Meaning | Source |
| --- | --- | --- |
| `focus` | Pane receiving the next key | `Model.focus` |
| `connection` | Disconnected, connecting, usable, lost, or failed | `Model.connection` |
| `query_phase` | Ready, running, or cancellation requested | `Model.phase` |
| `editor_has_text` | Whether there is a statement to act on | `Model.editor` |
| `result_state` | No result, rows, no rows, filtered rows, or failed transaction | `Model.last_execution`, `Model.result_filter`, `Model.transaction` |
| `objects_state` | Hidden, loading, unavailable, empty, filtered, or populated | `Model.sidebar_visible`, `Model.tree` |
| `history_state` | Whether saved or historical statements are available | `Model.history`, configured history state |
| `keymap` | Actual active keys for candidate actions | `Keymap` |

The context is recalculated for each render or palette opening. It is not stored
in `Model`, so it cannot become stale independently of the state it describes.

## Contextual hint

| Field | Rule |
| --- | --- |
| Action | Must be an existing `Action` handled by the reducer |
| Key label | Comes from an actual active `Binding` or chord |
| Short label | Plain-language label already used by keymap/help |
| Priority | Determines order within the current focus and state |
| Applicability | Candidate is shown only when it can act, or its prerequisite is stated by the surrounding empty state |

The renderer receives at most five hints. It may shorten spacing to fit the
footer, but it must not remove the key or action meaning silently.

## Discovery palette

The existing `Palette` remains the interaction state: query text, selected
match, entries, and purpose. Feature015 changes the general command palette's
derived entries and copy.

| Entry property | Rule |
| --- | --- |
| Label | Describes user intent, not an internal enum or implementation name |
| Detail | Actual key, scope, current state, or missing prerequisite |
| Group | Session, Editor, Results, Objects, or Files/History |
| Command | Existing reducer action or existing object/query command |
| Availability | Only actionable entries are selectable; unavailable searches receive recovery wording |

## Empty-state recovery

An empty-state message is a derived tuple of heading, current-state sentence,
primary next action, and optional discovery path. It is rendered in the pane
already being viewed and never changes the model by itself.

## State transitions

- Focus changes recalculate hints but do not change SQL or result data.
- Typing or deleting editor text recalculates whether run, complete, and save
  actions are applicable.
- Execution, connection, filter, and tree events recalculate copy from their
  existing messages and do not create a second discovery state machine.
- Opening, searching, and dismissing the palette change only palette overlay
  state; choosing an entry delegates to the existing action/effect contract.
