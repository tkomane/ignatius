# Research: Intentional export shapes

## Existing repository facts

1. `Format` already owns CSV, TSV, JSON, NDJSON, Markdown and table output, and
   `OutputOptions` is shared by buffered and streaming CLI paths.
2. `StreamWriter` already receives columns before rows, so SQL INSERT can stream
   one complete statement per row without buffering a large result.
3. `Export` writes to a `.partial` path, refuses an existing destination and
   renames only after flush and sync. Interactive format choice must reuse it.
4. The interactive export currently opens `NamePrompt` directly and writes CSV.
   The palette already supports typed commands, deterministic entries,
   subsequence search and modal rendering, so a format-purpose palette is the
   smallest GUI-like extension.
5. `quote_identifier` unconditionally quotes and doubles embedded quotes. It is
   the correct boundary for both the INSERT table name and column names.
6. `Cell` distinguishes SQL NULL from empty text and exposes server-rendered
   text. The INSERT encoder must preserve that distinction and refuse NUL.

## Decisions

### Format first, path second

The interactive flow is format palette, then the existing visible path prompt.
The path never chooses a format by extension, which keeps the action explicit
and makes a later format picker easy to extend.

### SQL INSERT is explicit and CLI-first

`--format insert` requires `--insert-table`. The interactive result already has
an export path prompt but no safe table-destination prompt; adding one would
make this slice larger and could make an executable file look harmless. The
CLI route is still useful for scripts and is clear about the target.

### Text literals, not guessed types

The query path deliberately retains server text. INSERT emits PostgreSQL
escape-string literals for non-NULL cells and `NULL` for SQL NULL. It does not
infer integers, dates or JSON from display text, because an inference would
change meaning for extension and domain values.

### Fail closed on ambiguous SQL

Duplicate column labels and NUL-containing values are refused. The ordinary
tabular formats remain permissive because duplicate labels are displayable
there; INSERT cannot safely choose a unique column list in that case.

## Rejected alternatives

- **Infer format from `.json` or `.sql`**: paths are names, not hidden options;
  this makes scripts and people less predictable.
- **Use a native clipboard or shell command for export**: file output already
  has an atomic, bounded boundary and no need for another transport.
- **Infer SQL types from values**: text rendering does not carry enough type
  authority, and inference could turn a literal into different data.
- **Silently omit duplicate columns**: dropping a source column is data loss and
  would make the generated statement look more complete than it is.
