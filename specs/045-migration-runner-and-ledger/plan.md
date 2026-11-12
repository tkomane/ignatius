# Implementation Plan: Migration Runner and Ledger (045)

## 1. Domain (`src/migrate/`)

- `MigrationFile` (`version: u64`, `raw_version: String`, `name`, `path`,
  `sql`, `checksum`).
- `MigrationRecord` (`version`, `name`, `checksum`, `applied_at`,
  `execution_ms`) matching one ledger row.
- `MigrationState` (`Applied`, `Pending`, `Drifted`, `Missing`).
- `MigrationEntry` combines a file or ledger row with its state and optional
  drift detail.
- `MigrationPlan` with `pending`, `drifted`, `missing`, `applied`, `is_clean`,
  `up_to(version)`.
- `discover(dir)` and `plan(files, records)` with bounds and duplicate refusal.
- SHA-256 via `sha2`; lowercase hex.
- `contains_transaction_control(sql)` masks literals/comments with
  `query::highlight::tokens` before matching.
- `render_status_table/json`, `render_plan_table/json/sql`.
- `LEDGER_TABLE`, `ADVISORY_LOCK_KEY` constants.

## 2. Database (`src/postgres/session.rs` or `src/postgres/migrate.rs`)

- `Session::migration_records()`: create the ledger only on apply, read rows on
  status/verify. A missing table is an empty ledger, not an error.
- `Session::apply_migrations(plan, dry_run, allow_production)`:
  1. Refuse drift/missing before writes.
  2. `SELECT pg_advisory_lock(<constant>)`.
  3. For each pending: `BEGIN`; `SET LOCAL lock_timeout`; execute the file;
     parameterized ledger insert with checksum and elapsed milliseconds;
     `COMMIT`. On error `ROLLBACK` and return a diagnostic naming version,
     SQLSTATE and position.
  4. Release the advisory lock on every path.
- Never use `simple_query` for the ledger insert; use parameters.
- Keep the `user_sql_gate` discipline used by other user SQL paths.

## 3. CLI (`src/cli/`)

- `Command::Migrate(MigrateArgs)` with subcommands:
  - `Status { dir, format }`
  - `Plan { dir, to, format }`
  - `Apply { dir, to, dry_run, allow_production, format }`
  - `Verify { dir, format }`
- Shared `--dir` default `migrations`, `--format table|json` (`plan` adds
  `sql`), `--to`, `--dry-run`, `--allow-production`.
- Exit 1 for drift/missing on verify, 7 for a failed migration, 2 for bad
  input, 3 for config. Reuse documented codes.
- Production guard inspects the resolved target environment before connecting;
  `--dry-run` bypasses only the write guard, not drift checks.

## 4. TUI Integration

- `Action::Migrations`, `Model.migrations: Option<MigrationViewState>`.
- `Ctrl+K :` (`migrations`) opens the overlay; `p` plan, `a` apply with a
  confirmation layer, `r` rescan, `Esc` dismiss.
- Loading is an effect; the reducer never reads files or the database.
- Applying requires a second confirmation action and shows the pending count.

## 5. Tests

- `src/migrate/tests.rs`: discovery order, duplicate versions, checksum drift,
  missing applied file, transaction-control refusal, plan/up-to, renderers,
  bounds.
- `src/postgres/session/tests.rs` or `tests/migration_contract.rs`: live apply,
  ledger rows, re-apply no-op, drift refusal, failed migration rollback,
  advisory-lock serialization where practical.
- `tests/migration_contract.rs`: CLI status/plan/verify/apply, dry-run writes
  nothing, production guard, exit codes, JSON shapes.
- Reducer and overlay tests for confirm/no-write paths.

## 6. Documentation

- README CLI block and feature section.
- Keymap table and bindable action list.
- CHANGELOG, status, roadmap.
- A migration authoring note: forward-only, no transaction control, checksums
  bind the exact bytes.

## 7. Verification

- focused migrate, update, widget and contract tests
- `cargo test --locked --test postgres_integration`
- `cargo xtask verify` with plain and TLS PostgreSQL
