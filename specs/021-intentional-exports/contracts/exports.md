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

## W10-010 complete-row and partial-file accounting - 2026-10-25

The current non-INSERT `StreamWriter::row` increments before writing, and the
real runtime export sink increments `Export` even when the row encoder returns
an error. Both contradict complete-row progress. In addition, `Export::abandon`
ignores a buffered-file flush failure while its message claims every counted
row reached the partial file.

Count a row in `StreamWriter` and `Export` only after the complete row write
returns success. This includes its row terminator and INSERT framing. Failed
header/column validation and failed/partial first or later rows do not advance
either count. Stop on the original writer error; never retry serialization,
replay SQL, cancel automatically, change format bytes or publish a destination.
Extract a small shared runtime event-writing seam if needed to exercise the
actual sink path, keeping I/O ownership in the runtime.

Keep buffered writes for normal throughput. A successful row write means its
complete encoding was accepted by the writer; it does not establish fsync or
durability. On abandonment, retain a typed successful-flush versus unconfirmed
state with the count. When flushing succeeds, preserve the existing incomplete
message/count/path. When flushing fails, state:

`Export incomplete - N complete row(s) accepted by the export writer; partial-file row count is unconfirmed because flushing failed: PATH`

Never label that unconfirmed count as rows present in the file. A `.partial`
file can also contain an incomplete trailing row. Do not guess a file row count
by counting newlines, since CSV/INSERT values can contain newlines. Finishing
failure must retain partial-path recovery guidance without asserting complete
rows are on disk. Completed exports still flush, sync and rename as before.
Existing stdout bytes, exit 9, no-overwrite behavior, successful export counts
and actual database/cancellation outcomes remain unchanged. Overflow/resource
budgets and slow-sink cancellation are separate W10-008 work.

Use failing-first injected `Write` failures before/mid-row and after a successful
prefix for CSV, TSV, NDJSON and INSERT; test the actual runtime sink's rejected
row and successful-prefix counts. Test flush success/failure reporting through
the production abandonment boundary, with a private injection seam if needed.
No full-disk experiment, unsafe file-descriptor manipulation, real values or
production fault switch is needed. Match new sidecar tests to repository policy;
move existing inline tests mechanically only in directly changed modules.
