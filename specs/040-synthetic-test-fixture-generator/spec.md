# Feature Specification: Synthetic Test Fixture Generator (040)

## Overview
Developers and QA engineers working with PostgreSQL often lack realistic, referentially sound test data. Traditional mock data libraries generate isolated values without understanding database schema rules, resulting in CHECK constraint failures, unique key collisions, or foreign key violations.

Feature 040 delivers a catalog-grounded synthetic test data and fixture generator for Ignatius:
1. **Catalog-Grounded Column Strategy Inference**: Inspects PostgreSQL table metadata (data types, nullability, primary keys, foreign key references, enums, check constraints) and maps each column to an optimal generator strategy.
2. **Deterministic PRNG Seeding**: Generates reproducible fixtures given a 64-bit seed (`--seed`), enabling deterministic test fixtures in CI/CD pipelines.
3. **Foreign Key DAG Preservation**: Leverages topological ordering from `src/sync/` to populate parent tables before child tables, sampling foreign key references from generated parent IDs to guarantee zero foreign key violations.
4. **Field Industrial TUI Cockpit (`Ctrl+K +` / `synthetic-seed`)**: Dieter Rams hardware status capsules (`[TABLES]`, `[ROWS/TABLE]`, `[SEED]`, `[INTEGRITY]`, `[FORMAT]`), tabbed navigation across `[1] Table Generator Strategies` and `[2] Generated Fixture Preview`, and hotkeys `[c]` (copy SQL via OSC 52), `[s]` (cycle seed), and `[r]` (refresh).
5. **Headless Scriptable CLI Subcommand `ignatius seed`**: Non-interactive fixture generation supporting `--schema`, `--table`, `-n, --rows`, `--seed`, `-f, --format sql|jsonl|csv|table`, `-o, --output`, `--force`, and `--insert` (direct transactional database insertion).

Fixture discovery covers permanent and unlogged user tables. Temporary
tables and their column and dependency metadata are excluded from plans.

With `--insert`, a production-classified target requires
`--confirm-production` before authentication or connection. Refusal
returns a usage error and emits no fixture report. Fixture generation
without `--insert` can read the catalogue and render output without
production confirmation.

A nonempty direct insert requires an idle session and owns one transaction
for all generated INSERT batches. Preserve active and failed caller
transactions. Roll back earlier batches after an INSERT failure. Report
an unconfirmed rollback or uncertain commit outcome and preserve the
validated SQLSTATE when available. Successful execution reports the
server's affected-row count after commit. Exported SQL remains a
standalone transactional script, and fixture reports describe generation.
