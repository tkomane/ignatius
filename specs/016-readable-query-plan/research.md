# Research: A plan you can read

## Decision: Use PostgreSQL's structured JSON EXPLAIN response

PostgreSQL documents the plan as a tree of plan nodes and recommends JSON,
XML, or YAML when another program needs to analyse the result. The JSON response
is an array whose first object contains a `Plan` root and optional summary
fields such as `Planning Time` and `Execution Time`. Node fields include
`Node Type`, `Plan Rows`, `Startup Cost`, `Total Cost`, `Actual Rows`,
`Actual Total Time`, and `Actual Loops`.

Rationale: JSON preserves the parent-child structure and lets the workbench
render a readable tree without parsing PostgreSQL's human-oriented text format.
Known fields are displayed, unknown fields are ignored or retained as safe
facts, and missing fields remain unavailable rather than being turned into
zeroes.

Source: [PostgreSQL 18 EXPLAIN documentation](https://www.postgresql.org/docs/18/sql-explain.html)
and [Using EXPLAIN](https://www.postgresql.org/docs/current/using-explain.html),
checked 2026-09-04.

## Decision: Compare actual rows per loop and keep cost units separate

`EXPLAIN ANALYZE` supplies actual row counts, timings, and loop counts. The
PostgreSQL documentation explains that actual rows and time for a node are
averages per execution when a node runs more than once, and that multiplying by
loops gives total work. The UI therefore compares the per-loop `Actual Rows`
value with `Plan Rows`, displays loops separately, and uses `Actual Total Time`
times loops only for the derived slow-node marker. Cost remains labelled as
planner cost units, never milliseconds.

Rationale: comparing unlike units or silently multiplying one side would make a
plan look precise while teaching the wrong performance model.

Source: [PostgreSQL 18 Using EXPLAIN, EXPLAIN ANALYZE](https://www.postgresql.org/docs/current/using-explain.html),
checked 2026-09-04.

## Decision: Always confirm EXPLAIN ANALYZE

PostgreSQL states that `EXPLAIN ANALYZE` actually executes the supplied
statement and that side effects occur normally, including for data-changing
statements. The client will therefore ask for an explicit one-shot confirmation
every time, even for a statement classified as read-looking. It will not invent
a transaction wrapper or automatic rollback. Existing production and
destructive-impact confirmation rules remain in force, and the UI says the
classification is advisory.

Rationale: a read-looking `SELECT` can invoke a side-effecting function, and a
hidden automatic rollback would change transaction semantics. The person must
choose execution knowingly.

Source: [PostgreSQL 18 EXPLAIN important notes](https://www.postgresql.org/docs/18/sql-explain.html#SQL-EXPLAIN-ANALYZE),
checked 2026-09-04.

## Decision: Bound the plan document before rendering

The parser accepts at most 1 MiB of JSON, 500 nodes, and 64 levels of depth.
When a bound is reached it retains the valid prefix, sets an explicit truncated
flag, and keeps selection and dismissal available. Malformed or incomplete JSON
is a failed plan, not a partial successful plan.

Rationale: EXPLAIN output can grow with nested subplans and verbose expressions;
the terminal client must not allow server-controlled text to consume unbounded
memory or make the UI unresponsive. A valid bounded tree is useful, while a
malformed response cannot support truthful metrics.

## Decision: Keep plan state separate from ordinary result state

Plan requests use a distinct reducer effect and completion message. The plan is
stored in a local view state over the Results pane, while the previous ordinary
result, filter, selected row, transaction state, and history remain untouched.
Plan requests do not enter history, plain output, JSON output, NDJSON output, or
streaming export. A stale plan completion is rejected by the same job identity
rule as a stale query result.

Rationale: treating the generated EXPLAIN command as ordinary user SQL would
pollute history and show a JSON document as a result grid. A separate effect
keeps the machine and user contracts honest.
