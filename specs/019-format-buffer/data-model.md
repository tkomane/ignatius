# Data model: Format the SQL buffer

## FormattedSql

An ephemeral pure-function result:

| Field | Meaning | Safety rule |
| --- | --- | --- |
| `text` | Deterministic formatted SQL | Protected source slices are copied exactly; no raw value enters model notices or Debug output |
| `cursor` | UTF-8-safe byte offset in `text` | Maps the source token or nearest logical whitespace boundary |

## FormatError

An ephemeral refusal:

| Variant | Meaning | User wording |
| --- | --- | --- |
| `TooLarge { bytes }` | Source exceeds 1 MiB | State the size and use a saved-query route; never build the formatted result |
| `Unterminated { kind, position }` | A protected lexical region has no closing boundary | State the region kind and line/column; do not echo source text |

The error has no SQL string and has a safe Debug representation.

## FormatNotice

Transient value-free state on the application model:

| Variant | Stored data | Meaning |
| --- | --- | --- |
| `Applied { before_lines, after_lines }` | Counts only | The editor changed in one undoable operation |
| `AlreadyFormatted` | None | The formatter found no change |
| `Empty` | None | There is no SQL to format |
| `Refused { message }` | Sanitised safe message | The formatter made no edit and gives a next action |

The notice is cleared by a later editor mutation and is not written to history,
machine output, or a file.

## ProtectedRegion

The scanner recognises these source slices and copies them as opaque bytes:

- ordinary and escape string literals
- quoted identifiers
- dollar-quoted bodies, including tags
- line and nested block comments

## State transitions

```text
editor buffer
    |
    +-- empty/comment-only ----------> Empty, no edit
    +-- over 1 MiB -------------------> Refused, no edit
    +-- unterminated protected slice -> Refused, no edit
    +-- formatted text differs -------> Applied, one undo entry
    +-- formatted text identical -----> AlreadyFormatted, no edit
```
