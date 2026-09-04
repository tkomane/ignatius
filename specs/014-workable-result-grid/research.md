# Research: A result grid you can work

**Date**: 2026-09-04

## Decision: Keep server result order separate from local grid order

`ResultSet` retains rows in the order PostgreSQL sent them and already bounds
memory by `row_cap`, while `Model::filtered_rows` currently returns source-row
indices for selection, inspection, expanded rows, and export. The grid will add
an application-owned ordered index list rather than sorting `ResultSet.rows`.

**Rationale**: The server order remains available for reset and support, stable
row numbers remain meaningful after a sort, and view-only behaviour cannot alter
exports or any future consumer of the raw execution. Sorting only retained
indices makes the truncation boundary explicit.

**Alternatives considered**:

- Mutating `ResultSet.rows` was rejected because it confuses server facts with a
  presentation preference and makes reset semantics lossy.
- Re-running SQL with `ORDER BY` was rejected because it changes query meaning,
  can introduce writes or side effects, and violates the no-automatic-rerun
  safety rule.

## Decision: Stable display comparison with deterministic NULL placement

The grid will compare retained cells locally. SQL NULL sorts after non-NULL
values in both directions, equal values preserve their incoming order, and
numeric-looking values use a decimal-aware comparison without converting them
to floating point. Other values use a case-folded, Unicode-safe display
comparison with the original text as a tie-breaker.

**Rationale**: A floating-point sort can change the order of large PostgreSQL
numeric values. A stable comparator gives a useful investigation order without
claiming PostgreSQL collation, locale, or `ORDER BY` equivalence. The NULL rule
keeps absence visible rather than making it jump between the top and bottom when
the direction changes.

**Alternatives considered**:

- Lexicographic comparison for everything was rejected because identifiers such
  as `2` and `10` appear in the wrong order for normal investigation.
- PostgreSQL-side sorting was rejected as a query rewrite and rerun.
- Locale-sensitive comparison was rejected because results must be repeatable
  across macOS, Windows, Linux, and plain output.

## Decision: Keep grid state ephemeral and scoped to one result

The state will retain source-column visibility, width overrides, sort choice,
horizontal position, selected source column, and presentation preferences in
memory. Sort, hidden columns, width overrides, and horizontal position reset
when a new execution replaces the result. Type-label visibility and freezing
the first column are session preferences and survive a new result, but are not
written to configuration.

**Rationale**: A width or hidden column chosen for one query is often wrong for
the next query. Resetting result-specific state prevents a stale view from
making new data disappear. The two presentation preferences are global reading
choices and are cheap to keep, but keeping them in memory only preserves the
local-first contract.

**Alternatives considered**:

- Persisting widths or hidden columns was rejected because it would need a
  stable column identity and a migration story, while the user asked for a
  current result work surface.
- Resetting every preference was rejected because repeatedly enabling type labels
  or a frozen identity column would make the feature tedious.

## Decision: Use a searchable controls palette rather than printable shortcuts

`Ctrl+K g` will open a result-grid controls surface, and the ordinary command
palette will expose the same action by name. The controls surface offers sort,
column chooser, width changes, type labels, freeze, and reset. The column
chooser lists source positions, so duplicate server labels remain distinct.

**Rationale**: Printable characters must continue to type SQL in the editor.
The project already treats palette entries and chords as discoverability
contracts, and a named surface can describe current state in words. A second
searchable palette for columns scales better than a row of single-letter modes.

**Alternatives considered**:

- Global single-key bindings were rejected because they steal normal editor
  input and behave differently depending on focus.
- A hidden configuration-only feature was rejected because it fails the
  keyboard-first and least-remembering product goal.

## Decision: Describe column types without changing the value path

`tokio-postgres` 0.7.18's `SimpleColumn` exposes only `name()`. The current
simple-query execution path is intentional: it preserves PostgreSQL's text
rendering for arbitrary and extension values. For a result that actually has
columns, the session will request a server parse/describe after the simple
query has completed. The statement is not executed by this request. Type names
are attached only when the described names and count match the received
headers; a failed or unavailable description leaves `None` for those labels.

**Rationale**: This adds truthful type visibility for ordinary row-producing
queries without switching the value path to binary decoding, which would lose
the existing support for types the client does not know. It also avoids making a
pre-execution description failure change whether the user query runs. The
description response carries type labels only, not row values, into the result
model.

**Alternatives considered**:

- Inferring a PostgreSQL type from a cell string was rejected because `"1"`,
  dates, identifiers, and extension values are ambiguous.
- Switching all execution to `query_raw` was rejected because the driver uses
  binary result formats and the client deliberately supports arbitrary text
  values without type decoders.
- Wrapping or replaying the user's SQL to call `pg_typeof` was rejected because
  it could change semantics, execute again, or expose a second write path.
- Using the completion catalogue as the result type authority was rejected
  because an arbitrary expression or alias need not map to one catalogue column
  and a stale snapshot is not server result metadata.

## Decision: Render a horizontal viewport with a first-column pin

The renderer will calculate a source-column window from the available display
width. Without freezing, the window keeps the selected visible column in view.
With freezing enabled, the first visible source column is always reserved and
the remaining width is used for the selected column and its neighbours. Hidden
columns are omitted without changing source order. A title line states the
visible range and hidden count when the full set cannot fit.

**Rationale**: Shrinking every column to three cells makes a wide result
technically present but practically unreadable. A viewport gives the selected
column enough space, while a frozen identity column preserves row context like
the pinned columns in a GUI grid.

**Alternatives considered**:

- Always rendering every column was rejected because it makes values unreadable
  on ordinary terminals.
- A horizontal scrollbar with no selected-column guarantee was rejected because
  keyboard navigation could move to a column that was not visible.
- Unicode-only pin markers were rejected because ASCII and no-colour modes are
  first-class contracts.

## Evidence boundary

Local unit and renderer tests can prove state transitions and styling-loss
behaviour without a server. A live PostgreSQL 18.4 test will prove that the
post-execution description obtains real type labels while preserving text cells.
The full locked verifier will prove repository gates and plain/TLS integration;
the Unix-socket and cross-platform manual gates remain separate evidence.
