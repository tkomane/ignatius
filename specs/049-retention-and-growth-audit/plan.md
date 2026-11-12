# Implementation Plan: Retention and Growth Audit (049)

## 1. Domain (`src/growth/`)

- `mod.rs`:
  - `GrowthSeverity` (`Info`, `Warning`, `Critical`) with `rank()`.
  - `GrowthFindingCode` (`SequenceExhaustion`, `VacuumDebt`,
    `PartitioningCandidate`) with `as_str()` and severity.
  - `RelationGrowth` (`schema`, `table`, `total_bytes`, `heap_bytes`,
    `index_bytes`, `toast_bytes`, `estimated_rows`, `dead_tuples`,
    `dead_ratio`, `last_vacuum`, `last_analyze`, `partitioned`,
    `inserts`, `updates`, `deletes`, `rows_per_day`, `stats_age_hours`).
  - `SequenceHeadroom` (`schema`, `name`, `last_value`, `max_value`,
    `remaining`, `used_fraction`, `is_cyclic`, `available`).
  - `GrowthFinding` (`code`, `severity`, `object`, `message`).
  - `GrowthSnapshot` (`relations`, `sequences`, `findings`, `stats_reset`,
    `truncated_relations`, `truncated_sequences`).
  - `GrowthOptions` (`schema`, `top`, `large_table_bytes`, `fail_on`).
  - `GrowthSnapshot::evaluate(options)` derives findings from the bounded
    model so severity policy is testable without a database.
  - `render_table`, `render_json`.
- Bounds: `MAX_RELATIONS = 200`, `MAX_SEQUENCES = 200`, `DEFAULT_TOP = 20`.
- Percentages computed with integer or f64 formatting rounded to one decimal;
  no NaN or infinity reaches the renderer.
- Ordering applied after evaluation so findings and entries are deterministic.

## 2. Catalog (`src/postgres/growth.rs`)

- `growth_snapshot(client, options)` issues two bounded queries:
  - Relations: `pg_class` joined to `pg_namespace`, `pg_stat_user_tables`
    and `pg_stat_database` (single row), filtered to `relkind IN ('r','p')`
    and non-system schemas, ordered by total bytes descending, `LIMIT $`.
  - Sequences: `pg_sequences` joined to `pg_namespace`, ordered by remaining
    fraction ascending with `NULLS LAST`, `LIMIT $`.
- Sizes use `pg_total_relation_size`, `pg_relation_size`,
  `pg_indexes_size`, and `pg_total_relation_size - pg_relation_size -
  pg_indexes_size` for TOAST.
- Partitioned parents report their own size (usually zero) and are marked
  partitioned; their children appear as ordinary relations.
- `pg_stat_database.stats_reset` supplies one window for all relations.
- `Session::growth_snapshot(options)` delegates to the extractor.

## 3. CLI (`src/cli/`)

- `Command::Growth(GrowthArgs)`:
  - `--schema`, `--top`, `--large-table-bytes`, `--format table|json`,
    `--fail-on never|warning|critical`, connection options.
- Validate `top` in `1..=200`; convert bytes with a checked parse.
- Exit 1 through `ExitCode::Findings` when the threshold is met; 0 otherwise.
- A clean run prints the report; findings print a summary line to stderr.

## 4. Tests

- `src/growth/tests.rs`: severity derivation, vacuum debt boundary, sequence
  thresholds including cyclic and unavailable last value, partitioning
  candidate threshold, ordering, empty snapshot, JSON escaping, ratio bounds.
- `tests/growth_contract.rs`: live fixture with a large dead-tuple table, an
  unpartitioned table, a sequence at 99 percent, and a cyclic sequence; table
  and JSON output; `--fail-on` exit codes; schema filter; read-only proof via
  a before/after catalog checksum and no table-row reads.
- `tests/documentation_matches_the_build.rs`: compare the documented section
  and finding codes with the build.

## 5. Documentation

- README feature block and CLI example.
- `docs/design/growth-audit.md`: sections, sources, bounds, limits.
- `CHANGELOG.md`, `docs/status.md`, roadmap evidence.
