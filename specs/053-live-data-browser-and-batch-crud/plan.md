# Implementation Plan: Live Data Browser and Batch CRUD (053)

## 1. Pure Domain Model (`src/crud/mod.rs`)

- Bounds:
  - `DEFAULT_PAGE_SIZE = 50`
  - `MAX_PAGE_SIZE = 500`
  - `MAX_BATCH_MUTATIONS = 200`
- Keyset pagination types and builder:
  - `SortOrder` (`Asc`, `Desc`) with `flipped()`, `sql_keyword()`, `label()`, `Display`.
  - `PageDirection` (`Next`, `Prev`) with `label()`, `Display`.
  - `KeysetCursor { pk_columns, pk_values }`.
  - `BrowseRequest { schema, table, columns, pk_columns, sort_column, sort_order, page_size, cursor, direction, filter_predicate }`
    with `effective_page_size()` (`1..=MAX_PAGE_SIZE`) and `needs_row_reversal()`.
  - `build_keyset_browse_sql(&BrowseRequest) -> Result<String, Diagnostic>`:
    - Validates schema, table, column names, NUL bytes, and filter predicate
      (`;`, `--`, `/*`, `*/`, unbalanced quotes/parentheses refused with
      `DiagnosticKind::Usage`).
    - Validates keyset cursor alignment against `pk_columns` and rejects `NULL`
      PK cursor values.
    - Flips effective sort order and comparison operator when `direction == Prev`
      with an active cursor, appending a reversal comment.
    - Uses `pk_columns` as deterministic tie-breakers (deduplicated against
      `sort_column`) or `ctid` fallback when no primary key exists.
    - Appends `LIMIT (clamped_page_size + 1)` for zero-`COUNT(*)` lookahead.
- Batch CRUD types and SQL synthesizer:
  - `RowMutation` (`Insert`, `Update`, `Delete`).
  - `BatchCrudPlan { schema, table, pk_columns, mutations, dry_run_rollback }`.
  - `BatchCrudPlan::build_reviewed_sql(&self) -> Result<String, Diagnostic>`:
    - Enforces `1..=MAX_BATCH_MUTATIONS`.
    - Refuses keyless `UPDATE`/`DELETE`, partial/mismatched `pk_match`, duplicate
      keys/columns, NUL bytes, empty `Insert` values, and empty `Update` changes
      with `DiagnosticKind::Usage`.
    - Escapes identifiers via `crate::query::identifiers::quote_identifier` and
      literals via single-quote doubling (`None` -> `NULL`).
    - Wraps statements in `BEGIN; ... ROLLBACK; -- dry-run` or `BEGIN; ... COMMIT;`.
- Page summary and renderers:
  - `BrowsePage` with `render_table()`, `render_plain()`, and `render_json()`,
    all using plain ASCII chrome and `[null]` markers via `crate::query::value`.
  - Pure cursor extractors `extract_cursor_from_row` and
    `extract_cursor_from_cells`.

## 2. Sidecar Unit Tests (`src/crud/tests.rs`)

- Verify `build_keyset_browse_sql` across:
  - Default and clamped page sizes (`0` -> `1`, `9999` -> `500`, `50` -> `LIMIT 51`).
  - Identifier quoting with embedded double quotes and mixed case.
  - `sort_column` + `pk_columns` tie-breakers, deduplication when `sort_column`
    is in `pk_columns`, and `ctid` fallback when `pk_columns` is empty.
  - Composite tuple keyset comparisons `(pk1, pk2) > ('v1', 'v2')` and `<` under
    `Desc` / `Prev` with order-flip reversal annotation.
  - Refusal of cursor mismatch, `NULL` cursor values, empty schema/table, and
    SQL injection attempts in `filter_predicate` (`;`, `--`, `/*`, unbalanced
    parens/quotes).
- Verify `BatchCrudPlan::build_reviewed_sql` across:
  - Mixed `Insert`, `Update`, and `Delete` with `dry_run_rollback = true` and `false`.
  - Refusal of empty batches, `> MAX_BATCH_MUTATIONS`, keyless `UPDATE`/`DELETE`,
    mismatched/partial `pk_match`, and empty `Update` changes.
  - Single-quote/semicolon literal escaping and `NULL` handling.
- Verify `BrowsePage` `render_table`, `render_plain`, and `render_json` output
  strictly plain ASCII chrome and `[null]` / empty-string distinction.

## 3. Async PostgreSQL Extractor (`src/postgres/crud.rs`)

- `inspect_table_columns_and_pk(client, schema, table)`:
  - Queries `pg_catalog.pg_attribute` (`attnum > 0 AND NOT attisdropped`) for
    column names in `attnum` order.
  - Queries `pg_catalog.pg_index` (`indisprimary`) joined via
    `generate_subscripts(i.indkey, 1)` for primary-key columns in key order.
- `browse_table_page(client, req)`:
  - Resolves columns/PK via `inspect_table_columns_and_pk` when `req.columns` is empty.
  - Builds SQL with `build_keyset_browse_sql`, runs read-only `client.simple_query`,
    converts cells with `Cell::from_option_bounded`, computes `has_more` / `has_prev`,
    trims to `page_size`, reverses rows for `PageDirection::Prev`, constructs
    `ResultSet` and `BrowsePage`, and derives `next_cursor` / `prev_cursor`.

## 4. Coordinator Integration & Verification (Deferred to Coordinator)

- Wire `pub mod crud;` in `src/lib.rs` and `src/postgres/mod.rs`, plus `Session`
  wrapper methods in `src/postgres/session.rs`.
- Add CLI and TUI browser/CRUD workflows in `src/cli/` and `src/app/` / `src/ui/`.
- Add live PostgreSQL contract tests and run `cargo xtask verify`.
