# Feature Specification: Microsoft Entra ID authentication

**Feature Branch**: `011-entra-authentication`

**Created**: 2026-08-16

**Status**: Specified. Not implemented.

**Input**: The owner's requirement of 2026-08-16 - connect to the COI
application's Azure Database for PostgreSQL Flexible Server, which authenticates
with Microsoft Entra ID - and ADR-0012, which decided this is a credential route
rather than a driver change.

## Scope note

This adds one credential route. It changes no adapter, no result model, no
diagnostics contract and no key binding. Everything the client already does with
a password it does with a token, and the differences are that the token is
fetched rather than typed, that it expires, and that it may never travel without
TLS.

## User Scenarios & Testing

### User Story 1 - Connect to an Entra-authenticated server (Priority: P1)

Someone runs the COI application's database in Azure. There is no password to
know: the database trusts Microsoft Entra ID, and their identity is their Entra
principal. They are already signed in with the Azure CLI, because everything
else they do that day needs it.

**Independent Test**: with `az` signed in to the right tenant, open a session
against the server with `--auth entra`, run a read query, and confirm the server
reports the connected role as the Entra principal.

**Acceptance Scenarios**:

1. **Given** a profile or target that names Entra authentication, **When** a
   connection is made, **Then** an access token is obtained for the OSS RDBMS
   resource and presented as the password, and the session opens.
2. **Given** a successful connection, **When** the header or `\conninfo` is
   read, **Then** it says the identity came from Entra, and names the principal,
   so nobody has to guess which credential route was taken.
3. **Given** any of this, **When** it is over, **Then** the token is in no file,
   no environment variable, no log, no history entry and no `Debug` output.

### User Story 2 - The route is not available, and says which part (Priority: P1)

Someone tries the same thing on a machine where something is missing. The value
of this story is entirely in the wording: four different problems produce four
different sentences, each naming what to do.

**Acceptance Scenarios**:

1. **Given** no Azure CLI on the path, **When** a connection is attempted,
   **Then** the client says the Azure CLI was not found, names it, and does not
   describe this as an authentication failure.
2. **Given** an Azure CLI that is not signed in, **When** a token is requested,
   **Then** the client says so and names `az login` as the way out.
3. **Given** a token that the server rejects, **When** the connection fails,
   **Then** the client says the token was obtained and the server refused it,
   and names the likely cause: an Entra principal that has no role on this
   server.
4. **Given** any of these, **When** the message is produced, **Then** it contains
   no part of a token, and no more of the CLI's own output than the reason.

### User Story 3 - The token never travels in the clear (Priority: P1)

**Acceptance Scenarios**:

1. **Given** a target whose TLS mode would permit an unencrypted connection,
   **When** Entra authentication is selected, **Then** the connection is refused
   before any token is requested, and the message says a bearer token may not be
   sent unencrypted.
2. **Given** a server that refuses TLS, **When** the connection is attempted,
   **Then** the existing behaviour holds: it fails, and nothing is retried
   without TLS.

### User Story 4 - A second connection needs a second token (Priority: P2)

The object tree opens a connection of its own. It must authenticate on its own
terms rather than reusing anything held from the first.

**Acceptance Scenarios**:

1. **Given** an Entra-authenticated session, **When** the object tree opens its
   connection, **Then** it obtains its own token and does not share one.
2. **Given** a token that has expired since the session opened, **When** a new
   connection is made, **Then** a fresh token is obtained for it, because a
   token is fetched per connection attempt and never stored.

### Edge Cases

- The Azure CLI is present but signed in to the wrong tenant: the server refuses
  and the reason must be legible.
- The Azure CLI takes a long time, or hangs waiting for interactive sign-in.
  There is a timeout, and it says what timed out.
- A managed identity on a virtual machine, where the CLI resolves an identity
  with no interactive sign-in at all. This must work unchanged, because the
  route is "ask the CLI", not "ask a human".
- A user name that is a service principal's application ID rather than an
  address.

## Requirements

### Functional

- **FR-1101**: A target MUST be able to name Entra authentication, from a
  connection profile and from the command line, and the profile field MUST be
  documented alongside the fields that already exist.
- **FR-1102**: The token MUST be obtained by running the Azure CLI, reading it
  from the program's standard output. Ignatius makes no network call of its own
  for this. A native token client is out of scope and needs its own
  specification and threat-model update.
- **FR-1103**: A token MUST be obtained per connection attempt and dropped with
  it. Nothing caches a token; the CLI's own cache is the only one.
- **FR-1104**: The user name presented to the server MUST be the one configured.
  It is never inferred from the signed-in CLI account, because connecting as
  somebody other than who was asked for is the kind of surprise principle I
  forbids.
- **FR-1105**: Each distinct failure - CLI absent, CLI not signed in, token
  request failed, server refused the token - MUST produce its own message
  naming its own remedy.
- **FR-1106**: A run that used this route MUST be identifiable in the session
  header and in `doctor` output, without revealing the token.
- **FR-1107**: The Azure CLI invocation MUST have a timeout, and exceeding it
  MUST be reported as a timeout rather than as an authentication failure.

### Security

- **SEC-1101**: The token is a bearer credential. It lives in `SecretString`,
  never in an environment variable, a process argument, a log, a diagnostic, a
  history entry or a `Debug` output.
- **SEC-1102**: A token MUST NOT be sent on a connection that is not encrypted.
  This is checked before the token is requested, so a misconfigured target costs
  nothing and reveals nothing.
- **SEC-1103**: The existing rule holds without exception: TLS is never
  downgraded and no attempt is retried without it.
- **SEC-1104**: The Azure CLI's standard error MUST be treated as untrusted text
  for display purposes and redacted through the one existing implementation.

### Exit codes

No new codes. The route reuses what is already documented:

| Situation | Code |
| --- | --- |
| The Azure CLI is absent, or the target's TLS mode forbids this route | 3, config |
| The CLI is not signed in, or the server refused the token | 5, authentication |
| The CLI failed for another reason, or timed out | 4, connection |
| TLS could not be established | 6, tls |

## Success Criteria

- **SC-1101**: A session opens against an Entra-authenticated Azure server, the
  server reports the connected role as the Entra principal, and the token
  appears nowhere in the process environment or in any file the run wrote.
  Live evidence, on the owner's machine against the COI server, recorded with
  the date and the server version.
- **SC-1102**: With the Azure CLI removed from the path, the failure names the
  Azure CLI and exits 3. Proven by a subprocess test with a controlled path.
- **SC-1103**: With a target whose TLS mode would allow a plain connection, no
  token is ever requested. Proven by a test using a token provider that records
  whether it was called.
- **SC-1104**: No automated test requires a real Azure subscription or a real
  database. The token provider is an interface with a fake in tests, which is
  the only way the constitution's rule about synthetic data can hold here.

## Assumptions

- The Azure CLI is a reasonable prerequisite for this route and for nobody else.
  Someone who does not use Entra never installs it and never hears about it.
- `az account get-access-token --resource-type oss-rdbms` is the supported way
  to obtain the token, and the resource it maps to is
  `https://ossrdbms-aad.database.windows.net`. Checked against Microsoft's
  documentation on 2026-08-16; sources are in ADR-0012.
- PostgreSQL authenticates a connection when it is made. An open session is not
  re-authenticated, so an expiring token ends the ability to open new
  connections rather than interrupting the one in hand. This assumption is
  written down because the design of FR-1103 rests on it, and because if Azure
  turns out to terminate sessions on expiry, the reconnect path must be
  specified rather than discovered.
- Managed identity needs no separate code path: it is the same CLI call on a
  machine whose CLI resolves an identity without a human.
