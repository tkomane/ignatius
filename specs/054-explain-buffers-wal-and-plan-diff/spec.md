# Feature Specification: Extended EXPLAIN (BUFFERS, WAL, SETTINGS) & A/B Plan Diff (054)

**Feature Branch**: `feat/m13-crud-plandiff-analytics`
**Created**: 2026-11-18
**Status**: In Progress
**Authority**: Roadmap Milestone 13 (M13: Live Data CRUD, Plan Diff & Analytics)

## 1. Context and Problem Statement

Standard `EXPLAIN (FORMAT JSON)` surfaces planner cost estimates and basic timing,
but performance tuning in PostgreSQL depends on buffer cache hits versus OS/disk
reads, temporary block spills from sorts and materializations, WAL write volume
for DML statements, and active planner GUC overrides (`SETTINGS`). Without a
structured A/B diff engine, operators comparing two queries or two index/GUC
configurations must manually scan nested JSON trees to spot scan-strategy
regressions, temp-file spills, or cache-hit degradation.

Feature 054 introduces a pure, bounded extended-EXPLAIN parser and A/B plan diff
engine (`src/plandiff/`) alongside a rollback-safe PostgreSQL runner
(`src/postgres/plandiff.rs`) compatible with PostgreSQL 14 through 18.

## 2. User Scenarios

### Scenario 1: Capturing an extended analyzed plan safely

An operator runs `EXPLAIN (ANALYZE, BUFFERS, WAL, SETTINGS, FORMAT JSON)` on a
DML or `SELECT` statement. The runner validates that exactly one non-empty
statement was provided, executes `BEGIN`, runs the extended `EXPLAIN`, and
unconditionally issues `ROLLBACK` (even when `EXPLAIN` fails with a SQL error)
so no data changes persist.

### Scenario 2: Capturing a side-effect-free planner estimate

When `analyze = false`, the runner executes `EXPLAIN (FORMAT JSON, VERBOSE, SETTINGS)`
without `WAL` or `ANALYZE`-only flags so PostgreSQL 14 through 16 accept the
request and no statement execution occurs.

### Scenario 3: Diffing Baseline (A) vs Candidate (B) plans

An operator compares a baseline plan against a candidate plan after adding an
index or tuning `work_mem`. The diff engine computes signed cost, execution-time,
buffer, temp-spill, and WAL deltas, highlights changed PostgreSQL `Settings`,
detects scan-strategy improvements (`Seq Scan -> Index Scan`) and regressions
(`Index Scan -> Seq Scan`, `Sort`/`Materialize` temp spills), derives a
deterministic `DiffVerdict` (`Improvement`, `Regression`, `Mixed`, or `Unchanged`),
and renders plain-ASCII tables, line-oriented plain text, or JSON.

## 3. Functional Requirements

- FR-5401: `src/plandiff/mod.rs` provides a pure, zero-IO domain model,
  telemetry parser, and diff engine reusing `MAX_PLAN_JSON_BYTES` (1 MiB) and
  `MAX_PLAN_NODES` (500) from `crate::query::plan`.
- FR-5402: `PlanNodeSummary` captures `path` (e.g. `"0.0"`), `depth`,
  `node_type`, `relation_name`, `index_name`, `startup_cost`, `total_cost`,
  `plan_rows`, `actual_rows`, `actual_total_time_ms`, `shared_hit_blocks`,
  `shared_read_blocks`, `temp_written_blocks`, and `wal_bytes`.
- FR-5403: `PlanSnapshot` captures `label` (bounded to 120 chars), `analyzed`,
  `planning_time_ms`, `execution_time_ms`, `total_cost`, `startup_cost`,
  `plan_rows`, `actual_rows`, shared/local/temp buffer counters, `wal_records`,
  `wal_fpi`, `wal_bytes`, `settings` (`BTreeMap<String, String>`), preorder
  flattened `nodes` (bounded to `MAX_PLAN_NODES`), and `buffer_hit_ratio_pct`.
- FR-5404: `parse_extended_explain_json(label, json_text)` enforces
  `MAX_PLAN_JSON_BYTES` and JSON nesting limits before deserialization, extracts
  PostgreSQL's JSON envelope (`Plan`, `Planning`, `Planning Time`,
  `Execution Time`, `Settings`), accumulates root/planning and node-level
  buffer/WAL counters without double-counting inclusive parent nodes, and
  computes `buffer_hit_ratio_pct = hit / (hit + read) * 100.0` when `hit + read > 0`.
- FR-5405: `diff_plans(baseline, candidate)` returns a `PlanDiffReport` with
  signed deltas (`total_cost_delta`, `total_cost_pct`, `execution_time_delta_ms`,
  `execution_time_pct`, `shared_hit_delta`, `shared_read_delta`,
  `temp_written_delta`, `wal_bytes_delta`), `setting_changes`, `node_changes`,
  human-readable `findings`, and a deterministic `DiffVerdict`.
- FR-5406: `PlanDiffReport` provides `render_table`, `render_plain`, and
  `render_json` emitting strictly plain ASCII output.
- FR-5407: `src/postgres/plandiff.rs` exposes
  `explain_extended(client, sql, analyze, label)`, validates single non-empty
  statements via `crate::query::statements::split`, uses `EXPLAIN (FORMAT JSON, VERBOSE, SETTINGS)`
  when `analyze == false`, and wraps `EXPLAIN (ANALYZE, BUFFERS, WAL, SETTINGS, FORMAT JSON)`
  in `BEGIN` ... `ROLLBACK` (always rolling back on both success and SQL error)
  when `analyze == true`.

## 4. Non-Functional Requirements

- Pure `src/plandiff/` domain with zero network, filesystem, process, or
  terminal dependencies.
- Strictly plain ASCII output in all rendered tables, plain text, and JSON.
- All diagnostic messages and labels pass through `crate::diagnostics::redaction`
  and `crate::query::value::sanitize_for_display`.
- Zero clippy warnings (`#[warn(clippy::all)]`) and sidecar tests in
  `src/plandiff/tests.rs`.

## 5. Out of Scope

- Saving plan snapshots to disk or sending telemetry externally.
- Multi-statement batch EXPLAIN execution.
- Auto-applying `SET` or `CREATE INDEX` recommendations.
