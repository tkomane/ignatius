# Tasks: Optimistic Cell Concurrency and Session Role Routing

**Input**: Design documents from `/specs/066-optimistic-cell-concurrency-and-session-routing/`
**Prerequisites**: `spec.md`, `plan.md`

## Phase 1: PR 1A - Optimistic Result-Cell Concurrency Guard (`feat/m20-optimistic-cell-update`)

- [x] T001 Add `optimistic_guard` and ordinary `Cell::Null` / non-`NULL` (`__ignatius_old_value`) `WHERE` predicates in `src/query/update.rs`, preserving post-binding 16 KiB size checks and secret redaction.
- [x] T002 Update `src/ui/review.rs` (`Kind::Update`) to disclose the optimistic observed-value check alongside existing literal/structured disclosures and `"Nothing has been sent yet."`.
- [x] T003 Update `src/app/update.rs` to report `Some(0)` affected rows as a concurrent value-change or deletion conflict without retrying and instruct the operator to refresh (`F6`).
- [x] T004 Update `docs/support/compatibility.md` to reflect optimistic cell-to-UPDATE concurrency semantics.
- [x] T005 Add unit tests (`src/query/update/tests.rs`), reducer and review tests (`src/app/update/tests.rs`, `src/ui/layout/tests.rs`), and live PostgreSQL 18.7 integration tests (`tests/postgres_integration.rs`) covering unchanged `NULL`/non-`NULL` updates and concurrent modification/deletion conflicts.
- [x] T006 Verify PR 1A with `cargo --locked xtask verify` against live PostgreSQL 18.7.

## Phase 2: PR 1B - `target_session_attrs` Session Role Routing (`feat/m20-target-session-attrs`)

- [ ] T007 Implement `TargetSessionAttrs` (`Any`, `ReadWrite`, `ReadOnly`, `Primary`, `Standby`), `PGTARGETSESSIONATTRS`, `SUPPORTED_KEYS`, and strict libpq resolution precedence in `src/connection/target.rs`.
- [ ] T008 Wire `target-session-attrs` into `Profile` (`src/config/schema.rs`) and `--target-session-attrs` into `ConnectionOptions` (`src/cli/mod.rs`).
- [ ] T009 Enforce `TargetSessionAttrs` in `src/postgres/session.rs` using `transaction_read_only` and `pg_is_in_recovery()` without silent downgrade or retry, and display non-`Any` posture in connection details/doctor surfaces.
- [ ] T010 Add unit tests and live PostgreSQL 18.7 integration tests for `target_session_attrs` and verify with `cargo --locked xtask verify`.
