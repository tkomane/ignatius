# Plan - Query plan history and regression timeline

**Specification** [spec.md](./spec.md)
**Evidence** [tasks.md](./tasks.md)

## Capture

`src/cli/plan_history.rs` resolves the target through the existing CLI
connection path with read-only forced. It bounds and validates the statement
before connecting. The per-series binding is checked before instrumentation
and checked again when saving to reject changes made by a concurrent process.
The adapter consumes a dedicated session, so the transaction has one owner.

`src/postgres/plan_history.rs` opens a repeatable-read, read-only transaction.
It validates one read with extended Parse and streams every warmup and sample
through that prepared statement. The exact EXPLAIN wrapper uses its own
single-statement extended request and accepts one bounded JSON row. Plan
history reuses the schema extractor, EXPLAIN parser and benchmark sample
collector, with a client deadline for each user SQL operation. Errors unwind
through ROLLBACK. On cancellation or deadline the owning connection closes.
Benchmarks use no inner transaction because the outer transaction owns
isolation. The catalog read precedes measurement. The store sees only a
digest-bound observation.

## Model and persistence

`src/plan_history/mod.rs` holds the capture record, digest projection and
change calculations. Keep numeric metrics and lowercase SHA-256 values and
use a numeric UTC timestamp. Estimate-only observations lack runtime metrics.
No session or network objects enter the stored format.

`src/plan_history/store.rs` owns one private versioned catalog, an advisory
lock file for mutating operations, bounded reads and the existing atomic file
writer. Readers observe an old or new complete file. Validation fails closed
on unexpected keys, invalid names and hashes, invalid numeric metrics or
unsupported versions. A named forget has the same lock and durability path.
Concurrent capture appends revalidate on the lock-protected catalog.

## Constitution check

The CLI is an explicit local-opt-in route and uses ordinary PostgreSQL
authorization without retries. Diagnostics use the existing privacy path for
SQL sent by this route. The model is independent of IO, serialization is
versioned, and rendering uses text and numeric values without relying on color.
No TUI work or additional dependency is required. Update the local data
handling and CLI docs and the changelog with the capture and erase routes.

## Verification

Use unit tests for exact digests, deltas, corruption, retention, lock
contention and atomic replacement. Exercise the CLI against the existing
PostgreSQL contract fixture through an index change, estimate and measure,
and confirm a refused write leaves the catalog unchanged. Check offline show
and forget through the binary. Exercise the same contracts on the hosted
14-18 matrix where the production runner crosses the catalog and SQL boundary.
