# ADR-0005: A hand-written lexer for statement boundaries

- Status: Accepted
- Date: 2026-08-15

## Context

Running "the statement at the cursor" and refusing to send a half-statement both
need to know where statements begin and end. Splitting on semicolons is wrong: a
semicolon inside a string, a quoted identifier, a comment or a dollar-quoted
function body is not a boundary.

## Decision

A small hand-written lexer over PostgreSQL's quoting rules, in
`src/query/statements.rs`. It understands single-quoted strings with `''`
escapes, `E'...'` backslash escapes, double-quoted identifiers with `""` escapes,
`$tag$` dollar quoting, line comments and nested block comments. It preserves
byte offsets so the editor can highlight the current statement and the server's
reported error position can be mapped back into the buffer.

It finds boundaries. It does not parse SQL and must not grow into something that
does.

## Alternatives considered

- **`sqlparser`.** A real parser, but it targets a generic SQL dialect and would
  need to keep pace with PostgreSQL syntax to avoid rejecting valid statements.
  A client must never refuse to send SQL the server would have accepted.
- **`pg_query.rs` (libpg_query).** Uses PostgreSQL's own parser, which is the
  correct answer for real parsing. Rejected for version 1 because it brings a C
  build dependency for a job that needs a lexer. Revisit when a feature needs
  actual parse trees, such as EXPLAIN targeting or refactoring.

## Consequences

- Assumes `standard_conforming_strings = on`, the default since PostgreSQL 9.1.
  Documented in `docs/support/compatibility.md`.
- Covered by tests for every quoting form, unterminated input and multi-byte
  text; unterminated quotes yield one statement rather than hanging or panicking.
- Any feature needing semantic understanding of SQL requires a new ADR rather
  than an extension of this lexer.

## Reversibility

High. One module with a narrow interface.
