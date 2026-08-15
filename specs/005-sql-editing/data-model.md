# Data model: Writing the SQL

## Editor

The `Editor` is the local SQL buffer owned by `Model`.

| Field | Meaning | Invariant |
| --- | --- | --- |
| `text` | Complete user-entered SQL | Valid UTF-8; no implicit persistence |
| `cursor` | Byte offset into `text` | `0 <= cursor <= text.len()` and on a character boundary |
| `modified` | Whether the buffer has edits since load | Loading text clears it; editing sets it |
| `goal_column` | Desired character column during vertical movement | Cleared by horizontal or editing actions |
| `past` | Prior snapshots | Capped at `HISTORY_LIMIT` |
| `future` | Redo snapshots | Cleared by a new edit |
| `last_edit` | Coalescing kind | Movement must clear its effective coalescing run |

## Snapshot

A snapshot contains `text` and `cursor`. Restoring one clamps the cursor to a
valid character boundary in the restored text.

## Edit kind

`Insert`, `Delete` and `Replace` describe the operation that may coalesce with
the preceding operation. A cursor movement is not an edit, but it is a history
boundary.

## Relationships

- `Model` owns one `Editor`.
- `Action` expresses intent; the reducer is the only route from an action to an
  editor mutation.
- `layout` derives the visible window from `Editor::position()` and the
  terminal size; it does not own a scroll offset.
- SQL statement selection reads the editor text but does not mutate it.
