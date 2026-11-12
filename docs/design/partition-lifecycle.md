# Partition Lifecycle

`ignatius partition` reports range, list and hash partition sets and generates
reviewed DDL for range partitioning. It never executes DDL. Applying a plan is
a schema change and belongs in the same reviewed path as a migration.

```bash
ignatius partition status --schema billing
ignatius partition status --schema billing --format json
ignatius partition plan --table billing.invoices --period monthly \
  --start 2026-12-01 --count 3 --expire-before 2025-01-01
```

## Status

`status` reads `pg_class`, `pg_namespace`, `pg_partitioned_table`,
`pg_partition_tree`, `pg_get_partkeydef` and `pg_get_expr(relpartbound)`.
It reports the parent, strategy, key definition, each child bound, child size
and planner row estimate. A default partition is listed with bound `DEFAULT`
and marked in the parent header.

The read is bounded to 500 partitioned parents and 5000 child partitions. The
report states when either bound truncated the snapshot. Partitions detached
with `DETACH PARTITION` are ordinary tables and no longer appear here.

## Plan

`plan` accepts only a single plain-column `RANGE` key whose type is `date`,
`timestamp` or `timestamptz`. It refuses, with a diagnostic:

- list, hash or unknown strategies;
- multi-column, expression or missing keys;
- key types other than date and timestamp;
- an existing child bound it cannot parse;
- a generated interval that overlaps an existing child;
- a start that is not aligned to the period;
- a count outside 1 through 120.

Start alignment is deliberate: daily accepts any date, weekly requires Monday,
monthly requires the first day of the month, and yearly requires 1 January.
This prevents a plan from generating a misleading partition name for a partial
interval.

Generated names are `<parent>_p<YYYY_MM_DD>` daily, `<parent>_p<YYYY_Www>`
weekly, `<parent>_p<YYYY_MM>` monthly and `<parent>_p<YYYY>` yearly. Names
longer than PostgreSQL's 63-byte identifier limit are truncated and given a
stable hexadecimal suffix derived from the full name. All identifiers are
quoted in the generated SQL.

`timestamptz` keys use explicit `+00` literals. The server interprets those in
UTC; a database whose partition boundaries are local-time based needs its own
review before applying the plan.

### Default partitions

A default partition does not overlap an explicit range in the catalog, so the
plan does not silently move rows. When a default exists, the SQL header carries
a comment: if the default holds rows for a generated range, PostgreSQL refuses
the creation and those rows must be moved first.

### Expiry

`--expire-before DATE` appends reviewed statements for existing partitions
whose upper bound is on or before the cutoff:

```sql
ALTER TABLE "billing"."invoices" DETACH PARTITION "billing"."invoices_p2024_12";
DROP TABLE "billing"."invoices_p2024_12";
```

A partition ending in `MAXVALUE` is never expired. The default partition is
never expired. The plan prints both statements; it does not run them.

## Output and exit codes

| Code | Meaning |
| --- | --- |
| 0 | status or plan completed; a plan is not an apply |
| 2 | usage, refused plan, connection or catalog error |

`plan` writes SQL to standard output and a summary to standard error, so
`ignatius partition plan ... > partition.sql` leaves a clean file.

## Bounds and limits

- Status reads at most 500 parents and 5000 children.
- Plan reads the existing set once. A concurrent partition creation can still
  make a generated statement fail; the plan is a review-time snapshot, not a
  lock.
- No `attach` or `apply` command is provided. Use the reviewed SQL through
  `ignatius migrate` so one ledger records the change.
- No table rows are read and no statistics are changed.
