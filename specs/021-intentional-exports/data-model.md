# Data model: Intentional export shapes

## Interactive state

```text
ExportFormat
  Csv | Tsv | Json | Ndjson | Markdown

NamePrompt
  typed: String
  subject: String
  purpose: ExportRows
  format: ExportFormat
  note: String

Effect::ExportRows
  path: String
  format: ExportFormat
```

`ExportFormat` is presentation state and carries no rows or values. The model
continues to hold the retained result as the source of truth. Format palette
opening and searching are pure; the effect is emitted only after a path is
accepted.

## CLI state

```text
OutputOptions
  format: Format
  header: bool
  null_encoding: String
  unicode: bool
  insert_table: Option<String>
```

`insert_table` is required only for `Format::Insert`. It is validated before
target resolution and is quoted at output time. No connection target,
credential or raw output value is stored in configuration or application
state.

## Invariants

- Interactive format order is deterministic and does not depend on a path.
- An interactive export effect has one path and one format.
- INSERT has a non-empty table name, unique column labels and no NUL text.
- NULL is emitted as `NULL`; non-NULL cells are SQL literals.
- Existing file and partial-file invariants apply to every file shape.
