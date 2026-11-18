# Feature Specification: Retention and Growth Audit (049)

**Feature Branch**: `feat/m11-operational-data`
**Created**: 2026-11-12
**Status**: Draft
**Authority**: Roadmap Milestone 11 (M11: Operational Data Lifecycle)

## 1. Context and Problem Statement

Storage growth is usually noticed when a volume fills, long after the table that
caused it became a problem. The catalog already knows enough to answer the
operational questions without reading a single row: which relations consume the
most space, how fast they have grown since statistics were reset, how much dead
tuple debt has accumulated, and which sequences are close to exhaustion.

This feature adds a read-only `ignatius growth` audit. It never samples rows,
never changes statistics, and never writes to the database. Its output is
explicit about which numbers are estimates and what window they cover.

## 2. User Scenarios

### Scenario 1: An operator asks why the volume is full

An operator runs `ignatius growth --top 10` and sees the ten largest user
relations with heap, index and TOAST bytes separated, estimated live rows, dead
tuple ratio and the last vacuum time. The top entry is 400 GiB with 38 percent
dead tuples and no vacuum in nine days, which names both the space consumer and
the maintenance debt.

### Scenario 2: A release gate checks sequence headroom

A pipeline runs `ignatius growth --fail-on critical --format json`. A sequence
backing an integer primary key is at 97 percent of its range, the report marks
it critical, and the job fails before inserts start returning errors.

### Scenario 3: A DBA reviews partitioning candidates

A DBA filters to the `orders` schema and sees that an unpartitioned 1.2 TiB table
has grown 40 million rows since the statistics reset. The report marks it as a
partitioning candidate with the measured growth rate, without claiming the
table is safe to partition automatically.

## 3. Functional Requirements

- FR-4901: `ignatius growth` reads catalog and statistics views only. It does
  not read table rows, take samples, run `ANALYZE`, or write anything.
- FR-4902: Report the top N relations by total size (heap + indexes + TOAST),
  N default 20 and bounded to 1 through 200. Each entry carries schema, table,
  total bytes, heap bytes, index bytes, TOAST bytes, estimated live rows from
  `pg_class.reltuples`, dead tuple count and ratio, last vacuum, last analyze,
  and whether the relation is partitioned.
- FR-4903: Report growth since the database statistics reset for each relation:
  inserts, updates and deletes from `pg_stat_user_tables`, the reset timestamp
  from `pg_stat_database`, and a net rows-per-day estimate. When the window is
  shorter than one hour or the reset timestamp is unavailable, report the rate
  as unavailable rather than dividing by zero.
- FR-4904: Report sequence headroom for user sequences. For non-cycling
  sequences, show last value, maximum value, used percentage and remaining
  values. Mark critical at 5 percent remaining or fewer than 1000 values, and
  warning at 20 percent remaining. Cyclic sequences are listed as cyclic and
  are never marked exhausted.
- FR-4905: Report unpartitioned ordinary tables above a size threshold as
  partitioning candidates. The threshold is configurable with
  `--large-table-bytes` and defaults to 1 GiB.
- FR-4906: Findings carry a stable code and severity: `sequence-exhaustion`
  (critical), `vacuum-debt` (warning when dead tuples are at least 20 percent
  and at least 10000), and `partitioning-candidate` (warning).
- FR-4907: `--schema NAME` filters every section to one user schema. Without
  it, all non-system schemas are included.
- FR-4908: `--format table|json` defaults to table. `--fail-on never|warning|critical`
  defaults to `never`; when findings meet the threshold the command exits 1.
  Exit 0 means the audit ran, not that the database is healthy.
- FR-4909: Output is deterministic: findings by severity then code then object,
  relations by total bytes descending then qualified name, sequences by
  remaining fraction ascending then qualified name.
- FR-4910: The report labels estimates honestly: sizes are allocated bytes,
  row counts are planner estimates, growth is bounded by the statistics reset,
  and dead tuple counts depend on autovacuum activity.
- FR-4911: Catalog reads are bounded in SQL: at most 200 relations and 200
  sequences are retained, and the size ranking applies its limit in the query.
- FR-4912: The queries use only columns available in supported PostgreSQL
  versions 14 through 18. A sequence whose `last_value` is not readable is
  reported as unavailable, not as zero.
- FR-4913: The audit is CLI-only. It is a review and automation surface, not a
  dashboard, and adds no terminal key binding.
- FR-4914: `docs/design/growth-audit.md` documents each section, its query
  source, its bounds and its known limits.

## 4. Non-Functional Requirements

- Pure `src/growth/` domain with no driver, filesystem or process calls.
- ASCII table output and stable JSON field names.
- No credentials, connection strings, or row data in any output.
- No panic on empty catalogs, missing privileges, or null statistics.

## 5. Out of Scope

- Historical snapshots or trend storage across runs.
- Automatic retention deletion, partition creation, or `VACUUM` execution.
- Reading row timestamps to infer data age, which would require scanning data.
- Point-in-time recovery or backup verification, covered by Feature 047.
