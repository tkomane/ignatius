# Feature Specification: Multi-Host Failover and Session Timeouts

**Feature Branch**: `feat/m23-multi-host-failover` (PR 1A) and `feat/m23-session-timeouts` (PR 1B)
**Created**: 2026-11-27
**Status**: Complete (PR 1A #199 `6b159f3` and PR 1B #200 `dc849e0` merged on `main`; Slice 2 delivered `statement_timeout` and `lock_timeout`)
**Milestone**: M23 - Multi-Host Failover & Session Timeouts

## Problem Statement

Two standard libpq connection and session-safety capabilities are currently missing:

1. **Multi-host and multi-port connection failover.** `ConnectionTarget` models only a single `host` and `port`, so comma-separated host/port lists in URIs (`postgres://user@h1:5432,h2:5433/db`), keyword/value strings (`host=h1,h2 port=5432,5433`), `pg_service.conf`, `PGHOST`/`PGPORT`, CLI `--host`, and TOML profiles cannot resolve into ordered failover candidates or work alongside `target_session_attrs` (`read-write`, `read-only`, `primary`, `standby`).
2. **Session-level idle transaction and lock timeouts (Slice 2, PR 1B).** Operators cannot configure bounded `lock_timeout` and `idle_in_transaction_session_timeout` defaults across profiles, CLI flags, and session bootstrap to prevent runaway locks or idle transactions.

## Principles This Feature Must Keep

- **Principle I (Delight must remain truthful).** `SessionInfo::target` must display the actual connected endpoint rather than the full candidate list, and total failover exhaustion must report every attempted endpoint and its redacted failure or rejection reason.
- **Principle II (PostgreSQL correctness before generic abstraction).** Multi-host/multi-port resolution and sequential failover follow libpq rules (`MAX_ENDPOINTS = 16`, single port applies to all hosts, multiple ports must match host count, per-endpoint `.pgpass` lookup when no explicit password was supplied, and `target_session_attrs` verification per candidate).
- **Principle IV (Safe by default, with expert escape hatches).** Default `sslmode` is `VerifyFull` whenever ANY candidate host in `endpoints` is non-local (`!host.is_local()`). TLS is never downgraded across failover candidates, and once a session succeeds and returns to the caller, statements are never retried across hosts.
- **Principle VIII (One source of truth).** Passwords and secrets remain in `SecretString` and pass through `diagnostics::redaction` before appearing in per-endpoint failure summaries or `doctor` output.

## User Scenarios & Testing

### User Story 1 - Libpq-Compatible Multi-Host Sequential Failover (Priority: P1, Slice 1 / PR 1A)

When an operator provides comma-separated hosts and optional comma-separated ports across a `postgres://` / `postgresql://` URI (including bracketed IPv6 `[::1]:5432,[::1]:5433`), keyword/value string, `pg_service.conf`, `PGHOST`/`PGPORT`, CLI `--host`, or TOML profile, Ignatius resolves ordered `Endpoint` candidates (up to `MAX_ENDPOINTS = 16`). During `Session::connect` / `connect_reported`, Ignatius tries each candidate sequentially in order - connecting, negotiating TLS without downgrade, running `bootstrap`, and checking `target_session_attrs` - returning the first qualifying `Session` with `SessionInfo::target` set to the actual connected endpoint and pinning any follow-up object-tree connection to that same endpoint. If all candidates fail or are rejected, Ignatius returns `DiagnosticKind::Connection` naming each attempted endpoint and its redacted reason.

### User Story 2 - Session Lock and Idle-in-Transaction Timeouts (Priority: P2, Slice 2 / PR 1B)

When an operator configures `lock_timeout` or `idle_in_transaction_session_timeout` via configuration, profile, CLI flags, or connection parameters, Ignatius applies those PostgreSQL session settings during bootstrap alongside `statement_timeout` and surfaces active timeout bounds truthfully.

## Functional Requirements

### Slice 1 - Multi-Host Connection Resolution and Sequential Failover (PR 1A)

- **FR-001**: In `src/connection/target.rs`, define `pub const MAX_ENDPOINTS: usize = 16;` and `pub struct Endpoint { pub host: Host, pub port: u16 }`. Add `pub endpoints: Vec<Endpoint>` on `ConnectionTarget`, keeping `target.host` and `target.port` synced to `endpoints[0]` so existing callers stay intact. Update `copy_with`, `for_object_tree`, `with_password`, `for_database`, and add `for_endpoint`.
- **FR-002**: Support comma-separated hosts and ports across URI (`postgres://user@h1:5432,h2:5433/db`, `postgres://user@h1,h2/db`, IPv6 `[::1]:5432,[::1]:5433`, mixed URI segment ports defaulting omitted segments to `5432`), keyword/value (`host=h1,h2 port=5432,5433`), `pg_service.conf`, `PGHOST`/`PGPORT`, and CLI `--host`. `--port` overrides or a single port applies to all hosts; multiple ports must match host count; empty host segments, invalid ports, mismatched port counts, or `> MAX_ENDPOINTS` hosts return `DiagnosticKind::Config`.
- **FR-003**: Update `default_sslmode` so that if ANY candidate host in `endpoints` is not local (`!host.is_local()`), the default `sslmode` is `SslMode::VerifyFull`.
- **FR-004**: Add `pub fn candidate_targets(&self) -> Vec<ConnectionTarget>` building one single-endpoint target per `Endpoint`, re-consulting `.pgpass` per host/port only when no explicit password was supplied from URI, keyword/value, service file, `PGPASSWORD`, or prompt/cloud provider (`with_password`). Keep `for_object_tree` scoped to a single endpoint.
- **FR-005**: In `src/postgres/session.rs` (`connect_reported` / `Session::connect`), iterate `target.candidate_targets()` in order: establish connection + TLS + `read_only` + `statement_timeout` + `bootstrap` + `verify_target_session_attrs`. On failure or `target_session_attrs` mismatch, record a redacted per-endpoint summary and try the next candidate. Never downgrade TLS and never retry once a session succeeds and returns to the caller. For single-endpoint targets, preserve exact per-class diagnostics (`Tls`, `Authentication`, `Connection`). On multi-endpoint total failure, return `DiagnosticKind::Connection` listing every attempted endpoint and why it failed/was rejected.
- **FR-006**: Ensure `SessionInfo::target` reflects the actual connected endpoint (`candidate.safe_display()`), store `connected_endpoint` on `Session`, and pin interactive object-tree metadata connections (`spawn_metadata_connect`) to the connected endpoint via `for_endpoint` + `for_object_tree`.
- **FR-007**: Surface multi-host target summaries cleanly in `ConnectionProfileSummary` (`src/app/connection_picker.rs`), `doctor` (`src/cli/interactive.rs`), and `docs/support/compatibility.md`.

### Slice 2 - Session Lock and Idle-in-Transaction Timeouts (PR 1B, Deferred)

- **FR-008**: Add `lock_timeout` and `idle_in_transaction_session_timeout` configuration, CLI, and target resolution parameters with strict validation.
- **FR-009**: Apply configured `lock_timeout` and `idle_in_transaction_session_timeout` during `Session` bootstrap and surface active session timeout posture in `SessionInfo` and `doctor`.

## Non-Functional Requirements

- **NFR-001 (Secret Redaction)**: Per-endpoint failure summaries and `doctor` checks must pass through `diagnostics::redaction::redact_text` and never expose passwords or secret tokens.
- **NFR-002 (Repository Hygiene)**: Unit tests reside in sidecar `tests.rs` files only, zero raw `Color::Rgb(` / `Color::Indexed(` in `src/ui/widgets/`, `AGENTS.md == CLAUDE.md`, and zero Unicode em/en dashes (`\u{2014}` / `\u{2013}`) in prose.
- **NFR-003 (Verification)**: Passes `cargo --locked xtask verify` against live plain (`127.0.0.1:55432`) and verified-TLS (`localhost:55433`) PostgreSQL 18.7 fixtures.
