# Architecture Plan: Cross-Database Copy and Sync Pipelines (Feature 039)

## Architecture Overview

```
Source Database                   Target Database
 [Session 1]                        [Session 2]
      |                                  ^
      | COPY TO STDOUT (BINARY)          | COPY FROM STDIN (BINARY)
      +---------------> Pipe ------------+
              (In-Memory Bounded Stream)
```

1. **Dependency Analysis**:
   - Query `pg_constraint` with `contype = 'f'` (foreign key) linking `conrelid` and `confrelid`.
   - Build a Directed Acyclic Graph (DAG) using Kahn's algorithm for topological sorting.
   - Detect and report cycles. The destination's configured constraint modes still apply.
2. **Stream Execution**:
   - For a nonempty write plan, require destination autocommit and hold an exclusive destination borrow through the driver's transaction.
   - Before table writes, check the destination transaction's database-scoped advisory marker from the source to reject the same database even through a connection alias.
   - If `--truncate`: execute one `TRUNCATE TABLE ...` with tables listed in reverse topological order. Add `CASCADE` only when explicitly requested.
   - For each table in topological order: execute `COPY <table> TO STDOUT` on source and `COPY <table> FROM STDIN` on target, streaming chunks with backpressure.
   - Roll back the destination after a failed copy and preserve the original failure. Return successful telemetry after acknowledged commit. Advise checking the destination when the outcome is unknown.
   - Map COPY startup, streaming and completion, source dry-run count and destination truncate failures through the value-free transfer diagnostic. Provide a fixed phase and validated SQLSTATE without a SQL statement ordinal.
3. **Field Industrial TUI Cockpit**:
   - Hardware status capsules: `[SOURCE]`, `[TARGET]`, `[TABLES]`, `[SYNC STATE]`, `[THROUGHPUT]`.
   - Tabbed view: `[1] Dependency Graph & Plan`, `[2] Live Transfer Telemetry`.
4. **CLI**:
   - `ignatius sync --source <SRC> --target <DST> [--schema <SCHEMA>] [--table <TABLE>] [--truncate] [--cascade] [--dry-run] [--format table|json|plain]`.
   - Resolve source and destination independently. Apply `--target-environment` to a literal destination and read the environment from a named destination profile. Refuse an unconfirmed production write before either endpoint authenticates or connects.
