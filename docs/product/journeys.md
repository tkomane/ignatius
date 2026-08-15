# Journeys

## First run, local database, target under two minutes

1. The user runs `ignatius` with a connection target.
2. The interface draws immediately, before the connection completes, showing
   `Connecting` rather than a blank screen or a spinner with no explanation.
3. The header fills in: environment classification, read/write posture, target,
   and the transport state the server reported.
4. The editor already contains a safe starter query and a comment naming the run
   key. There is nothing to type to see something happen.
5. F5 runs it. The results pane shows columns, values, row count and elapsed time,
   and the status reads `Completed`.
6. Ctrl+Q. The terminal is exactly as it was.

Nothing in that path asks for an account, offers a tutorial, requires a profile,
or asks about telemetry.

## When the connection fails

`connect --check` reports stages separately, so the answer is the first `FAIL`
line rather than a single opaque error:

```text
OK      target              app@db:5432/orders with sslmode=verify-full (encryption, a trusted certificate, and a matching host name)
OK      address resolution  db resolves to 10.0.0.5
FAIL    tcp                 10.0.0.5:5432 refused the connection: Connection refused
                            -> check that PostgreSQL is listening on this port and that a firewall allows it
SKIPPED postgres            skipped because the server could not be reached
```

Later stages are skipped rather than repeating the same failure in different
words.

## When a statement fails

The error replaces the results pane and is layered: the server's own message,
what the client was doing, the likely cause when one can honestly be named, a
next action, and a count of hidden technical fields with the key to expand them.
The server's reported position is rendered against the buffer with a caret.

The transaction case matters most: an aborted transaction says, in words, to run
`ROLLBACK`, because that is the only thing that will work next.

## When a query runs too long

Ctrl+C. The status immediately reads `Cancellation requested`, and the interface
keeps redrawing, because the request has been made but the outcome is unknown.
When the server confirms, the status becomes `Query cancelled by server`. Nothing
is re-run. Pressing cancel again sends nothing further and does not change the
wording, because nothing has changed.

## From the terminal into a script

The query that was being explored becomes the query that runs in CI, unchanged:

```bash
ignatius query "$DATABASE_URL" -f checks.sql --format json > result.json || case $? in
  4) echo "database unreachable" ;;
  7) echo "a statement failed" ;;
esac
```

Data on stdout, diagnostics on stderr, and an exit code that says which kind of
failure it was.

## Journeys that do not exist yet

Creating a named profile, unlocking a credential from the OS store, browsing the
object tree, searching history, and exporting a large result with a progress
report. Features 002 to 005.
