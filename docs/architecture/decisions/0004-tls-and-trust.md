# ADR-0004: rustls with the platform trust store

- Status: Accepted
- Date: 2026-08-15

## Context

`sslmode` values are not interchangeable, and conflating them is the most
consequential lie a database client can tell. `require` encrypts and verifies
nothing. `verify-full` checks the chain and the host name. A user who believes
they have the second while holding the first has no protection against an active
attacker.

## Decision

rustls 0.23 with the `ring` provider, `tokio-postgres-rustls` as the connector,
and `rustls-platform-verifier` for trust decisions so the macOS Keychain, the
Windows certificate stores and the usual Linux bundles work without exporting
anything.

Supported modes in this release: `disable`, `prefer`, `require`, `verify-full`.
`verify-ca` is **refused with an explanatory error**, not silently treated as
something else.

The negotiated state is not assumed from what was requested. It is read back from
the server through `pg_stat_ssl`, and reported as `Unknown` when that view is not
readable.

## Alternatives considered

- **native-tls.** Uses each platform's own stack, which is attractive, but gives
  less control over the policy differences between the `sslmode` values and less
  consistent error detail across platforms.
- **rustls with `webpki-roots`.** A bundled root store means the product's idea
  of trust silently diverges from the machine's, including for corporate
  interception roots that are legitimately installed. Rejected.
- **Implementing `verify-ca` by ignoring host name errors.** Possible, but it is
  a security-relevant custom verifier written under time pressure. Refusing is
  the honest answer until it can be built and tested properly.

## Consequences

- A TLS failure is never retried without TLS. Verified by test.
- Remote hosts default to `verify-full`; loopback and Unix sockets default to
  `prefer`, decided from the literal address, never guessed from a name.
- Client certificates, `sslrootcert`, `channel_binding` and `gssencmode` are not
  supported, and supplying any of them fails the connection rather than
  proceeding with weaker protection than the user asked for.

## Reversibility

Medium. The policy is in `src/postgres/tls.rs` and `src/connection/target.rs`.
