# Information architecture

## The full layout, at 80x24 and wider

```text
 Ignatius  [PROD]  [read-write] │  app@pg-01:5432/orders │  TLS TLSv1.3 verify-full │  42 ms
╭  Editor  line 4, column 18 [modified] ───────────────────────────────────────────────╮
│  1 SELECT customer_id, total, created_at                                             │
│  2 FROM orders                                                                       │
│  3 WHERE created_at >= current_date - 7                                              │
│  4 ORDER BY created_at DESC;                                                         │
╰──────────────────────────────────────────────────────────────────────────────────────╯
┏  Results  Completed  1,248 rows, 87 ms [focused] ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓
┃    customer_id │ total    │ created_at                                             ┃
┃ ──────────────────────────────────────────────────────────────────────────────────█┃
┃  1       10482 │  1245.00 │ 2026-08-15 08:14:22+02                                 ┃
┃  2       10483 │   [null] │ 2026-08-15 09:02:11+02                                 ┃
┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛
 Ready  "$user", public   Ctrl+R Run  Ctrl+T Run statement  Ctrl+C Cancel  Ctrl+Q Quit  F1 Help
```

Row 1 is the header: who you are connected to and how. Rows 2 to n are the two
panes. The last row is the footer: what the client is doing, and what to press.

## What each region owns

**Header.** Facts about the connection that change rarely but matter constantly:
product name, environment classification, read/write posture, target, transport
state, and the elapsed time of the last statement. Nothing here is about the
current keystroke.

**Editor pane.** The SQL buffer, its cursor position, whether it has unsaved
edits, and whether it has focus. All four are in the title so the pane body is
only ever SQL.

**Results pane.** Column headers, rows, and in the title the execution status and
summary. Truncation is stated in the summary, never implied by an absence. When
an error is present it replaces this pane, because an error is the result.

**Footer.** What the client is doing right now (`Ready`, `Running`,
`Cancellation requested`), the session's search path, and up to five key hints.

## Narrow and small

- **Below 80x24**: one pane at a time, the focused one, with a shortened header
  carrying the environment, the database, and which pane is focused. Panes are
  dropped deliberately rather than squeezed.
- **Below 40x8**: three lines stating the current size, the required size, and
  `Run: ignatius query`. No layout is attempted.

## Focus

Focus is shown three ways at once: the pane title says `[focused]`, the border
becomes heavier, and the border takes the focus colour. The first two survive
with colour off, which is why the border weight changes rather than only its
colour.

## Overlays

Help is the only overlay. It is centred, lists every binding with its
description, and says `Esc to close` in its own title. There is no other modal
state, and no confirmation dialog exists yet, because nothing in this release is
destructive enough to earn one.
