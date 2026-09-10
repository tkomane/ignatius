# Quickstart: A result grid you can work

## Focused local checks

```bash
cargo test app::grid
cargo test query::result
cargo test postgres::session
cargo test app::update
cargo test ui::keymap
cargo test ui::layout
git diff --check
```

These checks prove deterministic sorting, state transitions, palette
discoverability, type metadata fallback, and full/compact/narrow rendering
without treating a local fixture as live PostgreSQL evidence.

## Disposable PostgreSQL evidence

```bash
cargo xtask db up
cargo xtask test
```

The live integration check should include a row-producing query with explicit
casts, such as `SELECT 2::integer AS n, 'ok'::text AS label, NULL::boolean AS
active`, and verify that the returned type labels are present while the text
values and NULL remain unchanged. PostgreSQL 18.4 plain and TLS are the local
boundary; the repository verifier injects their URIs when the host can observe
the disposable services.

## Manual TUI check

1. Connect to the disposable database and run a result with at least six
   columns, including a numeric column, a text column, and a NULL value.
2. Focus Results and press `Ctrl+K g`, or open the command palette and search
   for `result grid`.
3. Sort by the selected column twice, then reset. Confirm the title states the
   local sort and the source row number remains visible.
4. Open the column chooser. Hide and restore two columns, attempt to hide the
   last visible column, and confirm the limitation is explicit.
5. Widen and narrow the selected column. Toggle type labels and freeze the
   first visible column, then move right across a wide result.
6. Repeat at 80 columns, 40 columns, ASCII, no colour, and reduced motion.
   Confirm words, row numbers, selected column, type availability, and frozen
   state remain understandable.
7. Run a new query and confirm sort, hidden columns, widths, and horizontal
   position reset while session preferences remain labelled.

## Preservation checks

After applying controls, confirm that:

- the SQL editor text and cursor are unchanged;
- the result's retained values and server row count are unchanged;
- `Enter` still opens the inspector for the selected source cell;
- export still uses the existing source-result contract;
- plain and JSON commands do not acquire grid state or terminal controls.

The Unix-socket, Windows Terminal, Linux hand-terminal, screen-reader, and Warp
renderer checks remain separate evidence gates. A skip is not a pass.
