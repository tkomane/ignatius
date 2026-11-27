# Tasks: Multi-Host Failover and Session Timeouts

**Input**: Design documents from `/specs/069-multi-host-failover-and-session-timeouts/`
**Prerequisites**: `spec.md`, `plan.md`

## Phase 1: PR 1A - Multi-Host Connection Resolution and Sequential Failover (`feat/m23-multi-host-failover`)

- [x] T001 Define `MAX_ENDPOINTS` (`16`), `Endpoint`, `ConnectionTarget::endpoints`, `candidate_targets()`, and `for_endpoint()` in `src/connection/target.rs`, keeping `host`/`port` synced to `endpoints[0]` and updating `copy_with`, `with_password`, `for_database`, and `for_object_tree`.
- [x] T002 Support comma-separated hosts and ports across URI (including bracketed IPv6 `[::1]:5432,[::1]:5433`), keyword/value, `pg_service.conf`, `PGHOST`/`PGPORT`, CLI `--host`/`--port`, and TOML profiles, enforcing host/port count matching, non-empty segments, `MAX_ENDPOINTS` cap, and conservative `VerifyFull` default whenever any candidate host is non-local.
- [x] T003 Implement sequential multi-host failover in `src/postgres/session.rs` (`connect_reported` / `Session::connect`) across `candidate_targets()` with `verify_target_session_attrs`, redacted per-endpoint failure summaries, `SessionInfo::target` reporting the connected endpoint, and object-tree pinning in `src/cli/interactive.rs`.
- [x] T004 Surface multi-host summary cleanly in `ConnectionPicker` (`src/app/connection_picker.rs`), `doctor` (`src/cli/interactive.rs`), and `docs/support/compatibility.md`.
- [x] T005 Add unit tests (`src/connection/target/tests.rs`, `src/postgres/session/tests.rs`, `src/app/connection_picker/tests.rs`, `src/cli/interactive/tests.rs`) and live PostgreSQL 18.7 integration tests (`tests/postgres_integration.rs`) covering plain failover, verified-TLS failover, and multi-endpoint `target_session_attrs=standby` rejection.
- [x] T006 Verify PR 1A with `cargo --locked xtask verify` against live PostgreSQL 18.7.

## Phase 2: PR 1B - Session Lock and Idle-in-Transaction Timeouts (`feat/m23-session-timeouts`)

- [x] T007 Add `lock_timeout` and `idle_in_transaction_session_timeout` configuration, profile, CLI, and connection parameters in `src/config/schema.rs`, `src/connection/target.rs`, and `src/cli/mod.rs`.
- [x] T008 Apply and report `lock_timeout` and `idle_in_transaction_session_timeout` during `Session` bootstrap in `src/postgres/session.rs` and `doctor` surfaces.
- [x] T009 Add unit tests and live PostgreSQL 18.7 integration tests for session/lock timeouts and verify with `cargo --locked xtask verify`.
