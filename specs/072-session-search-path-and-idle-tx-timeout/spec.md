# Feature Specification: Session Search Path and Idle-in-Transaction Timeout

**Feature Branch**: `feat/m26-idle-in-tx-timeout` (PR 1A) and `feat/m26-session-search-path` (PR 1B)
**Created**: 2026-11-28
**Status**: In Progress
**Milestone**: M26 - Session Search Path & Idle-in-Transaction Timeout

## Problem Statement

Two standard PostgreSQL session safety and workflow capabilities remain open after M23:

1. **Configurable `idle_in_transaction_session_timeout` (Slice 1, PR 1A).** M23 delivered `statement_timeout` and `lock_timeout`, leaving `idle_in_transaction_session_timeout` unconfigurable. Operators opening manual `BEGIN` transactions in an interactive or scripted workbench need a bounded `idle_in_transaction_session_timeout` across `[connection]`, TOML profiles, URI/keyword/service parameters, and `--idle-in-transaction-session-timeout` so forgotten idle transactions do not hold locks or block vacuum indefinitely.
2. **Configurable `search_path` and interactive schema switching (Slice 2, PR 1B).** Operators working across multi-schema PostgreSQL databases (such as `app`, `analytics`, `billing`, and `$user`) currently cannot configure `search_path` on connection startup (`--search-path`, URI/keyword/service `search_path`, or profile `search-path`) or switch the active session's `search_path` inside the TUI without manually typing raw `SET search_path` statements.

## Principles This Feature Must Keep

- **Principle I (Delight must remain truthful).** `SessionInfo` reports the authoritative `search_path` read back from PostgreSQL (`current_setting('search_path')`) and the enforced `idle_in_transaction_session_timeout`. Inside an active transaction, interactive `set-search-path` honestly notes that `SET search_path` reverts if the transaction rolls back.
- **Principle II (PostgreSQL correctness before generic abstraction).** Timeout parsing reuses `parse_timeout_duration` (`0` / `0ms` / `0s`, integer seconds, or `ms`/`s`/`m`/`h` up to 24h) and enforces via integer-millisecond `SET idle_in_transaction_session_timeout = <ms>`. `search_path` parsing accepts `"$user"` / `$user` and PostgreSQL schema identifiers up to `MAX_SEARCH_PATH_SCHEMAS` (32) and emits `SET search_path TO ...` exclusively through `quote_identifier`.
- **Principle IV (Safe by default).** Never interpolate raw user strings into `SET` statements. Internal read-only object-tree connections keep their bounded metadata posture without inheriting long user idle-in-transaction overrides or arbitrary user `search_path` mutating catalog resolution. Stale async `SetSearchPath` replies are rejected by connection generation.
- **Principle V (Keyboard-first, never keyboard-only).** `set-search-path` is bindable in `[keys]` and discoverable via the Command Palette.

## User Scenarios & Testing

### User Story 1 - Configurable `idle_in_transaction_session_timeout` (Priority: P1, Slice 1 / PR 1A)

An operator configures `idle_in_transaction_session_timeout` via `--idle-in-transaction-session-timeout`, connection URI/keyword/service `idle_in_transaction_session_timeout`, profile `idle-in-transaction-session-timeout`, or `[connection] idle-in-transaction-session-timeout-seconds`. Ignatius validates the duration, applies integer-millisecond `SET idle_in_transaction_session_timeout = <ms>` on session startup, surfaces the bound in `SessionInfo`, `ConnectionPicker`, **Connection and auth details**, and `ignatius doctor`, and lets PostgreSQL terminate an idle-in-transaction session with SQLSTATE `25P03` when exceeded.

### User Story 2 - Configurable `search_path` and Interactive TUI Switcher (Priority: P1, Slice 2 / PR 1B)

