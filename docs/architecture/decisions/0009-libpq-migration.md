# ADR-0009: Move the PostgreSQL adapter to libpq

- Status: Accepted. Supersedes ADR-0003. Implementation scheduled, not started.
- Date: 2026-08-15

## Context

ADR-0003 chose `tokio-postgres` and accepted that libpq's connection surface
becomes our work: `.pgpass`, `pg_service.conf`, `sslmode=verify-ca`, client
certificates, and the enterprise authentication mechanisms. That ADR listed the
questions that would reverse it and said only the owner could answer them.

The owner answered on 2026-08-15: those capabilities are needed. That is the
trigger the earlier ADR named, so this supersedes it.

## Decision

The PostgreSQL adapter moves to libpq. The work is scheduled before Feature 002
hardens connection profiles on top of the current model, because profiles are
where credential routes become a stored contract.

## Consequences

Stated plainly, because each of these is a real cost the earlier decision was
buying its way out of:

1. **Unsafe code.** FFI needs `unsafe`, and ADR-0001 set `unsafe_code = "deny"`
   at the crate level with the rule that any exception requires an ADR. This is
   that ADR. The exception is confined to the adapter: `src/postgres` may allow
   `unsafe` at the FFI boundary, wrapped so that no `unsafe` escapes the module,
   and every wrapper carries a safety comment stating the invariant it upholds.
   Preference order: a vetted safe wrapper crate first, our own bindings only if
   no maintained wrapper fits.
2. **Distribution.** The binary stops being self-contained. On macOS and Linux
   libpq is commonly present or trivially installed; on Windows it is neither, so
   packaging must either bundle it or state the prerequisite. Whichever is
   chosen, `install and run` changes meaning on Windows and the documentation
   must say so before, not after.
3. **Concurrency.** libpq is synchronous by default. Keeping the promise that
   input and rendering never wait on the database means either its non-blocking
   polling API driven by our own event loop, or a dedicated thread per
   connection. That choice needs its own ADR once the surface is known.
4. **Type rendering stays as it is.** The text-format decision from ADR-0003 is
   independent of the driver and is not being revisited: values continue to
   arrive rendered by the server.

## Sequencing

The cheap half is needed either way and does not depend on the driver:
`.pgpass` and `pg_service.conf` are file formats, and parsing them is ours to do
regardless. Those land first in Feature 002, which keeps that feature moving
while the driver work is scoped.

The expensive half, client certificates and enterprise authentication, is what
libpq is actually being adopted for, and is what the migration must deliver.

## What does not change

The ports boundary holds. `src/postgres` is the only module that knows a driver
exists, so the migration is contained: the reducer, the result model, the value
rendering, the diagnostics and the interface are untouched. That containment was
the point of the boundary, and this is the first time it is being cashed in.

## What has changed since this was accepted

**2026-08-15, same day.** Four of the five capabilities this ADR was accepted for
have since been implemented without libpq, and tested against a real server:

| Capability | State |
| --- | --- |
| `.pgpass` password files | Implemented natively |
| `pg_service.conf` service files | Implemented natively |
| `sslmode=verify-ca` | Implemented natively with rustls |
| Client certificates | Implemented natively with rustls, tested against a TLS server |
| GSSAPI, Kerberos, Windows SSPI | **Not implemented, and the only remaining reason to adopt libpq** |

This is reported rather than acted on. The decision to adopt libpq stands until
the owner revisits it; what has changed is the price and the prize. The price is
unchanged: an `unsafe` exception, a changed Windows distribution story, and a
concurrency model that needs its own ADR. The prize is now GSSAPI and SSPI alone.

The question worth putting to the owner is narrower than it was: do you need to
reach a PostgreSQL server that authenticates with Kerberos or Windows integrated
authentication? If yes, the migration is still worth its cost. If no, this ADR
should be superseded in turn, and the remaining gaps closed natively as these
four were.

## Reversibility

Low once shipped. A released client that reads `.pgpass` and presents client
certificates cannot quietly stop doing so.
