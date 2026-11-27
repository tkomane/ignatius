# Implementation Plan: Multi-Host Failover and Session Timeouts

**Spec**: [spec.md](spec.md)

## Delivery Slices

Feature 069 (Milestone M23) is delivered across two focused pull requests:

### PR 1A - Multi-Host Connection Resolution and Sequential Failover (`feat/m23-multi-host-failover`)

1. **`src/connection/target.rs` & `src/connection/mod.rs` & `src/connection/target/tests.rs`**:
   - Define `MAX_ENDPOINTS = 16` and `Endpoint { host: Host, port: u16 }`.
   - Add `endpoints: Vec<Endpoint>`, `passfile: Option<PathBuf>`, and `explicit_password: bool` tracking on `ConnectionTarget`, syncing `host`/`port` to `endpoints[0]`.
   - Update `parse_uri` to split comma-separated authority segments while preserving bracketed IPv6 (`[::1]:5432,[::1]:5433`), unbracketed IPv6, and empty-authority `?host=...` URIs.
   - Update `resolve` to validate host segments (`1..=MAX_ENDPOINTS`, no empty segments, platform Unix-socket check on each segment) and port lists (single port broadcast or exact count match).
   - Update `default_sslmode` to choose `VerifyFull` if any candidate host is non-local (`!host.is_local()`).
   - Implement `candidate_targets(&self)` with per-endpoint `.pgpass` re-lookup when `!explicit_password`, plus `for_endpoint(&self, endpoint: &Endpoint)` and single-endpoint `for_object_tree(&self)`.
   - Update `safe_display(&self)` to render single endpoints unchanged and comma-separated `host:port` lists for multi-endpoint targets.
2. **`src/postgres/session.rs` & `src/postgres/session/tests.rs` & `src/cli/interactive.rs`**:
   - Store `connected_endpoint: Endpoint` on `Session` and expose `Session::connected_endpoint()`.
   - Update `connect_reported` to iterate `target.candidate_targets()` sequentially (connect + TLS + `read_only` + `statement_timeout` + `bootstrap` + `verify_target_session_attrs`), returning immediately on first match without TLS downgrade or post-success retry.
   - Preserve exact single-candidate diagnostics; on multi-candidate exhaustion return `DiagnosticKind::Connection` naming every attempted endpoint and its redacted reason.
   - Pin interactive object-tree metadata connections (`spawn_metadata_connect`) to `opened.connected_endpoint()`.
3. **`src/app/connection_picker.rs`, `src/config/schema.rs`, `src/cli/interactive.rs` (`probe_target`), & `docs/support/compatibility.md`**:
   - Validate comma-separated hosts in `Profile::validate` and display multi-host counts/ports cleanly in `ConnectionProfileSummary::location`.
   - Update `doctor` (`probe_target`) to report an `endpoints` check and test reachability across all candidates rather than skipping `postgres` when only an earlier failover candidate is down.
   - Document multi-host failover (`host1,host2` / `port1,port2`) in `docs/support/compatibility.md`.
4. **`tests/postgres_integration.rs`**:
   - Add live PG 18.7 integration tests covering plain failover (`127.0.0.1:1,127.0.0.1:55432`), verified-TLS failover (`127.0.0.1:1,localhost:55433`), and total failure with `target_session_attrs=standby` (`127.0.0.1:55432,localhost:55432`).

### PR 1B - Session Lock and Idle-in-Transaction Timeouts (`feat/m23-session-timeouts`, Pending)

1. **`src/config/schema.rs`, `src/connection/target.rs`, & `src/cli/mod.rs`**:
   - Add `lock_timeout` and `idle_in_transaction_session_timeout` configuration, profile, CLI, and connection parameter support.
2. **`src/postgres/session.rs` & `tests/postgres_integration.rs`**:
   - Apply bounded `lock_timeout` and `idle_in_transaction_session_timeout` during session bootstrap and verify server enforcement against live PostgreSQL.

## Verification Gates

- `cargo --locked xtask verify` against live disposable PostgreSQL 18.7 plain (`127.0.0.1:55432`) and verified-TLS (`localhost:55433`) fixtures.
- `cargo --locked xtask security` verifying sidecar test hygiene, `AGENTS.md == CLAUDE.md`, and zero governed em/en dashes.
