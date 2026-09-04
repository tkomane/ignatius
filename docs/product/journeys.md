# Journeys

## First run, local database, target under two minutes

1. The user runs `ignatius` with a connection target.
2. The interface draws immediately, before the connection completes, showing
   `Connecting` rather than a blank screen or a spinner with no explanation.
3. The header fills in: environment classification, read/write posture, target,
   and the transport state the server reported.
4. The editor already contains a safe starter query and a comment naming the run
   key. There is nothing to type to see something happen.
5. Ctrl+R runs it. The results pane shows columns, values, row count and elapsed time,
   and the status reads `Completed`.
6. Ctrl+Q. The terminal is exactly as it was.

Nothing in that path asks for an account, offers a tutorial, requires a profile,
or asks about telemetry.

## When starting among named databases

1. The user runs `ignatius` without a target while `[profiles]` contains named
   connections. If they typed a target, profile or route option, the existing
   explicit path remains in force.
2. The first frame opens `Connection picker` before target resolution or
   authentication. `Use default connection settings` is first, followed by
   deterministic profile rows with location, database, role, TLS, environment,
   read-only and provider hints. No password, token or unknown field is shown.
3. Search is local. The user can type `prod`, `read-only` or a profile name;
   the editor and connection remain unchanged until Enter.
4. Enter sends only the selected profile name to the runtime. The existing
   resolver validates it, resolves the target, obtains any provider credential,
   and opens the connection. An invalid profile fails explicitly and does not
   fall back to default settings.
5. After a quiet session, `Ctrl+K n` opens the picker again. Switching removes
   the old result, tree, completion and server facts, while keeping the SQL
   buffer and local result-reading preferences. A running query or plan cannot
   be interrupted by an accidental switch.

## When the first frame is empty or blocked

The first frame does not depend on a remembered shortcut. A disconnected editor
says that a connection is required and that results appear after a query. A
connected empty editor says to type SQL, names the active run key, and points to
the command palette. Results and Objects use the same pattern: they distinguish
waiting, empty, filtered-empty and failed states, then name the next safe action.

Tab changes focus, and the footer follows it with no more than five useful
action/key pairs. The pairs come from the configured keymap, so a replacement
such as `F2` is what the screen displays. Escape closes a discovery overlay one
layer at a time without changing the buffer, selection, connection, history or
result.

When the user cannot remember an action, the command palette is searchable by
intent: `run`, `inspect`, `help`, `objects`, `save` and `result` are useful words,
not internal command names. An action that needs a connection or retained result
states that prerequisite instead of silently doing nothing.

## When SQL is hard to read

1. In the editor, press `Ctrl+Shift+F`, choose Format SQL from the palette, or
   press `Ctrl+K q`. The same local operation is available as `\format` in plain
   mode.
2. Clauses, predicates and eligible lists become readable while strings,
   quoted identifiers, dollar bodies and comments stay byte-for-byte exact.
   The cursor follows the same logical token, and the footer says whether the
   buffer changed, was already formatted, was empty, or was refused safely.
3. A changed buffer is one undoable edit. An ambiguous or over-limit buffer is
   left untouched and names the next action. No query, metadata load, history
   write, file write or network operation occurs; plain mode keeps the formatted
   SQL pending until the user enters a terminator.

## When one template needs rerunning

1. Keep SQL such as `SELECT ... WHERE customer_id = :customer_id` in the
   editor and press run. The production confirmation, if required, comes
   before the value prompt.
2. The full-screen prompt shows `Parameter n of total`, masks the value, and
   says that it is literal text. Enter accepts an empty value and advances;
   Escape cancels without sending anything.
3. Plain mode asks on stderr with no echo. If no terminal is available, it
   explains the `--param-env` route instead of hanging.
4. Scripts use `query -c ... --param-env name=VARIABLE`; mapping validation
   happens before target resolution and stdout stays data-only.
5. History keeps the template, repeated names bind the same value, and errors
   after expansion do not move the editor caret to a guessed location.

## When the result needs comparing or reshaping

1. The result pane says how many rows are retained, how many the server
   returned, and which source column is selected. The server-owned row order and
   values are still the source of truth.
2. With Results focused, `Ctrl+K g` opens a searchable result-grid palette.
   `Sort selected column` cycles through ascending, descending, and original
   order without rewriting SQL or sending another request. The selected source
   row remains selected after the reorder.
3. `Choose visible columns` opens a second searchable palette. Duplicate header
   labels remain separate because each entry includes its source position, and
   the last visible column cannot be hidden.
4. Width controls stay bounded. `Show or hide type labels` uses server-described
   names when available and says `unavailable` when they are not. `Freeze first
   visible column` keeps an identity column in view while the selected column is
   reached across a wide result.
