# Implementation Plan: Logical Replication and Slot Safety Manager (051)

## 1. Pure Domain Model (`src/replication/mod.rs`)

- Constants:
  - `MAX_PUBLICATIONS = 200`
  - `MAX_PUBLICATION_TABLES = 200`
  - `MAX_SUBSCRIPTIONS = 200`
  - `MAX_REPLICATION_SLOTS = 500`
  - `INACTIVE_SLOT_WARNING_BYTES = 100 * 1024 * 1024` (100 MiB)
  - `INACTIVE_SLOT_CRITICAL_BYTES = 1024 * 1024 * 1024` (1 GiB)
- Types:
  - `ReplicationSection` (`All`, `Publications`, `Subscriptions`, `Slots`) with
    `label()`, `Display`, `ACCEPTED`, and `from_str_loose()`.
  - `SlotRiskSeverity` (`Ok`, `Warning`, `Critical`) with `label()` returning
    `"ok"`, `"warning"`, `"critical"`, plus `Display` and serde lowercase mapping.
  - `PublicationSummary` (`name`, `owner`, `all_tables`, `insert`, `update`,
    `delete`, `truncate`, `tables`, `tables_truncated`) with `operations_label()`.
  - `SubscriptionSummary` (`name`, `owner`, `enabled`, `conninfo_redacted`,
    `slot_name`, `sync_state`, `received_lsn`, `latest_end_lsn`, `lag_bytes`)
    and `redact_conninfo(raw)` delegating to
    `crate::diagnostics::redaction::redact_connection_string`.
  - `ReplicationSlotSummary` (`slot_name`, `plugin`, `slot_type`, `database`,
    `temporary`, `active`, `active_pid`, `wal_status`, `retained_bytes`,
    `safe_wal_size`, `risk`, `finding`) with `assess_risk(...)` and
    `with_assessed_risk()`.
  - `ReplicationReport` (`publications`, `subscriptions`, `slots`,
    `publications_truncated`, `subscriptions_truncated`, `slots_truncated`,
    `subscription_access_restricted`) with summary helpers
    (`inactive_slots_count()`, `max_retained_bytes()`, `has_critical_findings()`,
    `has_warning_findings()`) and renderers (`render_table(section)`,
    `render_plain(section)`, `render_json()`).
- Reviewed SQL generators (pure string output, never auto-executed):
  - `drop_slot_sql(slot_name)`: warning comment + single-quote escaped
    `SELECT pg_catalog.pg_drop_replication_slot('...');`.
  - `create_publication_sql(name, all_tables)`: identifier-quoted via
    `crate::query::identifiers::quote_identifier`.
  - `alter_subscription_toggle_sql(name, enable)`: identifier-quoted
    `ALTER SUBSCRIPTION ... ENABLE;` or `DISABLE;`.

## 2. Unit Tests (`src/replication/tests.rs`)

- Sidecar test module verifying:
  - `redact_conninfo` on libpq `password=...` (including single-quoted and
    mixed-case keywords) and `postgres://user:secret@host/db` URIs.
  - `SlotRiskSeverity` thresholds across `ok`, inactive, `unreserved`,
    `>= 100 MiB`, `>= 1 GiB`, `lost`, and `None` retained bytes.
  - `ReplicationReport` summary counts and truncation reporting.
  - `render_table`, `render_plain`, and `render_json` across all
    `ReplicationSection` filters, empty catalogs, and `subscription_access_restricted`.
  - SQL quoting safety for `drop_slot_sql`, `create_publication_sql`, and
    `alter_subscription_toggle_sql` against single quotes, double quotes, and
    injected SQL fragments.

## 3. PostgreSQL Catalog Extractor (`src/postgres/replication.rs`)

- `pub async fn replication_report(client: &Client) -> Result<ReplicationReport, Diagnostic>`
- Queries:
  - `pg_catalog.pg_publication` (`LIMIT MAX_PUBLICATIONS + 1`) +
    `pg_catalog.pg_publication_tables` bounded via `ROW_NUMBER()` per retained
    publication (`<= MAX_PUBLICATION_TABLES + 1`).
  - `pg_catalog.pg_subscription` filtered to `current_database()`, lateral-joined
    to `pg_catalog.pg_stat_subscription` (`relid IS NULL`, ordered `LIMIT 1` so
    PG 14-18 parallel or table-sync workers never duplicate rows) and
    `pg_catalog.pg_subscription_rel` counts (`LIMIT MAX_SUBSCRIPTIONS + 1`).
    SQLSTATE `42501` (`SqlState::INSUFFICIENT_PRIVILEGE`) degrades cleanly to
    `subscription_access_restricted = true` with empty subscriptions.
  - `pg_catalog.pg_replication_slots` (`LIMIT MAX_REPLICATION_SLOTS + 1`),
    ordering inactive and highest-retaining slots first, using
    `CASE WHEN pg_catalog.pg_is_in_recovery() OR s.restart_lsn IS NULL THEN NULL ...`
    around `pg_wal_lsn_diff(pg_current_wal_lsn(), s.restart_lsn)` and keeping
    columns compatible across PostgreSQL 14 through 18.

## 4. Coordinator Integration & Verification (Deferred to Coordinator)

- Wire `pub mod replication;` into `src/lib.rs`, `src/postgres/mod.rs`, and
  `Session::replication_report` in `src/postgres/session.rs`.
- Add CLI subcommands/flags in `src/cli/mod.rs` and TUI integration in `src/app/*`.
- Add live PostgreSQL contract tests and update documentation parity.
