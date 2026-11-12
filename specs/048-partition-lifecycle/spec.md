# Feature Specification: Partition Lifecycle (048)

**Feature Branch**: `feat/m11-operational-data`
**Created**: 2026-11-12
**Status**: Draft
**Authority**: Roadmap Milestone 11 (M11: Operational Data Lifecycle)

## 1. Context and Problem Statement

Range-partitioned tables only stay healthy when someone creates the next
partition before the current one fills. That work is usually a calendar
reminder and a hand-written `CREATE TABLE ... PARTITION OF` statement copied
from the previous month. The copy is easy to get wrong: an overlapping bound
fails late, a mistyped date silently creates the wrong month, and dropping old
partitions is often done with a one-off statement nobody reviews.

This feature makes the partition set visible and generates the reviewed DDL for
the next intervals and for expiry. It never executes DDL. A partition change is
a schema change, so it belongs in the same reviewed path as a migration.

## 2. User Scenarios

### Scenario 1: A DBA sees the partition set

An operator runs `ignatius partition status --schema billing`. The report lists
each partitioned table, its strategy and key, every partition with its bound,
size, estimated rows and attached state. A table with no default partition and
one month of headroom is visible immediately.

### Scenario 2: The next three months are prepared for review

A release engineer runs `ignatius partition plan --table billing.invoices
--period monthly --start 2026-12-01 --count 3`. The command reads the existing
bounds, refuses to generate any interval that overlaps an existing partition,
and prints three `CREATE TABLE ... PARTITION OF ... FOR VALUES FROM ... TO ...`
statements. A reviewer commits them as a migration.

### Scenario 3: Expired partitions are reviewed, not dropped silently

The same engineer adds `--expire-before 2025-01-01`. The plan appends
`ALTER TABLE ... DETACH PARTITION ...` and `DROP TABLE ...` statements for
partitions wholly before the cutoff, with the partition name and bound in a
comment. Nothing runs until the reviewed file is applied.

## 3. Functional Requirements

- FR-4801: `ignatius partition status` lists partitioned tables and their
  partitions from `pg_partition_tree`, `pg_class`, `pg_get_partkeydef` and
  `pg_get_expr(relpartbound)`, bounded to 500 partitioned tables and 5000
  partitions.
- FR-4802: Status includes schema, parent, child, strategy (`RANGE`, `LIST`,
  `HASH`), key definition, bound expression, total bytes, estimated rows,
  whether the partition is attached, and whether a default partition exists.
- FR-4803: `ignatius partition plan` generates DDL only for single-column
  `RANGE` partitioning on a `date`, `timestamp` or `timestamptz` key. It
  refuses list, hash, multi-column and expression keys with a diagnostic that
  names the unsupported shape.
- FR-4804: `--period daily|weekly|monthly|yearly` and `--start YYYY-MM-DD` are
  required for a plan; `--count` is 1 through 120 and defaults to 3.
- FR-4805: Generated names are deterministic and safe:
  `<parent>_p<YYYY_MM_DD>` daily, `<parent>_p<YYYY_Www>` weekly,
  `<parent>_p<YYYY_MM>` monthly, `<parent>_p<YYYY>` yearly, truncated to the
  63-byte identifier limit with a stable hexadecimal suffix when necessary.
- FR-4806: The plan reads existing bounds and refuses to generate an interval
  that overlaps any existing partition whose bound it can parse. If any
  existing bound cannot be parsed, the plan refuses rather than guessing.
- FR-4807: `--expire-before YYYY-MM-DD` appends reviewed detach and drop
  statements only for partitions whose upper bound is on or before the cutoff.
  A partition with `MAXVALUE` is never expired.
- FR-4808: The plan never executes SQL. It prints SQL to standard output and a
  one-line summary to standard error, so `> plan.sql` is safe.
- FR-4809: Status supports `--format table|json`; plan always emits SQL.
- FR-4810: No write commands are added. Applying the reviewed plan through
  `ignatius migrate` keeps one audited write path and one ledger.
- FR-4811: The existing partition set is read at plan time; a concurrent
  partition creation can still make a generated statement fail. The command
  documents this as a review-time snapshot, not a lock.
- FR-4812: `docs/design/partition-lifecycle.md` documents supported shapes,
  naming, bound parsing, expiry rules and limits.

## 4. Non-Functional Requirements

- Pure `src/partition/` model and bound arithmetic with no driver or process
  calls.
- Date arithmetic is calendar-correct across month lengths, leap years and
  year boundaries; no floating-point time arithmetic.
- ASCII output; JSON is machine-only.
- No credentials or row data in any output.

## 5. Out of Scope

- Creating, attaching, detaching or dropping partitions directly.
- Default partition maintenance and row movement.
- Sub-partitioning beyond reporting the catalog shape.
- Partition-key changes or data migration between partition schemes.
