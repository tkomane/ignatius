# Implementation Plan: Extended EXPLAIN (BUFFERS, WAL, SETTINGS) & A/B Plan Diff (054)

## 1. Pure Domain Model, Parser & Diff Engine (`src/plandiff/mod.rs`)

- **Bounds & Validation**:
  - Reuse `crate::query::plan::{MAX_PLAN_JSON_BYTES, MAX_PLAN_NODES, MAX_PLAN_DEPTH}`.
  - Enforce `json_text.len() <= MAX_PLAN_JSON_BYTES` and brace/bracket nesting
    bounds before `serde_json::Deserializer` with `disable_recursion_limit()`.
  - Bound `PlanSnapshot.label` to 120 characters after redaction and ASCII
    whitespace normalization.
- **Types**:
  - `PlanNodeSummary`: preorder node snapshot with `path`, `depth`, `node_type`,
    `relation_name`, `index_name`, cost/row estimates, actual rows/time, and
    buffer/WAL counters.
  - `PlanSnapshot`: full plan summary including `Planning` + `Plan` buffer
    totals, `wal_records`, `wal_fpi`, `wal_bytes`, `settings` (`BTreeMap`),
    bounded `nodes`, and `buffer_hit_ratio_pct`.
  - `DiffVerdict`: `Improvement`, `Regression`, `Mixed`, `Unchanged`.
  - `SettingChange`: `name`, `baseline`, `candidate`.
  - `NodeChange`: `index_a`, `index_b`, `description`, `node_a`, `node_b`,
    `cost_delta`, `time_delta_ms`, `severity`.
  - `PlanDiffReport`: deltas, `setting_changes`, `node_changes`, `findings`,
    `verdict`, plus strictly plain-ASCII `render_table()`, `render_plain()`, and
    `render_json()`.
- **Counter Aggregation Rules**:
  - PostgreSQL emits inclusive buffer/WAL counters on parent nodes; synthetic
    fixtures may omit parent counters and populate only leaves.
  - Recursive subtree aggregation uses `if node_val >= children_sum { node_val } else { node_val + children_sum }`
    for each buffer/WAL metric so real PG plans never double-count and leaf-only
    fixtures accumulate accurately.
  - Envelope-level `"Planning"` buffer counters (PG 13-18) are added on top of
    execution `"Plan"` totals.
- **Diff & Verdict Engine (`diff_plans`)**:
  - Computes signed cost, runtime, buffer, temp-spill, and WAL deltas.
  - Diffs `BTreeMap` `settings` deterministically.
  - Aligns nodes by `path` and also checks relation-level scan transitions
    (`Seq Scan -> Index Scan` improvement, `Index Scan -> Seq Scan` regression)
    and `Sort`/`Materialize` temp-spill regressions.
  - Derives `DiffVerdict` (`Improvement`, `Regression`, `Mixed`, `Unchanged`)
    from latency/cost/spill/strategy signals.

## 2. Rollback-Safe PostgreSQL Runner (`src/postgres/plandiff.rs`)

- `pub async fn explain_extended(client: &Client, sql: &str, analyze: bool, label: &str) -> Result<PlanSnapshot, Diagnostic>`:
  1. Validate single non-empty statement via `crate::query::statements::split`.
  2. When `analyze == false`: run `EXPLAIN (FORMAT JSON, VERBOSE, SETTINGS) <stmt>`.
  3. When `analyze == true`: run `BEGIN`, then `EXPLAIN (ANALYZE, BUFFERS, WAL, SETTINGS, FORMAT JSON) <stmt>`,
     and unconditionally execute `ROLLBACK` afterwards (even if `EXPLAIN` fails).
  4. Extract the single JSON payload row via `simple_query_raw`, enforcing
     `MAX_PLAN_JSON_BYTES`, then call `parse_extended_explain_json(label, &json)`.

## 3. Sidecar Unit Tests (`src/plandiff/tests.rs`)

- PG 16/18 extended JSON fixture parsing (`BUFFERS`, `WAL`, `SETTINGS`,
  `Planning` buffers, inclusive parent counters vs leaf-only counters).
- Estimate-only (`analyze == false`) parsing and `buffer_hit_ratio_pct == None`
  when `hit + read == 0`.
- `ResponseTooLarge` enforcement at `MAX_PLAN_JSON_BYTES + 1` and malformed/missing
  root diagnostics.
- Plan A/B comparison covering `Improvement` (`Seq Scan -> Index Scan`),
  `Regression` (`Index Scan -> Seq Scan` and `Sort`/`Materialize` temp spill),
  `Mixed`, and `Unchanged`.
- `Settings` diffing and strictly plain-ASCII verification for `render_table()`,
  `render_plain()`, and `render_json()`.

## 4. Coordinator Integration (Deferred to Coordinator)

- Export `pub mod plandiff;` in `src/lib.rs` and `pub mod plandiff;` in
  `src/postgres/mod.rs`.
- Wire `Session` wrapper, CLI subcommand, and TUI overlay in coordinator follow-up.
