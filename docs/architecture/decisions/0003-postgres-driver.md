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

## What would change this decision

The trade is one thing against one thing: a single binary that depends on nothing
on the user's machine, versus getting every libpq behaviour for free. Rebuilding
libpq's surface ourselves is the price of the first.

These questions decide it, and only the owner can answer them:

1. **Client certificates.** Do any databases you reach require `sslcert` and
   `sslkey`? Reimplementing certificate authentication is real work; libpq has it.
2. **`verify-ca`.** Do you have a certificate setup where the chain is trusted but
   the host name will not match, such as a load balancer or an internal PKI?
3. **`.pgpass` and `pg_service.conf`.** Do you already rely on them, so that a
   client which ignores them is a downgrade from `psql`?
4. **Enterprise authentication.** Do you need GSSAPI, Kerberos or Windows SSPI?
   These are substantial to implement and libpq already has them.
5. **Cloud token authentication**, such as Entra ID for Azure Database for
   PostgreSQL. This is additive either way and does not by itself decide the
   question.
6. **Distribution.** How much do you care that the binary depends on nothing? A
   libpq dependency means either bundling it per platform or requiring the user
   to have it, which changes what "install and run" means on Windows in
   particular.

If the answer to 1, 3 or 4 is yes for databases you use regularly, the balance
moves towards libpq and this ADR should be superseded before Feature 002 builds
profiles on top of the current model. If they are all no, the current choice
stands and the remaining gaps are ordinary work.

## Reversibility

Medium. Confined to `src/postgres`, which is the only module that knows the
driver exists.
