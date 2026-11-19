# Feature Specification: Logical Replication and Slot Safety Manager (051)

**Feature Branch**: `feat/m12-psql-and-cluster-parity`
**Created**: 2026-11-18
**Status**: In Progress (Domain, Extractor, and Specification)
**Authority**: Roadmap Milestone 12 (M12: Complete psql & Server/Replication Parity)

## 1. Context and Problem Statement

Logical replication outages rarely start with a dramatic failure. More often, a
consumer stops or falls behind, its replication slot stays `inactive` or turns
`unreserved` or `lost`, and unrecycled WAL segments silently accumulate until
the primary volume fills. Operators also need to inspect publications,
subscriptions, and slot health without exposing subscription connection
passwords or accidentally executing destructive slot drops.

This feature delivers a read-only bounded catalog inspector for
`pg_publication`, `pg_publication_tables`, `pg_subscription`,
`pg_stat_subscription`, `pg_subscription_rel`, and `pg_replication_slots`,
paired with pure risk classification, central credential redaction, structured
output renderers, and reviewed SQL generators that are never executed
automatically.

## 2. User Scenarios

### Scenario 1: An operator checks whether an inactive slot is hoarding WAL

An operator runs the replication inspection and sees every replication slot
with its plugin, slot type, target database, active state, `wal_status`,
retained WAL bytes, and `safe_wal_size`. An inactive slot holding 1.5 GiB of
retained WAL is flagged `critical` with a clear finding, and a reviewed
`pg_drop_replication_slot` script is available for manual review.

### Scenario 2: A standby replica is inspected in recovery

When connected to a standby server where `pg_is_in_recovery()` is true,
`pg_current_wal_lsn()` cannot be executed. The inspector skips
`pg_current_wal_lsn()` cleanly, reports `retained_bytes` as `unavailable`
rather than failing the report or fabricating zero, and still reports
`wal_status`, `safe_wal_size`, and active state.

### Scenario 3: A non-superuser inspects publications and slots

A monitoring or application role without `pg_subscription` privileges inspects
replication. Because `pg_subscription` is restricted by default in PostgreSQL
(SQLSTATE `42501`), the inspector catches permission denial cleanly, marks
`subscription_access_restricted = true`, leaves subscriptions empty, and still
returns complete publication and slot telemetry.

### Scenario 4: A subscription connection string contains a password

Any `password=...` libpq keyword/value pair or `postgres://user:secret@host/db`
URI password in `pg_subscription.subconninfo` is passed through the repository's
central redaction path (`crate::diagnostics::redaction`) before entering the
domain model, so secrets never appear in table, plain, or JSON output.

## 3. Functional Requirements

- FR-5101: Report publications from `pg_publication` and `pg_publication_tables`
  with name, owner, `all_tables`, DML flags (`insert`, `update`, `delete`,
  `truncate`), bounded table list, and `tables_truncated` indicator.
- FR-5102: Report subscriptions from `pg_subscription`, `pg_stat_subscription`
  (apply worker `relid IS NULL`), and `pg_subscription_rel` with name, owner,
  enabled state, redacted connection string (`conninfo_redacted`), slot name,
  derived sync state, `received_lsn`, `latest_end_lsn`, and `lag_bytes`.
- FR-5103: Redact all subscription connection strings through
  `crate::diagnostics::redaction::redact_connection_string` so both keyword
  (`password=...`) and URI (`user:secret@`) credentials become `[redacted]`.
- FR-5104: Handle SQLSTATE `42501` (`INSUFFICIENT_PRIVILEGE`) on
  `pg_subscription` gracefully by setting `subscription_access_restricted = true`
  with an empty subscription list instead of failing the report.
- FR-5105: Report replication slots from `pg_replication_slots` with slot name,
  plugin, slot type, database, temporary flag, active flag, `active_pid`,
  `wal_status`, `retained_bytes`, `safe_wal_size`, `risk`, and `finding`.
- FR-5106: Compute `retained_bytes` using `pg_wal_lsn_diff(pg_current_wal_lsn(),
  restart_lsn)` only when `NOT pg_is_in_recovery()` and `restart_lsn IS NOT NULL`;
  otherwise return `None`. Remain compatible with PostgreSQL 14 through 18.
- FR-5107: Classify slot risk deterministically:
  - `critical` if `wal_status == "lost"` or (`!active` and `retained_bytes >= 1 GiB`);
  - `warning` if `wal_status == "unreserved"`, or (`!active` and `retained_bytes >= 100 MiB`), or `!active`;
  - `ok` otherwise.
- FR-5108: Bound catalog queries in SQL to `MAX_PUBLICATIONS = 200`,
  `MAX_SUBSCRIPTIONS = 200`, `MAX_REPLICATION_SLOTS = 500`, and
  `MAX_PUBLICATION_TABLES = 200` per publication, setting explicit truncation
  booleans when limits are exceeded.
- FR-5109: Render the report deterministically into aligned ASCII tables
  (`render_table`), untruncated plain text (`render_plain`), and JSON
  (`render_json`), filtered by `ReplicationSection` (`all`, `publications`,
  `subscriptions`, `slots`).
- FR-5110: Provide reviewed SQL generators (`drop_slot_sql`,
  `create_publication_sql`, `alter_subscription_toggle_sql`) with proper
  single-quote escaping and identifier quoting via `quote_identifier`. Generated
  SQL is never executed automatically.

## 4. Non-Functional Requirements

- Pure `src/replication/` domain model with zero database, filesystem, network,
  or terminal dependencies.
- ASCII-only text and comments in code and output renderers.
- Clippy clean under `-D warnings` and sidecar unit tests in `src/replication/tests.rs`.
- No unredacted credentials in memory structs, logs, diagnostics, or reports.

## 5. Out of Scope

- Automatic execution of `pg_drop_replication_slot`, `CREATE PUBLICATION`, or
  `ALTER SUBSCRIPTION`.
- Cross-cluster publisher-to-subscriber live connection hopping.
- Historical WAL retention time-series storage.
