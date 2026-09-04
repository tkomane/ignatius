# Contract: Format SQL buffer

## Interactive action

- `Ctrl+Shift+F`, `Ctrl+K q`, the command palette entry, and the configured
  `format-buffer` binding all dispatch `Action::FormatBuffer`.
- The action is available when the editor contains non-whitespace text. It is a
  local action and does not require a connection.
- When a modal confirmation, palette, completion menu, filter, inspector,
  definition, or trust surface is active, existing surface precedence applies;
  the hidden editor is never changed by an unrelated key.

## Successful change

1. The reducer calls the pure formatter with the editor text and cursor.
2. If the result differs, the editor receives one whole-buffer replacement and
   the mapped cursor.
3. The model reports line counts and the existing editor stale-buffer path
   invalidates an old server error location.
4. The reducer returns no database, file, history, metadata, or network effect.

## No-change and refusal

- Already formatted, empty/comment-only, malformed, and over-limit input leaves
  text, cursor, revision, modified state, and undo history unchanged.
- Each outcome has explicit plain-language wording and a next action where one
  is useful. Raw SQL and protected contents never appear in that wording.

## Formatting invariants

- Protected regions are copied byte-for-byte.
- Major clauses and eligible list continuations use deterministic two-space
  indentation.
- Formatting output is idempotent.
- The returned cursor is a valid UTF-8 boundary.

## Plain mode

- `\\format` is recognised as an out-of-band command while SQL is pending.
- The message stream prints a heading and the formatted pending buffer; stdout
  remains reserved for query result data.
- The pending buffer is replaced only when formatting succeeds; no query is
  executed by the command.
