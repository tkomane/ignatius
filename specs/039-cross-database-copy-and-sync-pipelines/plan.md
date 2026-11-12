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
   - Detect cycles and identify tables requiring deferred constraints (`SET CONSTRAINTS ALL DEFERRED`).
2. **Stream Execution**:
   - If `--truncate`: execute `TRUNCATE TABLE ...` in reverse topological order.
   - For each table in topological order: execute `COPY <table> TO STDOUT` on source and `COPY <table> FROM STDIN` on target, streaming chunks with backpressure.
3. **Field Industrial TUI Cockpit**:
   - Hardware status capsules: `[SOURCE]`, `[TARGET]`, `[TABLES]`, `[SYNC STATE]`, `[THROUGHPUT]`.
   - Tabbed view: `[1] Dependency Graph & Plan`, `[2] Live Transfer Telemetry`.
4. **CLI**:
   - `ignatius sync --source <SRC> --target <DST> [--schema <SCHEMA>] [--table <TABLE>] [--truncate] [--cascade] [--dry-run] [--format table|json|plain]`.
