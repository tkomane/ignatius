# Feature Specification: Enterprise PostgreSQL authentication

**Feature Branch**: `001a-libpq-migration`

**Created**: 2026-08-16

**Status**: Closed without implementation by ADR-0012 on 2026-08-16.
The following migration proposal is historical; unchecked native tasks are
inactive. Cloud token authentication uses the existing driver in Feature 011.

**Input**: ADR-0009, the product roadmap and the remaining requirement for
GSSAPI, Kerberos or Windows integrated authentication.

## Scope note

This feature changes the PostgreSQL adapter only if the owner confirms that the
remaining enterprise authentication routes justify the migration cost. The
current application, reducer, result model, diagnostics contract and terminal
interface remain the compatibility baseline. The package deliberately records
the gates before code is written.

## User Scenarios & Testing

### User Story 1 - Connect to an enterprise-authenticated server (Priority: P1)

An organisation uses Kerberos, GSSAPI or Windows integrated authentication and
does not issue a separate database password to every operator. The user wants
Ignatius to reach that server through the supported enterprise route and to say
plainly when the route is unavailable or misconfigured.

**Independent Test**: With an approved synthetic or controlled enterprise test
environment, connect using the named authentication route, run a read query,
and verify the authenticated identity and transport state without exposing a
credential or claiming a weaker fallback.

### User Story 2 - Keep existing query behaviour during the adapter change (Priority: P1)

A user already relies on the current connection, TLS, cancellation, transaction
and diagnostic behaviour. They want the driver change to be invisible except
where a newly supported authentication route is selected.

**Independent Test**: Run the existing CLI, unit, layout and PostgreSQL contract
suite against the migration build and compare connection precedence, TLS policy,
result rendering, notices, cancellation, failed transactions and exit codes
with the current baseline.

### User Story 3 - Understand the installation and release requirement (Priority: P1)

An operator installs the client on macOS, Windows or Linux. They need to know
whether the required native library is bundled, discovered on the host, or must
be installed separately before the client can start.

**Independent Test**: Build and run the release candidate on each supported
platform from a clean machine or clean image, follow the documented install
steps, and confirm that the binary either starts with the stated dependency or
fails with a repairable diagnostic.

### User Story 4 - Stop safely when the native adapter cannot continue (Priority: P1)

A connection, query, cancellation request or native library operation fails.
The user needs a truthful state, a safe next action and no automatic replay of a
statement whose commit outcome is uncertain.

**Independent Test**: Exercise authentication failure, TLS policy failure,
server cancellation, connection loss and shutdown during a transaction. Confirm
that each outcome has the existing exit and diagnostic meaning, and that no SQL
or credential is repeated or written to a log.

## Requirements

### Functional Requirements

- **FR-1001**: The client MUST support each enterprise authentication route
  explicitly approved for this feature, including the required platform scope.
- **FR-1002**: The client MUST preserve the existing connection target
  precedence, TLS policy, result formats, transaction state and exit-code
  contracts for targets that do not use a new route.
- **FR-1003**: Operations on one session MUST execute in server order, and a
  later operation MUST NOT use a session while an earlier operation still owns
  it.
- **FR-1004**: Cancellation MUST request server cancellation, wait for the
  server's final answer and distinguish confirmed cancellation from an unknown
  outcome.
- **FR-1005**: Connection loss MUST remain an unknown query outcome whenever
  the server cannot establish whether the statement committed.
- **FR-1006**: Native-driver and server failures MUST map to the existing
  layered diagnostics with a likely cause, a safe next action and technical
  fields where available.
- **FR-1007**: Unsupported authentication, platform or native-library states
  MUST fail explicitly rather than silently selecting a weaker route.
- **FR-1008**: The application and pure query/UI layers MUST not depend on the
  selected PostgreSQL driver.
- **FR-1009**: The adapter MUST close native resources on success, failure,
  cancellation, connection loss and process shutdown.
- **FR-1010**: The release process MUST state how the native dependency is
  bundled, located, versioned and verified on macOS, Windows and Linux.

