# Tasks: Query Performance & pg_stat_statements Forensics (Feature 034)

- [x] T001: Implement pure domain models and calculations in `src/perf/mod.rs` (`QueryPerformanceSnapshot`, `StatStatementEntry`, `PerformanceSortOrder`, `PerformanceOutputFormat`)
- [x] T002: Add comprehensive unit tests in sidecar `src/perf/tests.rs` (sorting, cache hit ratios, table/json/plain outputs, empty states)
- [x] T003: Implement catalog extractors in `src/postgres/metadata.rs` and methods in `src/postgres/session.rs` (`Session::query_performance`, `Session::reset_performance_stats`)
- [x] T004: Add keymap chord `Ctrl+K %` and configurable action `query-performance` in `src/ui/keymap.rs` and `docs/design/keymap.md`
- [x] T005: Integrate TEA messages and reducer update branches in `src/app/model.rs`, `src/app/message.rs`, and `src/app/update.rs`
- [x] T006: Build the Field Industrial TUI Performance Forensics overlay widget in `src/ui/widgets/overlays/performance.rs`
- [x] T007: Implement the `ignatius profile-queries` CLI subcommand with sorting, formatting, filtering, and reset flags
- [x] T008: Add CLI contract tests in `tests/profile_queries_contract.rs`
- [x] T009: Run full verification battery (`cargo xtask verify`) and document in `docs/status.md`
