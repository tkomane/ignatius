# Diagnostics

## What to send with a report

```bash
ignatius version --verbose
ignatius doctor --json
```

`version --verbose` contains build identity rather than user data. `doctor`
prints the names of any `PG*` variables that are set but never their values,
because one of them is a password, and is designed to omit credential values.
Review its path-bearing details before sharing. `version` never probes a
database; `doctor` probes one only when you provide an explicit target.

If the problem involves a connection, add:

```bash
ignatius connect --check "postgres://user@host:5432/db"
```

Its output contains the host, port, database and role you passed, and no
password. Review it before sharing if the host name itself is sensitive.

## Privacy-safe support identity bundle

Support needs to identify the exact installation and the failure boundary, not
the user's database contents. Start with:

```bash
ignatius version --verbose
ignatius doctor --json
```

Use the output to fill this small bundle, or attach it to the release record
when one exists:

```text
Product version:
Source revision and cleanliness:
Build identity:
Target:
Compiler:
Platform:
Archive basename and checksum, if applicable:
Terminal and terminal size, if relevant:
PostgreSQL version and transport posture, if the user chose to test a connection:
Error category and exit code:
Next action shown by the client:
```

The bundle must not contain a password, token, credential-file content, full
connection URI, SQL text, result data, or an unrestricted environment dump.
`doctor --json` is designed to omit credential values and reports names of
relevant environment variables only, but its directory checks can contain local
absolute paths. Replace home-directory and organisation-specific path segments
before sharing. Review host names, database names, role names and terminal
program names for local sensitivity as well.

If the problem is connection-specific, add a reviewed `connect --check` result
with a target that contains no password. It can distinguish address, TCP,
PostgreSQL, TLS and session-fact failures without exporting SQL or result data.
If logs are requested, provide only the relevant time window after review; do
not attach `config.toml`, saved queries or `history.jsonl` by default. Those
files are local support material and can contain environment-specific content.

## Native authentication and dependency failures

Feature 001a has no implemented native route yet. When that work is approved,
support reports must describe only the safe failure boundary: target, platform,
build or native dependency identity, error category, exit code and the next
repair action. Do not paste tickets, tokens, passwords, native connection
strings or raw provider diagnostics into a report. Provider text is untrusted
and must be redacted and terminal-sanitised before it is shown or logged.

A missing or incompatible native dependency must be reported as a repairable
dependency failure, not silently retried through a weaker authentication route.
This guidance records the privacy and failure contract; it does not claim that
the dependency, authentication provider or packaging route has been selected.

## Logs

Off by default. To capture one:

```bash
IGNATIUS_LOG=debug ignatius query "$DATABASE_URL" -c "SELECT 1"
```

The file is under the `Logs:` path from `ignatius config paths`, is capped at
5 MB with one previous file kept, and contains no SQL text and no row values. A
statement appears as a job id and a character count, which is enough to correlate
events without revealing the query.

## Reading an error

Errors are layered on purpose:

```text
Query error: relation "orders" does not exist
  While: running statement 1
  Likely cause: the relation is not visible under the current search_path
  Next: qualify it with a schema, or check search_path in the status bar
  Details: 3 field(s) available, re-run with --verbose
```

The headline is the server's own message. `While` is what the client was doing.
`Likely cause` appears only when the SQLSTATE is one where a plain-language cause
can be stated honestly; it is omitted rather than guessed. `--verbose`, or Ctrl+D
in the client, expands SQLSTATE, severity, detail, hint, schema, table, column,
constraint and routine as the server sent them.

## Exit codes

| Code | Meaning |
| --- | --- |
| 0 | Success |
| 2 | Usage error: bad arguments |
| 3 | Configuration error |
| 4 | Connection failed: could not reach the server |
| 5 | Authentication failed: reached the server, credentials rejected |
| 6 | TLS failed under the requested policy |
| 7 | The server reported an error for a statement |
| 8 | Cancelled |
| 9 | Export interrupted; a `.partial` file may remain |
| 70 | Internal error. Always a defect worth reporting |

These are a stable contract. Changing a value requires a major version.

## What a crash message tells you

A panic restores the terminal first, then prints where the log would be and
confirms that nothing was retried and nothing was written. The Rust backtrace is
not shown in the normal interface; set `RUST_BACKTRACE=1` to get it when
preparing a report.