### User Experience Requirements

- **UX-1001**: Connection summaries MUST identify the authentication posture,
  TLS posture and database identity without relying on colour or an icon.
- **UX-1002**: A missing, incompatible or unavailable native dependency MUST
  name the repair action and the affected platform.
- **UX-1003**: A failed authentication attempt MUST not suggest retrying with a
  weaker authentication or transport policy.
- **UX-1004**: A migration build MUST preserve the existing plain and
  full-screen interaction wording for cancellation, failed transactions and
  connection loss.

### Security Requirements

- **SEC-1001**: Enterprise credentials, tickets, tokens and connection strings
  MUST never appear in debug output, diagnostics, logs or test failure text.
- **SEC-1002**: FFI boundaries MUST be confined to the PostgreSQL adapter and
  MUST document the memory and ownership invariant they rely on.
- **SEC-1003**: The adapter MUST preserve the requested TLS guarantee and MUST
  never downgrade transport or authentication silently.
- **SEC-1004**: Native library loading MUST use an explicit, documented trust
  path and MUST not search an unbounded or user-controlled path silently.

### Compatibility Requirements

- **COMPAT-1001**: Existing connection strings, password files, service files,
  TLS modes and environment precedence MUST retain their documented meaning.
- **COMPAT-1002**: Supported release targets MUST either run from the documented
  install procedure or fail before a connection is attempted with a clear
  dependency diagnostic.
- **COMPAT-1003**: The migration MUST keep the existing exit-code vocabulary
  and stream separation for scripts.

## Edge Cases

- Enterprise authentication is requested on a platform that does not provide
  its native support.
- The native library exists but is an incompatible version or architecture.
- Authentication succeeds but the server identity or TLS policy is not
  verifiable.
- A cancellation request races with a server error or connection loss.
- A process exits while a transaction or native operation is in progress.
- A connection is opened successfully, then the native library disappears from
  the environment before the next session starts.
- A server sends hostile error, notice, identity or authentication text.
- A release archive is unpacked without the expected native companion files.

## Key Entities

- **Enterprise authentication route**: the explicitly named mechanism,
  platform support, required local prerequisites and authenticated identity.
- **Adapter session**: one connection's lifecycle, transaction ordering,
  cancellation state, notices and server identity.
- **Native dependency identity**: the library source, version, architecture,
  packaging mode and verification evidence used by a build.
- **Operation outcome**: succeeded, failed, cancelled, connection lost or
  unknown, with the existing diagnostic and exit-code contract.

## Success Criteria

- **SC-1001**: In each approved enterprise test environment, an operator reaches
  the database and identifies the authenticated role without entering a second
  password route.
- **SC-1002**: The complete existing verification suite remains green, with no
  changed result, transaction, cancellation, TLS, diagnostic or exit-code
  meaning for the current connection routes.
- **SC-1003**: On every supported release platform, a clean install either
  starts using the documented dependency procedure or fails with a diagnostic
  that names one actionable repair.
- **SC-1004**: No migration test, log or diagnostic contains a credential,
  ticket, token or secret-bearing connection string.
- **SC-1005**: A cancellation or connection-loss test never replays the SQL and
  never claims success when the server's final outcome is unknown.
- **SC-1006**: The adapter can be replaced or disabled without changing files in
  the pure application, query or UI layers.

## Assumptions

- The owner confirms that at least one remaining enterprise authentication route
  is a real product requirement before implementation begins.
- The current native implementation remains the baseline until the migration
  passes the full compatibility matrix.
- A controlled server or CI environment will be supplied for each approved
  enterprise authentication route; local macOS evidence alone is insufficient.
- Existing `.pgpass`, service-file, TLS and client-certificate support is not
  removed merely because the adapter changes.

## Out of Scope

- Connection profiles, OS credential stores or password prompting.
- Query history, result inspection, filtering, clipboard support or UI redesign.
- A general PostgreSQL client API for other applications.
- Claiming support for an enterprise authentication route that has not passed a
  server-backed test on its named platform.
