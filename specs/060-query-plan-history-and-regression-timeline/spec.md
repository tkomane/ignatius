# Feature 060 - Query plan history and regression timeline

**Created** 2026-11-19
**Milestone** M15
**Status** Implemented locally

## User journeys

1. A developer captures an estimate before and after creating an index. The
   timeline identifies changes in the captured schema, plan shape and planner
   settings, and shows the estimated cost change.
2. An operator explicitly measures a read query twice against the same route.
   The timeline shows warmup-separated p50 and p95, server execution time,
   buffer reads and WAL bytes. A p95 increase greater than 20% is highlighted.
   Measurements are observations and cannot establish the cause of a change.
3. An operator reads the timeline offline, or clears a named series using
   `forget`. An interrupted capture leaves the previous timeline usable.

## Requirements

- FR-6001. `plan-history capture` requires a series name, a schema name and a
  single SELECT, WITH, TABLE or VALUES statement. PostgreSQL validates the
  statement, and the client refuses statements classified as writes. Default
  capture plans the query. `--measure` explicitly runs it using the existing
  benchmark warmup and sampling defaults and extended EXPLAIN telemetry.
- FR-6002. Capture uses one bounded PostgreSQL read-only, repeatable-read
  transaction. Catalog reads, planning and measurements share its snapshot.
  The bound is 10 seconds per statement and 60 seconds for capture. A failed
  capture, rollback, or incomplete schema snapshot saves no observation.
- FR-6003. Keep at most 32 named local series and 64 observations per series
  under the resolved Ignatius data directory. Show the count of evicted
  observations. Bind each series to the configured route, connected role,
  search path, observed TLS posture, schema scope and exact parsed statement
  using a digest. A
  mismatch asks for another name. Keep schema, plan and settings digests and
  numeric measurements. Store no SQL text, connection string, result rows,
  plan literals, object names or setting values.
- FR-6004. `plan-history show` and `forget` run offline. Order observations
  by the recorded UTC capture time. Reports offer plain
  ASCII and JSON. Each consecutive pair shows schema, plan and settings
  changes. Compare costs and p95 only across the same PostgreSQL version
  and capture mode. The first observation and undefined deltas are explicit.
- FR-6005. Serialize local writes and replace the versioned catalog atomically.
  Reject oversized, malformed or unsupported catalogs before changing them.
  Explicit capture and `--no-history` conflict. A failed capture does not retry.

## Boundaries

No background recording, TUI binding, hosted service or dependency change.
PostgreSQL read-only transactions enforce database write refusal. Operators
choose what SQL to measure and must account for external effects of installed
functions, database load and warm-cache effects. The schema digest covers only
the catalog objects supported by Feature 057 in the selected schema.
