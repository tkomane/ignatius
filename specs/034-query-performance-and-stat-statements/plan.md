# Implementation Plan: Query Performance & pg_stat_statements Forensics

**Feature**: `034-query-performance-and-stat-statements`
**Milestone**: M7 Enterprise Observability & Query Performance Forensics

## Architecture & Module Layout

```
src/
├── perf/                         # Pure domain logic for query performance forensics
│   ├── mod.rs                   # QueryPerformanceSnapshot, StatStatementEntry, SortOrder, Formatters
│   └── tests.rs                 # Comprehensive sidecar unit tests
├── postgres/
│   ├── metadata.rs              # query_performance extractor & pg_stat_statements queries
│   └── session.rs               # Session::query_performance & Session::reset_performance_stats
├── app/
│   ├── message.rs               # Action::QueryPerformance, Message variants for performance telemetry
│   ├── model.rs                 # QueryPerformanceOverlayState in Model
│   └── update.rs                # TEA reducer update branches for performance overlay
├── ui/
│   ├── keymap.rs                # Ctrl+K % chord binding and "query-performance" configurable action
│   └── widgets/
│       └── overlays/
│           └── performance.rs   # Field Industrial TUI performance forensics overlay
└── cli/
    └── mod.rs                   # ignatius profile-queries CLI subcommand
```

## Implementation Phases

1. **Phase 1: Pure Domain Model (`src/perf/`)**:
   - Define `StatStatementEntry` with calculations for hit ratio, execution timing metrics, formatted outputs.
   - Define `QueryPerformanceSnapshot` with sorting, filtering, ranking, table/json/plain renderers.
   - Implement `build_reset_sql() -> &'static str`.
   - Sidecar unit tests in `src/perf/tests.rs` covering all sort orders, hit ratios, edge cases (0 calls, 0 hit blocks).

2. **Phase 2: PostgreSQL Catalog Extraction (`src/postgres/`)**:
   - `Session::query_performance`: queries `pg_extension` first; queries `pg_stat_statements` with limit.
   - `Session::reset_performance_stats`: executes `SELECT pg_stat_statements_reset();`.
   - Handles version differences (PostgreSQL 14 through 18).

3. **Phase 3: Elm Reducer & State Management (`src/app/`)**:
   - Add `Action::QueryPerformance` (`query-performance`) to `CONFIGURABLE`.
   - Add `('%', Action::QueryPerformance, "Open query performance forensics and stat statements")` to `CHORDS`.
   - Handle open, sort, filter, navigate, reset, and load query into editor in `update.rs`.

4. **Phase 4: Field Industrial TUI Overlay (`src/ui/widgets/overlays/performance.rs`)**:
   - Dual-pane layout: ranked telemetry table on top, detailed query buffer/timing distribution below.
   - SurfaceAlt statusbar header with hardware indicators.
   - Keyboard navigation (`j`/`k`, `m`/`t`/`c`/`h` sorting, `e` load editor, `x` reset, `Esc` close).

5. **Phase 5: CLI Subcommand (`src/cli/`)**:
   - `ignatius profile-queries` with `--sort`, `--limit`, `--min-calls`, `--format`, `--output`, `--reset`.

6. **Phase 6: Verification & Documentation**:
   - Sidecar tests in `src/perf/tests.rs`.
   - CLI contract tests in `tests/profile_queries_contract.rs`.
   - Document in `docs/design/keymap.md` and `docs/support/diagnostics.md`.
   - `cargo xtask verify`.
