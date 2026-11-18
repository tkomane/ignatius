# Growth and Retention Audit

`ignatius growth` answers storage questions from catalog and statistics views
only. It never reads table rows, never samples data, never runs `ANALYZE`, and
never writes to the database.

```bash
ignatius growth --top 20
ignatius growth --schema orders --format json --fail-on critical
```

## Sections

### Statistics window

One timestamp from `pg_stat_database.stats_reset` bounds every growth number.
The report prints the reset time and its age. A window shorter than one hour
does not produce a rows-per-day rate.

### Largest relations

The top `--top` relations by total allocated size, bounded to 200. Total size is
`pg_total_relation_size`, split into the heap main fork, indexes, and the TOAST
table plus its index. Total size also includes free-space map and visibility map
forks, so the component columns need not add up to the total. Row estimates
come from `pg_class.reltuples`; an unanalyzed relation shows `?` rather than a
guess. Dead tuples and the dead ratio come from `pg_stat_user_tables`. The last
maintenance column is the later of the vacuum and analyze timestamps.

A partitioned parent appears as partitioned and usually reports its own size,
not the sum of its children. Children appear as ordinary relations with their
own sizes.

### Sequence headroom

User sequences from `pg_sequences`, bounded to 200 and ordered by remaining
fraction. Remaining values are computed from the last value, the maximum or
minimum, and the increment. A sequence whose `last_value` is not readable is
shown as `UNAVAILABLE`. A cyclic sequence is shown as `CYCLIC` and is never
treated as exhausted.

Thresholds:

| State | Condition |
| --- | --- |
| `CRITICAL` | at most 5 percent remaining, or at most 1000 values |
| `WARNING` | at most 20 percent remaining |
| `OK` | more than 20 percent remaining |

### Findings

| Code | Severity | Condition |
| --- | --- | --- |
| `sequence-exhaustion` | `CRITICAL` | a non-cyclic sequence is at or below the critical threshold |
| `vacuum-debt` | `WARNING` | at least 10000 dead tuples and at least 20 percent dead |
| `partitioning-candidate` | `WARNING` | an unpartitioned relation is at or above `--large-table-bytes` |

Findings sort by severity, then code, then object. The default
`--large-table-bytes` is 1073741824 (1 GiB).

## Exit codes

| Code | Meaning |
| --- | --- |
| 0 | the audit ran; this does not claim the database is healthy |
| 1 | `--fail-on` was met by a warning or critical finding |
| 2 | a usage, connection or catalog error |

`--fail-on never` is the default. `--fail-on warning` fails on any finding;
`--fail-on critical` fails only on sequence exhaustion.

## Bounds and limits

- At most 200 relations and 200 sequences are retained. The report states when
  either set was truncated.
- Ordering and rendering are deterministic for the same snapshot.
- Sizes are allocated bytes, not logical data size. A table that was recently
  vacuumed can still occupy its high-water mark.
- Dead tuple counts depend on autovacuum activity and are estimates.
- The audit is a snapshot. It is not a time series, and it stores no history
  between runs.
- The audit is CLI-only by design; it has no terminal key binding.

## PostgreSQL support

The queries use `pg_class`, `pg_namespace`, `pg_stat_user_tables`,
`pg_stat_database` and `pg_sequences`, all available in PostgreSQL 14 through
18. No extension is required.
