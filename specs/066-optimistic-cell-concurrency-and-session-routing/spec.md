# Feature Specification: Optimistic Cell Concurrency and Session Role Routing

**Feature Branch**: `feat/m20-optimistic-cell-update` (PR 1A) and `feat/m20-target-session-attrs` (PR 1B)
**Created**: 2026-11-26
**Status**: In Progress (PR 1A implemented; PR 1B pending)
**Milestone**: M20 - Optimistic Cell Concurrency & Session Role Routing

## Problem Statement

Two safety and routing gaps existed in PostgreSQL session handling and interactive cell updates:

1. **Ordinary result-cell updates lacked optimistic concurrency protection.** While structured JSONB/array edits already verified the observed cell value before updating, ordinary cell edits (`Ctrl+K u` / `plan_update`) matched only primary-key columns. If another session modified or deleted the observed cell between the user's `SELECT` snapshot and `UPDATE` confirmation, the ordinary `UPDATE` silently overwrote the concurrent change.
2. **PostgreSQL `target_session_attrs` routing was unsupported.** Users could not constrain connections using standard libpq `target_session_attrs` values (`any`, `read-write`, `read-only`, `primary`, `standby`) across CLI flags, connection URIs/kv pairs, `pg_service.conf`, `PGTARGETSESSIONATTRS`, or TOML profiles.

## Principles This Feature Must Keep

- **Principle I (Delight must remain truthful).** Zero-row cell-update outcomes must explicitly report that the row was deleted or the observed cell value changed concurrently, without pretending a write succeeded or silently retrying.
- **Principle II (PostgreSQL correctness before generic abstraction).** Optimistic cell checks use PostgreSQL's own `IS NULL` or bytewise `pg_catalog.format('%s', col) COLLATE pg_catalog."C" OPERATOR(pg_catalog.=)` semantics; session routing enforces `transaction_read_only` and `pg_is_in_recovery()` semantics directly.
- **Principle IV (Safe by default, with expert escape hatches).** Never lock rows during review, never silently retry a conflicted write, and never silently downgrade or retry a mismatched `target_session_attrs` session.
- **Principle VIII (One source of truth).** Observed cell values remain secret-bound in `ParameterBindings` (`__ignatius_old_value`) and are never leaked into `sql_template`, `Debug`, history, logs, or diagnostics.

## User Scenarios & Testing

### User Story 1 - Optimistic Result-Cell Concurrency Protection (Priority: P1, Slice 1 / PR 1A)

When an operator selects a result cell (`Ctrl+K u`), enters a replacement value, and confirms the reviewed `UPDATE`, Ignatius verifies in the same `UPDATE` statement's `WHERE` predicate that the observed cell still holds its retained value (`IS NULL` when observed as `NULL`, or bytewise equal to `:__ignatius_old_value` when non-`NULL`). If another connection changed or deleted the cell concurrently, PostgreSQL affects 0 rows, preserves the concurrent state, and Ignatius reports the conflict clearly with a prompt to refresh (`F6`).

### User Story 2 - Standard `target_session_attrs` Session Role Routing (Priority: P1, Slice 2 / PR 1B)

When connecting to a PostgreSQL target via CLI (`--target-session-attrs`), URI/key-value connection string, `pg_service.conf`, `PGTARGETSESSIONATTRS`, or TOML profile (`target-session-attrs`), Ignatius enforces the requested session posture (`any`, `read-write`, `read-only`, `primary`, `standby`) right after bootstrap using `transaction_read_only` and `pg_is_in_recovery()`, refusing mismatched sessions with `DiagnosticKind::Connection` and never downgrading or retrying.

## Functional Requirements

### Slice 1 - Optimistic Cell Concurrency (`066-optimistic-cell-concurrency-and-session-routing`, PR 1A)

- **FR-001**: In `src/query/update.rs` (`build_update` / `plan_update`), inspect `selected_row[source.selected_result_column]` even when `replacement.guard` is `None`.
  - For `Cell::Null`, append `format!("{column} IS NULL")` to the `WHERE` predicate.
  - For non-`NULL` (`Cell::Text`), bind secret parameter `__ignatius_old_value` and append `format!("{column} IS NOT NULL AND pg_catalog.format('%s', {column}) COLLATE pg_catalog.\"C\" OPERATOR(pg_catalog.=) :__ignatius_old_value")`.
- **FR-002**: Keep `UpdatePlan::MAX_UPDATE_SQL_BYTES` (16 KiB) checking after literal binding, keep `__ignatius_old_value` hidden in `sql_template` and `Debug`, and add `pub optimistic_guard: bool` on `UpdatePlan` while keeping `guarded` reserved for structured origin+type checks.
- **FR-003**: Update `src/ui/review.rs` (`Kind::Update`) to disclose clearly that the generated `UPDATE` verifies the observed cell value is still unchanged before applying, while keeping `"Nothing has been sent yet."` and structured-edit disclosures intact.
- **FR-004**: Update `src/app/update.rs` so `Some(0)` rows affected on a cell update reports: `"UPDATE matched 0 rows: the row was deleted or the observed cell value changed concurrently. Nothing was retried; refresh (F6) to read current data."`
- **FR-005**: Update `docs/support/compatibility.md` and add unit tests (`src/query/update/tests.rs`), reducer tests (`src/app/update/tests.rs`), review UI tests (`src/ui/layout/tests.rs`), and live PostgreSQL integration tests (`tests/postgres_integration.rs`).

### Slice 2 - `target_session_attrs` Session Routing (PR 1B)

- **FR-006**: Add `TargetSessionAttrs` (`Any`, `ReadWrite`, `ReadOnly`, `Primary`, `Standby`) in `src/connection/target.rs` with strict libpq precedence (CLI -> URI/kv -> `pg_service.conf` -> `PGTARGETSESSIONATTRS` -> profile config -> `Any`) and explicit validation.
- **FR-007**: Add `target_session_attrs` to `Profile` in `src/config/schema.rs` (`target-session-attrs`) and `--target-session-attrs` to `ConnectionOptions` in `src/cli/mod.rs`.
- **FR-008**: Enforce `target_session_attrs` in `src/postgres/session.rs` (`bootstrap` / `connect`) using `transaction_read_only` and `pg_is_in_recovery()`, closing mismatched sessions with `DiagnosticKind::Connection` without retry or downgrade, and expose non-`Any` posture in connection details/doctor surfaces.

## Non-Functional Requirements

- **NFR-001 (Secret Redaction)**: `__ignatius_old_value` must never appear in `sql_template`, `Debug`, statement history, logs, or diagnostics.
- **NFR-002 (Repository Hygiene)**: Sidecar `tests.rs` only, zero raw `Color::Rgb(` / `Color::Indexed(` in `src/ui/widgets/`, `AGENTS.md == CLAUDE.md`, zero Unicode em/en dashes (`\u{2014}` / `\u{2013}`) in prose.
- **NFR-003 (Verification)**: Passes `cargo --locked xtask verify` against live PostgreSQL 18.7.
