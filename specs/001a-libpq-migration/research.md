# Research: Enterprise PostgreSQL authentication

**Status**: Repository-grounded planning research. Vendor, wrapper and
platform validation remain Phase 0 gates and are not represented as completed.

## Decision 1: Reconfirm the reason to migrate

**Decision**: Keep the package decision-gated until the owner confirms that
GSSAPI, Kerberos or Windows SSPI is required.

**Rationale**: ADR-0009 was accepted when five capabilities motivated libpq.
The repository now records native support for `.pgpass`, service files,
`verify-ca` and client certificates. Enterprise integrated authentication is
the remaining stated benefit, so the cost case must be rechecked rather than
assumed.

**Alternatives considered**:

- Proceed immediately because ADR-0009 is Accepted. Rejected because the
  recorded capability gap and cost have materially narrowed.
- Supersede ADR-0009 immediately and implement native authentication. Deferred
  until the owner answers the remaining requirement.

## Decision 2: Preserve the adapter port

**Decision**: Keep all driver knowledge inside `src/postgres` and preserve the
existing session, result, diagnostic and cancellation contracts.

**Rationale**: The component architecture already names `postgres` as the
adapter and the pure core has no reason to know whether the driver is native or
FFI-backed. This limits migration risk and makes a reversal possible before a
release contract changes.

**Alternatives considered**:

- Expose native handles to the reducer or UI. Rejected because it violates the
  existing module boundary and makes unsafe lifecycle behaviour untestable.
- Rewrite the query and result layers with the adapter. Rejected because value
  rendering and server text format are explicitly driver-independent.

## Decision 3: Use a serialized session owner as the working concurrency model

**Decision**: Use ADR-0010's proposed one-owner session actor as the planning
baseline, subject to a spike and owner approval.

**Rationale**: It keeps operations on one connection ordered, puts blocking
  native work away from rendering, and gives cancellation and shutdown one
  lifecycle owner.

**Alternatives considered**:

- Drive non-blocking libpq polling from the async runtime. Keep as a viable
  alternative if a maintained wrapper exposes it without a larger unsafe state
  machine.
- Spawn an unrelated blocking task for every operation. Rejected because it can
  reorder transactions and race cancellation.

## Decision 4: Treat packaging as a release gate

**Decision**: Do not choose bundled versus host-installed libpq in this package
without a clean-machine test on macOS, Windows and Linux.

**Rationale**: ADR-0008 makes distribution identity and platform artefacts
explicit, while ADR-0009 identifies Windows as the largest packaging cost. A
development machine with a working library is not evidence for a user install.

**Alternatives considered**:

- Assume the host has libpq. Rejected because it would make a release claim
  that cannot be reproduced on a clean machine.
- Bundle immediately. Deferred until licensing, architecture, update and
  vulnerability ownership are documented.

## Decision 5: Security evidence must include the native boundary

**Decision**: Extend the existing threat model and data-handling review before
the first release that ships the native adapter.

**Rationale**: The current threat model covers hostile PostgreSQL input,
credentials, logs and release artifacts, but a native library adds ownership,
loading-path and platform distribution risks. The existing no-secret and no-
silent-downgrade rules remain unchanged.

**Open validation**:

- Exact wrapper or binding and its license and maintenance status.
- Supported enterprise authentication mechanisms by platform and server setup.
- Native library loading path, architecture matching and update ownership.
- Cancellation API semantics and connection lifecycle under shutdown.
- Clean-machine packaging and CI service availability.
