# Contract: Results grid - wording, sort, bounds

Pinned wording shapes and bounds for the grid story. An implementer who
needs a value that is not here stops and reports.

## Viewport position wording

The Results title area states the visible range in words. Exact shapes,
with thousands separated by commas exactly as existing counts do:

| State | Shape |
| --- | --- |
| Plain result | `rows 120-160 of 1,248 retained` |
| Truncated result | `rows 120-160 of 10,000 retained, of 200,000 returned` |
| Filtered result | `rows 3-42 matching 1,032 of 1,248 retained` |
| Empty result | the existing empty-result wording, unchanged; never `rows 0-0` |

- The range is the rows actually rendered in the viewport, one-based,
  inclusive.
- The existing filter count line and truncation facts remain; this line
  extends them and never contradicts them.

## Sort

- Cycle per column: ascending, descending, then original server order.
- State wording beside the existing order note: `sorted by {column}
  ascending`, `sorted by {column} descending`, or the existing
  server-order wording when unsorted.
- Reachable by: left click on the column's header cell, and the existing
  grid-controls route. Sorting never emits an execution effect and
  preserves selected source-row identity exactly as today.
- Clicking a different column starts that column at ascending; the
  previous sort is replaced, not stacked. One sort column at a time, as
  today.

## Column widths

- Bounds: minimum 4 cells, maximum 80 cells, inclusive, applied to the
  content width before padding.
- Keyboard: `Ctrl+K [` narrows and `Ctrl+K ]` widens the selected column
  by 2 cells per press, clamped to the bounds. Configurable action names:
  `narrow-column`, `widen-column`.
- Mouse: dragging the column's header-rule edge sets the width to the
  dragged position, clamped to the same bounds.
- Widths are local, reversible view state under the existing grid-shape
  rules: reset and a new execution clear them; the last visible column
  keeps its existing protection.

## Scrolling

- Wheel: 3 rows per notch. Shift+wheel: 1 visible column per notch.
- Wheel scrolling moves the viewport only; selection identity does not
  change. Keyboard paging continues to page by what the pane actually
  shows.

## Interaction timing

- Double-click window: 400 ms, same-target, as pinned in the mouse
  contract.
- No other timing exists in this feature; nothing animates beyond the
  existing spinner and meter.

## Splits and zoom (layout bounds)

- Sidebar split: minimum 20 cells, maximum 60 cells. Keyboard step: 2
  cells per press.
- Editor/results split: editor minimum 3 rows, results minimum 3 rows.
  Keyboard step: 1 row per press.
- `Ctrl+K ,` shrinks and `Ctrl+K .` grows the focused pane's primary
  split (sidebar width when the tree is focused, editor share otherwise).
  Configurable action names: `shrink-pane`, `grow-pane`.
- `Ctrl+K z` zooms the focused pane over the body area and restores the
  exact prior split state on repeat. Configurable action name:
  `zoom-pane`.
- On the compact one-pane layout, split and zoom keys explain that the
  layout is already single-pane.
- All values are session-local and never persisted.
