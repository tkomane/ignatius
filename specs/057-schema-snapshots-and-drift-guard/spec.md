# Feature Specification: Schema Snapshots & Drift Guard (057)

**Feature Branch**: `feat/m14-bench-snapshot-runbook-polish`
**Created**: 2026-11-19
**Status**: Phase 1 Implemented (Library Module, Catalog Extractor & Sidecar Tests)
**Authority**: Roadmap Milestone 14 (Benchmarks, Schema Snapshots, SQL Runbooks & Polish)

## 1. Context and Problem Statement

Teams shipping PostgreSQL schema changes through migrations, CI gates, or
multi-environment promotions need a deterministic, zero-row-read way to capture
a canonical schema fingerprint and detect unintended schema drift between an
expected baseline snapshot and a live database or second snapshot. Manual
catalog inspection misses subtle regressions such as column nullability flips,
dropped Row-Level Security (RLS) policies, disabled RLS posture, dropped
indexes/constraints, removed enum labels, or changed view definitions.

Feature 057 introduces a pure canonical schema snapshot and drift-diff engine
(`src/snapshot/mod.rs`) paired with a bounded, read-only PostgreSQL 14-18
catalog extractor (`src/postgres/snapshot.rs`) that reads zero table rows and
zero credentials.

## 2. User Scenarios

### Scenario 1: Capturing a canonical, deterministic schema snapshot

An operator captures a schema snapshot from a PostgreSQL 14-18 server with an
optional `--schema` filter. When no schema filter is supplied, system schemas
(`pg_catalog`, `information_schema`, `pg_toast`, and temporary schemas) are
excluded by default. All tables, columns, constraints, indexes, RLS settings
and policies, views/materialized views (hashed via SHA-256 of server-rendered
definitions), sequences, and enums are sorted deterministically and hashed into
a canonical lowercase 64-hex SHA-256 `fingerprint_sha256`. Two captures of
structurally identical schemas produce byte-for-byte identical canonical JSON
and identical fingerprints regardless of OID order or query arrival order.

### Scenario 2: Detecting schema drift against an expected snapshot

An operator compares an expected `SchemaSnapshot` against an actual
`SchemaSnapshot` via `diff_snapshots(&expected, &actual)`. The diff engine
classifies each finding deterministically as `Breaking`, `Warning`, or `Info`:

- **Breaking**: dropped tables, dropped views, dropped sequences, dropped enums,
  dropped columns, column `data_type` changes, column `nullable` flips (either
  direction), table RLS disabled, dropped RLS policies, view kind flips
  (regular vs materialized), or sequence `data_type` changes.
- **Warning**: dropped or modified constraints, dropped or modified indexes,
  modified RLS policies, `FORCE ROW LEVEL SECURITY` disabled, column default
  changes, view definition SHA-256 changes, sequence attribute changes
  (`start_value`, `increment_by`, `cycle`), or enum label removals/reorderings.
- **Info**: added tables, added columns, added constraints, added indexes,
  added views, added sequences, added enums, added enum labels, enabled RLS /
  forced RLS, added RLS policies, or column ordinal shifts.

### Scenario 3: Gating CI pipelines with configurable drift fail thresholds

A CI check or script evaluates `report.exceeds_threshold(threshold)` against
`DriftFailThreshold` (`Breaking`, `Warning`, `Any`, `Never`, parsed flexibly via
`parse_loose`) and consumes strictly plain-ASCII `render_table()`,
`render_plain()`, or `render_json()` output.

## 3. Functional Requirements

- **FR-5701**: `src/snapshot/mod.rs` defines bounded limits
  `MAX_SNAPSHOT_TABLES = 500`, `MAX_SNAPSHOT_COLUMNS = 5000`,
  `MAX_SNAPSHOT_INDEXES = 2000`, and `MAX_SNAPSHOT_POLICIES = 1000`.
- **FR-5702**: Snapshot model types (`ColumnSnapshot`, `ConstraintSnapshot`,
  `IndexSnapshot`, `PolicySnapshot`, `TableSnapshot`, `ViewSnapshot`,
  `SequenceSnapshot`, `EnumSnapshot`, `SchemaSnapshot`) derive
  `Debug, Clone, PartialEq, Eq, Serialize, Deserialize`.
- **FR-5703**: `SchemaSnapshot::finalize(mut self) -> Self` deterministically
  sorts all collections (`tables`, `views`, `sequences`, `enums` by
  `(schema, name)`; table `columns` by `(ordinal, name)`; `constraints`,
  `indexes`, and `policies` by name; policy `roles` lexicographically), enforces
  bounds (`truncated = true` when any cap is exceeded), clears
  `fingerprint_sha256` to `""`, serializes canonical JSON, hashes via
  `sha2::Sha256` into lowercase hex, and stores `fingerprint_sha256`.
- **FR-5704**: `DriftSeverity` (`Breaking`, `Warning`, `Info`),
  `DriftFailThreshold` (`Breaking`, `Warning`, `Any`, `Never` with
  `parse_loose`), `DriftFinding`, and `SnapshotDiffReport` support
  `diff_snapshots(&expected, &actual)`, `exceeds_threshold(threshold)`, and
  strictly plain-ASCII `render_table()`, `render_plain()`, and `render_json()`.
- **FR-5705**: `SchemaSnapshot::from_json` and `SchemaSnapshot::render_json`
  support bounded JSON round-tripping and version validation.
- **FR-5706**: `src/postgres/snapshot.rs` exposes
  `pub async fn capture_schema_snapshot(client: &Client, schema: Option<&str>) -> Result<SchemaSnapshot, Diagnostic>`,
  queries `pg_catalog` views/tables (`pg_class`, `pg_namespace`, `pg_attribute`,
  `pg_attrdef`, `pg_constraint`, `pg_index`, `pg_policy`, `pg_roles`,
  `pg_sequence`, `pg_type`, `pg_enum`) compatibly across PostgreSQL 14-18,
  excludes `pg_catalog`, `information_schema`, `pg_toast`, and temporary schemas
  when `schema` is `None`, reads zero table rows and zero credentials, and
  returns `.finalize()`.

## 4. Non-Functional Requirements

- Pure `src/snapshot/mod.rs` domain with zero network, filesystem, process, or
  terminal I/O.
- Strictly plain ASCII in comments and all rendered outputs (`render_table`,
  `render_plain`, `render_json`), with control/bidirectional escape
  sanitization.
- Zero `unsafe` code, zero new Cargo dependencies, Rust 1.90+ (edition 2024)
  compatible, zero clippy warnings.
- Sidecar unit tests only in `src/snapshot/tests.rs` (no inline test modules).

## 5. Out of Scope

- Capturing or hashing table row contents, sequence `last_value` runtime
  counters, credentials, or FDW/subscription connection strings.
- Automatically executing `ALTER` / `DROP` remediation DDL against live
  databases.
