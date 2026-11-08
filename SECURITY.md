# Security

## Reporting a vulnerability

Do not open a public issue. Use GitHub's private vulnerability reporting for
this repository: <https://github.com/tkomane/ignatius/security/advisories/new>.
A report there is visible only to the maintainer.

Include what you did, what happened, what you expected, and the version from
`ignatius version --verbose`. When environment, configuration or transport
diagnostics are relevant, also include `ignatius doctor --json` (redacted of
private filesystem paths or connection targets if needed). Please do not
include real credentials or real data in a report; synthetic examples are enough
to reproduce anything this project does.

Expect an acknowledgement within seven days and an assessment within thirty. If a
fix is warranted, you will be credited unless you prefer otherwise.

The maintainer owns triage: identify the affected version or source revision,
assess confidentiality and correctness impact, record a safe synthetic
reproduction, and coordinate the fix and affected verification. Keep the report
and any accidentally supplied data private. Public disclosure, release timing
and changes to access or reporting controls require the owner's decision.
The acknowledgement and assessment targets above do not promise a resolution
date. The reporting setting was confirmed enabled on 25 October 2026;
submission and notification handling have not been tested by sending a report.

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

## Severity triage and response targets

Security reports and data-loss defects are triaged ahead of feature work while
preserving the exact affected build identity (`version`, `git_sha`, `build_id`,
and `target`). Ignatius uses a 4-tier severity rubric:

- **Critical**: Remote or local credential/secret disclosure, secret exfiltration,
  or unauthenticated TLS/transport bypass. Target initial response: within 48 hours;
  targeted advisory and patch candidate: within 7 days.
- **High**: Query or result privacy leakage across boundaries, inadvertent logging
  of SQL or cell payloads, or improper authorization bypass during write operations.
  Target initial response: within 5 days; targeted patch candidate: within 14 days.
- **Medium**: Local confidentiality boundary weaknesses, unexpected environment
  exposure, or local availability defects causing persistent inability to run queries.
  Target initial response: within 7 days; targeted patch candidate: within 30 days.
- **Low**: Minor diagnostic disclosure without secret exposure, non-critical terminal
  restoration defects, or cosmetic inconsistencies. Triaged during normal release
  maintenance cycles.

## Trust-key revocation and rollback handling

In the event of a compromised build workflow identity, a revoked Sigstore signing
certificate, or a defective release candidate, Ignatius enforces a strict fail-closed
posture:

1. **Catalogue revocation**: The candidate record in the release catalogue is immediately
   marked with status `blocked` or `withdrawn`, recording the revocation reason in its
   blockers list.
2. **Fail-closed verification**: Both `cargo xtask release trust verify` and
   `cargo xtask release check` refuse revoked, rotated or unverified signing bundles,
   blocking release readiness across all tooling.
3. **Advisory publication**: A private or public security advisory is published
   documenting the affected build identity and revocation details.
4. **Operator rollback**: Operators are guided to immediately roll back to the last
   verified build following the rollback runbook in `docs/operations/release.md`,
   preserving local configuration and query history without data loss.

## Supported versions

Nothing is released, so nothing is supported. This section will list supported
versions and their end dates once there is a release. See
[docs/support/compatibility.md](docs/support/compatibility.md) for the validated
platform and PostgreSQL server matrix (PostgreSQL 14-18 across macOS, Linux, and
Windows).

