# Research: Errors that point at the problem

## Existing seams

- `src/postgres/session.rs` splits a submitted buffer with
  `query::statements::split`, executes each statement in order, and already
  stops at the first failure while retaining earlier real results.
- `src/postgres/error.rs` receives the failing statement ordinal and maps
  `tokio_postgres::error::ErrorPosition::Original` into a one-based PostgreSQL
  character position. It also retains SQLSTATE, severity, detail, hint, schema,
  table, column, constraint, and routine in the shared diagnostic.
- `src/query/statements.rs` preserves the original statement start and end byte
  offsets after trimming surrounding whitespace. Its lexer already ignores
  semicolons inside strings, quoted identifiers, comments, and dollar-quoted
  bodies.
- `src/query/highlight.rs` provides non-overlapping UTF-8-safe token spans for
  literals, numbers, comments, keywords, quoted identifiers, and placeholders.
  Bare identifiers need a small local word-span fallback for error marking.
- `src/app/editor.rs` owns a byte-offset cursor and whole-buffer undo snapshots.
  Cursor movement does not create undo entries, so a diagnostic can reposition
  the cursor without changing SQL or undo history. A revision counter is needed
  to reject an async result after the buffer changes without storing a duplicate
  copy of the SQL in diagnostic state.
- `src/app/update.rs` is the pure reducer. `running_sql` is retained until an
  execution finishes for history, and job identity already rejects stale async
  results. Run-the-statement currently sends the trimmed statement text, so the
  original buffer span must be retained separately for a correct return jump.
- `src/ui/layout.rs` renders the editor and error surface from model state. The
  current error panel calls `render_position_marker` against the whole buffer,
  which is only correct for a one-statement run and does not move or mark the
  editor caret. Full layout can show both panes; compact layout needs a bounded
  error-plus-editor arrangement when a valid location exists.
- `src/cli/plain.rs` owns the submitted SQL at the moment it prints a query
  error, so it can add a source excerpt without putting SQL into `Diagnostic`.
  The outer command error path may only have the structured diagnostic, so its
  stable fallback is statement and character wording.

## Position and token decision

Map the driver position relative to the exact failing `Statement.text`, then add
the statement's original byte start to get an absolute editor offset. Convert
the one-based character index with `char_indices`, never with byte arithmetic.
The mapped location carries the absolute cursor byte, the containing token range
when one can be identified, and absolute one-based line and column. A position
at the end of a line is a valid caret location but has no token to colour. A
position outside the statement, zero, or an internal/re-written driver position
returns no location.

The source caret renderer consumes the mapped absolute location, so multi-line
and multi-statement excerpts cannot accidentally be calculated from the wrong
origin. It stays static: an error marker is frequent feedback, not an animation.

## Stale-buffer decision

At execution start, retain the editor revision and, for a run of one statement,
its original source span and ordinal. On completion, map only when the editor
revision equals the revision captured when the job began. A changed buffer keeps
the diagnostic but removes the old marker and says that its position belongs to
the previous submission. This is more truthful than trying to find similar text
after an edit and costs no SQL copy in the long-lived model.

## Object-context decision

The PostgreSQL fields already collected are the authoritative facts. Promote
non-empty schema, table, column, and constraint fields into a labelled context
line in the TUI and plain diagnostic. Quote names for display as SQL identifiers
only after sanitising the final terminal text. If the existing completion
catalogue has a matching relation and column, include its stored type as a
catalogue-derived detail. If it is absent, loading, unavailable, or stale, say
so and retain the server fields rather than issuing a second query or declaring
the object missing.

## Presentation decision

The editor receives focus after a valid query location so the next keystroke is
usefully placed. The error panel remains visible and keeps its existing
collapsed technical detail, dismissal, transaction recovery, and retry wording.
The editor gutter uses an explicit ASCII `!` on the error line, and the token
uses the existing semantic danger style. The same words appear in no-colour and
ASCII modes, and compact layout reserves a small error summary above the editor
when both must be visible.

## Verification decision

Pure tests prove exact byte boundaries, UTF-8 characters, tabs, newlines,
comments, strings, dollar quotes, multi-statement ordinals, token spans, and
unmappable positions. Reducer tests prove focus, cursor movement, unchanged undo
history, stale revision rejection, and marker invalidation after edits. Renderer
tests prove the error line, line/column text, object context, ASCII output, and
compact layout. Plain/JSON tests prove line-oriented output, no control
sequences, stable numeric position, and unchanged stdout behavior. Live tests
prove PostgreSQL syntax position and constraint fields against the disposable
server. The full locked verifier remains the release gate, not a substitute for
the explicit skips and platform hand checks.
