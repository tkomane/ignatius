# Vision

## Why Ignatius exists

PostgreSQL practitioners usually get stuck choosing between two imperfect tools:

- `psql` is fast, scriptable, and faithful to PostgreSQL, yet line-oriented:
  wide results wrap into unreadable walls of text, inspecting query plans or
  table relationships takes repetitive meta-commands, and multi-buffer query
  iteration has no visual workspace.
- Heavy Electron and Java desktop GUIs give you grids, diagrams, and plan
  trees, yet take seconds to start, consume hundreds of megabytes of memory,
  require a window server, and hide PostgreSQL semantics behind lowest-common-denominator
  database abstractions.

Ignatius eliminates that trade-off: a fast, full-powered PostgreSQL workbench
for your terminal, shipped as one local Rust binary that serves both
interactive studio work and scripted CI automation.

## What Ignatius delivers

Ignatius brings complete PostgreSQL studio workflows directly to the terminal
without sacrificing speed, clarity, or keyboard flow:

- **SQL Studio and Results Workbench.** Multi-tab SQL buffers with schema-aware
  autocomplete, local formatting, bracket matching, prompted named parameters,
  pinned columns, server/local sort, structured JSONB/array/composite
  inspection, and reviewed single-cell updates.
- **Query Plan Forensics (`EXPLAIN` / `EXPLAIN ANALYZE`).** Interactive plan
  trees with exclusive self-time and self-cost bottleneck scoring, proportional
  micro-bars, estimate-vs-actual mismatch markers, and persisted plan comparison
  history.
- **A/B Query Benchmarks.** Repeatable warmup + measured benchmark runs comparing
  two statements side by side with percentile latency deltas, safe rollback
  isolation, and live cancellation.
- **Schema Diagrams and Diffs.** Terminal ER diagrams with primary/foreign-key
  relationships, deep catalog browsing, DDL generation, schema snapshot diffs,
  and migration checks.
- **Live Server Activity and Maintenance.** Real-time session, lock-tree, and
  blocking diagnostics alongside guided vacuum, analyze, reindex, and partition
  operations.
- **Streaming COPY and Scripted CI CLI.** Fast streaming CSV/TSV/JSON/NDJSON/SQL
  import and export, plus a non-interactive CLI sharing the same engine, SQL
  linter, and documented exit codes.

## Practical built-in guardrails

When a workbench handles real databases, safety features belong in the engine
as sharp, low-friction engineering guardrails:

- **Server-verified transport.** TLS status is read back from `pg_stat_ssl`,
  distinguishing `require` from `verify-full`, and options the client cannot
  honour fail loudly instead of silently downgrading.
- **Explicit posture markers.** `[PROD]`, `[STAGING]`, `[READ ONLY]`,
  `[TX ACTIVE]`, and `[TX FAILED]` are plain-text state indicators backed by
  typed-name confirmations on production writes and `--allow-write` gates in CLI
  runs.
- **Truthful async state.** Cancellation reports `Cancellation requested` until
  the server confirms stop; dropped connections state `Connection lost - query
  outcome unknown` rather than guessing.
- **Accessible across terminals.** Every state and action survives `NO_COLOR`,
  ASCII fallback, narrow terminals, and `ui.reduced-motion = true`, with full
  terminal restoration on exit or signal.

## Non-goals

Ignatius stays focused on being the best terminal workbench for PostgreSQL. It
will not become a multi-database client, a cloud-synced SaaS account, a BI
dashboard builder, or an AI wrapper that uploads your schema or queries to third
parties.

## How success is measured

An engineer, DBA, or backend developer opens Ignatius over SSH or in a local
terminal and has the full power of a PostgreSQL IDE at native Rust speed -
with nothing held back and nothing hidden.
