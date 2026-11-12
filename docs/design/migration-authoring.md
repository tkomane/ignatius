# Migration authoring

`ignatius migrate` is forward-only. It applies files once, records what it ran,
and refuses to guess when the files and the ledger disagree.

## File naming

A migration is named `<version>_<name>.sql`, for example
`001_create_users.sql`. The version must be numeric; files are ordered by its
numeric value, so `2_` follows `1_` and `010_` follows `9_`. Duplicate versions
are refused before anything runs.

## The runner owns the transaction

Every migration and its ledger row commit together. A file must not contain
`BEGIN`, `COMMIT`, `ROLLBACK`, `SAVEPOINT` or `RELEASE`; the runner refuses such
a file before writing. String literals, quoted identifiers, comments and
dollar-quoted function bodies are ignored by that check, so a PL/pgSQL body may
still contain `BEGIN ... END`.

If any statement in a migration fails, the whole migration rolls back and the
runner stops. Earlier migrations in the same run remain applied and ledgered;
that is the forward-only contract. Fix the failing file with a new version.

## Checksums and drift

The ledger records the SHA-256 of the exact file bytes. Editing an applied file
makes it `drifted`; deleting it makes it `missing`. Both refuse the whole run
before the first write, including later pending migrations. Restore the exact
applied bytes, or add a new migration that makes the intended change.

Line endings are part of the bytes. Keep the checkout stable across platforms
(for example with `.gitattributes`) if a migration will be applied from more
than one machine.

## Commands

```bash
ignatius migrate status "$DATABASE_URL" --dir migrations
ignatius migrate plan "$DATABASE_URL" --dir migrations --format sql
ignatius migrate apply "$DATABASE_URL" --dir migrations --dry-run
ignatius migrate apply "$DATABASE_URL" --dir migrations --to 012
ignatius migrate apply "$DATABASE_URL" --dir migrations --allow-production
ignatius migrate verify "$DATABASE_URL" --dir migrations
```

`--dry-run` prints the pending SQL and writes nothing. `verify` exits 1 on
drift or a missing file. Production targets require `--allow-production`; a
read-only session still fails at the server.

Concurrent runners serialize on a PostgreSQL advisory lock. The ledger is read
again after the lock is acquired, so two pipelines cannot apply the same
version.

## Ledger

The table is `ignatius_schema_migrations`:

| Column | Meaning |
| --- | --- |
| `version` | The version text from the file name, primary key |
| `name` | The name after the version prefix |
| `checksum` | Lowercase SHA-256 of the applied bytes |
| `applied_at` | Server timestamp |
| `execution_ms` | Server execution duration recorded by the runner |

The ledger is ordinary data. Back it up with the database; do not edit it by
hand. A ledger row with a non-numeric version is refused as a corrupt ledger.

## Not provided

- Down migrations or automatic rollback.
- A background scheduler.
- Inferring a baseline from an existing schema. Adopting an existing database
  requires an explicit, reviewed decision outside this tool.
