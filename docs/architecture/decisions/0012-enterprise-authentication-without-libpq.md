# ADR-0012: Stay on tokio-postgres, and reach Entra ID with a token

- Status: Accepted. Supersedes ADR-0009, which superseded ADR-0003.
- Date: 2026-08-16
- Related: ADR-0004 (TLS and trust), ADR-0007 (secret handling),
  `specs/001a-libpq-migration/`, `specs/011-entra-authentication/spec.md`

## Context

ADR-0009 accepted a migration to libpq on 2026-08-15, for five capabilities the
hand-written adapter did not have: `.pgpass`, `pg_service.conf`,
`sslmode=verify-ca`, client certificates, and enterprise authentication. It was
accepted on the owner's answer that those capabilities are needed.

Four of the five were then implemented natively, each with evidence against a
real server. Only "enterprise authentication" remained, and the project has been
reading that phrase as GSSAPI, Kerberos and Windows SSPI, because that is what
libpq means by it.

On 2026-08-16 the owner named the actual requirement: Microsoft Entra ID
authentication against Azure Database for PostgreSQL Flexible Server, for a
production application. That is a different mechanism from the one this project
has been costing.

## The finding

Microsoft Entra authentication to Azure Database for PostgreSQL Flexible Server
is an OAuth 2.0 access token presented **as the password**, on an ordinary
password-authenticated connection over TLS. The user name is the Entra
principal. The token is obtained from Entra for the OSS RDBMS resource
(`https://ossrdbms-aad.database.windows.net`, or `--resource-type oss-rdbms`
through the Azure CLI) and is valid for between 5 and 60 minutes. Checked
against Microsoft's own documentation on 2026-08-16; sources at the end.

It is not GSSAPI. It is not Kerberos. It is not SSPI. It needs no driver
capability that `tokio-postgres` does not already have, because presenting a
password over a TLS connection is the thing it already does.

So the one remaining justification for libpq does not apply to the requirement
that prompted it.

## Decision

1. **The PostgreSQL adapter stays on `tokio-postgres`.** ADR-0009 is superseded.
   Feature 001a is closed without implementation, and its planning package is
   kept as the record of a decision that was correctly gated and then answered
   by evidence rather than by effort.
2. **Entra ID is implemented as a credential route, not a driver change.**
   Ignatius acquires an access token and presents it as the password on a
   connection that must be TLS. Specified in `specs/011-entra-authentication/`.
3. **GSSAPI, Kerberos and Windows SSPI remain unsupported**, and stay named in
   `docs/support/compatibility.md` as unsupported rather than approximated, per
   principle II. If a real Kerberos requirement appears, this decision is
   reopened on the same terms: the capability must be needed by something
   actually being connected to, not anticipated.
4. **The token is acquired by running the Azure CLI**, not by Ignatius speaking
   to Entra itself. `az account get-access-token --resource-type oss-rdbms`
   already handles sign-in, tenant selection, device code, managed identity and
   its own token cache. A direct MSAL or IMDS route inside Ignatius would be
   the product's first network call that is not a database connection, which
   principle III says needs its own specification and threat-model update
   first. That route is deliberately not taken here.

## What this buys back

Each of these was a cost ADR-0009 accepted, and each is now simply not paid:

- **`unsafe_code = "deny"` stays crate-wide with no exception.** ADR-0009's
  scoped FFI exception is withdrawn along with it.
- **The binary stays self-contained.** This is not academic: the artefact built
  for hand-verification on 2026-08-16 is a statically linked musl binary that
  runs on any x86-64 Linux with nothing installed. Under ADR-0009 that artefact
  could not exist, and "install and run" would have changed meaning on Windows.
- **No concurrency ADR is needed.** ADR-0010 exists to answer a question libpq
  would have raised; it becomes moot and is marked so.
- **Type rendering, cancellation, TLS behaviour and the diagnostics contract are
  not disturbed**, so no regression surface is opened for a capability that was
  never the one required.

## Consequences, stated plainly

1. **A bearer token is a credential with a clock on it.** It is a secret in
   every sense the constitution means: `SecretString`, never an environment
   variable, never a log line, never in history, redacted anywhere displayable.
   It differs from a password only in expiring, which makes it less dangerous if
   leaked and no less dangerous to leak.
2. **Ignatius now depends on an external program for one credential route.** If
   `az` is absent, not signed in, or pointed at the wrong tenant, the failure
   must say which of those it is and what to run. A route that fails obscurely
   is worse than a route that does not exist.
3. **The route requires TLS and may never proceed without it.** Azure requires
   encrypted connections, and principle IV forbids a silent downgrade in any
   case. Presenting a bearer token on a connection that turned out to be plain
   would hand the token to anyone on the path, so this is refused rather than
   warned about.
4. **This is where the product beats `psql` rather than matching it.** psql
   cannot take the token at its prompt - the token is longer than the password
   its prompt accepts - so Microsoft's own instructions are to put it in
   `PGPASSWORD`. That places a live bearer credential in the environment of
   every child process and in shell history, and it expires mid-session with a
   failure that reads like a network fault. Ignatius acquires it for one
   connection attempt, keeps it out of the environment entirely, and when it
   expires says so in those words. The security improvement and the usability
   improvement are the same change.

## Alternatives considered

- **Migrate to libpq anyway, for future Kerberos.** Rejected. It pays every cost
  in ADR-0009 now for a capability nothing being connected to requires. The
  gate in ADR-0009 was written precisely to stop this.
- **Implement token acquisition natively with MSAL.** Deferred, not rejected. It
  removes the `az` prerequisite and is the better end state for a distributed
  build, but it adds a network call, a browser-based device flow and a token
  cache of our own. Each needs specification and a threat model. Shipping the
  `az` route first makes the requirement real before that cost is taken on.
- **Tell the user to run `az` themselves and paste the token.** Rejected. That
  is `PGPASSWORD` with extra steps, and pasting a bearer token into a terminal
  is exactly the leak this ADR is trying to close.

## Sources

Checked 2026-08-16:

- [Use Microsoft Entra ID authentication in Azure Database for PostgreSQL Flexible Server](https://learn.microsoft.com/en-us/azure/postgresql/security/security-entra-configure)
- [Microsoft Entra authentication in Azure Database for PostgreSQL Flexible Server](https://learn.microsoft.com/en-us/azure/postgresql/security/security-entra-concepts)
- [Connect with managed identity in Azure Database for PostgreSQL Flexible Server](https://learn.microsoft.com/en-us/azure/postgresql/security/security-connect-with-managed-identity)
