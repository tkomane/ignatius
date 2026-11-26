# Implementation Plan: Optimistic Cell Concurrency and Session Role Routing

**Spec**: [spec.md](spec.md)

## Delivery Slices

Feature 066 (Milestone M20) is delivered across two focused, independent pull requests:

### PR 1A - Optimistic Result-Cell Concurrency Guard (`feat/m20-optimistic-cell-update`)

1. **`src/query/update.rs` & `src/query/update/tests.rs`**:
   - Add `pub optimistic_guard: bool` to `UpdatePlan` while keeping `guarded` strictly for structured origin+type guards.
   - Update `build_update` so ordinary `plan_update` calls inspect `selected_row[source.selected_result_column]`:
     - `Cell::Null` appends `{column} IS NULL`.
     - Non-`NULL` binds secret `__ignatius_old_value` and appends `{column} IS NOT NULL AND pg_catalog.format('%s', {column}) COLLATE pg_catalog."C" OPERATOR(pg_catalog.=) :__ignatius_old_value`.
   - Enforce `MAX_UPDATE_SQL_BYTES` (16 KiB) after binding and verify secret redaction in `Debug` / `sql_template` / `UpdateRefusal`.
2. **`src/ui/review.rs` & `src/ui/layout/tests.rs`**:
   - Disclose in `Kind::Update` that the generated `UPDATE` verifies the observed cell value is still unchanged before applying, while preserving `"Nothing has been sent yet."` and structured-edit disclosures.
3. **`src/app/update.rs` & `src/app/update/tests.rs`**:
   - Report `Some(0)` rows affected on cell updates with `"UPDATE matched 0 rows: the row was deleted or the observed cell value changed concurrently. Nothing was retried; refresh (F6) to read current data."`
   - Update and add reducer tests for ordinary optimistic template binding, review disclosure, and 0-row conflict notices.
4. **`docs/support/compatibility.md` & `tests/postgres_integration.rs`**:
   - Update compatibility documentation for optimistic cell-to-UPDATE semantics.
   - Add live PostgreSQL tests verifying unchanged non-`NULL` and `NULL` cell updates affect 1 row, and concurrent modification (value-to-value, `NULL`-to-value, value-to-`NULL`) or deletion affects 0 rows and preserves the peer change.

### PR 1B - `target_session_attrs` Session Role Routing (`feat/m20-target-session-attrs`)

1. **`src/connection/target.rs` & `src/connection/target/tests.rs`**:
   - Introduce `TargetSessionAttrs` (`Any`, `ReadWrite`, `ReadOnly`, `Primary`, `Standby`), add `target_session_attrs` to `SUPPORTED_KEYS`, `PGTARGETSESSIONATTRS` to `EnvSnapshot::from_process()`, and strict libpq resolution precedence.
2. **`src/config/schema.rs` & `src/cli/mod.rs`**:
   - Add `target_session_attrs` profile field (`target-session-attrs`) and `--target-session-attrs` CLI flag with validation.
3. **`src/postgres/session.rs` & `tests/postgres_integration.rs`**:
   - Query `pg_is_in_recovery()` during bootstrap alongside `transaction_read_only`, enforce `TargetSessionAttrs` without retry or downgrade, and expose non-`Any` posture in connection details/doctor output.

## Verification Gates

- `cargo --locked xtask verify` against live disposable PostgreSQL 18.7 plain (`127.0.0.1:55432`) and verified-TLS (`localhost:55433`) fixtures.
- `cargo --locked xtask security` confirming sidecar test hygiene, `AGENTS.md == CLAUDE.md`, and zero governed em/en dashes.
