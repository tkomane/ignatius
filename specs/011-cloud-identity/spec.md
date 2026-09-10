# Feature Specification: Cloud identity authentication

**Feature Branch**: `011-cloud-identity`

**Created**: 2026-08-16

**Status**: Mechanism implemented and verified against synthetic TLS PostgreSQL.
Live Entra/AWS/GCP accounts and native Windows provider-launch evidence remain
open in `docs/support/compatibility.md`; synthetic provider evidence does not
close those rows.

**Input**: The owner's requirement of 2026-08-16 - connect to a production
application's Azure Database for PostgreSQL, which authenticates with Microsoft
Entra ID - followed by the instruction to implement Entra first but to build for
a first-class experience across other cloud providers, because not everyone will
be using the same sign-in pattern. ADR-0012 decided this is a credential route
rather than a driver change.

**Renamed** from `011-entra-authentication` when that instruction arrived. The
original name described the first provider, not the feature.

## Why one feature and not three

The three major clouds authenticate a PostgreSQL connection in exactly the same
way, and it is worth stating plainly because it is what makes a single
implementation honest rather than a forced abstraction:

| Provider | Where the token comes from | Lifetime | Transport |
| --- | --- | --- | --- |
| Microsoft Entra ID | `az account get-access-token --resource-type oss-rdbms`, JSON, `accessToken` | 5 to 60 minutes | TLS required |
| AWS RDS and Aurora IAM | `aws rds generate-db-auth-token --hostname H --port P --username U`, raw | 15 minutes | TLS required |
| Google Cloud SQL IAM | `gcloud sql generate-login-token`, raw | 1 hour | TLS required |

In every case a command-line tool returns a short-lived bearer token, and that
token is presented as the password on an ordinary password-authenticated
connection. Each vendor's own documentation reaches the same workaround for
`psql`: put the token in `PGPASSWORD`, because it is longer than the password
field a prompt will accept. All three checked against vendor documentation on
2026-08-16; sources at the end.

So the feature is not "Entra support". It is: obtain a short-lived credential by
running a program, present it as the password, and refuse to do any of that
without encryption.

## User Scenarios & Testing

### User Story 1 - Connect to a cloud-authenticated server (Priority: P1)

Someone runs a database in Azure, AWS or Google Cloud that trusts the cloud's
own identity system. There is no password to know. They are already signed in
with that cloud's command-line tool, because everything else they do that day
needs it.

**Independent Test**: with a provider configured, open a session, run a read
query, and confirm the server reports the connected role.

**Acceptance Scenarios**:

1. **Given** a target that names an authentication provider, **When** a
   connection is made, **Then** the provider's command is run, its output is
   used as the password, and the session opens.
2. **Given** a successful connection, **When** the session header is read,
   **Then** it names the provider the credential came from, so nobody has to
   guess which route was taken.
3. **Given** any of this, **When** it is over, **Then** the token is in no file,
   no environment variable, no process argument, no log, no history entry and no
   `Debug` output.

### User Story 2 - A provider that is not one of the three (Priority: P1)

Someone uses a cloud, an internal identity service or a wrapper script that this
build has never heard of. The mechanism is the same; only the command differs.

**Acceptance Scenarios**:

1. **Given** a provider defined in configuration with a command and where to
   find the token in its output, **When** it is named on a target, **Then** it
   works exactly as a built-in one does.
2. **Given** a definition that reuses a built-in name, **When** configuration is
   loaded, **Then** the definition replaces the built-in and validation says so
   rather than silently preferring one.
3. **Given** a command that names `{host}`, `{port}`, `{user}` or `{database}`,
   **When** it is run, **Then** those are substituted from the resolved target
   as whole argument elements, never through a shell.

### User Story 3 - The route is unavailable, and says which part (Priority: P1)

The value of this story is entirely in the wording. Four problems, four
sentences, each naming what to do.

**Acceptance Scenarios**:

