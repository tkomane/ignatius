# Feature Specification: Migration Runner and Ledger (045)

## 1. Context and Problem Statement

Feature 030 generates reviewable migration SQL, and Feature 043 protects
unsaved editor work, but nothing applies an ordered migration directory with an
audit trail. Teams currently glue this together with shell scripts that do not
detect an edited migration, a skipped version or a partially applied run.

This feature adds a deterministic migration runner backed by a ledger table. It
never edits a migration, never applies a drifted file, and never leaves a half
migration committed.

## 2. User Scenarios

### Scenario 1: Plan before applying

An operator runs `ignatius migrate status --dir db/migrations` and sees three
applied versions, two pending and no drift. `ignatius migrate apply --dir
db/migrations --dry-run` prints the exact order and exits without writing.

### Scenario 2: An edited migration is refused

A migration has already been applied. Someone edits the file. `ignatius migrate
apply` refuses before running anything, names the version and both checksums,
and exits non-zero.

### Scenario 3: A migration fails midway

A migration creates two tables and then fails. The transaction rolls back, the
ledger records nothing, and the error names the migration version, SQLSTATE and
statement position. A later fix can be applied cleanly.

### Scenario 4: Production requires an explicit decision

The target profile is production. `ignatius migrate apply` refuses before
connecting writes and tells the operator to use `--allow-production` after
reviewing the plan. `--dry-run` remains available without the flag.

## 3. Functional Requirements

- FR-4501: Discover `*.sql` files in a directory. A file name starts with a
  numeric version, an underscore, then a name (`001_create_users.sql`). Sort by
  numeric version; refuse duplicate versions and files without a prefix.
- FR-4502: Each migration has a SHA-256 checksum over its raw bytes. A changed
  applied migration is `Drifted`; an applied version with no file is `Missing`.
- FR-4503: The ledger is `ignatius_schema_migrations(version text primary key,
  name text not null, checksum text not null, applied_at timestamptz not null,
  execution_ms bigint not null)`, created on first apply.
- FR-4504: `ignatius migrate status` reports each file as applied, pending,
  drifted or missing. `--format table|json`; no writes.
- FR-4505: `ignatius migrate plan` prints the pending order up to `--to`.
  `--format table|json|sql`; no writes.
- FR-4506: `ignatius migrate apply` applies pending migrations in order. Each
  migration runs inside one transaction with the ledger insert; a failure rolls
  back that migration and stops. A PostgreSQL advisory lock serializes runners.
- FR-4507: Drift or missing applied files refuse the whole apply before the
  first write. `--dry-run` is always non-destructive.
- FR-4508: Production or read-only targets require `--allow-production` for
  apply. Read-only sessions still fail at the server if the flag is given.
- FR-4509: A migration that contains top-level `BEGIN`, `COMMIT`, `ROLLBACK`,
  `SAVEPOINT` or `RELEASE` is refused: the runner owns the transaction.
- FR-4510: `ignatius migrate verify` checks checksums only and exits 1 on
  drift or missing files.
- FR-4511: There is no interactive apply surface. The CLI is the reviewed
  route: it can be scripted, its flags are visible in the pipeline, and the
  full-screen client already has a separate reviewed schema-diff overlay for
  ad-hoc DDL. A second write path would be harder to audit, not easier.
- FR-4512: The ledger insert and migration run use the same session and the
  same transaction. Nothing is applied twice.

## 4. Non-Functional Requirements

- Pure `src/migrate/` discovery, parsing, checksum and rendering.
- Bounded: 1000 files, 4 MiB per file, one runner at a time.
- No credentials or SQL text in diagnostics beyond the bounded statement
  excerpt already used by query errors.
- Deterministic output ordering and stable JSON field names.

## 5. Out of Scope

- Automatic rollback/down migrations. A forward-only ledger is the contract.
- A background scheduler or daemon.
- Rewriting migration SQL or inferring a baseline from an existing schema.
