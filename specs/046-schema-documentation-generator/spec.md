# Feature Specification: Schema Documentation Generator (046)

## 1. Context and Problem Statement

Architecture reviews, onboarding and change approval all need a current data
dictionary. Today an operator must query `pg_catalog` by hand or maintain a
document that drifts from the database. Ignatius already reads catalog
metadata for DDL and diagrams; this feature turns the same source into a
reviewable Markdown or JSON document.

The generator is read-only. It never creates a file unless the operator names
one, and it never uploads a schema anywhere.

## 2. User Scenarios

### Scenario 1: A data dictionary for a review

An architect runs `ignatius docs "$DATABASE_URL" --schema public --format
markdown --output docs/data-dictionary.md --force`. The file lists every table,
column, type, nullability, default, primary key, foreign key, index, constraint,
RLS posture and comment, plus a Mermaid ER diagram.

### Scenario 2: Machine-readable schema inventory

A pipeline runs the same command with `--format json` and compares the output
with a retained baseline. The JSON has stable field names and ordering.

### Scenario 3: Empty or restricted schema

A schema with no visible tables exits successfully and writes a document that
says it is empty. A permission error is reported with the catalog relation that
could not be read; no partial file replaces an existing one.

## 3. Functional Requirements

- FR-4601: Read one schema (default `public`) from `pg_catalog`, including
  tables, views, materialized views, columns, types, nullability, defaults,
  primary keys, foreign keys, unique constraints, check constraints, indexes,
  comments, row estimates, RLS enabled/forced and policies.
- FR-4602: Bound the read to 500 relations and 5000 columns; when a bound is
  reached, the document reports truncation instead of silently omitting rows.
- FR-4603: `--format markdown|json|mermaid` (`mermaid` emits only the ER
  diagram). Markdown includes a table of contents, one section per relation,
  a column table, indexes, constraints and RLS rows.
- FR-4604: `--output PATH` writes atomically; an existing file is refused
  unless `--force` is given. Standard output is used when no path is given.
- FR-4605: Output is deterministic for unchanged catalog state and ordering is
  schema, relation kind, relation name, column ordinal.
- FR-4606: Comments and identifiers are escaped for Markdown and Mermaid;
  control characters cannot alter the document.
- FR-4607: The command is read-only and works on production and read-only
  sessions without a confirmation flag.
- FR-4608: JSON includes a schema version and generation timestamp; Markdown
  states the database, schema and generation time.
- FR-4609: No TUI surface is required; the artifact belongs in a repository or
  review packet. The command is documented in the README and CLI help.

## 4. Non-Functional Requirements

- Pure `src/dbdoc/` renderers and model, separate from catalog IO.
- No result rows, credentials or connection strings in the document.
- ASCII-safe fallback for Mermaid identifiers; no terminal decoration in
  machine output.
- Bounded memory: the document is built from the bounded catalog read, not
  from unbounded query results.

## 5. Out of Scope

- HTML sites, PDF rendering or hosting.
- Cross-database comparison (Feature 030 owns schema diff).
- Writing comments back to the database.
