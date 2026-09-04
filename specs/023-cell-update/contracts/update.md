# Cell update contract

## Source analyzer

`parse_source(sql, result_columns, selected_result_column)` returns a value-free
`UpdateSource` only for one `SELECT` with:

- one base relation in `FROM`
- optional schema qualification and one simple alias
- direct source columns, `*`, or a simple `AS` output alias
- no joins, nested source, CTE, set operation, expression or duplicate mapping

The analyzer is lexical and deliberately conservative. A refusal names the
unsupported shape and never returns a partial target.

## Relation discovery

`LoadUpdateTarget` carries only a request identity and a parsed relation
reference. The runtime reads the relation using bound PostgreSQL catalogue
parameters. `UpdateTargetLoaded` carries only resolved relation names, object
kind, privileges and column metadata.

The reducer accepts the answer only when the request, result job, source row,
selected column and retained source SQL still match.

## Planner

`plan_update` requires:

- an eligible table or partitioned table
- update privilege and a non-read-only live session
- one or more primary-key columns
- every primary-key column in the source projection
- non-NULL values for every key in the selected source row
- a non-key selected target column present in the relation metadata

It returns a template shaped like:

```sql
UPDATE "public"."orders"
SET "note" = :__ignatius_new_value
WHERE "order_id" = :__ignatius_key_0
```

The values are held in `ParameterBindings`. The renderer derives the escaped
literal form for review; the bound form is not placed in history or tracing.

## Review and execution

The review modal must state:

- the resolved target relation
- the selected retained source row and target column
- all key predicate column names
- the exact bound statement that would be submitted
- that nothing has been sent yet
- that the source result will not be refreshed automatically

Enter after review emits exactly one `ExecuteParameterized` effect. Esc emits
none. The reducer checks the candidate identity and current connection posture
again immediately before emitting it.

## Refusal messages

Refusals are user-visible words, not silent no-ops. At minimum the build covers:

- no retained visible result cell
- source unavailable or unsupported
- relation not found or not a writable table
- missing update privilege or server read-only posture
- no primary key or key not projected
- NULL key value
- selected key column
- stale result, source or selection
- NUL or oversized replacement
- production-classified connection