1. **Given** the provider's program is not on the path, **When** a connection is
   attempted, **Then** the client names the program and does not describe this
   as an authentication failure.
2. **Given** the program runs and fails, **When** it does, **Then** the client
   says the credential could not be obtained, shows the program's own reason
   redacted, and names the provider's usual remedy - `az login`, `aws configure`,
   `gcloud auth login`.
3. **Given** a token that the server rejects, **When** the connection fails,
   **Then** the client says the credential was obtained and the server refused
   it, and names the likely cause: an identity with no role on this server.
4. **Given** the program hangs, **When** the timeout passes, **Then** it is
   reported as a timeout and not as anything else.
5. **Given** any of these, **When** a message is produced, **Then** it contains
   no part of a token.

### User Story 4 - The token never travels in the clear (Priority: P1)

**Acceptance Scenarios**:

1. **Given** a target whose `sslmode` would permit an unencrypted connection,
   **When** a provider is named, **Then** the connection is refused **before the
   provider's command is run**, and the message says a bearer token may not be
   sent unencrypted. Nothing is fetched, so nothing is at risk.
2. **Given** a server that refuses TLS, **When** the connection is attempted,
   **Then** the existing behaviour holds: it fails, and nothing is retried
   without TLS.

### User Story 5 - No credential route silently substitutes for another (Priority: P1)

**Acceptance Scenarios**:

1. **Given** a provider is named and its command fails, **When** the connection
   would otherwise be attempted, **Then** it is not attempted at all, and the
   password prompt does not appear. Nothing a person can type will help, so
   asking them is a lie about what is wrong.
2. **Given** a provider is named, **When** a connection is opened by any path in
   the product, **Then** it carries the credential from that provider or it does
   not open. A route that forgot to fetch fails loudly rather than falling back.

### Edge Cases

- The tool is present but signed in to the wrong tenant, account or project: the
  server refuses and the reason must be legible.
- A managed identity, an instance profile or a service account, where the tool
  resolves an identity with no human involved. This works unchanged, because the
  route is "ask the tool", not "ask a person".
- The AWS built-in needs a region, which its own tool resolves from environment
  and configuration. When it cannot, the failure must say that is what happened.
- A user name that is a service principal, an application ID or a service
  account address. None of these is parsed or validated.

## Requirements

### Functional

- **FR-1101**: A target MUST be able to name an authentication provider, from a
  connection profile and from the command line.
- **FR-1102**: A provider MUST be defined by a program and its arguments, and by
  where the token is in that program's standard output - the whole of it, or a
  named field of a JSON object. Built-in definitions exist for Entra, AWS IAM
  and Google Cloud SQL IAM; configuration may add or replace any of them.
- **FR-1103**: A credential MUST be obtained per connection sequence and held
  only as long as the connections it opens. The second connection the object
  tree opens moments later carries the same one deliberately: it is seconds old,
  and running the provider twice for one session would double the latency of
  opening the client for no security gain. A future reconnection after a dropped
  connection MUST obtain a fresh one, because by then it may have expired; that
  path does not exist yet and is not built here.
- **FR-1104**: The user name presented to the server MUST be the one configured,
  passed through unaltered and unvalidated. It is never inferred from whoever
  the cloud tool happens to be signed in as, and its shape is never assumed:
  an address, a service principal, an application ID and a bare role are all
  just text. Connecting as somebody other than who was asked for is the kind of
  surprise principle I forbids, and every cloud spells identity differently.
- **FR-1105**: Each distinct failure - program absent, program failed, timed
  out, server refused the credential - MUST produce its own message naming its
  own remedy.
- **FR-1106**: A session opened this way MUST say so in its header and in
  `doctor` output, naming the provider and never the credential.
- **FR-1107**: The provider's command MUST have a timeout, and exceeding it MUST
  be reported as a timeout.
