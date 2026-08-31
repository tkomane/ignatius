# Threat model

**Written: 2026-08-15**, covering Feature 001. **Feature 001a addendum added:
2026-08-16.** The addendum is a decision-gated design review for a possible
native PostgreSQL adapter; it is not evidence that the adapter, its dependency
or an enterprise route exists. Revisit before any change that adds a credential
route, a network call, or a way to write files.

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
| A cloud identity token is sent on a connection that turns out not to be encrypted | The token is never requested for a target whose `sslmode` would permit plain text; the check runs before the program does, and the transport rules are otherwise unchanged | None for the fetch. A user may still choose `require`, which encrypts without checking identity |
| A cloud provider's command line is built from a hostile host, user or database name | The command is an argument vector run directly with no shell; substitutions replace whole elements | None known. A provider whose own tool re-interprets an argument is that tool's problem |
| A cloud tool's error text carries a credential or terminal escapes into a message | Its standard error is redacted through the one existing implementation before display | Redaction is pattern-based, as everywhere else |
| A token reaches a log, the history, or a process listing | It exists only in `SecretString`, is never an environment variable and never an argument; the history's credential rule is unchanged | Same residual as any other secret in memory |
| Configuration names a program that this client then executes | Inside the user's trust boundary and stated as such: the file is theirs, owner-only on Unix, and anything able to write it can already run programs as them. Definitions are validated when configuration loads, not at the moment of connecting | **Real where the boundary is weaker.** On Windows the configuration file relies on the profile ACL, so a machine where that is loose gains a new way to be abused rather than a first one |

## Explicit non-goals

- Defending against the user, or against anyone with the user's privileges.
- Defending against a compromised operating system, terminal emulator, or a
  debugger attached to the process.
- Preventing the user from choosing weak transport security deliberately, once
  they have been told plainly what it means.
- Guaranteeing that a secret is gone from memory. Zeroing is best-effort and no
  userspace program can promise more.

## Feature 008 release evidence trust path

The current Feature 008 release path is a non-publishing evidence chain:

```text
source revision and lockfile -> build identity -> exact target archive
  -> SHA-256 sidecars -> release record -> retained evidence -> operator review
```

The chain makes the facts and their boundaries visible, but it does not create
trust by itself. Source control, a local build, a workflow record and a checksum
are separate evidence classes. A checksum proves the exact archive bytes and
basename; it does not prove signing, provenance, platform installation or
publication. The release record must keep product version, source revision,
working-tree state, build identity, target, archive size and digest bound to the
same candidate.

This boundary follows ADR-0008 and the provider-neutral contracts in
`release-evidence/README.md` and `release-evidence/inventory.md`.
The schema-1 allowlist in `release-evidence/scope.schema.json` constrains the
staging file set; it is separate from secret scanning, signing, provenance
verification and publication.

Current retained evidence may contain safe identity and gate facts such as the
product version, source revision, target, compiler, archive basename and size,
digest, build identity, verification result and explicit status values. Signature
and provenance states such as `not-configured`, `unsigned`, `missing`, `invalid`,
`mismatch` and `expired` remain blocking as applicable. `declared`, `observed`
and `blocked` evidence levels do not substitute for verification; a `verified`
level requires the exact binding checks.
It must not contain passwords, tokens, SQL text, result rows, configuration
files, environment dumps, unrestricted logs or credential-bearing connection
strings. Local, hosted and hand evidence remain distinct, and a candidate
remains blocked while the inventory, signature, provenance or owner
authorization gates are absent. Only verified signature and provenance states
may contribute to `ready`, and `published` additionally requires explicit owner
authorization and live publication evidence.

The current release workflow is deliberately unsigned, unattested and
non-publishing. Until the unresolved decision gates are resolved and an
approved signing, provenance and SBOM trust path is evidenced, the residual
supply-chain risk remains explicit rather than being hidden behind a successful
build or checksum.

## Feature 001a addendum: native adapter and enterprise credentials

### Status and boundary

ADR-0012 closed Feature 001a without implementation after the owner identified
Entra token authentication as the actual enterprise requirement. No native
dependency, FFI boundary or loader was introduced. The controls below are
retained as the historical gate that would apply if a real GSSAPI, Kerberos or
Windows SSPI requirement reopens the decision; they are not current release
prerequisites.

If the migration is approved, native-driver knowledge remains inside
`src/postgres`. The reducer, query model, UI, plain mode, configuration schema
and diagnostics contract remain outside the FFI boundary. The native library,
the PostgreSQL server and enterprise authentication infrastructure are external
inputs and must be treated as untrusted or semi-trusted according to the table
below.

### Additional assets

| Asset | Where it lives | Why it matters |
| --- | --- | --- |
| Native library and its code-signing identity | Platform installation or release artefact | It executes with the client's privileges and can read credentials and query data |
| Native connection handles and buffers | Adapter process memory | Incorrect ownership or lifetime can cause memory corruption, leaks or cross-session disclosure |
| Enterprise tickets, tokens and delegated credentials | OS authentication subsystem, native handles and process memory | They can grant access beyond the current database session |
| Native loading and packaging metadata | Installer paths, environment and release evidence | A wrong library, architecture or search path can execute unintended code or fail misleadingly |
| Cancellation and worker lifecycle state | Adapter worker and session state | An incorrect outcome can replay SQL, abandon a transaction or leak a live handle |

