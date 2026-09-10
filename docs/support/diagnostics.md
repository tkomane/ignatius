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
events without revealing the query. `IGNATIUS_LOG` accepts only `off`, `error`,
`warn`, `info`, `debug` or `trace`. Ignatius-owned events are redacted as complete
events before writing; dependency target directives are refused because database
driver debug traces may contain full SQL.

## Reading an error

Errors are layered on purpose:

```text
Query error: relation "orders" does not exist
  While: running statement 1
  Position: statement 1, character 15
  Location: statement 1, line 2, column 5
     2 | FROM orders
       |     ^
  Likely cause: the relation is not visible under the current search_path
  Next: qualify it with a schema, or check search_path in the status bar
  Details: 3 field(s) available, re-run with --verbose
```

The headline is the server's own message. `While` is what the client was doing.
`Position` preserves PostgreSQL's one-based character coordinate and `Location`
is the mapped editor coordinate when the submitted statement and buffer still
match. The source excerpt is sanitized and line-oriented; a missing, internal,
rewritten, or stale position is reported as unavailable rather than estimated.
`Likely cause` appears only when the SQLSTATE is one where a plain-language cause
can be stated honestly; it is omitted rather than guessed. `--verbose`, or Ctrl+D
in the client, expands SQLSTATE, severity, detail, hint, schema, table, column,
constraint and routine as the server sent them. Constraint object context is
shown before that expandable section, and JSON retains the numeric position,
statement number, object fields, and technical array without embedding SQL.

## Reading and reporting a result-grid view

The interactive result grid is a local view over retained rows. Its sort is
labelled `local` and says `retained rows only`; it never means PostgreSQL added
an `ORDER BY`, and it never fetches another page. A filtered or truncated result
continues to state what was retained and what the server returned.

`Ctrl+K g` opens the controls when Results has focus. The searchable chooser can
hide or restore source columns, adjust a bounded width, show server-described
type labels, freeze the first visible column, or reset the view. Duplicate names
are identified by source position. If a type description fails or is only
available for some columns, the grid says `unavailable` or `partial`; it does
not infer a type from displayed text.

If a result looks wrong after using these controls, press Reset before reporting
it and include the visible state words, terminal size, glyph tier, and whether
the result was filtered or truncated. The view does not alter plain, JSON,
streaming export, history, or editor behaviour, so report a scripted-output
problem separately from a grid-layout problem.

## Reading a cell-update refusal or review

`Ctrl+K u` is interactive-only and intentionally supports a narrow source shape:
one direct single-table `SELECT` with its primary-key columns projected. The
client resolves the live relation in the session search path and checks table
kind, read privilege, update privilege and server read-only posture. Joins,
expressions, CTEs, set operations, views, missing or NULL key values,
primary-key cells, production-classified connections and oversized generated
statements are refused before a write effect.

The replacement prompt is not a write. The following review shows the exact
bound `UPDATE` and says whether anything has been sent. Only Enter on that
review emits one parameterized update; Esc emits none. The prior result remains
a snapshot and is not refreshed automatically. If the result or selection
changes while metadata is loading or review is open, the candidate is discarded
as stale. Run the original `SELECT` again deliberately after a successful
update.

## Reading a retained-result refresh outcome

`F6` is the portable direct key for an interactive convenience over the source
attached to the completed result. `Ctrl+Shift+R` is an additional alias when
the terminal preserves enhanced modifier reporting. It is offered only for one retained, read-classified
statement with Results focused, an idle usable connection and a non-failed
transaction. The current editor buffer is not used. Multi-statement, write,
structural, destructive and unrecognised sources are refused before any
execution effect.

Named parameters are prompted again and their previous values are not reused.
The Results title says `Refreshing retained result` while the request is in
flight. The completion notice identifies success, cancellation, failure or an
unknown connection outcome, and says that nothing was retried. A successful
refresh replaces the result through the ordinary execution lifecycle and keeps
only the safe template subject to the usual history rules. Focus changes,
filtering, sorting, layout controls, reconnect and a prior outcome do not
trigger refresh.

## Reading a query plan

The plan pane is a structured PostgreSQL `EXPLAIN` response, not ordinary query
output. Plain `EXPLAIN` is labelled `estimate only`: planner costs are cost
units, not milliseconds, and the target statement is not executed. The summary
shows planning time when supplied and says `not measured` for execution time.

`EXPLAIN ANALYZE` is different by design. The confirmation says that the target
will execute, that side effects are possible, and that the client will not
automatically roll it back. After confirmation, actual rows, actual startup and total time
are shown only when PostgreSQL supplies them. Loop counts are shown when supplied too. Rows
and node times are per-loop values where PostgreSQL reports loops; the plan
labels an estimate mismatch when observed rows differ by at least 10x.

The attention marker says whether it uses estimated cost or measured time. A
plan that cannot be parsed, exceeds the 1 MiB response or 500-node display
bound, loses its connection, or fails on the server is shown as unavailable or
truncated, never as a successful plan. Esc closes a completed failure or plan
and restores the prior ordinary result without rerunning it. If a connection is
lost during analysis, treat the statement's outcome as potentially unknown and
check the session before retrying.

## Reading guided discovery

The first frame and empty panes are state-specific. `Not connected`, `Connecting`,
`Connection lost`, `Connection failed`, `Ready`, `No rows returned`, filtered
empty and failed-transaction wording are different facts, not interchangeable
empty messages. Each surface names a safe next action or says why none is
available yet.

The footer is a bounded contextual hint rail with no more than five complete
action/key pairs. Keys come from the active configuration, not from this
document's defaults. The command palette identifies itself, searches by intent,
and retains a prerequisite for actions that cannot act in the current state.

Discovery is interactive presentation only. Opening, searching, rendering or
dismissing it does not execute SQL, reload catalogue data, write files, record
history, transfer clipboard data, or create persisted onboarding state. A report
that mentions changed stdout, stderr, JSON, export, history or terminal
restoration belongs to the existing scripted or terminal contract, not to
guided discovery.

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