- **FR-1108**: `{host}`, `{port}`, `{user}` and `{database}` MUST be substituted
  into argument elements whole. No shell is involved at any point.

### Security

- **SEC-1101**: The credential is a bearer token. It lives in `SecretString`,
  never in an environment variable, a process argument, a log, a diagnostic, a
  history entry or a `Debug` output.
- **SEC-1102**: A credential MUST NOT be requested at all for a target that
  would permit an unencrypted connection, and MUST NOT be sent on one. The check
  happens before the program runs.
- **SEC-1103**: TLS is never downgraded and no attempt is retried without it.
- **SEC-1104**: The program's standard error is untrusted text and is redacted
  through the one existing implementation before display.
- **SEC-1105**: Configuration can name a program that this client will execute.
  That is inside the user's own trust boundary - it is their file, and anything
  able to write it can already run programs as them - but it is a capability the
  configuration format did not previously have, and the threat model says so.

### Exit codes

No new codes:

| Situation | Code |
| --- | --- |
| Unknown provider name, program absent, or `sslmode` forbids the route | 3, config |
| The program failed, or the server refused the credential | 5, authentication |
| The program timed out | 4, connection |
| TLS could not be established | 6, tls |

## Success Criteria

- **SC-1101**: The whole route is proven end to end against the disposable
  container, with no cloud account of any kind: a provider defined in
  configuration whose command prints the container's synthetic password. The
  subprocess runs, the token reaches `SecretString`, the server authenticates it
  and the session opens. This exercises the same code Entra will, which is why
  it is evidence rather than decoration.
- **SC-1102**: The same route against the TLS container, proving the transport
  requirement does not merely permit the connection but is satisfied by it.
- **SC-1103**: For a target that permits an unencrypted connection, the
  provider's command is never run. Proven with a provider whose command would
  create a file: the file does not exist.
- **SC-1104**: With the program absent, the failure names the program and exits
  3.
- **SC-1105**: No automated test requires a cloud account, a real cloud database
  or a network call to any cloud.
- **SC-1106**: Live evidence for each built-in provider is recorded separately,
  per provider, with the date and what it was run against. Until then the
  compatibility document says the mechanism is proven and the provider is not.

## Assumptions

- The cloud's own command-line tool is a reasonable prerequisite for the person
  using that cloud, and is never mentioned to anyone else.
- The tool caches and refreshes its own tokens. This client keeps no token cache
  and should not: a second cache would be a second thing to get wrong, and the
  token is short-lived by design.
- PostgreSQL authenticates a connection when it is made. An open session is not
  re-authenticated, so an expiring token ends the ability to open new
  connections rather than interrupting the one in hand. This assumption is
  written down because FR-1103 rests on it.
- The commands and their token lifetimes are the vendors' current documented
  ones, checked on 2026-08-16. They are defaults, not law: configuration can
  replace any of them without waiting for a release, which is the main reason
  providers are data rather than code.

## Sources

Checked 2026-08-16:

- [Use Microsoft Entra ID authentication in Azure Database for PostgreSQL Flexible Server](https://learn.microsoft.com/en-us/azure/postgresql/security/security-entra-configure)
- [IAM database authentication for MariaDB, MySQL, and PostgreSQL - Amazon RDS](https://docs.aws.amazon.com/AmazonRDS/latest/UserGuide/UsingWithRDS.IAMDBAuth.html)
- [Connecting to your DB instance using IAM authentication from the command line: AWS CLI and psql](https://docs.aws.amazon.com/AmazonRDS/latest/UserGuide/UsingWithRDS.IAMDBAuth.Connecting.AWSCLI.PostgreSQL.html)
- [Log in using IAM database authentication - Cloud SQL for PostgreSQL](https://docs.cloud.google.com/sql/docs/postgres/iam-logins)
- [gcloud sql generate-login-token](https://docs.cloud.google.com/sdk/gcloud/reference/sql/generate-login-token)
