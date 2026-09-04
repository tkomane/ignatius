# Data model: Errors that point at the problem

## Pure query model

### `StatementSource`

The original source span of a statement that was run from an editor buffer:

- `start`: UTF-8 byte offset of the first non-whitespace statement character;
- `end`: byte offset just after the statement text, before its semicolon;
- `number`: one-based ordinal in the full editor buffer.

The span is metadata about the submitted buffer, not a second SQL value. It is
used only while the corresponding editor revision is still current.

### `ErrorLocation`

A validated answer to "where should the editor point?":

- `cursor`: absolute UTF-8 byte offset for the caret;
- `token`: optional absolute byte range for the smallest identifiable token;
- `line` and `column`: absolute one-based editor coordinates;
- `statement_number`: one-based ordinal in the editor buffer;
- `character`: the original one-based PostgreSQL character position.

Every byte offset is a character boundary. A caret can exist without a token
when the server points at whitespace or the end of a statement.

Pure operations:

- map a `SqlPosition` and statement source into an `ErrorLocation`;
- choose a token span from existing syntax spans or a UTF-8-safe identifier word;
- render one source line and a caret from an `ErrorLocation`.

No operation mutates the editor or talks to PostgreSQL.

## Diagnostic model

### `Diagnostic`

The existing layered diagnostic gains two structured facts:

- `statement_number`: the server execution ordinal when known;
- `object`: optional `ObjectContext` containing only server-supplied schema,
  table, column, and constraint values.

The existing numeric `position` remains the raw PostgreSQL character position.
JSON adds the statement and object fields without adding source SQL or result
data. Plain and TUI renderers label the object context before expandable
technical fields.

### `ObjectContext`

An optional redacted set of independent facts:

- schema, relation/table, column, and constraint, each independently optional;
- no field is filled from a guess or from parsing the human message.

The TUI may add a catalogue-derived type only after matching a current
`CompletionCatalog` relation and column. A missing match is a stated limitation,
not evidence that the server object does not exist.

## Application model

`Model` holds transient location state:

- `error_location`: the current mapped location, if any;
- `error_location_note`: why a location is unavailable or why a prior location
  was invalidated;
- `running_editor_revision`: revision captured when the job started;
- `running_source`: optional `StatementSource` for a single-statement run.

`Editor` holds a monotonically wrapping in-memory revision that changes whenever
its text changes, including load, undo, redo, insertion, deletion, and range
replacement. Cursor movement and error positioning do not change it.

## State transitions

```text
Idle --RunBuffer/RunStatement--> Running(revision, source)
Running --editor text change--> Running(revision differs)
Running --stale ExecutionFinished--> Idle, diagnostic ignored by job id
Running --failed with valid position and same revision--> Idle,
  ErrorLocation, editor cursor moved, focus Editor
Running --failed with missing/invalid/stale position--> Idle,
  error, Location unavailable note, cursor unchanged
ErrorLocation --editor text change--> error retained,
  marker removed, previous-submission note
Error --Dismiss--> no error, no location, no location note
```

The editor is not modified by locating an error, so no undo snapshot is added.
No transition from an error state starts a retry or a new database operation.
