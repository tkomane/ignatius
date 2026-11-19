# Query plan history

Use an estimate to compare plans before and after a schema or index change.
Keep the same query, schema and connection route for a series.

```bash
ignatius plan-history capture @app-dev --series orders-plan --schema public --sql-file orders.sql
ignatius plan-history show --series orders-plan --format json
```

For a latency timeline, repeat the same capture with `--measure`. This uses
two warmup and ten measured runs, plus one `EXPLAIN (ANALYZE, BUFFERS, WAL,
SETTINGS, FORMAT JSON)`. Runtime metrics include p50 and p95 in milliseconds,
server execution time, blocks read, temp blocks written, and WAL bytes.
A p95 increase above 20% is highlighted for adjacent measured captures on
the same PostgreSQL version. Each observation shows changes in the schema
fingerprint, plan structure and planner settings. Cache, load and planner
settings can all affect observed performance.

Capture accepts one read statement, up to 64 KiB, via `-c` or `--sql-file`. A
read-only, repeatable-read transaction protects database state. PostgreSQL
enforces its own privileges and read-only rules. Choose measurements with
care around expensive queries and any installed functions with external
effects. Capture has a 10 second statement timeout and a 60 second overall
deadline after connecting. Incomplete catalog captures are refused.

History is explicit and local. It keeps 32 series of up to 64 observations
each under the Ignatius data directory. Only labels, numeric telemetry and
digests are retained. The schema fingerprint covers the objects supported by
[schema snapshots](../../specs/057-schema-snapshots-and-drift-guard/spec.md).
The first observation and capture-mode or version changes have unavailable
numeric deltas. Reads and deletion work offline.

```bash
ignatius plan-history forget --series orders-plan
```

An active writer can temporarily refuse a concurrent update. If the catalog
is damaged, move `plan-history/timeline.json` aside to start a new history.
