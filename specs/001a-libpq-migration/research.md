# Research: Enterprise PostgreSQL authentication

**Status**: Repository-grounded planning research. Vendor, wrapper and
platform validation remain Phase 0 gates and are not represented as completed.

**External evidence checked**: 2026-08-16 against PostgreSQL 18.6
documentation and the current published metadata for the candidate Rust
crates. This comparison is research evidence, not a dependency selection.

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

## Decision 6: Compare native binding candidates before the spike

**Decision**: Keep the candidate set open. The first implementation spike should
compare a safe wrapper over libpq with the lower-level `pq-sys` bindings against
the existing `tokio-postgres` baseline. No candidate is approved by this
research entry.

| Candidate | Evidence checked | Fit for this repository | Open gate |
| --- | --- | --- | --- |
| `libpq` with `libpq-sys` | The published `libpq` crate describes itself as a safe binding, depends on `libpq-sys`, and is MIT licensed. The indexed 6.0.1 release had 79.34% item documentation and only two examples. PostgreSQL's libpq connection options include `gss` and `sspi`. | The smallest initial unsafe surface if its safe API exposes the required authentication, async-result and cancellation operations. | Prove that the wrapper exposes the required GSSAPI or SSPI route, notices, result draining, cancellation and shutdown on all target platforms. Treat documentation coverage as a maintenance signal, not a safety proof. |
| `pq-sys` with a repository-owned adapter | The published crate is direct one-to-one bindings to `libpq-fe.h` and `postgres_ext.h`, Apache-2.0 or MIT licensed. It supports target-specific `PQ_LIB_DIR`, `pkg-config`, `pg_config`, MSVC `vcpkg`, and optional bundled builds. | Covers the native surface, including whatever authentication the linked libpq provides, and makes packaging options visible, but leaves ownership, lifetimes, error mapping and thread confinement to our adapter. | Write and review the smallest safe wrapper, prove the bundled TLS choice, and test every handle cleanup path. The bindings alone do not prove a server-backed GSSAPI or SSPI route. |
| Keep `tokio-postgres` | The current `Cargo.toml` uses `tokio-postgres` 0.7.18 and no native libpq dependency. | Preserves the current async and packaging model and remains the correct baseline for parity. It does not by itself provide the libpq enterprise authentication routes named by ADR-0009. | Owner must confirm that the remaining enterprise route justifies the native cost before this baseline is replaced. |
| Bespoke bindings | No repository or upstream comparison found that justifies owning a third binding generator before the two existing options are spiked. | Avoid as the default because it expands the unsafe and maintenance surface without a demonstrated capability gap. | Consider only if the selected wrapper and `pq-sys` cannot expose an approved route on a supported platform. |

The metadata checked lists `libpq` 6.0.1 (2025-10-29) and `pq-sys` 0.7.5
(2025-10-15) as their indexed releases. Both have an active source repository
and recent published history, but neither metadata page supplies a maintenance
SLA or a guarantee that every libpq function needed by this client is wrapped.
Maintenance is therefore a selection input, not a completed finding.

The PostgreSQL API surface makes the concurrency choice material. The current
documentation describes nonblocking connection through
`PQconnectStartParams` and `PQconnectPoll`, asynchronous execution through
`PQsendQuery`, `PQconsumeInput` and `PQgetResult`, and cancellation followed by
normal result draining until `PQgetResult` returns null. It also prohibits
concurrent manipulation of one `PGconn`; PostgreSQL 17 and later document
libpq as reentrant and thread-safe subject to that per-connection restriction.
The adapter spike must therefore exercise the exact calls exposed by the chosen
Rust candidate rather than infer compatibility from its name.

### Evidence links and follow-up

- [PostgreSQL 18 connection control](https://www.postgresql.org/docs/current/libpq-connect.html)
- [PostgreSQL 18 asynchronous command processing](https://www.postgresql.org/docs/current/libpq-async.html)
- [PostgreSQL 18 threaded-program behaviour](https://www.postgresql.org/docs/current/libpq-threading.html)
- [`libpq` crate metadata and source](https://docs.rs/crate/libpq/latest)
- [`pq-sys` crate metadata and build options](https://docs.rs/crate/pq-sys/latest)
- [`pq-sys` upstream repository](https://github.com/sgrif/pq-sys)

This completes the research comparison requested by T009. T010 and T011 remain
open because packaging and the session actor require an owner-approved spike;
T008 remains an explicit owner confirmation rather than an inference from this
research.
