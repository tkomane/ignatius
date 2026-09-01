# Compatibility

**Evidence reconciled on 2026-08-31.** This is the authoritative list of what
this build does and does not support. Where something is unsupported, the
program says so at the point of use rather than failing quietly. Individual
rows retain the date and evidence class that actually proved them.

## PostgreSQL server versions

Supported window follows upstream: **14 through 18**, per
<https://www.postgresql.org/support/versioning/> as read on 2026-08-15, where 18.6
was current and 13 had already reached end of life.

**Tested: 14, 16 and 18**, by the integration suite in CI on Linux, and 18.4
locally on macOS. Versions 15 and 17 are within the supported window but are not
exercised, so they are expected to work rather than known to.

## Authentication

| Method | Status |
| --- | --- |
| SCRAM-SHA-256 | Supported and tested |
| `password` (cleartext over the connection) | Supported by the driver, untested here |
| `trust` | Supported |
| Certificate | Supported and tested, with `sslcert` and `sslkey` |
| GSSAPI, Kerberos, SSPI | Not supported, and not planned. See ADR-0012 |
| Cloud token authentication | Supported. Mechanism proven against a real server; see the table below for what has been proven per cloud |

### Cloud identity providers

A cloud identity provider obtains a short-lived token by running that cloud's
own command-line tool, and presents it as the password. Named with `--auth` or
with `auth` in a profile. The transport must be encrypted: a target whose
`sslmode` would permit a plain connection is refused **before** the token is
requested, because a bearer token is usable by whoever sees it.

| Provider | Tool it runs | Mechanism | Live evidence |
| --- | --- | --- | --- |
| `entra` | `az account get-access-token --resource-type oss-rdbms` | Proven | None yet |
| `aws` | `aws rds generate-db-auth-token` | Proven | None yet |
| `gcp` | `gcloud sql generate-login-token` | Proven | None yet |
| Your own | Whatever `[auth.providers]` says | Proven | Not applicable |

"Mechanism proven" and "live evidence" are separate columns on purpose. The
route - run a program, read a token, present it as the password over TLS - is
tested end to end against a real PostgreSQL server with TLS, using a provider
defined in configuration. It is only tested with TLS because it refuses to run
without it. What has **not** happened is anyone running each built-in against
that cloud's own database. Until that is done and dated here, the built-in
commands are transcriptions of vendor documentation checked on 2026-08-16, and
no more than that.

The `aws` provider needs a region, which its own tool resolves from the
environment and from AWS configuration. When it cannot, the failure says so.

A provider's command is run directly, never through a shell. `{host}`, `{port}`,
`{user}` and `{database}` are substituted as whole arguments.

Credentials may come from a connection URI, a libpq keyword string, `PGPASSWORD`,
a `.pgpass` password file, or the prompt: when the server asks for a password and
none was found, both the full-screen client and plain mode ask for one, without
echoing it, and try again. Neither asks unless there is a terminal at both ends,
so a script fails rather than hanging. The OS credential store is rejected by
ADR-0011, and a connection profile never holds a password.

A password file that is readable by anyone but its owner is **not used**, and the
client says so and gives the `chmod` that fixes it. A file that is found but
matches nothing is also reported, because a password file that is quietly not
helping looks exactly like one that is.

## Transport security

| `sslmode` | Status |
| --- | --- |
| `disable` | Supported |
| `allow`, `prefer` | Supported, treated as `prefer`. No identity guarantee |
| `require` | Supported. Encryption only, no certificate verification |
| `verify-ca` | Supported. Chain checked, host name deliberately not |
| `verify-full` | Supported. Chain and host name both checked |

Trust roots come from the operating system unless `sslrootcert` is given, in
which case that root **replaces** the system store rather than adding to it,
which is what libpq does and what pinning an internal authority means.

Client certificates are supported through `sslcert` and `sslkey`. Both must be
given together; one without the other is refused rather than half-applied.
Encrypted private keys are not supported yet and say so.

Every one of these is tested against a server that really speaks TLS: verify-full
succeeding on a matching name, verify-full refusing a name mismatch, verify-ca
accepting that same mismatch, an untrusted chain being refused, and a client
certificate authenticating with no password at all.

Defaults: remote hosts get `verify-full`; loopback and Unix sockets get `prefer`,
decided from the literal address and never guessed from a name. A TLS failure is
never retried without TLS. The state shown is read back from `pg_stat_ssl`, and
is shown as unknown when that view is not readable.