### Additional trust boundaries

- **Native library boundary.** The client calls code outside the Rust safety model.
  Its ABI, version and ownership rules are not inferred from a successful link.
- **Platform loader boundary.** The operating system resolves a library and its
  transitive dependencies. A writable or unbounded search path is an execution
  path, not a convenience.
- **Enterprise authentication boundary.** Kerberos, GSSAPI or SSPI may consult
  tickets, agents, domain controllers or credential caches outside the client.
- **Release artefact boundary.** A bundled library and its metadata become part
  of the distributable trust chain and must be tied to the exact source and
  target.
- **Session actor boundary.** One owner must serialize operations on one native
  connection. No other task, reducer or UI component may manipulate its handle.

### Additional abuse cases and required controls

| Abuse case | Required control before implementation | Residual risk until evidence exists |
| --- | --- | --- |
| A user-controlled or writable directory causes an unintended native library to be loaded | Define an explicit, bounded loading path per platform; reject ambiguous discovery; record the selected path and library identity in diagnostics and release evidence | The platform-specific route is not selected; T010 remains open |
| The library has the wrong architecture, ABI or version | Validate architecture and required symbols before opening a database connection; fail with the affected platform and repair action | No clean-machine or missing-dependency evidence exists |
| An FFI wrapper uses a handle after close, frees it twice or leaks it on an error path | Confine `unsafe` to the adapter, document ownership invariants beside every wrapper, and test success, error, cancellation, connection loss and shutdown cleanup | The wrapper and its safety review do not yet exist |
| Two operations manipulate one native connection concurrently | Give each connection one serialized owner and reject or queue other operations according to the approved back-pressure rule | The actor design is proposed, not spiked or owner-approved; T011 remains open |
| Cancellation returns an unknown outcome and the client replays the SQL | Request cancellation once, drain the server result state, preserve unknown outcome wording and never automatically replay | Exact wrapper cancellation semantics are unverified |
| A ticket, token or connection string appears in logs, diagnostics or test failure text | Reuse the single redaction path, use non-printing secret types, prohibit raw native diagnostics from crossing the boundary, and include adversarial redaction tests | Enterprise routes and their provider-specific diagnostic shapes are unknown |
| An authentication failure silently falls back to a weaker route or TLS posture | Preserve the requested authentication and TLS guarantee; fail before connection or state the server-confirmed posture; never retry with weaker settings | The approved enterprise route and compatibility mapping are unresolved |
| A missing or incompatible native library falls back to the old driver while claiming enterprise support | Fail before an enterprise connection attempt with one repairable diagnostic, and label the existing driver as the non-enterprise baseline | Packaging and fallback policy require T008 and T010 decisions |
| A bundled dependency is replaced or mismatched after release | Tie the dependency inventory, checksum, target, provenance and artefact to one release record; verify before publication | Signing, provenance and clean-install evidence are not yet available |
| Shutdown leaves a worker, ticket-bearing handle or connection alive | Make shutdown ownership explicit, drain or terminate according to the approved lifecycle, and test normal exit, error, cancellation, panic and supported signals | Cross-platform native shutdown and ConPTY evidence remain open |

### Required data-handling rules

- Enterprise credentials, tickets, tokens and native connection strings are
  secrets even when a provider calls them metadata. They must use the existing
  non-printing secret boundary and never enter release evidence or support
  bundles.
- Native diagnostic text is untrusted input. It must pass through the existing
  redaction and terminal-sanitisation paths before it is displayed or logged.
- A support identity bundle may include product version, source/build identity,
  target, native dependency identity and error category, but not a ticket,
  token, password, SQL text or result value.
- No loader, authentication provider or packaging step may add telemetry,
  update checks, query upload or result upload. Any future network activity
  beyond the chosen database connection requires a new specification and threat
  model review.

### Traceability and evidence gate

| Feature 001a requirement | Threat-model control | Evidence required before the task can be closed |
| --- | --- | --- |
| SEC-1001 | Secret types, one redaction path and adversarial native diagnostics | Redacted provider fixtures and subprocess/log assertions with no credential material |
| SEC-1002 | FFI confined to `src/postgres`, with per-wrapper ownership invariants | Static boundary review, safety review and success/error/cancel/shutdown lifecycle tests |
| SEC-1003 | No authentication or TLS downgrade and no automatic replay | Server-backed refusal, cancellation and connection-loss tests with exact outcome wording |
| SEC-1004 | Bounded loader path, architecture/symbol validation and explicit missing-dependency failure | Clean macOS, Windows and Linux installation evidence plus dependency identity and mismatch tests |

These controls were prerequisites for an implementation that ADR-0012 rejected.
The former T008, T010 and T011 gates were never approved, and no Phase 3 native
source task is authorized by this retained addendum.

## Verification

Every mitigation above that is claimed as done has a test. The mapping is in the
traceability table in `specs/001-foundation-vertical-slice/plan.md`, under
SEC-001 to SEC-006 and REL-002. Anything without a test is listed as residual
risk rather than as a mitigation.
