# Contract: Intentional export shapes

## Interactive save flow

`Ctrl+K e` and the existing export action open a `Formats` palette. It lists
CSV, TSV, JSON, NDJSON and Markdown, states that the source is retained visible
rows, and performs no I/O. Enter closes that palette and opens the existing path
prompt. Escape from either surface leaves the result and filesystem unchanged.

The accepted path emits one `Effect::ExportRows { path, format }`. The runtime
uses the selected format, the current retained rows and current filter through
the existing partial-file writer. File extension is never interpreted.

## CLI INSERT

```text
ignatius query --format insert --insert-table <identifier> [--output <path>]
```

The command refuses a missing or empty table identifier and refuses duplicate
result column labels. Every result set is emitted as a separate statement:

```sql
INSERT INTO "orders" ("id", "note") VALUES (E'1', NULL);
```

The exact literal spelling is an implementation detail of the encoder, but the
contract is stable: the table and columns are quoted, NULL is SQL NULL, text is
escaped, and raw cell text cannot close a literal or add a statement. `--no-header`
is refused for INSERT because removing the column list would make the output
depend on destination table order.

The buffered and streaming paths share the encoder. Streaming remains bounded,
uses the existing `.partial` destination and writes diagnostics only to stderr.

## Boundaries

- No interactive format choice is available in plain mode.
- No format adds history, clipboard transport, telemetry or a network request.
- Existing table, CSV, TSV, JSON, NDJSON and Markdown output remains unchanged.
- A file at the final destination is complete; interrupted work is named
  `<path>.partial` and exits through the existing export code.
