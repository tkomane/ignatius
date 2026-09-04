# Result-grid contracts

These are user-visible and internal contracts for the interactive grid. Plain,
JSON, streaming export, history, diagnostics, and the SQL editor retain their
existing contracts unless explicitly noted below.

## Interactive controls contract

- The result pane has one named grid-controls entry point reachable from the
  command palette and the advertised `Ctrl+K g` chord.
- The controls surface lists sort, column chooser, width, type-label, freeze,
  and reset actions in plain language and includes current state where useful.
- The column chooser lists every source column once, including duplicate labels,
  and identifies `shown` or `hidden` without requiring colour.
- Enter toggles a column. The last visible column cannot be hidden and the
  limitation is stated in the chooser or result notice.
- Escape peels the chooser/controls surface without changing the result.

## Ordering and selection contract

- Sort cycles ascending, descending, and original server order.
- Only retained rows are sorted. A truncated result says so beside the sort
  state and never implies that unseen rows were fetched.
- Equal values retain source order. SQL NULL is deterministic and distinct from
  an empty string or text `NULL`.
- The selected source row remains selected when a sort changes its display
  position. Row gutters continue to identify source position.
- Sorting, filtering, hiding, resizing, and viewport movement emit no execution
  or network effect.

## Rendering contract

- Headers contain safe column labels. When enabled, available PostgreSQL type
  labels appear on a separate line so type and value cannot be confused.
- Missing or partial type metadata is named as unavailable; cell text never
  becomes a guessed type claim.
- Width adjustments are bounded and explicit. A horizontal viewport keeps the
  selected column visible; a frozen first visible column remains visible when
  space permits, and a narrow terminal states when the selected column is the
  only column that can fit.
- The title states source-column range, hidden count, sort direction, type
  availability, and freeze state whenever those facts are not obvious from the
  layout.
- ASCII and no-colour presentation retains words, source row numbers, selected
  column identity, and control meaning. Reduced motion changes no result state.
- All labels, type names, values, and filter text pass through the existing
  terminal-sanitisation boundary. No terminal control sequence is emitted from
  server content.

## Result and export contract

- `ResultSet` remains the server order and remains bounded by the configured
  retention cap.
- The grid view never mutates `ResultSet` rows, cell values, or column order.
- The existing export action continues to write source columns and filtered
  retained rows, not a silently rewritten query or a server-fetched page.
- Plain and JSON output do not gain grid-only sorting, hiding, widths, or type
  headers. Their existing machine-readable field meanings remain stable.
- A new execution resets result-specific grid state and cannot inherit a hidden
  column or width from an unrelated result.

## Server description contract

- Type description is parse/describe only and is performed after the result has
  been received; it never replays the user statement.
- A description failure cannot turn a successful query into a failed execution.
  It produces unavailable type labels and an honest UI state.
- Type labels are accepted only when source header count and names match.
- Type metadata contains no row values, credentials, tokens, or SQL text in
  long-lived diagnostic or log state.
