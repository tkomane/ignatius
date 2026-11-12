# Implementation Plan: Transaction & WAL Observability (Feature 036)

## Overview
Implement Feature `036-transaction-and-wal-observability` as part of Roadmap Milestone 7 (Enterprise Observability & Query Performance Forensics).

## Architecture & Module Layout

### 1. Pure Domain Layer: `src/telemetry/`
- `src/telemetry/mod.rs`:
  - `CheckpointerStats`: timed checkpoints, requested checkpoints, write time ms, sync time ms, buffers written by checkpoint, bgwriter, and backends.
  - `WalStats`: current LSN, records generated, fpi generated, bytes generated, buffers written, archived count, failed count, last archived WAL, last failed WAL.
  - `DatabaseXactStats`: database name, xact commit, xact rollback, rollback ratio (f64), deadlocks, conflicts, temp files, temp bytes.
  - `XidFreezeMetric`: database name, oldest datfrozenxid age, autovacuum_freeze_max_age, pct of emergency horizon consumed, oldest table relname, oldest relfrozenxid age.
  - `ReplicationStandby`: pid, client addr, application name, state, sync state, sent LSN, write lag ms/bytes, flush lag ms/bytes, replay lag ms/bytes.
  - `TelemetryReport`: complete snapshot joining checkpointer, WAL, database transactions, freeze forensics, and replication standbys.
  - `TelemetrySection`: `All`, `Checkpointer`, `Wal`, `Transactions`, `Freeze`, `Replication`.
  - `TelemetryFormat`: `Table`, `Json`, `Plain`.
- `src/telemetry/tests.rs`:
  - Pure unit tests for rollback ratio math, checkpointer timed ratio, XID wraparound percentage calculation, JSON serialization, and aligned table rendering.

### 2. PostgreSQL Extraction Layer: `src/postgres/`
- Catalog queries:
  - Checkpointer: `SELECT version();` to detect PG 17+ `pg_stat_checkpointer` vs PG 14-16 `pg_stat_bgwriter`.
  - WAL: `pg_stat_wal`, `pg_stat_archiver`, and `pg_current_wal_lsn()` / `pg_last_wal_replay_lsn()`.
  - Database Transactions: `pg_stat_database` aggregated and per-database.
  - XID Freeze: `SELECT datname, age(datfrozenxid) ... FROM pg_database;` joined with `current_setting('autovacuum_freeze_max_age')` and oldest `relfrozenxid` from `pg_class`.
  - Standbys: `pg_stat_replication`.
- Session methods in `src/postgres/session.rs`:
  - `Session::wal_and_transaction_telemetry(&self) -> Result<TelemetryReport, Error>`

### 3. Keymap & UX: `src/ui/` & `src/app/`
- Keymap:
  - Register `&` in `CHORDS`: `('&', Action::WalObservability, "Open transaction, WAL, and checkpointer observability")`.
  - Add `wal-observability` to `CONFIGURABLE`.
- Application Reducer (`src/app/`):
  - Model: `TelemetryState` with selected tab (`CheckpointsWal`, `TransactionsFreeze`, `Replication`), cursor index, snapshot report.
  - Messages: `OpenWalObservability`, `WalObservabilityLoaded`, `WalObservabilityTabChanged`, `WalObservabilityNext`, `WalObservabilityPrev`.
- Field Industrial TUI Widget (`src/ui/widgets/overlays.rs`):
  - Status capsules: `[COMMITS/S]`, `[ROLLBACK %]`, `[WAL RATE]`, `[CHECKPOINT HEALTH]`, `[MAX XID AGE]`, `[REPLICATION]`.
  - Dual-pane overview and inspector with hotkey actions.

### 4. Non-Interactive CLI: `src/cli/`
- `ignatius wal-stats [TARGET]`:
  - `--section all|checkpointer|wal|transactions|freeze|replication`
  - `--format table|json|plain`
  - `--output <path>`
  - `--force`

### 5. Verification & Contracts
- Contract test: `tests/wal_observability_contract.rs`.
- Update `docs/design/keymap.md`, `README.md`, `CHANGELOG.md`, `docs/status.md`.
- Complete verification run via `cargo xtask verify`.
