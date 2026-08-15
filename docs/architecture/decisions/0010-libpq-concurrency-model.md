# ADR-0010: Proposed concurrency model for the libpq adapter

- Status: Proposed. Owner confirmation is required before implementation.
- Date: 2026-08-16
- Related: ADR-0001, ADR-0003, ADR-0008 and ADR-0009

## Context

ADR-0009 accepts a move to libpq, but records that libpq is synchronous by
default and that the concurrency choice needs its own decision. The current
client promises that input and rendering remain responsive while a statement
runs, that statements on one session keep their server order, and that
cancellation is reported only after the server confirms it.

The migration is also decision-gated. Four of the five original reasons for
adopting libpq now work natively, so implementation must not begin merely
because a dependency is named in the roadmap. The owner must still confirm
that GSSAPI, Kerberos or Windows SSPI is required and accept the cost of FFI,
platform packaging and a changed Windows installation story.

## Decision proposal

Use one serialized session actor for each libpq connection. The actor owns the
native connection and all native handles. Calls from the application become
bounded commands; results, notices, connection state and errors return as
messages. A blocking native operation runs away from the UI and reducer, while
the actor keeps transaction order and prevents two operations from using one
connection concurrently.

Cancellation is a separate request to the same adapter boundary. It must not
kill the process, replay the statement or report success before the server's
final result arrives. The adapter may use the native cancellation mechanism or
another supported connection to request cancellation, but that choice must be
covered by the adapter contract and tests.

The proposal does not approve a particular Rust wrapper or binding generator.
ADR-0009's preference remains: use a maintained safe wrapper if it satisfies
the required authentication and platform surface; write the smallest possible
bindings only if no suitable wrapper exists. Any FFI is confined to
`src/postgres`, with an explicit safety invariant at every unsafe boundary.

## Alternatives considered

### Native non-blocking polling

Drive libpq's non-blocking polling interface from the existing async runtime.
This may reduce thread usage, but it makes readiness, callbacks, cancellation,
connection setup and platform-specific authentication a larger state machine.
It remains a viable alternative if a maintained wrapper exposes it safely and
the spike proves that the UI and transaction invariants stay simple.

### A new blocking task for every operation

Use a blocking task per query. This is easy to start but unsafe as a session
model: concurrent tasks can reorder transactions, race cancellation and close
handles while another operation uses them. It is rejected unless it is wrapped
inside a single session actor, in which case it is the actor proposal above.

### Keep the current driver

Retain the native Rust adapter and implement the remaining authentication
routes separately. This is the correct outcome if the owner does not need
enterprise integrated authentication. It should supersede ADR-0009 rather than
become an unrecorded partial migration.

## Consequences

- The application, reducer, result model and UI remain unaware of libpq.
- One active session consumes one worker and one bounded command path; back
  pressure and shutdown behaviour must be measured rather than assumed.
- Transaction state, notices, SQLSTATE mapping, TLS posture and unknown
  connection outcomes remain part of the existing adapter contract.
- Native library discovery or bundling becomes part of the build and release
  contract on every supported operating system, especially Windows.
- No claim of GSSAPI, Kerberos or Windows SSPI support is valid until a real
  server-backed test proves it on the named platform.

## Packaging options to validate

This is an evidence matrix, not a packaging decision. The current repository
has no native libpq dependency or clean-install result, so T010 remains open.

| Platform | Host discovery to test | Bundled dependency to test | Evidence required before selection |
| --- | --- | --- | --- |
| macOS | `PQ_LIB_DIR`, `pkg-config` or `pg_config`, with the target architecture checked | A `pq-sys` bundled build with the selected TLS implementation | Clean Apple silicon artifact, load-path identity, TLS connection, missing-library diagnostic and notarisation implications |
| Linux | Target-specific `PQ_LIB_DIR`, `pkg-config` or `pg_config` | A bundled build with the libc and TLS assumptions recorded | Clean glibc and any supported musl artifact, architecture identity, dependency closure and repairable missing-library diagnostic |
| Windows MSVC | `vcpkg` and an explicitly documented `VCPKG_ROOT` or equivalent release prerequisite | Bundled libpq and TLS libraries with the MSVC runtime and DLL search path recorded | Clean Windows Terminal install, x64 or approved architecture identity, no uncontrolled DLL search, TLS connection and missing-dependency repair path |

Both paths must preserve the application's explicit trust rule: the loader may
not silently search an unbounded or user-controlled directory. The selected
path must also identify the libpq and TLS versions in the artifact evidence and
allow the dependency to be updated or revoked without an undocumented user
action. These are validation criteria only; they do not approve discovery,
bundling or a particular crate.

## Decision gates before implementation

1. The owner confirms that the remaining enterprise authentication capability
   is required and that the migration cost is accepted.
2. A wrapper or binding approach is selected with license, maintenance,
   platform and unsafe-code evidence.
3. The session actor and cancellation contract are accepted, including
   shutdown, timeout, connection loss and back-pressure behaviour.
4. The Windows distribution choice is documented before CI or release work
   claims a self-contained binary.

## Reversibility

High while this ADR is Proposed and no adapter code has shipped. Once a native
library becomes a release prerequisite or an authentication route becomes a
public contract, revisit ADR-0009 and this ADR together rather than silently
returning to the current driver.
