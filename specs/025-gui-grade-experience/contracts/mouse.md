# Contract: Mouse - the verb table

Pinned mouse behaviour. Every verb resolves through the pure
`region_at(model, area, col, row)` hit-region function and dispatches to
the same `Action` the keyboard emits. Anything not in this table changes
nothing. An implementer who needs a verb that is not here stops and
reports.

## Defaults and the capture trade

- `ui.mouse` defaults to `true` (a declared amendment of the previous
  default; the design-document mode table, its rationale and the two tests
  pinning the old default are updated in the same change).
- `ui.mouse = false` disables capture entirely; every capability remains
  reachable by keyboard, proven by running the interaction test set with
  the mouse off.
- Help and the compatibility documentation state: capture takes the
  terminal's own text selection; Shift plus selection restores it in the
  supported terminals; in-app copy remains the existing opt-in OSC 52
  route.
- Windows capture toggling goes through the console API as Crossterm
  already does; the restoration-order test asserts the Unix escape
  sequence and the shared mode ordering exactly as today.

## Verb table

Modifier column: `-` means no modifier; events with unlisted modifiers are
ignored. Ctrl+click is deliberately unbound everywhere: macOS reports
Ctrl+left-click as a right click, so binding either would make one gesture
mean two things.

| Region | Event | Modifier | Action | Keyboard route |
| --- | --- | --- | --- | --- |
| Any pane interior | left Down | - | focus that pane | `Tab` focus cycle |
| Sidebar row | left Down | - | focus tree and select that row | arrows after focus |
| Sidebar row | left double | - | toggle expand or open, exactly as Enter on that row | `Enter` |
| Editor text cell | left Down | - | focus editor and move the caret to that position | arrows |
| Results cell | left Down | - | focus results and select that cell | arrows |
| Results cell | left double | - | open the inspector on that cell | the existing inspector toggle |
| Results header cell | left Down | - | sort cycle for that column: ascending, descending, server order | the existing grid sort controls |
| Results column edge (the header rule cell) | left Drag | - | resize that column within the grid contract's bounds | `Ctrl+K [` / `Ctrl+K ]` |
| Vertical split bar | left Drag | - | move the sidebar split within documented minimums | `Ctrl+K ,` / `Ctrl+K .` |
| Horizontal split bar | left Drag | - | move the editor/results split within documented minimums | `Ctrl+K ,` / `Ctrl+K .` |
| Overlay item | left Down | - | select that item | arrows |
| Overlay item | left double | - | accept that item, exactly as Enter | `Enter` |
| Pane under pointer | ScrollDown / ScrollUp | - | scroll that pane three rows per notch; keyboard focus does not move | `PageDown` / `PageUp`, arrows |
| Results under pointer | ScrollDown / ScrollUp | Shift | scroll visible columns | the existing column scroll keys |
| Header, footer, borders outside the regions above | any | any | nothing | not applicable |
| Anywhere | right or middle button | any | nothing (no context menus in this feature) | not applicable |
| Anywhere | Moved (no button) | any | nothing | not applicable |

## Drag, wheel and double-click mechanics

- A drag is: left Down on a drag region (`VerticalSplit`,
  `HorizontalSplit` or `ColumnEdge { column }`), one or more `Drag(Left)`
  events, ended by `Up` or by any non-mouse user event (`Action`,
  `InputAtSize`, `Pasted` or `Resized`) or modal overlay that interrupts
  it.
- The `Up` event's button is not trusted (some terminals substitute Left);
  a drag ends on any `Up`.
- A drag that leaves the region or the window applies the clamped
  in-bounds position (`20..=60.min(body.width.saturating_sub(30))` for
  `VerticalSplit`, `3..=body.height.saturating_sub(3)` for
  `HorizontalSplit`, and `MIN_COLUMN_WIDTH..=MAX_COLUMN_WIDTH` for
  `ColumnEdge`). A terminal resize during a drag cancels the drag and
  re-clamps every split.
- Double-click window: two left Down events on the same double-clickable
  region target (`SidebarRow { index }`, `ResultsCell { row, column }` or
  `OverlayItem { index }`) within 400 ms (`DOUBLE_CLICK_WINDOW`). Elapsed
  time enters the pure reducer through `Model`'s monotonic mouse clock
  (advanced by the interactive input loop and `Message::Tick`) without
  reading the system clock inside `update`. A third click starts a new
  count, and any non-mouse user input or click on a different region clears
  the tracker.
- Wheel scrolling (`ScrollDown` / `ScrollUp` with no modifiers) over any
  pane interior steps that pane three rows per notch through the existing
  pane movement rules while preserving `model.focus`. `Shift` plus
  `ScrollDown` / `ScrollUp` over the results pane steps one visible column
  through `move_result_column` while preserving `model.focus`.

## Dispatch rule

`Message::Mouse { kind, column, row, modifiers }` is translated by the
pure reducer: `region_at` names the region, this table names the `Action`,
and the reducer handles that `Action` identically to its keyboard route.
Wheel and drag geometry (rows per notch, clamps) live in the reducer as
data so the table above stays the single description.
