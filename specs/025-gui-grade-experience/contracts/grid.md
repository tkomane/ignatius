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

## Selected expanded fields - correction specified 2026-10-25

In the existing expanded-row Results view, the selected field's header,
selection arrow and value use the active Selection foreground and background
at truecolor and 256-colour depth. Explicit child-span Header, Focus, Text or
NullValue colours must not override that selected ink. Preserve their existing
semantic modifiers, the selection arrow, field text and NULL/empty/literal-NULL
distinction. An unselected field keeps its ordinary token styling.

At basic16 and colour-off depths, retain the existing reversed selection and
colour-depth rules; do not introduce a painted background or colour under
colour-off. Selection movement, scrolling, source identity and all model state
remain unchanged. This corrects a reproduced span-composition defect, without
changing palette RGB values, geometry, bindings or the custom-theme contract.
Prove actual rendered Buffer cells across the three built-ins, four depths,
three glyph tiers and both densities. A palette contrast calculation alone
does not prove the composed renderer.

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

### Keyboard geometry clarification - 2026-10-25

These choices resolve the previously unspecified interactions before T039's
keyboard core is built. They preserve the existing initial geometry and add
no new numeric default or persistent setting.

- Until adjusted, sidebar width and editor share use the current default
  heuristics. Stored preferences are distinct from effective rectangles:
  resizing clamps what can be drawn without destroying the saved preference.
  The three-row pane minimum means the outer pane including its border.
  The main column retains its existing 30-cell minimum where the full layout
  fits; a smaller layout uses its existing compact/too-small tier instead.
- In Results focus, shrink/grow still changes the editor share, as specified
  above. A query plan occupies Results and uses the same split and zoom target.
- Zoom preserves the unzoomed preferences. At unchanged terminal size, repeat
  zoom restores the exact prior rectangles. At a changed size, preferences
  restore exactly while effective rectangles clamp to the current body.
  Moving keyboard focus while zoomed shows the newly focused pane, so focus
  never moves to a hidden pane. Hiding the sidebar keeps its existing focus
  normalization to Editor. Each zoomed pane title includes `(zoomed)`.
- A split action while zoomed first restores the saved layout, then applies
  its ordinary step. No invisible split adjustment occurs behind the zoom.
- Existing modal overlays retain precedence; these actions do not act through
  help or another open overlay. A palette command closes the palette before
  applying to its underlying pane focus. Compact layout feedback is
  `Layout is already single-pane at this terminal size.` and survives render
  ticks until the next user action. Zero-area rectangles remain safe even
  when there is no space to display feedback.
- Starting a chord while completion, a definition, the inspector or a filter
  owns input keeps that owner until the continuation is resolved. The new
  layout actions remain no-ops against that owner through both default chords
  and configured direct bindings. Other existing commands keep their current
  modal handling after resolution; password and confirmation precedence stays
  unchanged.
- Rendering, hit regions, editor completion anchors and paging consume the
  same effective geometry. Page steps use each renderer's actual content
  capacity, including grid toolbar/type rows, tree titles and Plan headings.
  A zero-capacity view retains a minimum navigation step of one without
  claiming that a row was drawn.

### Error visibility and narrow feedback clarification - 2026-10-25

Zoom must not hide an active diagnostic or failed-transaction recovery surface.
After a reducer transition, if either state is active and focus is outside
Results, restore the unzoomed layout without changing saved split preferences
or editor focus. This applies regardless of which message produced the error.
Results may still be zoomed because its error/recovery surface remains visible.
At the minimum usable compact size (40 columns by 8 rows), the ordinary
diagnostic/source view reserves at least three outer rows for the diagnostic
and three for the editor, so the error headline has a content row. A border-
only error pane is insufficient. Larger compact error views retain the existing
height heuristic and eight-row cap; the too-small tier is unchanged.
Moving focus away from Results while such a safety state is active also
restores the layout. A non-Results zoom request explains:
`Zoom Results to keep error and recovery details visible.`

Footer feedback keeps existing query/transaction state. If the full sentence
does not fit, use these ordered short forms before ordinary truncation:

| Notice | First short form | Smallest form |
| --- | --- | --- |
| Already single-pane | `Already single-pane.` | `Single-pane` |
| Error/recovery zoom refusal | `Zoom Results for details.` | `Zoom Results` |

Use the first wording that fits the actual available cells. The refusal stays
through ticks until the next user action, like the full-size notice. These
exceptions preserve existing safety meaning; they do not dismiss errors,
change transactions or emit database effects.

If even the smallest wording cannot fit beside the footer's query/transaction
facts, retain those facts and show the first fitting notice wording in the
body's final border row, inset inside its existing corners. This temporary
caption replaces border cells only, never pane content or geometry. Clearing
the notice restores the border. An ellipsis that loses `Single-pane` or
`Zoom Results` is not a sufficient explanation at the usable 40-column tier.

This subset does not enable mouse capture, implement split dragging, add
density settings or complete US6/US7 acceptance. Those remain their existing
tasks and independent tests.
