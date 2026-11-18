# Implementation Plan: Restore Drills (047)

## 1. Domain (`src/drill/`)

- `mod.rs`:
  - `ArchiveFormat` (`Custom`, `Directory`, `Tar`) from an extension or
    `--format`, with `PlainSql` rejected.
  - `DrillRequest` (`source_database`, `scratch_database`, `archive_path`,
    `format`, `keep`, `drop_on_failure`).
  - `DrillPlan` with ordered `DrillStep` values: inspect, create, restore,
    verify, drop. `render_plan` prints the redacted steps for `--dry-run`.
  - `DRILL_SUFFIX` and `scratch_name(source, timestamp, override)` with
    63-byte truncation and collision-safe fallback.
  - `TocSummary` (`entries`, `object_types`) parsed from `pg_restore --list`.
  - `DrillReport` (`result`, `timings`, `counts`, `failure`) and renderers
    `render_table`, `render_json`.
  - `Compare` evaluates source and restored counts into pass/fail.
- Timestamp is injected as a parameter so naming is testable.

## 2. Execution (`src/cli/`)

- `Command::RestoreDrill(RestoreDrillArgs)` with `--backup`, `--format`,
  `--scratch-database`, `--keep`, `--drop-on-failure`, `--dry-run`,
  `--allow-production`, `--output`/`--format` report options and connection
  options.
- Reuse the connection resolution and production guard already used by
  `backup` and `restore`.
- Archive inspection: `pg_restore --list <path>` with the same credential
  environment path; parse and bound the output (at most 20000 lines).
- Create: open a session against the source server but connect to `postgres`
  or the source database, then `CREATE DATABASE <scratch> TEMPLATE template0`.
  The DDL uses a quoted identifier.
- Restore: build `RestoreOptions` for the scratch database and invoke
  `build_pg_restore_command` with `--exit-on-error` support added to the
  transfer layer, capturing bounded stderr.
- Verify: query `pg_class` for user relations in both source and scratch and
  `pg_database_size` in scratch.
- Drop: terminate other backends in the scratch database, then
  `DROP DATABASE <scratch>`. Never runs against the source name, checked again
  immediately before the statement.
- Every branch that fails after creation decides keep/drop from the request and
  records the decision in the report.

## 3. Tests

- `src/drill/tests.rs`: archive format acceptance and refusal, scratch naming
  and truncation, plan ordering, TOC parsing, comparison pass/fail, JSON shape,
  redaction of a synthetic password, failure retention policy.
- `tests/drill_contract.rs`: live fixture. Back up a small schema, run the
  drill, assert pass and scratch removal; run with `--keep` and assert the
  scratch database exists then remove it; corrupt the archive and assert exit 1
  with no source change; `--dry-run` asserts no scratch database was created;
  a source-name scratch is refused with exit 2.
- `tests/documentation_matches_the_build.rs`: documented exit codes, formats
  and flags match the build.

## 4. Documentation

- README feature block and examples.
- `docs/design/restore-drills.md`: steps, safety checks, retention policy,
  credentials and limits.
- `CHANGELOG.md`, `docs/status.md`, roadmap evidence.
