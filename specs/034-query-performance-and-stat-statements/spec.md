# Feature Specification: Query Performance & pg_stat_statements Forensics

**Feature Branch**: `feat/034-query-performance-and-stat-statements`
**Created**: 2026-11-12
**Status**: In Progress
**Authority**: Roadmap Milestone 7 (M7: Enterprise Observability & Query Performance Forensics)

## User Scenarios & Testing

### User Story 1 - Profile Cluster Query Performance & Bottlenecks (Priority: P1)

An engineer or DBA observing database latency needs to identify the cluster's most expensive, slowest, and most frequent queries without writing bespoke catalog queries or attaching external monitoring agents. They trigger Query Performance Forensics (`Ctrl+K %` or `ignatius profile-queries`) and immediately see ranked statement telemetry from `pg_stat_statements` with call counts, total/mean/stddev execution times, and buffer cache hit ratios.

**Why this priority**: Query performance bottlenecks and cache misses are the primary daily operational challenges in production PostgreSQL databases. Instant in-terminal query forensics eliminates context switching to heavyweight external web monitors.

**Independent Test**: Connect to a PostgreSQL instance with `pg_stat_statements` enabled, run sample queries, run `ignatius profile-queries --sort mean --format json`, and assert that the ranked queries appear with accurate timing and buffer metrics.

### User Story 2 - Investigate Buffer Cache Misses & I/O Hotspots (Priority: P1)

An engineer tuning an application query wants to understand whether a slow query is CPU-bound or disk I/O bound. They inspect the query's buffer statistics: blocks read from OS cache/disk (`shared_blks_read`), blocks found in PostgreSQL shared buffers (`shared_blks_hit`), and blocks dirtied (`shared_blks_dirtied`). They toggle sort order by hit ratio (`[h]`) to surface queries with poor cache locality.

**Why this priority**: High shared block reads directly cause IOPS saturation on cloud managed databases. Pinpointing queries with low hit ratios is critical for indexing and buffer pool sizing.

### User Story 3 - Load Query into Editor for EXPLAIN ANALYZE (Priority: P2)

Having discovered a problematic query in the forensics table, the engineer presses `[e]` to copy its SQL statement directly into the SQL editor buffer as an undoable edit (`Ctrl+Z`), allowing immediate visual query plan inspection (`Ctrl+K l` / `Ctrl+K a`) and index optimization.

### User Story 4 - Graceful Degradation When Extension Absent (Priority: P2)

When connected to a database where `pg_stat_statements` is not installed or enabled in `shared_preload_libraries`, Ignatius refuses to crash or fail silently. It displays an informative guidance modal explaining the exact SQL needed (`CREATE EXTENSION pg_stat_statements;`) and configuration requirements (`shared_preload_libraries = 'pg_stat_statements'`).

### User Story 5 - Guarded Performance Statistics Reset (Priority: P3)

Before running an A/B benchmark or post-deployment test, an operator wants to reset execution counters (`pg_stat_statements_reset()`). Ignatius requires explicit confirmation with `Enter` (and `--confirm-production` on production targets), preventing accidental clearing of production performance history.

## Technical Requirements & Constraints

1. **Pure Domain Model (`src/perf/`)**:
   - `QueryPerformanceSnapshot`, `StatStatementEntry`, `PerformanceSortOrder`, and `PerformanceOutputFormat`.
   - Zero terminal, filesystem, or network imports in the core domain module.
2. **Catalog Extraction (`src/postgres/`)**:
   - Safe queries against `pg_extension` and `pg_stat_statements` across PostgreSQL 14, 15, 16, 17, and 18.
3. **Elm Reducer Integration (`src/app/`)**:
   - State stored in `model.query_performance: Option<QueryPerformanceOverlayState>`.
   - Explicit `Message` and `Action` variants.
4. **Interactive TUI Widget (`src/ui/widgets/overlays/`)**:
   - Field Industrial Teenage Engineering styling with telemetry capsules and responsive dual-pane viewport.
5. **Headless Scriptable CLI (`src/cli/`)**:
   - `ignatius profile-queries` subcommand with `--sort`, `--limit`, `--min-calls`, `--format table|json|plain`, `--output`, and `--reset`.
