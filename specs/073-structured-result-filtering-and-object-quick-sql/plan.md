# Implementation Plan: Structured Result Filtering and Object Quick SQL

**Branch**: `feat/m27-structured-result-filter` (PR 2A) and `feat/m27-object-quick-sql` (PR 2B)
**Spec**: [spec.md](spec.md)

## Architecture Decisions

1. **Pure Bounded Structured Filter (`src/app/result_filter.rs`)**:
   Tokenizes up to `MAX_FILTER_TERMS = 16` whitespace-separated terms over up to `MAX_FILTER_LEN = 1024` bytes. Supports optional double-quoted column names (`"col name":=val`) and values (`col:"foo bar"`). Resolves column names case-insensitively against `ResultSet.columns`. When a token contains `:` and the left-hand side does not match any column in the active `ResultSet`, it triggers an explicit unknown-column warning only if the right-hand side uses an explicit structured operator (`=`, `!=`, `>`, `>=`, `<`, `<=`, `is:null`, `!null`, `notnull`); otherwise (e.g., timestamps `12:30:00` or URLs), it safely falls back to substring search across all columns.
2. **Pure Object-Tree SQL Builders (`src/query/object_sql.rs`)**:
   All four builders validate non-empty identifiers, reject NUL bytes, cap column lists at `MAX_OBJECT_SQL_COLUMNS = 200` and total SQL at `MAX_OBJECT_SQL_BYTES = 1 MiB`, and quote every identifier via `crate::query::identifiers::quote_identifier`. Parameter names for `INSERT INTO ... VALUES (:param, ...)` sanitize non-identifier characters and deduplicate collisions with numeric suffixes (`_2`, `_3`, ...).
3. **Zero-Execution Editor Integration**:
   All four object actions mutate the active editor buffer through `edit_editor(...)` (recording 1 undo step, syncing `BufferManager`, and retiring stale error markers) and move focus to `Focus::Editor` without emitting any database effect.
