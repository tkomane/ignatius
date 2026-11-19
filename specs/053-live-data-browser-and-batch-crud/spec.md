# Feature Specification: Live Data Browser and Batch CRUD (053)

**Feature Branch**: `feat/m13-crud-plandiff-analytics`
**Created**: 2026-11-18
**Status**: In Progress (Phase 1: Pure Domain Model, Async Extractor, and Unit Tests)
**Authority**: Roadmap Milestone 13 (M13: Live Data CRUD, Plan Diff & Analytics)

## 1. Context and Problem Statement

Inspecting table rows with ad-hoc `SELECT *` queries quickly breaks down on
large tables: `OFFSET` pagination degrades linearly on deep pages, row edits
without primary-key guards risk modifying unintended rows, and multi-row edits
need transactional review before execution.

Feature 053 introduces a keyset-paginated live table browser and a guarded
batch CRUD SQL synthesizer:

1. Read-only keyset pagination (`build_keyset_browse_sql` + `browse_table_page`)
   using `LIMIT (page_size + 1)` lookahead instead of `COUNT(*)`, deterministic
   tie-breaker ordering (`pk_columns` or `ctid` fallback), safe `Prev`/`Next`
   cursor reversal, and validated single-expression filter predicates.
2. Guarded transactional batch CRUD synthesis (`BatchCrudPlan::build_reviewed_sql`)
   for `INSERT`, `UPDATE`, and `DELETE`, enforcing primary-key equality matches
   on every `UPDATE`/`DELETE`, strict escaping of identifiers and string
   literals, bounded batch sizes, and `BEGIN; ... ROLLBACK; -- dry-run` or
   `COMMIT;` wrapping without automatic execution.

## 2. User Scenarios

### Scenario 1: Browsing a multi-million-row table without OFFSET or COUNT(*)

An operator opens a table in the data browser with `page_size = 50`. Ignatius
inspects `pg_catalog.pg_attribute` and `pg_catalog.pg_index` (`indisprimary`)
to discover columns in `attnum` order and primary key columns in key order, then
queries `LIMIT 51`. Receiving 51 rows proves `has_more = true` without running
`COUNT(*)`; the 51st row is trimmed and the 50th row's primary-key values
become `next_cursor`.

### Scenario 2: Navigating backward with `PageDirection::Prev`

From page 2, the operator steps backward (`direction = PageDirection::Prev`)
using `prev_cursor`. The query flips the comparison operator (`>` <-> `<`) and
`ORDER BY` direction (`ASC` <-> `DESC`), fetches `LIMIT (page_size + 1)`, trims
the lookahead row first, and reverses the retained slice back into the
requested display sort order.

### Scenario 3: Filtering rows safely

The operator supplies `filter_predicate = Some("status = 'active' AND total > 0")`.
The query builder validates the predicate as a single expression, refuses `;`,
`--`, `/*`, `*/`, NUL bytes, and unbalanced quotes or parentheses with an
explicit `DiagnosticKind::Usage` error, and parenthesizes the predicate before
combining it with keyset cursor clauses.

### Scenario 4: Synthesizing a reviewed batch CRUD script

The operator stages up to `MAX_BATCH_MUTATIONS = 200` row inserts, updates, and
deletes in a `BatchCrudPlan`. Every `UPDATE` and `DELETE` must match every
column of a non-empty `pk_columns` list; keyless updates and deletes are
refused with `DiagnosticKind::Usage`. All identifiers use `quote_identifier`
and all non-null values use doubled-quote SQL string literals so embedded quotes
or semicolons cannot break out of literals. When `dry_run_rollback = true`,
the generated script wraps mutations in `BEGIN; ... ROLLBACK; -- dry-run`.

## 3. Functional Requirements

- FR-5301: Define pagination and batch bounds in `src/crud/mod.rs`:
  `DEFAULT_PAGE_SIZE = 50`, `MAX_PAGE_SIZE = 500`, and
  `MAX_BATCH_MUTATIONS = 200`.
- FR-5302: Implement `build_keyset_browse_sql(req: &BrowseRequest) -> Result<String, Diagnostic>`
  to reject empty schema/table/column identifiers, clamp `page_size` to
  `1..=MAX_PAGE_SIZE`, quote `"schema"."table"` and columns via
  `quote_identifier`, order deterministically (`sort_column` first if present,
  followed by `pk_columns` tie-breakers or `ctid` when `pk_columns` is empty),
  apply tuple keyset predicates `(pk1, pk2) > ('v1', 'v2')` (or `<` when
  descending or moving `Prev` with order flip and row reversal note), validate
  `filter_predicate` against `;`, `--`, `/*`, `*/`, NUL bytes, and unbalanced
  quotes/parentheses, and query `LIMIT (page_size + 1)`.
- FR-5303: Implement `BatchCrudPlan::build_reviewed_sql(&self) -> Result<String, Diagnostic>`
  enforcing `1..=MAX_BATCH_MUTATIONS` mutations, refusing keyless or partial-PK
  `UPDATE`/`DELETE` mutations and empty `UPDATE` change sets with
  `DiagnosticKind::Usage`, escaping identifiers and string literals (`'` -> `''`,
  `None` -> `NULL`), and wrapping statements in `BEGIN; ... ROLLBACK; -- dry-run`
  or `BEGIN; ... COMMIT;`.
- FR-5304: Provide `BrowsePage` with strictly plain-ASCII `render_table`,
  `render_plain`, and `render_json` formatters, distinguishing SQL `NULL`
  (`[null]`) from empty strings and sanitizing control characters.
- FR-5305: Implement `inspect_table_columns_and_pk` in `src/postgres/crud.rs`
  querying `pg_catalog.pg_attribute` in `attnum` order and
  `pg_catalog.pg_index` (`indisprimary`) in key-subscript order.
- FR-5306: Implement `browse_table_page` in `src/postgres/crud.rs` to resolve
  missing column/PK metadata, execute the read-only keyset `SELECT` via
  `simple_query`, convert rows into `ResultSet` (`Cell`) and `BrowsePage`,
  compute `has_more` / `has_prev`, trim to `page_size`, reverse rows when
  paginating `Prev`, and derive `next_cursor` / `prev_cursor`.

## 4. Non-Functional Requirements

- Pure `src/crud/mod.rs` domain model with zero database, filesystem, network,
  or terminal dependencies.
- Sidecar unit tests in `src/crud/tests.rs` (no inline `#[cfg(test)]` modules in
  production files).
- Strictly plain-ASCII formatting chrome in `render_table`, `render_plain`, and
  `render_json`.
- Clean under `rustfmt --edition 2024` and `clippy --all-targets -- -D warnings`.
