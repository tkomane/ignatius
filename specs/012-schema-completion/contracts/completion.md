# Completion contracts

These are internal, testable contracts rather than a public network API.

## Runtime message/effect contract

- `Action::Complete` is the only explicit completion request.
- `Message::CompletionLoaded` carries `Result<CompletionCatalog, Diagnostic>`
  and a runtime-generated snapshot timestamp. It has no SQL text or credential.
- `Effect::LoadCompletionCatalog` is issued only from initial/reload object-tree
  loading and uses the existing metadata session.
- A catalogue response is advisory UI state. Its failure does not mark the
  database connection failed, and an old response cannot replace a newer reload
  state.

## Catalogue contract

- Queries are fixed `pg_catalog` SQL with no identifier interpolation.
- System schemas follow the object-tree exclusion rule.
- Relation rows carry schema, exact name, object kind, readability, and detail.
- Column rows carry schema, relation, exact name, and PostgreSQL-formatted type.
- Functions carry schema, exact name, execute/readability fact, and return detail.
- The result is a snapshot. No catalogue query runs on `Insert`, `Backspace`, or
  menu navigation.

## TUI contract

- `Ctrl+Space` opens completion in the editor and is listed in help and the
  keymap document.
- Up/Down moves visibly; Enter accepts; Esc dismisses; printable keys edit the
  current prefix; no default selection is silently inserted.
- The menu shows candidate name, kind, schema/source, and type/detail where
  available. It says loading, unavailable, stale timestamp, and `shown of
  matching/total` states in words.
- ASCII/no-colour rendering remains useful without relying on icon shape or
  token colour.

## Plain-mode contract

- A whole line beginning with `\complete` or `\use` is recognized even while
  the SQL reader holds a partial statement, provided the completion cursor is
  not inside a literal or comment. Other meta commands remain start-only.
- Output is ASCII, line-oriented, and sent to the message stream.
- `\complete [prefix]` prints one candidate per line with a stable number.
- `\use <number|exact-name>` accepts only one printed candidate. Invalid choices
  do not mutate SQL or execute anything.
- Existing `\q`, `\?`, `\c`, semicolon boundaries, and production confirmation
  behavior remain unchanged.

## Security contract

- Catalogue names are untrusted display text and pass through the existing
  display sanitizer before rendering or printing.
- Accepted identifiers use `quote_identifier`, including embedded double quotes.
- Completion never runs or previews a statement as an implicit side effect.
- No secret-bearing value enters a candidate, diagnostic, history entry, or
  completion snapshot.