## Connection parameters

**Applied**: `service`, `passfile`, `host`, `port`, `dbname`, `user`, `password`,
`sslmode`, `application_name`, `connect_timeout`.

**Environment variables read**: `PGHOST`, `PGPORT`, `PGDATABASE`, `PGUSER`,
`PGPASSWORD`, `PGAPPNAME`, `PGSSLMODE`, `PGCONNECT_TIMEOUT`, `PGSERVICE`,
`PGSERVICEFILE`, `PGPASSFILE`, `PGSYSCONFDIR`.

**Files read**: `pg_service.conf`, from `PGSERVICEFILE`, then
`~/.pg_service.conf`, then `$PGSYSCONFDIR/pg_service.conf`. `.pgpass`, from
`PGPASSFILE`, then `~/.pgpass` on Unix or `%APPDATA%\postgresql\pgpass.conf` on
Windows.

**Applied for TLS**: `sslmode`, `sslrootcert`, `sslcert`, `sslkey`, and their
`PGSSLMODE`, `PGSSLROOTCERT`, `PGSSLCERT`, `PGSSLKEY` equivalents.

**Refused, because ignoring them would weaken security**: `sslcrl`, `sslcrldir`,
`sslpassword`, `requiressl`, `channel_binding`, `gssencmode`, `krbsrvname`,
`requirepeer`, `sslsni`, and the `PG*` equivalents of those.

**Reported rather than ignored**: any other keyword in a connection string or a
service file that this build does not apply.

Precedence, highest first: command-line arguments, then a named profile or a
connection string, then the named service, then environment variables, then
built-in defaults. A profile and a connection target may not both be given: both
say where to connect, and choosing between them quietly would be worse than
asking. The password has its own order: the connection string, then `PGPASSWORD`,
then the password file, then the prompt.

**Profiles**: a `[profiles]` table in `config.toml` names a connection, reached
as `@name` or `--profile name`. A profile carries `host`, `port`, `dbname`,
`user`, `sslmode`, `environment`, `read-only`, `description` and `auth`, and nothing
else. It never carries a password.

## Differences from `psql`

This is not a `psql` replacement and does not claim meta-command compatibility.
Deliberate differences:

- **Command tags.** The driver exposes a row count rather than the full tag text,
  so a completed statement reads "3 rows affected" rather than "INSERT 0 3".
- **Statement boundaries** assume `standard_conforming_strings = on`, the default
  since PostgreSQL 9.1. Backslash escapes are honoured only inside `E'...'`.
- **NULL rendering.** `psql` shows an empty cell by default. This shows `[null]`,
  because a NULL must be distinguishable from an empty string and from the text
  `NULL`. In CSV and TSV, NULL is an empty field by default, matching `psql`, and
  `--null` changes it.
- **On error, execution stops.** Statements after a failure do not run, and the
  results produced before it are still returned.
- **No meta-commands.** No `\d`, `\dt` or `\copy` yet.

## Transaction state

Read from the server after each execution, at the cost of one extra round trip,
rather than inferred from the statements sent. Inference is wrong exactly when it
matters: after a server-side rollback, or an error raised inside a function.

`pg_stat_activity` cannot answer this question about the asking backend, which is
always `active` because it is running the question. The client compares the
transaction's timestamp with the statement's, which differ only inside an
explicit transaction block, and treats a probe that fails with SQLSTATE 25P02 as
the definition of a failed transaction.

## Production safety

A connection is classified only by `--environment`, never inferred from a host
name. When it is classified as production, anything that is not a read is held
back: refused in `query` unless `--allow-write` is given, and held for
confirmation in the client, where a statement that destroys data also needs the
database's own name typed.

The classification reads leading keywords. It handles comments and writes hidden
in a common table expression, and it treats anything it does not recognise as a
write. It cannot see inside a function, so `SELECT wipe_all()` is a read as far
as it can tell. **It is advisory and is described that way wherever it appears.**

`--read-only` is the real control: it asks the server to refuse writes for the
session, and PostgreSQL enforces it.

## Export

`--output` streams a result into a file. Supported formats: **csv, tsv,
ndjson**. json, markdown and the aligned table are refused, because each needs
the whole result before its first byte is correct and buffering it would give up
the bounded memory that makes an export worth having.

