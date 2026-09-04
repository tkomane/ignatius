# Error-location contracts

These are internal, testable contracts rather than a network API.

## PostgreSQL adapter contract

- A failed execution retains the one-based statement ordinal already used in the
  `While: running statement N` wording.
- `ErrorPosition::Original` is retained as a one-based character position. An
  internal or absent position is not converted into an approximate editor byte.
- Existing SQLSTATE, severity, detail, hint, schema, table, column, constraint,
  and routine fields remain redacted and available.
- Earlier statement results remain in the execution when a later statement fails.

## Pure mapping contract

- Mapping uses the exact failing statement span and the existing PostgreSQL
  boundary lexer.
- Character positions are converted through UTF-8 character boundaries.
- A valid map returns cursor, optional token, absolute line, absolute column,
  statement ordinal, and raw character position.
- Zero, out-of-range, internal, rewritten, and source-mismatch positions return
  no map and never select a nearby character.
- The source marker is one line plus caret, contains no terminal control
  sequences, and uses absolute buffer line numbering.

## Reducer contract

- Execution results are still accepted only for the job currently in flight.
- A valid location is applied only when the editor revision equals the revision
  captured when the job began.
- Applying a location changes only cursor and focus state; it does not change
  text, modified state, or undo history.
- Any editor text change removes the previous location marker and labels the
  diagnostic as referring to the previous submission.
- Missing or invalid location leaves the existing cursor unchanged.
- No location transition emits an effect.

## TUI contract

- The editor marks the error line with an explicit `!` and the token with the
  semantic danger style; no colour or icon is required to understand the state.
- The error surface states statement, line, column, and the source caret when
  available, while keeping the existing error headline, cause, next action,
  detail expansion, and dismissal behavior.
- A valid location makes the editor active and compact layout keeps a bounded
  error summary visible above it.
- Object context is labelled and sanitized. A matching catalogue relation and
  column may add a type; a stale, unavailable, or non-matching catalogue cannot
  replace or contradict server fields.

## Plain and JSON contract

- Plain interactive output is line-oriented and contains no cursor-addressing or
  colour sequences. A mapped error prints statement, line, column, source line,
  and caret; an unmapped error prints the limitation.
- The non-interactive structured diagnostic fallback retains numeric position and
  statement/object fields without embedding SQL source.
- JSON result output continues to write no result document for a failed query;
  diagnostic JSON retains stable position and technical fields on stderr or the
  diagnostic command path.

## Security contract

- No source SQL, result value, password, token, or connection credential is
  copied into long-lived diagnostic, catalogue, history, or log state for this
  feature.
- All new server-derived display values pass through the existing redaction and
  terminal-sanitisation paths.
- Mapping never executes a query, requests a catalogue refresh, retries a
  statement, changes TLS policy, or writes a file.
