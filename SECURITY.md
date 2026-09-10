# Security

## Reporting a vulnerability

Do not open a public issue. Use GitHub's private vulnerability reporting for
this repository: <https://github.com/tkomane/ignatius/security/advisories/new>.
A report there is visible only to the maintainer.

Include what you did, what happened, what you expected, and the version from
`ignatius version --verbose`. Please do not include real credentials or real data
in a report; synthetic examples are enough to reproduce anything this project
does.

Expect an acknowledgement within seven days and an assessment within thirty. If a
fix is warranted, you will be credited unless you prefer otherwise.

## What this project protects

- Database credentials never appear in output, logs, diagnostics, `Debug`
  representations, or error messages, including when a connection fails.
- A TLS failure is never followed by an unencrypted retry, and the transport
  state shown is what the server reports rather than what was requested.
- A connection parameter that would weaken confidentiality or identity checking
  and that this build does not implement fails the connection rather than being
  ignored.
- Values and identifiers from the database are escaped before display, so a
  hostile value cannot emit terminal control sequences.
- Configuration is written atomically with owner-only permissions where the
  platform enforces them, and contains no secret values.
- Ignatius has no telemetry, update check or query-upload service. A selected
  cloud credential provider runs its own authentication tool, which may contact
  the cloud. Confirmed, opt-in OSC 52 can send one value through the terminal,
  SSH or multiplexer path. Those boundaries are documented in
  [the runtime context](docs/architecture/context.md).

## What it cannot protect

Stated plainly, because a security document that only lists strengths is
marketing:

- **Anyone who can read your process memory can read your credentials.** No
  userspace program can prevent this.
- **Process arguments are visible to other users on the machine.** A password in
  a URI on the command line is exposed to them and to your shell history. The
  client warns and offers safer routes; it cannot undo the exposure.
- **`PGPASSWORD` is visible to other processes.** It is consumed when present and
  never displayed, but the environment itself is not a secret store.
- **Redaction is pattern-based.** It recognises URI userinfo and libpq keyword
  forms. It cannot recognise a bare secret pasted into prose.
- **SQL classification is advisory.** Guardrails around destructive statements
  are a usability feature, not a security boundary. PostgreSQL permissions,
  transaction controls and server-side read-only settings are authoritative.
- **On Windows, owner-only file permissions are not enforced by this program.**
  Files inherit the profile's ACL, which is weaker than the Unix guarantee.
- **Nothing is signed or notarised.** Do not treat any artefact as authenticated
  until this document says otherwise.

## Supported versions

Nothing is released, so nothing is supported. This section will list supported
versions and their end dates once there is a release.
