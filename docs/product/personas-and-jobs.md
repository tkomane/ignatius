# Personas and jobs

## Primary: the backend developer and database engineer

Builds services on PostgreSQL every day, splitting time between local Docker
instances, staging clusters, and production diagnostics over SSH. Wants a fast,
capable workbench that launches instantaneously inside the terminal alongside
the rest of their tooling instead of context-switching to a heavy Electron or
Java GUI.

Their jobs:

- Iterate rapidly across multi-tab SQL buffers with schema-aware completions,
  formatting, and named parameters.
- Inspect wide result sets, JSONB documents, arrays, and composites without
  wrapping into unreadable `psql` text.
- Profile slow queries with interactive `EXPLAIN` trees, bottleneck micro-bars,
  and A/B query benchmarks to verify fixes before merging.
- Keep `.sql` queries and schema baselines versioned in Git and automate checks
  with the same CLI binary in CI.

What makes them switch to Ignatius: instant startup, rich studio features in
the terminal, full keyboard speed, and zero memory bloat.

## Secondary: the DBA and infrastructure engineer

Operates staging and production PostgreSQL clusters across cloud and
on-premises environments. Needs deep PostgreSQL visibility - live sessions,
lock trees, schema diffs, ER relationships, and streaming `COPY` - combined
with deterministic connection hygiene and clear environment boundaries.

Their jobs:

- Inspect `pg_stat_activity` and blocking chains, and cancel a runaway query
  with confirmed server feedback.
- Verify TLS mode (`verify-full` vs `require`), read-only posture, and `[PROD]`
  targeting at a glance before issuing any statement.
- Compare schema snapshots across environments, generate DDL, and run guided
  maintenance (`VACUUM`, `ANALYZE`, `REINDEX`) with explicit preview and write
  confirmations.
- Export results or run scripted operational checks from cron or CI with
  documented exit codes.

What makes them trust Ignatius: server-reported TLS and transaction state,
fail-closed configuration, clear plain-text safety markers, and predictable CLI
exit codes.

## Tertiary: the terminal-native data analyst

Fluent in SQL and command-line pipelines, working directly over terminal
sessions without needing a desktop BI suite.

Their jobs:

- Explore unfamiliar schemas with catalog search and interactive ER diagrams.
- Slice retained results locally (column visibility, pinning, sorting, value
  profiling) without re-running expensive queries.
- Stream large datasets cleanly to CSV, TSV, JSON, NDJSON, or Markdown with
  explicit `NULL` vs empty-string clarity and completion guarantees.

## Who this is not for

- Someone who needs one generic client for PostgreSQL, MySQL, and MongoDB.
  Ignatius goes deep on PostgreSQL semantics rather than lowest-common-denominator
  SQL.
- Someone who prefers a mouse-driven desktop window manager or spreadsheet BI
  tool over a keyboard-first terminal workspace.
- A team looking for cloud-hosted shared query folders and user accounts.
  Ignatius keeps queries, configs, and history local.
