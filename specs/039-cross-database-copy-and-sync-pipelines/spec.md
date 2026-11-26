# Specification: Cross-Database Copy and Sync Pipelines (Feature 039)

## Purpose
Enables direct, high-throughput streaming data synchronization between two PostgreSQL databases (e.g. staging to development, or service-to-service), computing foreign-key topological dependency ordering to ensure constraint-safe truncation and insertion without intermediate disk dumps.

## User Scenarios
1. **Constraint-Safe Schema Synchronization**:
   A developer synchronizes a schema with inter-dependent tables from staging to a local container. Ignatius discovers foreign keys, groups the selected tables into one truncate when requested, and copies the parent rows before their dependent rows.
2. **Subprocess-Free Stream Pipelining**:
   Unlike pgAdmin or shell scripts that write intermediate `.sql` or `.dump` files to disk, Ignatius pipes binary `COPY TO STDOUT` directly into `COPY FROM STDIN` across two active asynchronous PostgreSQL sessions.
3. **Interactive TUI Data Sync Cockpit (`Ctrl+K |`)**:
   Users inspect discovered table dependencies, cycle warnings, and live transfer progress (rows/sec, transfer rate, progress bars) within a tactile Field Industrial dashboard.
4. **Scriptable CLI (`ignatius sync`)**:
   CI/CD pipelines and automated seed scripts invoke `ignatius sync --source <SRC> --target <DST> --truncate` to hydrate test environments deterministically.

## Failure diagnostics

Failures during a source dry-run count, destination truncate or binary COPY
identify the affected side and operation. Diagnostics preserve a valid SQLSTATE
when available and omit server and driver text that could contain row values.
A connection loss reports that the outcome is unknown and advises checking the
destination before continuing.

## Destination transaction

One PostgreSQL transaction owns all destination writes, including optional
truncation and every selected table's COPY. An idle destination is required.
The command reports success after PostgreSQL acknowledges commit. After a
failure, the caller gets the original phase and SQLSTATE and learns when the
rollback or commit outcome could not be confirmed. Dry runs and empty plans
start no destination transaction.

The source and destination must be different databases. Two independent
sessions may use the explicit `pg_temp` alias in the single-table helper
for their distinct temporary tables. Source reads use the existing session's
isolation, and destination constraints are enforced as configured.
