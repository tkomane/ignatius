# Implementation Plan: Schema Snapshots & Drift Guard (057)

## 1. Pure Domain Model & Drift Engine (`src/snapshot/mod.rs`)

- **Constants & Bounds**:
  - `SNAPSHOT_SCHEMA_VERSION: u32 = 1`
  - `MAX_SNAPSHOT_TABLES: usize = 500`
  - `MAX_SNAPSHOT_COLUMNS: usize = 5000`
  - `MAX_SNAPSHOT_INDEXES: usize = 2000`
  - `MAX_SNAPSHOT_POLICIES: usize = 1000`
  - `MAX_SNAPSHOT_CONSTRAINTS: usize = 5000`
  - `MAX_SNAPSHOT_JSON_BYTES: usize = 16 * 1024 * 1024`
- **Canonical Snapshot Types**:
  - `ColumnSnapshot { name, data_type, nullable, default_expr, ordinal }`
  - `ConstraintSnapshot { name, kind, definition }`
  - `IndexSnapshot { name, is_unique, is_primary, definition }`
  - `PolicySnapshot { name, cmd, permissive, roles, qual, with_check }`
  - `TableSnapshot { schema, name, columns, constraints, indexes, rls_enabled, rls_forced, policies }`
  - `ViewSnapshot { schema, name, materialized, definition_sha256 }`
  - `SequenceSnapshot { schema, name, data_type, start_value, increment_by, cycle }`
  - `EnumSnapshot { schema, name, labels }`
  - `SchemaSnapshot { schema_version, schema_filter, fingerprint_sha256, tables, views, sequences, enums, truncated }`
- **Canonicalization & Hashing**:
  - `sha256_hex(bytes: &[u8]) -> String` using `sha2::{Digest, Sha256}`.
  - `SchemaSnapshot::finalize(mut self) -> Self` sorts all collections
    deterministically (preserving `EnumSnapshot.labels` semantic sort order),
    enforces object caps with `truncated = true`, clears `fingerprint_sha256` to
    `""`, serializes canonical compact JSON, hashes with SHA-256 into lowercase
    64-char hex, and stores `fingerprint_sha256`.
  - `SchemaSnapshot::compute_fingerprint(&self) -> String` and
    `validate_fingerprint(&self) -> bool`.
  - `SchemaSnapshot::from_json(json: &str) -> Result<Self, Diagnostic>` and
    `render_json(&self) -> String`.
- **Drift Engine**:
  - `DriftSeverity` (`Breaking`, `Warning`, `Info`), ordered cleanly for
    highest-severity-first sorting.
  - `DriftFailThreshold` (`Breaking`, `Warning`, `Any`, `Never`) with
    `parse_loose(&str) -> Option<Self>` and `label(self)`.
  - `DriftFinding { severity, kind, object, detail }`.
  - `SnapshotDiffReport` with `expected_fingerprint`, `actual_fingerprint`,
    `fingerprints_match`, `findings`, `breaking_count`, `warning_count`,
    `info_count`, `exceeds_threshold(&self, threshold)`, `render_table(&self)`,
    `render_plain(&self)`, and `render_json(&self)`.
  - `diff_snapshots(expected: &SchemaSnapshot, actual: &SchemaSnapshot) -> SnapshotDiffReport`
    comparing tables, columns, constraints, indexes, RLS flags, policies, views,
    sequences, and enums deterministically.
  - Plain-ASCII sanitization via `crate::query::value::sanitize_for_display`,
    escaping non-ASCII code points cleanly.

## 2. Bounded PostgreSQL 14-18 Catalog Extractor (`src/postgres/snapshot.rs`)

- `pub async fn capture_schema_snapshot(client: &Client, schema: Option<&str>) -> Result<SchemaSnapshot, Diagnostic>`
- Rejects empty/whitespace-only explicit `schema` filter with `DiagnosticKind::Usage`.
- Applies uniform schema filter clause:
  - When `schema` is `Some(s)`: `n.nspname = $1::text`.
  - When `schema` is `None`: excludes `'pg_catalog'`, `'information_schema'`,
    `'pg_toast'`, `'pg_temp_%'`, and `'pg_toast_temp_%'`.
- Runs bounded `pg_catalog` queries (`LIMIT + 1` truncation detection):
  1. Tables (`pg_class` `relkind IN ('r', 'p')`, `relrowsecurity`, `relforcerowsecurity`).
  2. Columns (`pg_attribute`, `pg_attrdef`, `format_type`, `pg_get_expr`, `attnum > 0 AND NOT attisdropped`).
  3. Constraints (`pg_constraint` with `contype IN ('p', 'f', 'u', 'c', 'x')` so PG 18 `contype = 'n'` does not diverge from PG 14-17, `pg_get_constraintdef`).
  4. Indexes (`pg_index` with `indislive`, `indisunique`, `indisprimary`, `pg_get_indexdef`).
  5. RLS Policies (`pg_policy`, `pg_roles` with `ARRAY[0::oid]` -> `PUBLIC`, `pg_get_expr`).
  6. Views & Materialized Views (`pg_class` `relkind IN ('v', 'm')`, `pg_get_viewdef(c.oid, true)` trimmed and SHA-256 hashed).
  7. Sequences (`pg_sequence`, `format_type(s.seqtypid, NULL)`).
  8. Enums (`pg_type` `typtype = 'e'`, `pg_enum` ordered by `enumsortorder`).
- Calls `.finalize()` before returning `Ok(snapshot)`.

## 3. Sidecar Unit Tests (`src/snapshot/tests.rs`)

- Canonical sorting determinism and idempotency of `SchemaSnapshot::finalize`
  (regardless of input ordering of tables, columns, constraints, indexes,
  policies, views, sequences, enums, and policy roles).
- Fingerprint sensitivity checks (adding/changing any attribute alters
  `fingerprint_sha256`; preserving enum label ordering affects fingerprint).
- All drift rules:
  - Identical snapshots (`fingerprints_match = true`, zero findings).
  - Breaking: dropped table, dropped view, dropped sequence, dropped enum,
    dropped column, column `data_type` change, column `nullable` flip (both
    directions), RLS disabled, dropped RLS policy, view kind change, sequence
    type change.
  - Warning: dropped/modified constraint, dropped/modified index, modified RLS
    policy, `rls_forced` disabled, column default change, view definition
    SHA-256 change, sequence attribute change, enum label removal & reorder.
  - Info: added table, added column, added constraint, added index, added view,
    added sequence, added enum, added enum label, RLS enabled, policy added.
- `DriftFailThreshold::parse_loose` and `SnapshotDiffReport::exceeds_threshold`
  across all combinations.
- Bounds enforcement (`MAX_SNAPSHOT_TABLES`, `MAX_SNAPSHOT_COLUMNS`,
  `MAX_SNAPSHOT_INDEXES`, `MAX_SNAPSHOT_POLICIES`) setting `truncated = true`.
- JSON round-trip (`render_json` / `from_json`) and version/size bounds.
- Strictly plain-ASCII output verification on `render_table`, `render_plain`,
  and `render_json` even with hostile Unicode / ANSI control sequences in
  identifiers or expressions.

## 4. Coordinator Integration (Deferred to `tea-wire` Phase 2)

- Export `pub mod snapshot;` in `src/lib.rs` and `pub mod snapshot;` in
  `src/postgres/mod.rs`.
- Wire `Session` wrapper, CLI subcommand (`ignatius snapshot`), and interactive
  TUI drift guard surface in `src/postgres/session.rs`, `src/cli/*`, `src/app/*`,
  and `src/ui/*`.
