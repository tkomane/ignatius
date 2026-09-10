# Research: Reviewable cell-to-UPDATE

## Existing product boundaries

The result grid already keeps PostgreSQL row order unchanged while the model
maps the displayed selection back to a retained source-row index. That is the
identity needed for a cell action: the update must read the source row, not the
row position after local sorting or filtering.

The completed execution currently does not retain its SQL after it is offered to
history. Feature 023 therefore adds a value-preserving `last_sql` snapshot to
the model, populated only when an execution completes and cleared on connection
switch. It is compared during the update workflow so an editor edit cannot
retarget an old result.

The catalogue adapter already reads relation columns and marks primary-key
membership. A small PostgreSQL-specific resolver will add the live relation
kind and `has_table_privilege(..., 'UPDATE')` result, and will resolve an
unqualified name using the session's `search_path`. No identifier will be
interpolated into that catalogue query.

## Design decisions

### Conservative source analysis

There is no full SQL parser in the project. A cell update must not pretend that
one exists, so the pure source analyzer accepts only a narrow shape that can be
explained in words:

```sql
SELECT order_id, note AS customer_note
FROM public.orders
WHERE customer_id = 42
ORDER BY order_id;
```

It rejects joins, subqueries, CTEs, set operations, aggregates, expressions,
duplicate projections and non-table relation kinds. The `WHERE`, `ORDER BY`,
`LIMIT` and similar clauses do not determine identity because the generated
predicate uses every primary-key column from the selected retained row.

### Primary key only

A unique index could identify a row, but metadata for partial, expression and
deferrable uniqueness has more semantics than this first slice can safely
explain. The implementation uses the primary key only and refuses tables that
do not expose one. The key values are taken from the same retained row and are
never inferred from a neighboring row or a filter expression.

### Parameterized generated statement

The review surface needs to show the bound statement, but the value must not
enter history or logging. The pending update therefore holds a generated SQL
template with private parameter names and `ParameterBindings` containing the
old key values and new value. The renderer derives a display-only bound form
from that state. Confirmation emits the existing `ExecuteParameterized` effect;
binding remains at the PostgreSQL simple-query boundary.

### Production and read-only policy

The action refuses immediately on a production-classified connection or a
session whose server posture is read-only. It does not ask for a replacement
value in either case. This is stricter than ordinary manually-authored local
SQL because the client invented this write from a result and cannot make the
operator review a hand-authored alternative on the same path.

### Result freshness

After a confirmed update the ordinary execution lifecycle replaces the result
with the update's affected-row outcome. It does not rerun the original query.
The UI states that a prior result is a snapshot and that the original query
must be run again deliberately to read the changed row.
