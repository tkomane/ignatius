# Data Model: A result grid you can work

## Result column descriptor

The server-owned result retains parallel column facts:

| Field | Meaning | Validation |
|---|---|---|
| `index` | Zero-based source position | Must be less than the header count |
| `name` | Server-provided column label | Render only after terminal sanitisation |
| `type` | Optional server-described PostgreSQL type name | Never inferred from cell text; `None` is a visible limitation |

The existing row vectors remain aligned to source positions. Missing cells are
treated as absent for display and never cause a renderer panic.

## Result grid state

`ResultGridState` is application state for the current interactive result. It
does not own rows or values.

| Field | Meaning | Initial/reset rule |
|---|---|---|
| `sort` | Optional source-column index and direction | `None` for a new result and reset |
| `hidden_columns` | Source indices omitted from the grid | Empty for a new result and reset |
| `widths` | Explicit display widths by source index | Empty means automatic; cleared for a new result and reset |
| `show_types` | Whether available type labels are drawn | `true` by default; session-only preference |
| `freeze_first` | Whether the first visible source column is pinned | `false` by default; session-only preference |
| `horizontal_start` | Start position in the visible source-column sequence | Zero for a new result and reset |
| `columns_signature` | Source labels/count used to detect stale view state | Replaced with the result's current headers |

Widths have a minimum of 3 display cells and a maximum of 80 display cells. The
automatic width calculation continues to use safe display text and display
width, not raw bytes. A column chooser addresses the source index, not only its
label, so duplicate labels remain independently controllable.

## Sort state

```text
None
Ascending(column)
Descending(column)
```

Choosing sort on a different column selects ascending. Choosing it on the same
column cycles ascending -> descending -> None. Sorting operates on the filtered
source indices used by the grid renderer, but the stable tie-breaker is the
original source index. The source row number in the gutter remains `index + 1`.

NULL values sort after non-NULL values in both directions. Non-NULL values use:

1. decimal-aware comparison when both values are valid numeric spellings;
2. case-folded Unicode comparison otherwise;
3. original sanitized text and source index as deterministic tie-breakers.

The comparator is a display investigation aid, not a claim about PostgreSQL
collation, locale, ICU, or query ordering.

## Displayed row mapping

The renderer derives a vector of source indices:

```text
retained rows -> filter match -> stable local sort -> displayed positions
```

`Model::selected_row` remains a displayed-position index. The selected source
row is captured before any sort or filter change and restored to its new
displayed position when possible. `selected_column` remains a source index and
is moved to the nearest visible source column when the current column is hidden.

Inspection, expanded rows, and result counts continue to use source rows. The
grid's hidden columns affect only grid presentation and column navigation; raw
exports retain their existing all-column source-result contract.

## Controls and transitions

| Intent | Transition | Effects |
|---|---|---|
| Open grid controls | Open a named palette over local commands | No I/O |
| Choose columns | Open a named palette over all source columns | No I/O |
| Toggle a source column | Flip visibility if another column remains; otherwise keep it visible and state why | Adjust selected column if needed |
| Sort selected column | Cycle `None`/ascending/descending | Reposition selected source row; no execution effect |
| Widen/narrow selected column | Add/subtract a bounded display-width step | Keep source index selected |
| Toggle type labels | Flip `show_types` | No result change |
| Toggle frozen first column | Flip `freeze_first` when a visible column exists | Recalculate viewport |
| Reset grid | Clear sort, hidden columns, widths, and horizontal position | Preserve retained rows and session preferences |
| New execution finishes | Replace signature and clear result-specific state | Existing result reset behavior also clears filter/inspector |

No transition emits `Effect::Execute`, `Effect::Connect`, a catalogue load, a
file write, or a clipboard operation.

## Type metadata state

The session may attach a vector of optional type names to a `ResultSet`. It is
valid only when its length equals the result header count and each described
column name matches the received name at the same source position. A missing
vector, a failed parse/describe request, or any mismatch becomes an unavailable
label. The UI distinguishes:

- all types available: each header can show its type;
- partial types available: available labels show and the title says some are
  unavailable;
- no types available: the title says type information is unavailable.

No row value is stored twice to support this state.
