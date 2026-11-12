# Implementation Plan: Index Advisor & Bloat Forensics (Feature 035)

## Overview
Implement Feature `035-index-advisor-and-bloat-forensics` as part of Roadmap Milestone 7 (Enterprise Observability & Query Performance Forensics).

## Architecture & Module Layout

### 1. Pure Domain Layer: `src/advisor/`
- `src/advisor/mod.rs`:
  - `RedundantIndex`: schema, table, redundant index name, covering index name, reason (`ExactDuplicate`, `PrefixCovered`, `Unused`), estimated size bytes.
  - `IndexBloatMetric`: schema, table, index name, total bytes, estimated wasted bytes, bloat ratio (0.0 to 1.0).
  - `UnindexedForeignKey`: constraint name, schema, child table, parent table, column list, suggested index name.
  - `HypoIndexTestResult`: table name, proposed index DDL, original plan cost, hypo plan cost, cost delta percent, index used in plan (bool).
  - `AdvisorSection`: `All`, `Redundant`, `Bloat`, `Unindexed`.
  - `AdvisorFormat`: `Table`, `Json`, `Plain`.
  - Aligned table renderers, JSON serializer, and clean human-readable summaries.
- `src/advisor/tests.rs`:
  - Pure unit tests for duplicate detection logic, prefix coverage matching, bloat calculation formulas, JSON serialization, and table formatting.

### 2. PostgreSQL Extraction Layer: `src/postgres/`
- Catalog queries:
  - Redundant indexes: self-join on `pg_index` checking `indrelid`, `indkey`, `indclass`.
  - Unused indexes: `pg_stat_user_indexes` joined with `pg_index` filtering `idx_scan = 0` and non-unique.
  - Bloat estimation: standard non-locking PostgreSQL catalog formula utilizing `reltuples`, `relpages`, and average column widths.
  - Unindexed foreign keys: `pg_constraint` with `contype = 'f'` compared with `pg_index` column leading prefixes.
  - HypoPG execution: `SELECT * FROM hypopg_create_index($1);`, `EXPLAIN (FORMAT JSON) ...`, `SELECT hypopg_reset();`.
- Session methods in `src/postgres/session.rs`:
  - `Session::analyze_indexes(&self) -> Result<AdvisorReport, Error>`
  - `Session::test_hypothetical_index(&self, query: &str, create_index_sql: &str) -> Result<HypoIndexTestResult, Error>`

### 3. Keymap & User Interface: `src/ui/` & `src/app/`
- Keymap:
  - Register `^` in `CHORDS`: `('^', Action::IndexAdvisor, "Open index advisor and bloat forensics")`.
  - Add `index-advisor` to `CONFIGURABLE`.
- Application Reducer (`src/app/`):
  - State `IndexAdvisorState` with selected tab (`Redundant`, `Bloat`, `Unindexed`), cursor index, report snapshot.
  - Messages: `OpenIndexAdvisor`, `IndexAdvisorLoaded`, `IndexAdvisorTabChanged`, `IndexAdvisorSelectNext`, `IndexAdvisorSelectPrev`.
- Field Industrial TUI Widget (`src/ui/widgets/overlays/advisor.rs`):
  - Teenage Engineering status capsules: `[INDEX HEALTH]`, `[REDUNDANT]`, `[BLOATED]`, `[UNINDEXED FK]`.
  - Dual-pane list/detail layout with high-contrast active cursor and syntax-highlighted SQL preview.
  - Hotkeys: `[1]`/`[2]`/`[3]` tabs, `[e]` load SQL into editor, `[c]` copy SQL.

### 4. Non-Interactive CLI: `src/cli/`
- `ignatius index-advisor [flags] [target]`:
  - `--section all|redundant|bloat|unindexed`
  - `--format table|json|plain`
  - `--output <path>`
  - `--min-size <MB>`
  - `--force`

### 5. Verification & Contracts
- Contract test: `tests/index_advisor_contract.rs`.
- Update `docs/design/keymap.md`, `README.md`, `CHANGELOG.md`, `docs/status.md`.
- Complete verification run via `cargo xtask verify`.
