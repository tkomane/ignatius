# Implementation Plan: Partition Lifecycle (048)

## 1. Domain (`src/partition/`)

- `mod.rs`:
  - `PartitionStrategy` (`Range`, `List`, `Hash`, `Unknown`).
  - `PartitionBound` (`FromTo { from, to }`, `Default`, `Unparsed(String)`).
  - `PartitionEntry` and `PartitionedTable` for status.
  - `PartitionPeriod` (`Daily`, `Weekly`, `Monthly`, `Yearly`) with
    `next(start)` calendar arithmetic.
  - `PlanRequest` (`table`, `period`, `start`, `count`, `expire_before`).
  - `PlanError` variants for unsupported shape, unparsable bound, overlap,
    invalid date and identifier overflow.
  - `render_status_table`, `render_status_json`.
  - `render_plan_sql(statements)` with a header comment naming the parent and
    the generation inputs, but no timestamp so output is reproducible.
- Date arithmetic: use `time::Date`/`PrimitiveDateTime` already in the
  dependency tree; weekly periods start on Monday. Month and year stepping use
  calendar addition, not fixed day counts.

## 2. Catalog (`src/postgres/partition.rs`)

- `partition_snapshot(client, schema)` reads partitioned parents and children
  in one bounded query.
- `pg_get_partkeydef` supplies strategy and key text; reject anything that is
  not `RANGE (<identifier>)` when planning.
- `pg_get_expr(c.relpartbound, c.oid)` supplies the bound text.
- Bound parser accepts `FOR VALUES FROM ('literal') TO ('literal')`,
  `MINVALUE` and `MAXVALUE`; literals are ISO dates or timestamps. Everything
  else is `Unparsed`.
- `Session::partition_snapshot(schema)` delegates.

## 3. CLI (`src/cli/`)

- `Command::Partition(PartitionArgs)` with subcommands `Status` and `Plan`.
- `StatusArgs`: `--schema`, `--format table|json`, connection options.
- `PlanArgs`: `--table`, `--period`, `--start`, `--count`, `--expire-before`,
  connection options.
- Plan path: read snapshot, resolve parent and key type, validate shape, parse
  existing bounds, generate intervals, refuse overlaps, optionally append
  expiry statements, print SQL.
- Exit 2 for usage or a refused plan; 0 for a clean plan or status.

## 4. Tests

- `src/partition/tests.rs`: period arithmetic over leap years and month ends,
  name generation and truncation, bound parsing, overlap detection, expiry
  selection, unsupported shapes, deterministic SQL rendering.
- `tests/partition_contract.rs`: live fixture with a monthly range-partitioned
  table; status table/JSON; plan for three months; overlap refusal; expiry
  output; confirmation that a plan run changes nothing.
- `tests/documentation_matches_the_build.rs`: supported periods and status
  fields documented and present.

## 5. Documentation

- README feature block and examples.
- `docs/design/partition-lifecycle.md`.
- `CHANGELOG.md`, `docs/status.md`, roadmap evidence.
