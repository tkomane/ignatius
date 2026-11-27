# Implementation Plan: Result-Grid Quick-Stats, Query-Ready SQL & JSON Copy, and Docs Alignment

**Spec**: [spec.md](spec.md)

## Delivery Slices

Feature 071 (Milestone M25) is delivered across two pull requests:

### PR 3A - Live Column Quick-Stats & Query-Ready SQL/JSON Copy (`feat/m25-grid-stats-sql-copy`)

1. **`src/app/grid.rs` & `src/app/grid/tests.rs`**:
   - Add `MAX_QUICK_STATS_DISTINCT = 10_000`, `NumericQuickStats`, `ColumnQuickStats::compute`, `GridCommand::ToggleColumnStats`, and `ResultGridState::show_quick_stats` (default `true`, retained across `reset_for_result` and `reset_view`).
2. **`src/query/copy_sql.rs`, `src/query/copy_sql/tests.rs`, & `src/query/mod.rs`**:
   - Export `pub mod copy_sql;` and implement `build_where_clause`, `build_in_list`, and `build_row_json` with quote escaping, NUL rejection, distinct encounter-order bounding (`200`), and 1 MiB payload limits.
3. **`src/app/message.rs`, `src/app/model.rs`, `src/ui/keymap.rs`, `src/app/discovery.rs`, `src/app/update.rs`, `src/ui/review.rs`, `docs/design/keymap.md`**:
   - Add `ToggleColumnStats` (`toggle-column-stats`), `InsertCellWhereClause` (`insert-where-clause`), `InsertColumnInList` (`insert-in-list`), and `CopyRowAsJson` (`copy-row-json`).
   - Route `InsertCellWhereClause` and `InsertColumnInList` through `edit_editor` (`insert_paste`) as single undo steps with `BufferManager` sync and clear notices, never running SQL.
   - Route `CopyRowAsJson` through `pending_copy` / `CopyKind::RowJson` / `ClipboardPayload::try_new` / `review.rs` so `[clipboard] osc52 = true`, `MAX_OSC52_BYTES`, and explicit confirmation are enforced.
4. **`src/ui/widgets/results.rs`, `src/ui/widgets/mod.rs`, `src/ui/widgets/results/tests.rs`, `src/app/update/tests.rs`**:
   - Render the 1-line summary bar at the bottom of the Results pane when `show_quick_stats` is true, columns and visible rows exist, and inner pane height has room (`>= header_rows + 2`), keeping `grid_capacity` and mouse hit-testing aligned.

### PR 3B - M23-M25 Docs, Roadmap & README Sync (`docs/m23-m25-roadmap-readme-sync`)

1. **`docs/product/roadmap.md`, `docs/status.md`, `docs/support/compatibility.md`, `CHANGELOG.md`, `README.md`, & `tests/documentation_matches_the_build.rs`**:
   - Reconcile completed M23, M24, and M25 deliverables, feature slugs, and verification evidence.

## Verification Gates

- `cargo --locked xtask verify` (formatting, clippy, unit tests, UI/layout/doc/CLI contracts, and integration suite).
