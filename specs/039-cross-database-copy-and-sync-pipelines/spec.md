# Specification: Cross-Database Copy and Sync Pipelines (Feature 039)

## Purpose
Enables direct, high-throughput streaming data synchronization between two PostgreSQL databases (e.g. staging to development, or service-to-service), computing foreign-key topological dependency ordering to ensure constraint-safe truncation and insertion without intermediate disk dumps.

## User Scenarios
1. **Constraint-Safe Schema Synchronization**:
   A developer synchronizes a complex schema with dozens of inter-dependent tables from staging to a local container. Ignatius automatically discovers foreign keys, computes topological insert ordering (parents before children) and reverse truncation ordering (children before parents), and copies data seamlessly without foreign key constraint violations.
2. **Subprocess-Free Stream Pipelining**:
   Unlike pgAdmin or shell scripts that write intermediate `.sql` or `.dump` files to disk, Ignatius pipes binary `COPY TO STDOUT` directly into `COPY FROM STDIN` across two active asynchronous PostgreSQL sessions.
3. **Interactive TUI Data Sync Cockpit (`Ctrl+K |`)**:
   Users inspect discovered table dependencies, cycle warnings, and live transfer progress (rows/sec, transfer rate, progress bars) within a tactile Field Industrial dashboard.
4. **Scriptable CLI (`ignatius sync`)**:
   CI/CD pipelines and automated seed scripts invoke `ignatius sync --source <SRC> --target <DST> --truncate` to hydrate test environments deterministically.
