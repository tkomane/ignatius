# Compatibility

**Verified on 2026-08-15.** This is the authoritative list of what this build
does and does not support. Where something is unsupported, the program says so at
the point of use rather than failing quietly.

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
| Certificate | Not supported. `sslcert` and `sslkey` fail the connection |
| GSSAPI, Kerberos, SSPI | Not supported |
| Cloud token authentication (for example Entra ID) | Not supported |

Credentials may come from a connection URI, a libpq keyword string, `PGPASSWORD`,
or a `.pgpass` password file. The OS credential store and password prompting are
not implemented yet.

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
| `verify-ca` | **Not implemented.** Refused with an explanation |
| `verify-full` | Supported, using the operating system's trust store |

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

**Refused, because ignoring them would weaken security**: `sslcert`, `sslkey`,
`sslrootcert`, `sslcrl`, `sslcrldir`, `sslpassword`, `requiressl`,
`channel_binding`, `gssencmode`, `krbsrvname`, `requirepeer`, `sslsni`, and the
`PG*` equivalents of those.

**Reported rather than ignored**: any other keyword in a connection string or a
service file that this build does not apply.

Precedence, highest first: command-line arguments, the connection string, the
named service, environment variables, then built-in defaults. The password has
its own order: the connection string, then `PGPASSWORD`, then the password file.

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

## Terminals

| Terminal | Status |
| --- | --- |
| macOS, in a pseudo-terminal | Verified: full-screen render, Ctrl+R executed a query, Ctrl+Q restored the terminal, with the exact escape sequences captured |
| Warp on macOS, interactive | **Not yet verified by hand.** The pty run above used a forced size, not Warp's own renderer |
| Live terminal resize | **Not verified.** Covered only by a unit test on the resize message |
| Windows Terminal with PowerShell 7 | Builds and passes tests in CI. The full-screen client has not been opened there by anyone |
| Any VT-capable terminal on Linux | Builds and passes tests in CI. The full-screen client has not been opened there by anyone |
| `TERM=dumb` | Full-screen client refuses and points at `query` |
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
nothing has been released.

Unix-domain sockets are supported on Unix only; requesting one elsewhere fails
with an explanation. Owner-only file permissions are enforced on Unix; on Windows
files inherit the profile ACL and this program makes no stronger claim.
