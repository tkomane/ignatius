# Tasks: Result-Grid Quick-Stats, Query-Ready SQL & JSON Copy, and Docs Alignment

**Input**: Design documents from `/specs/071-result-grid-quick-stats-sql-copy-and-docs/`
**Prerequisites**: `spec.md`, `plan.md`

## Phase 1: PR 3A - Live Column Quick-Stats & Query-Ready SQL/JSON Copy (`feat/m25-grid-stats-sql-copy`)

- [x] T001 Implement `MAX_QUICK_STATS_DISTINCT`, `NumericQuickStats`, `ColumnQuickStats::compute`, `GridCommand::ToggleColumnStats`, and `ResultGridState::show_quick_stats` in `src/app/grid.rs` with unit tests in `src/app/grid/tests.rs`.
- [x] T002 Implement pure SQL/JSON builders (`build_where_clause`, `build_in_list`, `build_row_json`) in `src/query/copy_sql.rs`, export in `src/query/mod.rs`, and add unit tests in `src/query/copy_sql/tests.rs`.
- [x] T003 Wire `ToggleColumnStats`, `InsertCellWhereClause`, `InsertColumnInList`, and `CopyRowAsJson` across `src/app/message.rs`, `src/app/model.rs`, `src/ui/keymap.rs`, `src/app/discovery.rs`, `src/app/update.rs`, `src/ui/review.rs`, and `docs/design/keymap.md`.
- [x] T004 Render the 1-line column quick-stats summary bar in `src/ui/widgets/results.rs` (synced with `grid_capacity` and `src/ui/widgets/mod.rs` hit-testing) using `Theme`/`Token` only, and add widget, discovery, and reducer tests in `src/ui/widgets/results/tests.rs` and `src/app/update/tests.rs`, verified with `cargo --locked xtask verify`.

## Phase 2: PR 3B - M23-M25 Docs, Roadmap & README Sync (`docs/m25-roadmap-and-readme-sync`)

- [x] T005 Reconcile `docs/product/roadmap.md`, `docs/product/agent-delivery-plan.md`, `docs/status.md`, `docs/support/compatibility.md`, `CHANGELOG.md`, `specs/069-*` / `070-*` / `071-*`, and `README.md` across M23, M24, and M25 without closing owner-gated tasks (`001` T051/T052, `007` T032/T033, `008` T033/T035).
- [x] T006 Update `tests/documentation_matches_the_build.rs` as needed and verify PR 3B with `cargo --locked xtask verify` and `cargo --locked xtask security`.
