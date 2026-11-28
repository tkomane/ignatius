# Tasks: Structured Result Filtering and Object Quick SQL

**Input**: Design documents from `/specs/073-structured-result-filtering-and-object-quick-sql/`
**Prerequisites**: `spec.md`, `plan.md`

## Phase 1: PR 2A - Structured Result Filtering & Filter-by-Cell (`feat/m27-structured-result-filter`)

- [x] T001 Implement pure structured filter engine (`src/app/result_filter.rs` + `src/app/result_filter/tests.rs`) with `MAX_FILTER_TERMS = 16`, `MAX_FILTER_LEN = 1024`, column predicates, `!negation`, `col:null` vs text `"null"`, numeric comparisons, unknown-column tracking, and `build_cell_filter_expression`.
- [x] T002 Wire `Model::filtered_rows()`, `Action::FilterBySelectedCell` (`"filter-by-cell"`), `Theme`/`Token` unknown-column warning in `src/ui/widgets/results.rs`, and `docs/design/keymap.md`, verified with `cargo --locked xtask verify`.

## Phase 2: PR 2B - Object-Tree Quick SQL Builders (`feat/m27-object-quick-sql`)

- [x] T003 Implement pure SQL builders (`src/query/object_sql.rs` + `src/query/object_sql/tests.rs`) for `SELECT ... LIMIT 100`, `SELECT count(*)`, `INSERT INTO ... VALUES (:...)`, and column lists with quoting, NUL checks, and 200-column / 1 MiB bounds.
- [x] T004 Wire `Action::ObjectSelectPreview`, `Action::ObjectCountRows`, `Action::ObjectInsertTemplate`, and `Action::ObjectColumnList` across `src/app/`, `src/ui/keymap.rs`, `docs/design/keymap.md`, and reducer/discovery tests, verified with `cargo --locked xtask verify`.
