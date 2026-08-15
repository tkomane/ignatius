# ADR-0003: tokio-postgres rather than a libpq binding

- Status: Accepted
- Date: 2026-08-15

## Context

The client needs asynchronous execution that never blocks rendering, protocol
level cancellation on a second connection, server notices delivered as they
arrive, and streaming of rows so a large result cannot exhaust memory.

## Decision

`tokio-postgres` 0.7.18 on the Tokio runtime, using the **simple query protocol**
for user SQL.

The simple query protocol is the significant part of this decision. It returns
values in PostgreSQL's own text rendering, which means numeric precision,
timestamp offsets, arrays, ranges, JSON and every extension type this client has
never heard of all display exactly as the server would print them, with no type
decoding to get wrong.

## Alternatives considered

- **libpq via FFI.** Perfect fidelity, including service files and the full
  `sslmode` matrix, for free. Rejected for version 1 on distribution cost: it
  makes every platform build depend on a C library and complicates static
  binaries. Revisit if the unsupported surface in
  `docs/support/compatibility.md` stops shrinking.
- **`sqlx`.** Excellent for applications with compile-time checked queries. Wrong
  shape for an interactive client running arbitrary user SQL.
- **Extended protocol with binary results.** Needed for parameter binding, which
  version 1 does not offer for user SQL. It would also require decoding every
  type correctly to display it, which is a large surface with no user benefit
  here.

## Consequences

- Verified against PostgreSQL 18.4 on 2026-08-15: streaming via
  `simple_query_raw`, cancellation via `CancelToken` confirmed by the server as
  SQLSTATE 57014, and notices delivered by polling the connection.
- The driver reports a row count on completion but not the full command tag
  text, so the interface says "3 rows affected" rather than "INSERT 0 3". Noted
  in `docs/support/compatibility.md`.
- libpq conventions the driver does not implement (`.pg_service.conf`, `.pgpass`)
  are ours to implement or to refuse loudly. See ADR-0004 and Feature 002.
- A `Session` depends on the Tokio runtime that created it, because the
  connection is driven on a spawned task. Dropping that runtime closes the
  connection. This is documented on the type and enforced in the test fixture.

## Reversibility

Medium. Confined to `src/postgres`, which is the only module that knows the
driver exists.
