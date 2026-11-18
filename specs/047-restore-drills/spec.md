# Feature Specification: Restore Drills (047)

**Feature Branch**: `feat/m11-operational-data`
**Created**: 2026-11-12
**Status**: Draft
**Authority**: Roadmap Milestone 11 (M11: Operational Data Lifecycle)

## 1. Context and Problem Statement

A backup that has never been restored is a file, not a recovery plan. Ignatius
already runs `pg_dump` and `pg_restore`, but nothing proves that an archive can
be read, loaded and queried. Restoring over the source would be destructive, and
restoring by hand into a scratch database is rarely rehearsed.

This feature adds `ignatius restore-drill`: it reads the archive table of
contents, restores into a newly created scratch database on the same server,
compares the restored catalog with the source, reports the result, and removes
the scratch database unless asked to keep it. It never restores into the source
database.

## 2. User Scenarios

### Scenario 1: A weekly drill proves the archive is restorable

An operator runs `ignatius restore-drill --backup /backups/orders.dump`. The
command confirms the archive has 214 entries, creates
`orders_ignatius_drill_20261112T190000Z`, restores with `--exit-on-error`,
compares 38 source relations with 38 restored relations, reports pass in nine
seconds, and drops the scratch database.

### Scenario 2: A truncated archive fails the drill

The same command runs against a copy truncated during transfer. `pg_restore`
exits non-zero. The drill report fails with the tool's diagnostic, keeps the
scratch database for inspection, and exits 1. The source database is untouched.

### Scenario 3: An operator inspects the restored data

A migration author adds `--keep`. The drill passes, prints the scratch database
name, and leaves it in place. A follow-up `ignatius query` against that database
can compare a table before the migration is applied to production.

## 3. Functional Requirements

- FR-4701: `ignatius restore-drill --backup PATH` accepts a custom, directory
  or tar archive. Plain SQL dumps are refused because they have no table of
  contents and cannot be inspected before loading.
- FR-4702: The archive's table of contents is read with `pg_restore --list`
  before any database is created. An empty, unreadable or non-archive input
  fails with a usage diagnostic and no server changes.
- FR-4703: The target connection supplies server, credentials and the source
  database. The drill never restores into the source database.
- FR-4704: The scratch database name defaults to
  `<source>_ignatius_drill_<UTC timestamp>` and is validated against the
  63-byte identifier limit. `--scratch-database NAME` overrides it.
- FR-4705: The scratch database is created with `TEMPLATE template0`. The
  command refuses a scratch name that already exists, equals the source
  database, or is a system database. A missing `CREATEDB` privilege produces a
  clear diagnostic and no changes.
- FR-4706: Restore runs with `--exit-on-error`, into the scratch database only.
  On success the drill compares the number of user relations in the source and
  the restored database, and records restored database size and elapsed time.
- FR-4707: A relation-count mismatch is a failed drill, not a warning.
- FR-4708: The scratch database is dropped after a passing drill unless
  `--keep` is given. A failed drill keeps it by default so the failure can be
  inspected; `--drop-on-failure` removes it. Dropping first terminates
  connections to the scratch database only.
- FR-4709: The report is available as `table` or `json` and carries the source,
  scratch name, archive format, TOC entry count, result, duration, source and
  restored relation counts, restored size and a bounded failure reason.
- FR-4710: Exit 0 means the drill passed; exit 1 means the drill ran and failed;
  exit 2 means it was refused before making changes.
- FR-4711: Production targets require `--allow-production` because the drill
  creates and drops a database on that server.
- FR-4712: `--dry-run` prints the archive inspection result and the exact
  actions it would take, with the redacted tool command, and changes nothing.
- FR-4713: Credentials are passed only through the child process environment
  and the inherited connection options; no password appears in output,
  diagnostics or the report.
- FR-4714: The source database is read only for relation and size counts. The
  drill does not write to it.
- FR-4715: The drill is CLI-only and adds no terminal key binding.

## 4. Non-Functional Requirements

- Pure drill planning and verification logic in `src/drill/` with no process
  calls; execution and tool invocation stay in the CLI layer.
- Output is ASCII and every child process result is bounded and redacted.
- No panic on a missing tool, a permission failure, or a server that refuses
  `CREATE DATABASE`.
- The drill works with PostgreSQL 14 through 18 and the matching client tools.

## 5. Out of Scope

- Scheduling drills or storing drill history.
- Point-in-time recovery, WAL replay and physical base backups.
- Restoring onto a different server, which needs its own credential path.
- Automatic deletion of production data or backups.
