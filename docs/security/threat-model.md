# Threat model

**Written: 2026-08-15**, covering Feature 001. Revisit before any change that
adds a credential route, a network call, or a way to write files.

## Scope

A local, single-user command-line program that connects to PostgreSQL servers the
user chooses. There is no server component, no account, and no network activity
other than the database connection.

## Assets

| Asset | Where it lives | Why it matters |
| --- | --- | --- |
| Database credentials | Process memory, environment, connection strings | Direct access to the data |
| Connection metadata | Configuration, process arguments, logs | Reveals infrastructure |
| SQL text | Process memory, terminal scrollback | May embed keys or personal data |
| Result data | Process memory, terminal, exports | The data itself |
| Clipboard contents | The OS clipboard | Readable by other applications |
| Local configuration | `~/.config/ignatius` | Trusted input to the program |
| Log and diagnostic output | `~/.local/share/ignatius/logs` | Often shared for support |

## Actors and trust boundaries

- **The user.** Trusted. The program acts on their behalf and does not defend
  against them.
- **Other local users and processes.** Semi-trusted. They can read process
  arguments and the process list; on a shared machine they may be able to read
  files whose permissions are wrong.
- **The PostgreSQL server.** **Untrusted input.** Whoever can write to the
  database controls values, identifiers, notices and error text that this program
  renders. A compromised or hostile server sends whatever it likes.
- **The network between here and the server.** Untrusted.
- **Dependencies and the release channel.** Semi-trusted, and the supply chain is
  the least controlled surface in this list.

## Abuse cases and what is done about them

| Abuse case | Mitigation | Residual risk |
| --- | --- | --- |
| A value contains terminal escapes that repaint the screen or forge a prompt | Every server-supplied string is escaped before display, including column names (`query::value::sanitize_for_display`) | Escaping is by code point; a terminal with a non-standard control channel is out of scope |
| A value uses bidirectional overrides to display in a misleading order | Bidi overrides and isolates are escaped | None known |
| An attacker on the network reads or alters traffic | Remote targets default to `sslmode=verify-full` with the platform trust store; TLS failure is never retried without TLS | `prefer` and `require` give no identity guarantee. Both say so in words |
| A user believes `require` verifies identity | The two are never described identically; the negotiated state is read from `pg_stat_ssl` | The user can still choose `require` deliberately |
| A parameter like `sslrootcert` is silently ignored, so verification is weaker than asked | Unsupported security parameters fail the connection | None; this is the deliberate trade |
| A password leaks into a log, error, or screenshot | One redaction implementation; secrets in types that do not print; diagnostics redact on construction | Redaction is pattern-based and cannot recognise a bare secret in prose |
| A password is exposed in the process list or shell history | Documented; `PGPASSWORD` produces a note offering a safer route | **Real and unmitigated.** Credential-store support is Feature 002 |
| Another local user reads the configuration file | Owner-only permissions on Unix; no secret values in the file at all | Windows relies on the profile ACL, which is weaker |
| A support bundle carries data the user did not intend to share | Logs exclude SQL text and row values by default; logging is off by default | Terminal scrollback is outside the program's control |
| A compromised dependency or release artefact | Committed lockfile, pinned toolchain, advisory and licence checks in CI | **Real.** Nothing is signed or notarised, and this is stated wherever artefacts are mentioned |
| A hostile server sends a huge result to exhaust memory | Rows are streamed and dropped past an explicit cap while still being counted | A single enormous value is still held in memory once |
| A hostile server sends an enormous error or notice | Rendered as text through the same escaping path | No length cap on a single message yet |

## Explicit non-goals

- Defending against the user, or against anyone with the user's privileges.
- Defending against a compromised operating system, terminal emulator, or a
  debugger attached to the process.
- Preventing the user from choosing weak transport security deliberately, once
  they have been told plainly what it means.
- Guaranteeing that a secret is gone from memory. Zeroing is best-effort and no
  userspace program can promise more.

## Verification

Every mitigation above that is claimed as done has a test. The mapping is in the
traceability table in `specs/001-foundation-vertical-slice/plan.md`, under
SEC-001 to SEC-006 and REL-002. Anything without a test is listed as residual
risk rather than as a mitigation.
