# Feature Specification: Index Advisor & Bloat Forensics

**Feature Branch**: `feat/035-index-advisor-and-bloat-forensics`
**Created**: 2026-11-12
**Status**: In Progress
**Authority**: Roadmap Milestone 7 (M7: Enterprise Observability & Query Performance Forensics)

## User Scenarios & Testing

### User Story 1 - Detect Duplicate & Prefix-Redundant Indexes (Priority: P1)

A database developer or SRE working on an evolving database schema notices high write amplification and storage growth. They open the Index Advisor (`Ctrl+K ^` or `ignatius index-advisor --section redundant`) and immediately see a catalog-grounded analysis of redundant indexes:
1. Exact duplicates: Two or more indexes with identical column lists, indexing method, and operator classes.
2. Prefix redundancy: Multi-column composite indexes (e.g., `(user_id, status)`) that render single-column leading prefix indexes (e.g., `(user_id)`) redundant for equality lookups.
3. Estimated reclaimable disk storage: Computed from `pg_relation_size` for each candidate.
4. Generated cleanup SQL: Safe, non-blocking `DROP INDEX CONCURRENTLY IF EXISTS ...` commands.

**Why this priority**: Unneeded and redundant indexes slow down all `INSERT`, `UPDATE`, and `DELETE` operations and waste expensive buffer cache memory. Surfacing them safely and deterministically provides immediate ROI.

**Independent Test**: Connect to a database with synthetic redundant indexes, run `ignatius index-advisor --section redundant --format json`, and assert that the redundant index is detected with its reason and reclaimable size.

---

### User Story 2 - Lock-Free B-Tree Page Bloat Estimation (Priority: P1)

An engineer troubleshooting unexpected disk space usage and degraded index traversal performance needs to measure index bloat without issuing locking queries or installing heavy external agents. They trigger the bloat forensics view (`Ctrl+K ^` tab `[2]` or `ignatius index-advisor --section bloat`), which applies a catalog-based estimation formula (`pg_class`, `pg_stats`, `reltuples`, `relpages`, and fillfactor) to calculate:
1. Estimated live pages vs. wasted (dead/sparse) pages.
2. Estimated wasted bytes and bloat percentage (`% bloat`).
3. Recommended remediation SQL: `REINDEX INDEX CONCURRENTLY ...` when bloat exceeds 30%.

**Why this priority**: Index bloat degrades cache hit ratios and inflates point-lookup latency. Lock-free estimation allows safe continuous production diagnosis without interrupting ongoing transactional traffic.

---

### User Story 3 - Missing Foreign Key Indexes & Opportunity Detection (Priority: P1)

An application engineer tuning database relations inspects foreign key constraints for missing supporting indexes. Missing indexes on foreign keys cause cascading deletes or updates on referenced parent tables to perform table-level sequential scans on child tables, causing severe lock contention. Ignatius audits all foreign keys against child table index definitions and surfaces:
1. Constraint name, child table, parent table, and referencing column list.
2. Index status: `MISSING` vs `INDEXED`.
3. Suggested non-blocking DDL: `CREATE INDEX CONCURRENTLY idx_<table>_<column> ON <table> (<column>);`.

---

### User Story 4 - Hypothetical Index Testing with HypoPG (Priority: P2)

When an engineer formulates a candidate index to speed up a slow query, they want to verify whether the PostgreSQL query optimizer will actually choose the proposed index before spending hours building it in production.
1. When the `hypopg` extension is available: Ignatius invokes `hypopg_create_index(...)`, runs `EXPLAIN (FORMAT JSON)` on the query, records whether the plan node switched from sequential scan to index scan and calculates the cost reduction percentage, and cleanly resets hypothetical state with `hypopg_reset()`.
2. When `hypopg` is not installed: Ignatius displays clean instructions explaining how to enable `hypopg` while gracefully serving static foreign key and redundancy analysis without failure.

---

### User Story 5 - Field Industrial TUI & Editor Loading (Priority: P2)

Within the interactive workbench, the engineer opens the Index Advisor with chord `Ctrl+K ^` (`index-advisor`):
1. Features Teenage Engineering status capsules: `[INDEX HEALTH: 92%]`, `[REDUNDANT: 2]`, `[BLOATED: 1]`, `[HYPOTHETICAL: READY]`.
2. Tabbed dual-pane view:
   - `[1] Redundant & Unused`: Lists duplicate/prefix indexes and scans.
   - `[2] B-Tree Bloat`: Ranked by wasted megabytes.
   - `[3] Unindexed FKs`: Foreign keys missing indexes.
3. Lower pane: Statement preview with syntax-highlighted SQL.
4. Pressing `[e]` loads suggested index DDL into the editor as an undoable edit (`Ctrl+Z`).
5. Pressing `[c]` copies the SQL to clipboard via OSC 52.

---

## Technical Architecture & Contracts

1. **Pure Domain (`src/advisor/mod.rs` & `tests.rs`)**:
   - `IndexOpportunity`, `RedundantIndex`, `IndexBloatMetric`, `HypoExplainComparison`, `AdvisorReport`.
   - Pure formatting: aligned tables, structured JSON, plain text.
2. **Catalog Extractors (`src/postgres/metadata.rs` & `session.rs`)**:
   - `Session::analyze_index_redundancy`: Queries `pg_index`, `pg_class`, `pg_am`.
   - `Session::analyze_index_bloat`: Lock-free catalog bloat calculations.
   - `Session::analyze_unindexed_foreign_keys`: Compares `pg_constraint` against indexed columns.
   - `Session::test_hypothetical_index`: Safe `hypopg` execution harness with guarantee of `hypopg_reset()`.
3. **Keymap & Configuration (`src/ui/keymap.rs` & `docs/design/keymap.md`)**:
   - Chord `Ctrl+K ^` mapped to `Action::IndexAdvisor`.
   - Configurable action `index-advisor`.
4. **TEA Model & Widget (`src/app/` & `src/ui/widgets/overlays/advisor.rs`)**:
   - Status capsules, responsive dual-pane list/detail, tab switching `[1]`/`[2]`/`[3]`, editor loading `[e]`.
5. **Headless CLI (`src/cli/mod.rs`)**:
   - Subcommand `ignatius index-advisor` with `--section all|redundant|bloat|unindexed`, `--format table|json|plain`, `--output <path>`, `--min-size <MB>`.