An operator configures `search_path` at startup via `--search-path`, URI/keyword/service `search_path`, or profile `search-path`. On startup, Ignatius sets `SET search_path TO ...` using `quote_identifier` and reads back `current_setting('search_path')` into `SessionInfo::search_path`. Inside the TUI, running `set-search-path` opens a prompt (prefilled with the selected schema in Objects or the current `search_path`), validates the schema list purely, executes `Session::set_search_path`, and updates `SessionInfo::search_path`.

## Functional Requirements

### Slice 1 - `idle_in_transaction_session_timeout` (PR 1A)

- **FR-001**: Add `idle_in_transaction_session_timeout: Option<Duration>` to `ConnectionArgs` and `ConnectionTarget` in `src/connection/target.rs`, add `"idle_in_transaction_session_timeout"` to `SUPPORTED_KEYS`, and resolve precedence: CLI > URI/kv/service > profile > `[connection] idle-in-transaction-session-timeout-seconds` > server default (`None`). Preserve in `for_endpoint()` and keep `for_object_tree()` safe (`None`).
- **FR-002**: Add `idle_in_transaction_session_timeout_seconds: u64` to `ConnectionConfig` and `idle_in_transaction_session_timeout: Option<String>` to `Profile` and `PROFILE_FIELDS` in `src/config/schema.rs` with 24h validation.
- **FR-003**: Add `--idle-in-transaction-session-timeout <DUR>` to `ConnectionOptions` in `src/cli/mod.rs` and wire `has_explicit_route()`, `to_args()`, and `apply_profile()`.
- **FR-004**: Add `idle_in_transaction_session_timeout: Option<Duration>` to `SessionInfo` in `src/postgres/session.rs` and enforce `SET idle_in_transaction_session_timeout = <ms>` in `connect_candidate()`.
- **FR-005**: Surface `idle_in_transaction_session_timeout` in `ConnectionProfileSummary` (`src/app/connection_picker.rs`), **Connection and auth details** (`src/ui/widgets/palette.rs`), `ignatius doctor` (`src/diagnostics/doctor.rs` and `src/cli/interactive.rs`), and `docs/support/compatibility.md`.

### Slice 2 - `search_path` & Interactive Schema Switcher (PR 1B)

- **FR-006**: Add `MAX_SEARCH_PATH_SCHEMAS = 32`, `parse_search_path`, and `format_set_search_path_sql` in `src/connection/target.rs`; add `search_path: Option<Vec<String>>` to `ConnectionArgs` and `ConnectionTarget`, `"search_path"` to `SUPPORTED_KEYS`, `--search-path` to `ConnectionOptions`, and `search-path` to `Profile` / `PROFILE_FIELDS`.
- **FR-007**: Apply `format_set_search_path_sql` on startup in `connect_candidate()` before `bootstrap()`, and add `Session::set_search_path(&self, schemas: &[String]) -> Result<String, Diagnostic>` serialized by `user_sql_gate`.
- **FR-008**: Add `Action::SetSearchPath` (`"set-search-path"`), `NamePurpose::SetSearchPath`, `Effect::SetSearchPath { generation, schemas }`, and `Message::SearchPathUpdated { generation, result }` across `src/app/`, `src/ui/`, and `src/cli/interactive.rs`, guarded against disconnected, busy, failed-transaction, and stale-generation states.

## Non-Functional Requirements

- **NFR-001 (SQL Safety)**: All `SET idle_in_transaction_session_timeout` values use integer milliseconds; all `SET search_path TO ...` schemas pass `parse_search_path` and `quote_identifier`.
- **NFR-002 (Repository Hygiene)**: Sidecar `tests.rs` files only, `AGENTS.md == CLAUDE.md`, zero Unicode em/en dashes (`\u{2014}` / `\u{2013}`) in prose.
- **NFR-003 (Verification)**: Passes `cargo --locked xtask verify` against live plain (`127.0.0.1:55432`) and verified-TLS (`localhost:55433`) PostgreSQL 18.7 fixtures.
