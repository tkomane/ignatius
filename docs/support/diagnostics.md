# Diagnostics

## What to send with a report

```bash
ignatius version --verbose
ignatius doctor --json
```

Both are safe to share. `doctor` prints the names of any `PG*` variables that are
set but never their values, because one of them is a password. Neither command
contacts anything except, optionally, the database target you name.

If the problem involves a connection, add:

```bash
ignatius connect --check "postgres://user@host:5432/db"
```

Its output contains the host, port, database and role you passed, and no
password. Review it before sharing if the host name itself is sensitive.

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
