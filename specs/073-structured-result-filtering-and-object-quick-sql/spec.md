# Feature Specification: Structured Result Filtering and Object Quick SQL

**Feature Branch**: `feat/m27-structured-result-filter` (PR 2A) and `feat/m27-object-quick-sql` (PR 2B)
**Created**: 2026-11-28
**Status**: Complete
**Milestone**: M27 - Structured Result Filtering & Object-Tree Quick SQL

## Problem Statement

Two everyday interactive workbench workflows currently require unnecessary round trips or manual typing:

1. **Column-aware structured filtering over retained results (Slice 1, PR 2A).** Pressing `/` in Results currently supports only a flat substring match across all columns. Operators inspecting wide or multi-column result sets cannot filter by a specific column (`status:=failed`, `amount:>100`, `deleted_at:null`, `env:!prod`), exclude terms (`!debug`), combine multiple criteria, or filter directly by the currently selected cell with one command.
2. **Safe quick SQL generation from the Object Tree (Slice 2, PR 2B).** Pressing `Enter` on an object in the Objects tree inserts its quoted name into the editor, but operators still have to type `SELECT ... LIMIT 100`, `SELECT count(*)`, `INSERT INTO ... VALUES (:param, ...)`, or an explicit loaded column list by hand.

## Principles This Feature Must Keep

- **Principle I (Delight must remain truthful).** Structured filtering operates strictly over retained rows and states clearly when a result was truncated or when a structured column filter names an unknown column (returning zero matches rather than silently ignoring the column). Object-tree quick SQL refuses unsupported node kinds and unloaded column lists with explicit guidance.
- **Principle III (Local-first and private by default).** Both structured result filtering and object-tree quick SQL run 100% in memory with zero database queries, zero disk writes, and zero clipboard calls.
- **Principle IV (Safe by default).** Object-tree quick SQL actions (`object-select-preview`, `object-count-rows`, `object-insert-template`, `object-column-list`) insert formatted SQL into the active editor buffer as a single undoable edit without ever executing SQL automatically. Identifiers use `quote_identifier` and refuse NUL bytes.
- **Principle V (Keyboard-first, never keyboard-only).** All new actions are configurable in `[keys]`, listed in `docs/design/keymap.md`, and discoverable in the Command Palette.
- **Principle VI (Accessible meaning survives styling loss).** Filter status and unknown-column indicators in the Results bar use `Theme`/`Token` only and preserve full meaning in `Basic16`, `NO_COLOR`, and ASCII modes.

## User Scenarios & Testing

### User Story 1 - Structured Result Filtering & Filter-by-Cell (Priority: P1, Slice 1 / PR 2A)

In the Results pane, pressing `/` accepts both plain substring searches (including timestamps like `12:30:00` and URLs) and structured column/negation predicates:
- Bare terms (`alice`) and quoted phrases (`"north america"`).
- Negation (`!test`).
- Column predicates (`col:val`, `col:=val`, `col:!=val`, `col:null`, `col:!null`/`col:notnull`, numeric `col:>N`, `col:>=N`, `col:<N`, `col:<=N`).
- `filter-by-cell` (`Action::FilterBySelectedCell`) populates `model.result_filter` directly from the selected cell (`col:=val` or `col:null`) and moves selection to row 0 without executing SQL.

### User Story 2 - Object-Tree Quick SQL Builders (Priority: P1, Slice 2 / PR 2B)

With an object selected in the Objects tree:
- `object-select-preview` inserts `SELECT ... FROM "schema"."name" LIMIT 100;` (using loaded columns when expanded or `*` otherwise), focuses Editor, and does not run SQL.
- `object-count-rows` inserts `SELECT count(*) FROM "schema"."name";` into the Editor.
- `object-insert-template` inserts `INSERT INTO "schema"."name" ("col1", ...) VALUES (:col1, ...);` for tables/partitioned tables/foreign tables with loaded columns, refusing views/materialized views/sequences/functions/schemas and unloaded column lists.
- `object-column-list` inserts `"col1", "col2", ...` for an expanded relation (or parent relation when a column node is selected), refusing unloaded relations cleanly.

## Functional Requirements

### Slice 1 - Structured Result Filtering (PR 2A)

- **FR-001**: Implement `src/app/result_filter.rs` (`MAX_FILTER_TERMS = 16`, `MAX_FILTER_LEN = 1024`, `ParsedResultFilter`, `filter_rows`, `build_cell_filter_expression`) and sidecar `src/app/result_filter/tests.rs`.
- **FR-002**: Wire `Model::filtered_rows()` and `Model::result_filter_unknown_columns()` in `src/app/model.rs`, add `Action::FilterBySelectedCell` (`"filter-by-cell"`), and surface structured filter / unknown-column warnings in `src/ui/widgets/results.rs` using `Theme`/`Token` only.

### Slice 2 - Object-Tree Quick SQL Builders (PR 2B)

- **FR-003**: Implement `src/query/object_sql.rs` (`DEFAULT_PREVIEW_LIMIT = 100`, `MAX_OBJECT_SQL_COLUMNS = 200`, `MAX_OBJECT_SQL_BYTES = 1 MiB`, `build_select_preview_sql`, `build_count_sql`, `build_insert_template_sql`, `build_column_list_sql`) and sidecar `src/query/object_sql/tests.rs`.
- **FR-004**: Wire `Action::ObjectSelectPreview` (`"object-select-preview"`), `Action::ObjectCountRows` (`"object-count-rows"`), `Action::ObjectInsertTemplate` (`"object-insert-template"`), and `Action::ObjectColumnList` (`"object-column-list"`) across `Action`, `CONFIGURABLE`, `discovery.rs`, `update.rs`, and `docs/design/keymap.md`.

## Non-Functional Requirements

- **NFR-001 (UI Token Discipline)**: Zero raw `Color::Rgb(` or `Color::Indexed(` literals in `src/ui/widgets/`.
- **NFR-002 (Repository Hygiene)**: Sidecar `tests.rs` files only, `AGENTS.md == CLAUDE.md`, zero Unicode em/en dashes (`\u{2014}` / `\u{2013}`) in prose.
- **NFR-003 (Verification)**: Passes `cargo --locked xtask verify`.