The destination is never written directly. Rows go to `<path>.partial`, which is
flushed, synced and renamed only when the export completes, so a file at the
destination is always a complete file. An existing destination stops the export
unless `--force` is given.

An interrupted export exits 9, leaves the partial file, and reports how many rows
reached it.

`Ctrl+K e` in the client writes **what is on screen** to a comma-separated file:
the rows the result kept, narrowed by the filter if one is on. That is smaller
than what the query returned whenever the result was truncated, so the prompt
says how many rows will be written and what the file will not contain before it
writes anything. To write the whole result, run the statement with `--output`,
which streams from the server and never holds it in memory.

## Terminals

| Terminal | Status |
| --- | --- |
| macOS, in a pseudo-terminal | Verified: full-screen render, Ctrl+R executed a query, Ctrl+Q restored the terminal, with the exact escape sequences captured |
| Warp on macOS, interactive | **Not yet verified by hand.** The pty run above used a forced size, not Warp's own renderer |
| Live terminal resize | **Not verified.** Covered only by a unit test on the resize message |
| Windows Terminal with PowerShell 7 | Builds and passes tests in CI. The full-screen client has not been opened there by anyone |
| Any VT-capable terminal on Linux | Builds and passes tests in CI. The full-screen client has not been opened there by anyone |
| `TERM=dumb` | Use `--plain`, which is a full client with no full-screen interface. The full-screen client refuses and says so |
| Not a terminal (piped) | Full-screen client refuses; `query` works normally |

Minimum size for the full layout is 80x24. Below that a single-pane layout is
used; below 40x8 a message states the current size, the minimum, and the
line-oriented alternative.

## Platforms

| Platform | Status |
| --- | --- |
| macOS on Apple silicon | Verified by hand, and by the CI macOS runner |
| Windows, GitHub `windows-latest` runner | Builds and passes unit, layout and CLI contract tests, plus a startup smoke test |
| Linux, GitHub `ubuntu-latest` runner | The same, plus the full PostgreSQL integration suite against 14, 16 and 18 |
| Any other architecture | Not built anywhere yet |

Claims are stated by runner rather than by target triple, because the runner is
what was actually exercised. Cross-architecture builds are release work and
nothing has been released. The target matrix below is the separate packaging
contract; its rows are not runtime or installation evidence until a workflow
run and the relevant platform checks are retained.

## Release target matrix

The non-publishing release workflow defines the first archive targets. A target
is not inferred from the host that verifies it, and a target outside this table
has no release artefact claim.

| Target triple | Archive format | Workflow runner | Evidence class | Current state |
| --- | --- | --- | --- | --- |
| `aarch64-apple-darwin` | `tar.gz` | `macos-latest` | Hosted build only | Run `33561901626` built the target at `247d1c7`, then failed before archive creation; no candidate artefact or installation result exists |
| `x86_64-pc-windows-msvc` | `zip` | `windows-latest` | Hosted build only | Run `33561901626` built the target at `247d1c7`, then failed before archive creation; no candidate artefact or installation result exists |
| `x86_64-unknown-linux-gnu` | `tar.gz` | `ubuntu-latest` | Hosted build only | Run `33561901626` built the target at `247d1c7`, then failed before archive creation; no candidate artefact or installation result exists |

Evidence classes have distinct meanings:

- **Automated packaging contract** means the source-controlled workflow names
  the target, builds it, records its identity, verifies its exact archive bytes
  and defines run-scoped files to upload. It does not prove that a hosted run
  succeeded, installation or hand use.
- **Hosted build only** means the target binary compiled on the named runner,
  but the archive and retained evidence steps did not complete. It is not
  packaging, installation or hand evidence.
- **CI platform evidence** means the existing CI runner built and tested the
  application on that runner. It does not prove that a release archive was
  produced for the exact target triple.
- **Hand evidence** means an operator used the client on the named platform and
  recorded the result. It is not supplied by a compile or a checksum.

Until a target has both the required packaging evidence and the applicable
installation and first-start evidence, it remains a candidate target rather
than a released support claim. Signing, provenance, publication and package
manager support are separate gates.

Unix-domain sockets are supported on Unix only; requesting one elsewhere fails
with an explanation. Owner-only file permissions are enforced on Unix; on Windows
files inherit the profile ACL and this program makes no stronger claim.
