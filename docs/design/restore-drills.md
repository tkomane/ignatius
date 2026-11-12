# Restore Drills

`ignatius restore-drill` proves that an archive can be read, loaded and queried.
It reads the archive table of contents, restores into a newly created scratch
database on the same server, compares the restored catalog with the source, and
removes the scratch database unless asked to keep it.

It never restores into the source database.

```bash
ignatius restore-drill "$DATABASE_URL" --backup /backups/orders.dump
ignatius restore-drill "$DATABASE_URL" --backup /backups/orders.dump --keep
ignatius restore-drill "$DATABASE_URL" --backup /backups/orders.dump --dry-run
```

## Steps

1. **Inspect.** `pg_restore --list` reads the table of contents before any
   database is created. An empty, unreadable or non-archive input is refused.
   Plain SQL dumps are refused because they have no table of contents.
2. **Create.** The scratch database is created from `TEMPLATE template0`, so it
   starts empty and does not inherit objects from the source database.
3. **Restore.** `pg_restore --exit-on-error` loads the archive into the scratch
   database only.
4. **Verify.** The command counts user relations in the source and in the
   scratch database and measures the restored database size. A count mismatch
   is a failed drill, not a warning.
5. **Decide.** The scratch database is dropped or kept according to the policy
   below.

## Safety checks

- The scratch name is validated against the 63-byte identifier limit, the source
  database name and the system databases (`postgres`, `template0`, `template1`).
- An existing database with the scratch name is refused before anything changes.
- The connected role must have `CREATEDB`; a missing privilege is reported
  before the scratch database is created.
- A connection target classified as production requires `--allow-production`,
  because a drill creates and drops a database on that server.
- Every identifier in DDL is quoted. The archive path is passed as an argument,
  never through a shell.
- Dropping terminates connections to the scratch database only, then drops that
  database.
- Child processes inherit no terminal input, so a missing password fails instead
  of prompting mid-drill.

## Retention policy

| Outcome | Default | `--keep` | `--drop-on-failure` |
| --- | --- | --- | --- |
| Passed | drop | keep | drop |
| Failed | keep for inspection | keep | drop |

`--keep` wins when it is combined with `--drop-on-failure`. A failed drill that
keeps its scratch database prints the name in the report and on standard error.

## Options

`--backup PATH` names the archive. `--scratch-database NAME` overrides the
generated name. `--keep` and `--drop-on-failure` control retention.
`--allow-production` is required before a production target may be used.
`--dry-run` inspects the archive and prints the plan without changing the
server. `--format table|json`, `--output PATH` and `--force` control the report.

## Report and exit codes

`--format table` is the default; `--format json` emits stable field names. The
report carries the source and scratch names, archive format, table-of-contents
entry count, outcome, duration, relation counts, restored size, whether the
scratch database was kept, and a bounded failure reason.

| Code | Meaning |
| --- | --- |
| 0 | the drill passed |
| 1 | the drill ran and failed |
| 2 | the drill was refused before making changes |

Exit 0 means this archive restored on this server at this time. It does not
prove point-in-time recovery, WAL replay or that every backup is healthy.

## Archive formats

| Format | Detected by |
| --- | --- |
| `custom` | default for a file that is not `.tar` or `.sql` |
| `directory` | the archive path is a directory |
| `tar` | a `.tar` extension |
| plain SQL | a `.sql` extension; refused |

The format is confirmed by `pg_restore --list`; an extension alone never
guarantees that the bytes are a valid archive.

## Credentials

The password travels only in the child process environment as `PGPASSWORD`.
It never appears in the report, diagnostics or standard output. `--dry-run`
prints the exact steps with `PGPASSWORD=[redacted]` and changes nothing on the
server.

## What the relation count compares

User relations of kind ordinary table, partitioned table, view, materialized
view, sequence and foreign table, in every schema except `pg_*` and
`information_schema`. The same query runs against both databases, so the
comparison is like for like. It is a catalog comparison, not a row-by-row data
comparison.

## Bounds and limits

- The table of contents is capped at 20000 lines; a larger listing is refused.
- A failure reason is capped at 2000 characters.
- The drill runs against the same server as the source; restoring to another
  server needs its own credential path and is out of scope.
- The archive must be readable by `pg_restore` on the machine running Ignatius.
- Supported server versions are PostgreSQL 14 through 18 with matching client
  tools.
- Scheduling, drill history, point-in-time recovery and physical base backups
  are out of scope.