5. Reset restores original server order, automatic widths, all columns, and the
   initial horizontal view. It keeps only the documented reading preferences.
   A new execution clears the old result shape automatically.

The grid is designed as a local view over retained rows. Filtering, the cell
inspector, the expanded row view, plain output, JSON, streaming export, history,
and the SQL editor do not inherit its local order or visibility choices.

## When one retained value needs to leave

1. Focus Results and select the text cell. `Ctrl+K c`, the inspector, and the
   command palette identify the same source row and column; printable `c` still
   types SQL in the editor.
2. The confirmation shows the row, column, UTF-8 byte count, and character
   count. It does not show the value. It also says that the terminal, SSH path,
   or multiplexer may observe or retain it.
3. Enter sends one OSC 52 write when `[clipboard] osc52 = true`; Esc sends
   nothing. NULL, a stale selection, a value over 1 MiB, a disabled setting, or
   an I/O failure says what happened and keeps the result available.
4. Success says only that the terminal sequence was written and flushed.
   Clipboard acceptance is unconfirmed because the client never reads or clears
   the destination. Use explicit export when a file is the intended boundary.

## When one retained cell needs a deliberate update

1. Run a direct single-table `SELECT` that includes the table's primary-key
   columns, focus Results, and press `Ctrl+K u` or choose the update action in
   the command palette. Printable `u` remains editor input outside the chord.
2. The client resolves the relation in the active session, checks that it is an
   ordinary or partitioned table, reads primary-key membership and verifies the
   current role can read and update it. Production-classified and server
   read-only sessions refuse before asking for a value.
3. Enter the replacement in its own prompt. Empty text is valid and is not a
   NULL shorthand. Joins, expressions, CTEs, set operations, views, missing or
   NULL keys, and primary-key cells are refused with a reason.
4. A separate review shows the resolved target, key identity and exact bound
   `UPDATE`, and says that nothing has been sent. Enter sends one parameterized
   statement. Esc sends nothing. The source result remains a snapshot and is
   never rerun automatically; run the `SELECT` again deliberately to read back.

## When a retained result needs a deliberate refresh

1. Focus Results after a completed result and press `F6`, choose `Refresh the
   retained result` from the command palette, or use the contextual footer. On
   a terminal with enhanced modifier reporting, `Ctrl+Shift+R` is also accepted.
   The action is offered only for a usable connection, a non-failed transaction
   and a retained result with a single read-classified source.
2. The client sends the retained source template, not the current editor
   buffer. A multi-statement, write, structural, destructive or unrecognised
   source is refused with a reason. A zero-row result is still eligible when
   its result set and source are retained.
3. If the source has named parameters, the existing masked prompt asks again in
   order. Values are not carried from the previous run. Until the prompt and
   execution are complete, the editor and prior result remain unchanged.
4. The Results title says `Refreshing retained result`, then reports the actual
   server outcome. A successful refresh replaces the result and resets its
   local view state under the ordinary execution rules. History keeps the safe
   template only, and no refresh starts because of focus, filtering, sorting,
   reconnect, cell-update completion or a prior failure.

## When retained rows need a file

1. Focus Results and press `Ctrl+K e`. A searchable format palette opens and
   says that the source is the retained result currently on screen, including
   the filtered scope and any truncation boundary. No file or partial file is
   created while choosing.
2. Search for CSV, TSV, JSON, NDJSON or Markdown and press Enter. The next
   prompt names the selected shape and asks for a path. A misleading or missing
   extension does not change the choice.
3. Type a destination and press Enter. The existing no-overwrite and atomic
   partial-file boundary applies, and the result stays available if the path is
   cancelled or refused.
4. For a script that needs executable data, request
   `--format insert --insert-table TABLE`, inspect the quoted statements, and
   apply them separately. Missing or ambiguous table and column information is
   refused; generated INSERT statements are never executed automatically.

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
When PostgreSQL reports a valid position, the editor becomes active, the error
line gets a visible `!` marker, the smallest known token is emphasized, and the
error states the statement, line, and column in words. The source remains
unchanged, so the person can correct it and run it again explicitly.

For a constraint failure, server-supplied schema, relation, column, and
constraint facts appear as object context before technical detail. The current
catalogue may add a type only after it matches the named relation and column;
unavailable or stale catalogue state is stated rather than guessed. If an error
arrives after the buffer changed, the old location is removed and labelled as
belonging to the previous submission.

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

## Journeys that remain open

Hand use of copy in Warp, Windows Terminal, Linux terminals, SSH and
multiplexers, use with a screen reader, live cloud-provider authentication
against each vendor service, and a readable query plan remain separate evidence
journeys. The OS credential store is not a planned journey: ADR-0011 rejected
it in favour of `.pgpass`, environment injection, connection strings, and the
prompt.
