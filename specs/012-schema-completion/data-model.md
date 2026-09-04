# Data model: Schema-aware completion

## Pure query model

The following values are independent of ratatui, crossterm, tokio, and
PostgreSQL client handles.

### `CompletionCatalog`

An immutable session snapshot:

- `objects`: schema-qualified object names with `CatalogObjectKind`, readable
  fact, and optional function return/detail text;
- `relations`: schema-qualified relation names with ordered
  `CatalogColumn { name, data_type }` values;
- `schemas`: the schema names represented by the snapshot, derived from the
  objects and relations where useful for schema-qualified lookup.

The names are stored exactly as PostgreSQL returned them. No display sanitizing
or quoting is stored in the catalogue.

### `CompletionContext`

The analyzer's answer for the editor cursor:

- `replacement: Range<usize>` - the exact byte range of the current word or
  quoted identifier prefix;
- `prefix: String` - the user-visible text used for case-insensitive prefix
  matching;
- `scope: CompletionScope` - keywords, relation objects, schema objects,
  relation columns, or a combination;
- `in_literal_or_comment: bool` - a hard no-candidate boundary.

The range always lies on UTF-8 boundaries. A scope decision is advisory and
never sends SQL; when syntax is incomplete it falls back to the broadest safe
scope rather than claiming a precise parse.

### `Candidate`

One possible completion:

- `label`: exact stored name for display and exact-name plain-mode selection;
- `insert_text`: quoted or keyword text to put in the buffer;
- `kind`: CTE, table, view, materialized view, foreign table, partitioned table,
  sequence, function, column, or keyword;
- `schema`: source schema when applicable;
- `source`: relation/table name for a column, or a short source label;
- `detail`: type, function return value, or permission wording;
- `rank`: derived sort key, never shown as a confidence claim.

`CompletionList` contains the full match count and a bounded visible prefix of
the sorted candidates. The renderer says both values when the list is bounded.

## Application model

`CompletionState` lives on `Model`:

- `enabled: bool` - automatic popup preference;
- `catalog: CatalogStatus` - `NotLoaded`, `Loading`, `Ready(snapshot,
  loaded_at)`, or `Unavailable(message)`;
- `menu: Option<CompletionMenu>` - active explicit or automatic popup;
- `menu` stores selected index and the current replacement/context so accepting
  a candidate cannot accidentally replace a newly moved cursor position.

Loading is entered by the same reload action as the object tree. A successful
  catalogue message replaces the snapshot and may refresh an active menu that
  was explicitly requested while loading. A failed message becomes
  `Unavailable` and does not retry on each keystroke. Reconnect/reload starts a
  new explicit loading state.

## State transitions

```text
NotLoaded --ReloadObjects--> Loading
Loading --CompletionLoaded(Ok)--> Ready(snapshot, timestamp)
Loading --CompletionLoaded(Err)--> Unavailable(message)
Ready/Unavailable --Ctrl+Space--> menu open (keywords plus honest status)
Ready --typed char/backspace--> menu filtered locally when automatic is on
menu open --Up/Down--> selected candidate changes
menu open --Enter--> one quoted range replacement, menu closes
menu open --Esc--> menu closes, editor bytes unchanged
menu open --other action--> menu closes, action is applied normally
```

`Ctrl+Space` is explicit and therefore bypasses `enabled`. Automatic menus are
suppressed when disabled, but typed characters remain ordinary editor edits.
The reducer never sends an effect for a keypress that only changes completion
filtering.

## Plain-mode model

Plain mode holds an optional `PlainCompletion` outside the SQL `Reader`:

- `\complete` computes from the same catalog/context at the current query
  buffer boundary and prints candidates, even while a partial SQL statement is
  held;
- `\complete prefix` filters the printed list;
- `\use 3` or `\use "exact name"` chooses one candidate and applies the same
  replacement/quoting function;
- `\use` with no valid candidate reports the issue and leaves the buffer as it
  was.

The catalogue is loaded once after connecting, not per line or character. If it
is unavailable, plain mode prints keywords and an explicit unavailable/loading
message rather than claiming no objects exist.

## Invariants

- No completion action changes the buffer before explicit acceptance.
- Dismissal preserves the exact buffer and cursor.
- Acceptance creates exactly one editor undo snapshot.
- Identifier insertion always uses PostgreSQL quoted-identifier escaping.
- A literal/comment cursor never receives a completion menu.
- Candidate counts are truthful for the loaded snapshot and visible limit.
- No completion value is written to configuration, history, logs, or telemetry.
