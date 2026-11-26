# Compatibility

**Evidence reconciled on 2026-09-04.** This is the authoritative list of what
this build does and does not support. Where something is unsupported, the
program says so at the point of use rather than failing quietly. Individual
rows retain the date and evidence class that actually proved them.

## PostgreSQL server versions

The repository's declared support window is **14 through 18**. Review the
[upstream version policy](https://www.postgresql.org/support/versioning/)
before a release or support-window change; this declaration does not assert
which upstream minor version is current.

On 20 November 2026, upstream listed PostgreSQL 14.25 as its final PostgreSQL
14 release and recorded end of support on 12 November 2026. PostgreSQL 19.0
was released on 19 November 2026. The declared Ignatius compatibility window
remains 14 through 18. [PostgreSQL versioning policy](https://www.postgresql.org/support/versioning/)

**Hosted evidence: 14, 15, 16, 17 and 18**, through plain/TLS integration
contracts on Linux at source `52872ac` in
[CI run 33720098318](https://github.com/tkomane/ignatius/actions/runs/33720098318),
whose executed jobs were rechecked on 2026-09-04. Its PostgreSQL 18 Unix-socket
job also executed successfully. **Local evidence: PostgreSQL 14.24, 15.19,
16.15, 17.11 and 18.4 on macOS**, 53/53 plain/TLS integration tests on each
major against the integration branch on 2026-09-30 (see `docs/status.md`),
with an unconfigured socket gate. Fresh local checks on 25 October 2026 passed
155 scoped contracts per major over PostgreSQL 14.24, 15.19, 16.15, 17.11 and
18.6 using reviewed immutable fixtures on macOS arm64, including verified-TLS
parameter/privacy and session guards. Clean source was `e5389f7`, whose
application code matches the security candidate `f99fe75`. The latter also
passed a real PostgreSQL 18.6 socket test, three normal-quit PTY contracts and
the Rust 1.90 workspace check in Linux arm64 containers. See
[verification](../operations/verification.md#frozen-postgresql-and-linux-acceptance---2026-10-25)
for exact scope and retained evidence. These frozen results exclude subsequent
UI edits. The hosted baseline does not cover the newer local worktree;
rerun the applicable matrix against the source to be released. Native Windows
database and hand-terminal claims remain separate evidence requirements.

The structured JSON `EXPLAIN` workflow follows the PostgreSQL 14-18 plan shape:
one JSON envelope with a `Plan` root, ordered `Plans` children, planner cost
fields, and optional analysis metrics. Unknown server fields are ignored rather
than guessed. The local display is bounded at 1 MiB and 500 retained nodes.

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

The mechanism covers launching a provider, reading its token and presenting
it over required TLS. A configured provider exercised that route end to end
on a disposable PostgreSQL server. The live column records authentication
against each vendor's own PostgreSQL service, which requires controlled
accounts and databases. The built-in commands follow vendor documentation
checked on 2026-08-16. The installed-launch checks below cover the specified
Windows packages and runner.

The `aws` provider needs a region, which its own tool resolves from the
environment and from AWS configuration. When it cannot, the failure says so.

A provider's command is run directly, never through a shell. `{host}`, `{port}`,
`{user}` and `{database}` are substituted as whole arguments.

A successful provider must supply its complete standard output within 1 MiB
before whitespace trimming or JSON extraction. A larger stream causes an
authentication refusal. On failure, the client reports the exit status and
redacts complete standard error up to 64 KiB. If standard error exceeds that
limit, the diagnostic explains that it was discarded. Both output streams
continue draining through the existing provider timeout.

An exact name in `[auth.providers]` replaces the built-in of the same name.
Connection details use the selected entry at startup and after a profile
switch. A configured provider shows its executable and output format, with
arguments hidden and a lifetime defined by the provider.

On the Windows x64 build, the built-in `entra` provider tries native `az`
first. When the native program is missing at spawn, the standard MSI under
the Program Files known folder can supply its bundled interpreter if the
installed `wbin` directory is on PATH. The client launches that interpreter
directly and retains the existing timeout and TLS requirement.

On Windows Server 2025 x64, the account-free probe confirmed that the
automatic launcher reported the registered Azure CLI MSI version `2.93.0`
with `version --output json`. Bare `az` reported a missing program on the
same MSI search path. Other Windows package layouts and live Entra PostgreSQL
authentication remain unverified. [Hosted Windows fallback probe](https://github.com/tkomane/ignatius/actions/runs/45695897697/job/139493085072)

On Windows Server 2025 x64 image `20261116.315.1`, the built-in `aws`
provider directly launched the standard AWS CLI v2 MSI's `aws.exe`.
The account-free `--version` result `2.40.3` matched registered MSI
`2.40.3.0`. Live RDS or Aurora IAM authentication requires a controlled
database test. [Hosted Windows AWS launch probe](https://github.com/tkomane/ignatius/actions/runs/45705066295/job/139524815561)

On Windows Server 2025 x64 image `20261123.321.1`, the built-in `gcp`
direct launch returned a missing-program diagnostic on Google's verified
bundled-Python archive `594.0.1`. Its packaged `gcloud.cmd` reported
`594.0.1` in the independent account-free version check.
[Hosted Windows Google Cloud CLI probe](https://github.com/tkomane/ignatius/actions/runs/45711578601/job/139549559128)

On image `20261116.315.1`, the verified archive's bundled interpreter
directly launched `lib/gcloud.py` with fixed version arguments and reported
`594.0.1`. An automatic built-in route requires trusted package discovery
and a supportable startup contract. Live Cloud SQL IAM requires its own
controlled database test.
[Hosted bundled-interpreter check](https://github.com/tkomane/ignatius/actions/runs/45716059236/job/139562288603)

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

## Interactive connection picker

When profiles exist and the interactive client was started without a target,
profile flag, route option, or safety option, the first frame opens a searchable
picker. The first row is `Use default connection settings`; it uses the ordinary
CLI, service-file, environment and built-in precedence. The remaining rows are
the profiles in deterministic configuration order. Their detail is a bounded,
display-only summary of known fields: location, database, role, TLS mode,
environment, read-only posture, provider name, description and configuration
validation. Unknown profile fields are not copied into the model or rendered.

Reading or searching the picker performs no network call, provider command,
metadata load, history write or configuration change. Enter sends only the
selected profile name to the runtime, which reuses the existing profile resolver,
target resolver and cloud-auth boundary. A profile with an invalid or secret-
shaped field therefore fails through the existing diagnostic rather than being
silently treated as the default route. Explicit targets and options retain the
existing pre-terminal resolution path, so this is an interactive convenience,
not a new connection precedence rule.

`Ctrl+K n` opens the picker again after a connection is quiet. A running query,
plan or connection attempt keeps the picker unavailable. After a selection the
old server facts, retained result, object tree, completion catalogue, pending
prompts and trust surface are cleared; the editor, type-label preference and
frozen-first-column preference remain. Late work from the previous target is
discarded by a runtime connection generation. Plain and scripted commands do
not open or inherit this surface.

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
- **Bounded meta-commands (Feature 050).** Built-in interactive and plain-mode
  meta-commands (`\d`, `\dt`, `\di`, `\dv`, `\dn`, `\df`, `\du`, `\l`,
  `\conninfo`, `\timing`, `\x`, `\watch`, `\format`, `\?`) cover daily
  inspection and watch loops; full `psql` scripting parity (`\if`, `\gset`,
  arbitrary shell `\!`) remains intentionally out of scope.
  Switching connections stops a running watch. After a switch, bare `\watch`
  needs a completed query from the current connection. Server positions for
  bare watch and periodic iterations identify the watched query and preserve
  the editor caret and focus. The first execution of literal companion SQL
  can point into its original editor draft. Scheduled runs wait while a
  named-parameter prompt or run confirmation is open.

## SQL buffer formatting

The full-screen editor exposes local formatting through `Ctrl+Shift+F`,
`Ctrl+K q`, and the command palette. Plain mode exposes the same operation as
`\format` before the pending statement is terminated. The formatter is
PostgreSQL-aware but intentionally lexical and conservative: major clauses,
boolean predicates and eligible top-level lists receive deterministic,
idempotent layout, while string literals, quoted identifiers, dollar-quoted
bodies and comments retain their exact source bytes.

A changed buffer is one undoable edit and the cursor is mapped to a UTF-8-safe
logical position. Empty, comment-only, already formatted, malformed and
over-limit input receives a value-free outcome; ambiguous or over-limit input
is left untouched and names the next action. The source bound is 1 MiB of
UTF-8 bytes. Formatting does not contact PostgreSQL, load metadata, write
history or files, read the clipboard, or emit telemetry. In plain mode, the
formatted buffer remains pending and interaction text goes to stderr, while
stdout remains reserved for result data.

## Prompted named parameters

The full-screen and plain interactive clients support bounded named placeholders
such as `:customer_id`. A placeholder is recognised only in executable SQL,
outside strings, quoted identifiers, dollar-quoted bodies and comments; casts
such as `value::text` and existing `$1` text remain unchanged. Up to 64 distinct
names are accepted, in first-use order, and repeated occurrences share one
answer.

The full-screen prompt and plain mode ask for hidden values after any required
production confirmation. Empty text is valid, values are treated as literal
text data, and NUL is refused before the statement is sent. Values are escaped
at the existing simple-query boundary without type inference. Non-interactive
`query` never prompts: it requires one `--param-env NAME=VARIABLE` mapping per
name, reads UTF-8 values from the environment, and validates the complete set
before resolving a target. The template, not the answers, is retained in
history and editor state. A server position after expansion is shown as
technical information without a guessed template caret.

Runtime binding requires a fresh `SHOW standard_conforming_strings` response
of exactly `on`. With `off` or an unavailable response, the request refuses
before the requested SQL is sent; restore the setting separately. Buffered
execution rechecks before each separately sent statement and keeps an already
completed prefix once. Streaming checks before its unchanged whole Query
message, preserving PostgreSQL's initial parsing and implicit transaction.
User-SQL methods on one session serialize that check/bind/send interval;
metadata reads and cancellation remain independent. The read-only setting
probe adds a database round trip per separately sent parameterized statement,
and parameterized elapsed time includes it. No setting, transaction or statement
is changed or retried automatically.

For parameterized execution/export errors, server-controlled message, detail,
hint, context and object fields are omitted because they can contain original,
transformed or truncated values. The omission is explicit; classification,
validated SQLSTATE, statement ordinal and numeric server position remain
available with built-in recovery guidance. Ordinary query diagnostics keep
their existing detail. From the first parameterized send until disconnect,
server notices use fixed privacy wording and validated SQLSTATE only, including
delayed notices from completed or interrupted work. Deliberately returned query
and export rows remain output.

Saved queries are regular `.sql` files. Loading or saving a linked/non-regular
entry refuses; the list skips it. Named replacements use exclusive private
temporary files and atomic rename, as do configuration writes and history
trimming. Earlier interrupted temporary files are preserved for inspection.

## Reviewable cell-to-UPDATE

The interactive client supports a deliberately narrow result-cell write flow.
With Results focused, `Ctrl+K u` or the command palette accepts only a direct
single-table `SELECT` with direct projections, optional aliases or `*`. The
query backend resolves the relation in its effective PostgreSQL search path,
including its temporary schema. An explicit `pg_temp` qualifier resolves to
that backend's physical temporary schema. PostgreSQL metadata supplies the
relation kind, read and update privileges, columns and primary-key membership.
Only ordinary and partitioned tables with all primary-key columns
projected and non-NULL in the selected row are eligible; a primary-key cell is
not an editable target.

Joins, subqueries, CTEs, expressions, set operations, views, missing keys,
read-only sessions and production-classified connections are refused. The
query backend checks its effective transaction read-only setting before
replacement input, including explicit read-only transactions and changes
to the session default. A refusal preserves the caller's transaction. A
`FROM ONLY` source is also refused because the generated statement would not
carry `ONLY` and could reach inherited rows. The
replacement prompt accepts literal text, including empty text, and has a
separate review step. The review shows the exact bound `UPDATE` and says that
nothing has been sent. Enter emits one `ExecuteParameterized` effect; Esc emits
none. The template can be recorded in history, but replacement and key values
remain secret-bound until execution. The prior result is a snapshot and is not
rerun automatically. The generated statement is bounded at 16 KiB after
binding.

The review is not a lock. It neither locks the row nor proves that the value read
when the cell was selected is unchanged, and the generated statement matches only
the primary key. A concurrent change to a non-key column is overwritten and a
concurrent delete reports zero rows affected. Ignatius does not detect the
conflict, retry the update, or silently roll back; the row count the server
returns is the truthful outcome.

## Explicit retained-result refresh

The interactive client exposes `F6`, an enhanced-terminal `Ctrl+Shift+R` alias,
the command palette and the contextual Results footer for one deliberate
refresh of a completed result. Traditional terminals can encode
`Ctrl+Shift+R` as `Ctrl+R`, so `F6` is the portable direct key.
Refresh is available only with Results focused, a usable idle connection, a
non-failed transaction, and a retained result whose source is exactly one
read-classified statement. The retained source is used instead of the current
editor buffer, so editing SQL after a result does not change what this action
would send. Multi-statement, write, structural, destructive and unrecognised
sources are refused. Classification is advisory and does not replace
PostgreSQL permissions or a server read-only setting.

Named parameters are prompted again with the existing masked prompt, and prior
values are never reused. The request follows the ordinary execution, result,
history, cancellation and diagnostic boundaries, produces at most one
execution effect, and replaces the result only after the server responds. A
zero-row result remains eligible when its result set and source are retained.
Focus changes, filtering, sorting, layout controls, reconnect, cell-update
completion and an earlier refresh outcome never start a request. Failures and
connection loss are stated as actual or unknown outcomes without retry.
If PostgreSQL reports an error position, the diagnostic identifies the
retained query as its source and leaves the editor caret and focus in place.

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
write. `EXPLAIN` is a read, but `EXPLAIN ANALYZE` (in either the word form or the
parenthesised option list) executes its statement, so it is judged by that
statement: `EXPLAIN ANALYZE DELETE ...` is destructive. It cannot see inside a
function, so `SELECT wipe_all()` is a read as far as it can tell. **It is
advisory and is described that way wherever it appears.**

The query benchmark (`Ctrl+K '`) wraps each run in `BEGIN` and `ROLLBACK` by default. `m` switches
rollback mode and never runs anything by itself. With rollback off every run
commits, so a statement that is not a read is refused on a production
connection and in a read-only session. Query A and Query B run one after the
other and B is skipped if A fails or is cancelled. Each run holds the session for its whole
length, so workbench statements, `\watch` iterations and other benchmarks wait
for it rather than landing between its `BEGIN` and `ROLLBACK`, and it waits for
them. Before its first `BEGIN` the session asks the server whether it is inside
a transaction, and the benchmark refuses to start if it is, because its
rollback would end that transaction. `Ctrl+C` requests cancellation of a running
benchmark (`Cancellation requested for the benchmark. The server may still be
running.`) using PostgreSQL's cancel connection (waiting for the server to close
the cancel socket within a bounded timeout) only while the timed statement is
executing, never during `BEGIN` or `ROLLBACK`, and stops further iterations via
a per-iteration flag. In rollback mode, a cancelled or failing iteration always
executes `ROLLBACK` before releasing the session so nothing commits. Failed
cancel delivery keeps cancellation pending so `Ctrl+C` can be retried; the
benchmark overlay stays open (`Esc` waits) until the run reports (or the
connection is lost), so no benchmark can be left running unseen behind a closed
overlay. Quitting Ignatius stops later runs, but a statement already executing
may still finish on the server. An analysed plan comparison (`EXPLAIN ANALYZE` in plan
diff) takes the session the same way and refuses to start inside a transaction
for the same reason; its overlay can still be closed mid-run, the comparison
continues, and statements started afterwards wait for it. A workbench Cancel
pressed while a statement waits behind it cancels whatever the server is
running at that moment, which may be the comparison.

`--read-only` is the real control: it asks the server to refuse writes for the
session, and PostgreSQL enforces it.

## Query plans

`Ctrl+K l`, or the command palette's `Explain plan`, asks for structured plain
`EXPLAIN` for exactly the statement under the editor cursor. It does not execute
that statement, write history, or change normal result and machine-output
routes. The plan tree shows operation, relation or index context, estimated
rows, row width, startup and total planner cost units, and safe known facts.

`Ctrl+K a`, or `Analyze plan`, always asks for an explicit confirmation before
sending `EXPLAIN ANALYZE`. Analysis executes in the current session and
transaction, can have side effects, and is not automatically rolled back by
the client. Existing PostgreSQL permissions, transaction state, production
classification, and destructive database-name confirmation remain authoritative.
Actual startup and total time, rows, and loops are displayed only when supplied;
observed values are per-loop where PostgreSQL defines them. The slowest analyzed
node is marked using measured time, while a plain plan uses estimated cost.

Plan selection, expansion, collapse and dismissal are local presentation state.
They do not issue another request, change the editor, alter the retained result,
or add a history entry. A malformed, failed, connection-lost, or bounded plan
response remains an unavailable or incomplete plan with a recovery action.

## Server notice retention

After the driver connects, each query, export or plan observation window admits
at most 256 server notices and 1 MiB of retained UTF-8 severity/message/SQLSTATE
fields. The existing 64 KiB per-message shortening remains. Successful statement
drains do not replenish the whole-operation allowance; idle/metadata storage is
separately bounded and pending idle entries are charged to the next operation.
Later messages are counted as omitted rather than causing SQL replay, rollback,
automatic cancellation or a server-setting change. These are application
retention limits, not incoming-frame, aggregate-result or process-memory limits.

Client omission, shortening and abandoned-delivery counts use explicit exact,
lower-bound or unknown wording. Scripted formats keep stdout data unchanged.
JSON success adds one client status object on stderr when needed; a JSON error
keeps its existing single diagnostic object with additive `notice_report`.
Plain mode adds accounting before its next prompt/interaction or exit. A closed
or failed output stream does not establish that a report was visibly delivered.
Public payload-only stream/explain/connect compatibility APIs do not return
reports; first-party consumers use their report-carrying variants.

The TUI retains fixed pending counts and offers **Server notice retention** in
the palette. Its global badge yields to query/transaction facts and protected
reviews rather than reducing their 40x8 input/body/command geometry. Opening the
reader captures a snapshot and acknowledges nothing. Esc closes without
acknowledging; its active Enter/Activate command explicitly acknowledges only
that captured snapshot, with new loss kept pending. Insufficient reader geometry
refuses acknowledgement. Cancel keeps its existing running-query recovery path.
The latest source summary remains inspectable after acknowledgement. Known
counts from locally held opened attempts survive replacement, profile change
and ordinary quit, using captured identities; this does not include future or
racing server observations after the retirement boundary.
Counts cover observed messages, not complete server warning delivery or proof
that a person read them. Metadata/idle observations do not prove statement cause.

## Bulk import transactions

`ignatius import` and the interactive import wizard normally wrap COPY in an
Ignatius-owned transaction. The wizard uses the connected query backend.
Wrapped import requires autocommit and refuses before COPY if a transaction
is already active, failed or its state is unavailable. End a caller-owned
transaction before retrying. CLI import with `--no-transaction` leaves
transaction control with the caller.

`--dry-run` requires the wrapper and conflicts with `--no-transaction`. Its
row count is reported after PostgreSQL acknowledges rollback. COPY and commit
errors retain SQLSTATE when available and omit diagnostic text that could
contain input values.

For production-classified targets, the wizard shows the database,
destination and commit or rollback intent. Type the exact database name and
accept to run either mode. Each approval is specific to one import and one
query session. The CLI requires `--confirm-production` for these targets.

The wizard requires a completed preview. An unreadable source or invalid
UTF-8 preview clears cached data and shows the file error. If the file
becomes unreadable after preview, the wizard stops before COPY and returns
to the source step. A readable header-only CSV remains a valid zero-row
import.

## Export

`--output` streams a result into a file. Supported formats: **csv, tsv, ndjson
and insert**. json, markdown and the aligned table are refused for this route;
their current encoders use buffered results. Streaming avoids retaining the
entire result. Individual values, rows and driver frames still use memory;
there is no whole-process memory ceiling.

Buffered CSV, TSV, NDJSON, Markdown and INSERT keep stdout as data and report
one stderr line per truncated result set, using retained versus received rows.
Result-set ordinals exclude command-only statements. A successful prefix keeps
its disclosure before a later query error; the actual error/exit code is
preserved. Table and JSON already carry their in-band counts. Successful query
execution remains exit zero even when client retention is explicitly limited.

`--format insert` is deliberately explicit: it requires a non-empty
`--insert-table TABLE` and rejects `--no-header`. The table and every result
column are quoted as identifiers; NULL becomes SQL `NULL`; and every non-NULL
cell is emitted as an escaped PostgreSQL text literal. The client does not infer
source types. Duplicate column labels and NUL-containing text are refused
before output, and multiple result sets become separate INSERT statements.
`--insert-table` is refused with every other format. Review generated SQL before
executing it; the client writes data and never executes the statements it emits.

The destination is never written directly. Rows go to `<path>.partial`, which is
flushed, synced and renamed only when the export completes, so a file at the
destination is always a complete file. An existing destination stops the export
unless `--force` is given.

An interrupted export exits 9 and leaves the partial file. Progress counts only
complete row encodings accepted by the writer. When the partial-file flush
succeeds, the existing written-row count is reported. If it fails, the message
states that the file's row count is unconfirmed, separately from accepted-row
progress. Best-effort buffered recovery is preserved. A partial file may contain
an incomplete final row; inspect it before continuing. No database write outcome
or automatic retry is inferred from a file failure.

`Ctrl+K e` in the client writes **what is on screen** through a small
save-as flow. A searchable format palette offers CSV, TSV, JSON, NDJSON and
Markdown, names the retained and filtered row scope, and opens the path prompt
only after a format is chosen. The selected format wins even when the path has
no familiar extension; the client never guesses from the filename. No file is
created while the palette is open or while the path is being edited. For a
future complete streaming export, choose `--output` before execution. No
completed statement is automatically replayed to recover discarded rows.

## Interactive clipboard copy

`Ctrl+K c` copies one selected text cell from the retained interactive result.
The action is also available from the cell inspector and command palette. It
preserves source-row identity through local filtering and sorting, leaves
printable `c` as editor input, and never reruns SQL.

The route is opt-in: `[clipboard] osc52 = false` is the default, and
`[clipboard] osc52 = true` permits the confirmed value to travel through the
terminal as an OSC 52 write. Clipboard acceptance is unconfirmed after the
write and flush. It never reads or clears the destination, and the
terminal, SSH path, or multiplexer may observe or retain the value.

SQL NULL, stale selections, values over 1 MiB of UTF-8 bytes, disabled or
unsupported transport, and write failures send nothing and state the next
action. Empty text is distinct from NULL and is copied as zero bytes. This is
interactive-only: plain, CSV, TSV, JSON, NDJSON, history, export, and SQL
editing do not emit OSC 52 sequences or inherit the copy action.

## Interactive result grid

Feature 014 adds a local result view without changing the query protocol or the
scripted output contract. With Results focused, `Ctrl+K g` opens searchable
controls for:

- stable ascending or descending sorting of retained rows, with a cycle back to
  original server order;
- source-column visibility, including duplicate names identified by position;
- bounded per-column widths and a frozen first visible column for wide results;
- optional server-described type labels, with `unavailable` shown when the
  description cannot be accepted; and
- reset to automatic widths, all source columns, original order, and the initial
  horizontal position.

Sorting and column shaping never rewrite SQL, rerun a statement, fetch a page, or
alter `ResultSet.rows`. The selected source row remains the selected record
through a local reorder. Type labels are obtained after a successful row-bearing
execution by a parse/describe request; a failed or mismatched description leaves
values and success status unchanged. Partial metadata is represented as partial,
not inferred from text.

The local view is interactive-only. CSV, TSV, JSON, NDJSON, Markdown, table
output, streaming export, history, and the editor continue to use server order,
server values, and their existing contracts. The grid degrades to words and
ASCII separators when colour or Unicode styling is unavailable.

## SQL buffer formatting

The interactive editor supports local formatting through `Ctrl+Shift+F`,
`Ctrl+K q`, and the command palette. Plain mode exposes the same operation as
`\\format` while a statement is pending. The formatter is lexical and
conservative: it lays out major clauses, predicates, joins, and eligible lists,
but never changes keyword case, literal bytes, quoted identifiers, dollar-body
contents, comment contents, or SQL values. The output is deterministic and
idempotent, and one Undo restores the complete prior buffer.

Formatting is bounded to 1 MiB of UTF-8 source bytes. Empty, comment-only,
already-formatted, malformed, and over-limit input is left untouched. An
unterminated protected region or an over-limit buffer is refused with its kind,
location or size, and a next action; raw SQL is not included in the message.
The returned caret remains on a valid UTF-8 boundary and follows its token when
layout changes.

This operation is local-only. It does not execute SQL, contact PostgreSQL,
write files, record history, transfer clipboard data, alter JSON, NDJSON, CSV,
TSV, Markdown, table, or streaming export output, or add telemetry. In plain
mode, a successful `\\format` preview is written to the message stream while
stdout remains reserved for result data; the pending buffer is replaced only
after formatting succeeds.

## Guided discovery

Feature 015 is a presentation layer over the existing interactive model. The
first frame, empty and blocked states, contextual footer, and general command
palette derive from current focus, connection, query, result, object and history
state. The active configured keymap is the authority for every key shown; the
footer is bounded to five complete pairs and the palette can search by intent.

Discovery is not available as a plain-mode feature because plain mode already
has its own line-oriented contract. It does not add telemetry, cloud calls,
automatic queries or metadata reloads, clipboard transfers, or persisted
onboarding state. Opening, searching, rendering and dismissing discovery leave
SQL, result data, selection, connection, transaction and history unchanged.

This slice does not change CSV, TSV, JSON, NDJSON, Markdown, table, streaming
export, history, diagnostics, or terminal-restoration output. Automated rendering
and reducer evidence is separate from hand use in Warp, Windows Terminal, Linux
terminals and screen readers.

## Keyboard pane controls

The full-screen client supports session-local split adjustment with `Ctrl+K ,`
and `Ctrl+K .`, and zoom/restore with `Ctrl+K z`. Objects focus changes sidebar
width within 20-60 cells; Editor and Results focus change editor height while
preserving at least three outer rows for each main pane. The active keymap and
palette expose the same actions. See [keymap](../design/keymap.md).

Zoom fills the body while retaining header/footer. Focus moves the visible
zoomed pane; restore returns the saved split preferences exactly, with effective
rectangles clamped to the current size. Paging and completion anchoring share
the renderer's geometry. Errors and failed transactions restore the ordinary
layout outside Results focus, while Results can zoom its diagnostic/recovery
surface. Chords retain modal input ownership until their action is resolved.
At the minimum usable compact size, diagnostic text has a content row and
layout feedback preserves transaction facts without covering pane content.

Split/zoom preferences are never persisted. `[ui] density = "comfortable"`
(default) or `"compact"` selects title padding, the empty grid toolbar row,
footer hint caps (six/four) and preferred overlay clearance from one table.
Notices and the existing one-cell grid padding remain present. Unknown density
values are refused with configuration exit 3 before taking terminal modes;
there is no density CLI flag or runtime toggle. Mouse split-bar and column-edge
dragging are implemented with the same bounds as the keyboard routes (20..=60
sidebar columns, minimum 3 pane rows, 4..=64 result column cells).
Rendered-buffer checks and actual native terminal use remain separate
acceptance classes. Density itself does not change acceptance policy.

Clipboard, generated UPDATE, production-run, EXPLAIN ANALYZE and named-parameter
reviews now share scrollable content and a fixed input/command rail. Both
acceptance aliases require usable geometry and the current final page, followed
by existing typed-name/identity/posture checks. Home/End and page/move actions
stay within the review. Actual resizing resets it; acceptance that first
observes a changed backend size waits for repaint and another explicit key.
Below 40x8 or without two readable body rows, the client explains how to enlarge
or cancel and sends nothing. Late cell-update metadata cannot cover a newer
input/review owner. Feature 018 T037-T040 records implementation and acceptance.

Help keeps every active binding/chord and complete description reachable by
local scrolling, with a fixed navigation/close/palette rail. Long and Unicode
key labels wrap without colliding with descriptions. At narrow widths the rail
teaches page scrolling; first/last keys remain available. Ordinary typing/paste
cannot edit covered SQL. A cancelled palette or prefix returns to its help
position; a real command choice reveals its target, including clearing covered
reading panels before object insertion. `Space` is named visibly. Feature 015
T042 records local implementation/rendered checks; native hand use is separate.

## Long SQL lines

The editor follows the caret horizontally while its line-number/statement
gutter stays fixed. Completion anchoring and editor hit regions use the same
sanitised display-cell view, including wide and combining graphemes and complete
escaped-control representations. Source SQL, scalar cursor columns and undo
state are unchanged; there is no soft wrap or persistent scroll setting.

When text capacity is positive, an oversized or zero-width caret unit has a
selected blank fallback. Partial wide/control units at a viewport edge become
position-preserving blanks. No text capacity means no caret-visibility claim
or completion popup. This does not enable mouse dispatch. Feature025 FR-2529 /
T058 records scoped implementation and evidence; native emulator/font and
whole daily-use acceptance remain separate.

T059 bounds retained display-unit metadata to the visible window plus a caret,
while line width/scalar facts still scan the source. Sanitisation precedes the
ASCII fast path; Unicode and escaped controls retain the same mapping. Exact
styled-buffer parity and local large-line diagnostics are recorded in
[verification](../operations/verification.md#w10-long-line-renderer-diagnostic---2026-10-25).
Observed process RSS and renderer times are not memory ceilings or release p95.

## Manual themes and presentation

The manual-theme slice supports `dark`, `light`, `high-contrast` and `custom`
in `[ui] theme`. A single optional `[theme]` table inherits a built-in palette
and overrides exact named RGB tokens. The complete resolved palette is checked
even when inactive or colour is disabled. Invalid definitions refuse with
configuration exit 3 before terminal acquisition or configuration writes.
Existing ordinary configuration validation and migration behavior are retained.
See [theme tokens](../design/theme-tokens.md) for grammar and contrast scope.

The presentation layer paints surfaces across a three-level elevation ladder:
`Surface` (base canvas), `SurfacePane` (pane interior) and `SurfaceOverlay`
(dialogs, prompts, palette and completion popups). Four colour depths are
supported: `TrueColor` (24-bit direct RGB), `Indexed256` (quantized 256-colour
cube and grayscale), `Basic16` (standard ANSI 16-colour table) and `None`
(monochrome / modifier-only). Depth can be selected via `--color-depth` or
`[ui] color-depth`. At `Basic16` and `None`, background surfaces remain
unpainted so cells inherit the terminal's own background, with semantic meaning
carried through borders, text modifiers and words.

The Session / Switch theme palette entry cycles the validated catalog without
writing files or changing connection/query state. Direct bindings preserve input
owners; palette cancellation retains covered reading positions. The choice
survives resize and profile changes for the session. Rendered-buffer evidence
includes five palettes and colour-off/depth/glyph/density combinations; native
font, DIM, emulator appearance and hand use remain separate. Automatic background
detection via OSC 11 (`ui.theme = "auto"`, `T028`, `T028b`, `T032`) probes the
terminal in raw mode with a 100 ms deadline before alternate-screen entry,
chooses `light` when relative luminance is at least 0.5 and `dark` otherwise
(including on timeout), filters late replies on the input thread, and reports
the outcome in `ignatius doctor`.
The integrated manual source passed full local verification on 25 October 2026:
1,497 harness cases, actual PostgreSQL 18.6 plain/TLS and styled-buffer review.
The local macOS socket case is skipped. Published `d92c416` in draft PR35 passed
hosted Linux/macOS/Windows, pinned/MSRV and PG14/18 plain/TLS plus a real Linux
socket. Main-targeted CodeQL subsequently passed at `354fc82`.
[CodeQL run](https://github.com/tkomane/ignatius/actions/runs/44660626847)
T057 corrects the existing ignored `[ui] color` startup/doctor route. The final
gate includes that correction; actual owned macOS PTY startup/quit probes show
configured never emitting no painted SGRs, with explicit always/NO_COLOR controls.

## Terminals

| Terminal | Status |
| --- | --- |
| macOS, in a pseudo-terminal | Verified: full-screen render, Ctrl+R executed a query, Ctrl+Q restored the terminal, with the exact escape sequences captured |
| Warp on macOS, interactive | Installed signed Warp `0.2026.11.11.08.17.01` launched the `5e5712a` archive via temporary launch configuration on 21 November 2026 (`116x30`, `WarpTerminal`). Verified OSC 11 reply (`\x1b]11;rgb:0000/1313/1919\x07`, luminance `0.005359` -> `dark`), `doctor --json` (`11 ok`), live `verify-full` PG 18.7 `--plain` journey, and full-screen `SIGTERM` restoration of `stty -g` and OSC 11 (`ui.theme = "auto"`, `NO_COLOR=1`, `--ascii`, `ui.mouse = false`). Human interactive keyboard/mouse/visual hand use and cursor appearance remain open. See [Warp acceptance](../operations/verification.md#installed-warp-acceptance-pr-121-and-adr-0008---21-november-2026) |
| Warp Integration on hosted Apple silicon | The source-built renderer displayed the `5e5712a` archive's live read-only result over `verify-full` TLS. Ctrl+Q restored the shell and PTY modes. The recovered prompt showed a cursor, and the OSC 11 response matched before and after. [Renderer evidence](../operations/verification.md#cancellation-fix-candidate---20-november-2026) |
| Live terminal resize | The macOS PTY and automated Linux xterm verified compact and minimum-size layouts, then recovered retained results |
| Windows Terminal, Windows Server 2025 x64 | The archived client restored console modes and main-screen pixels in Windows Terminal 1.25.3131.0. All 24 standalone Guard cases restored the native modes, cursor, paste state and main screen. [Packaged renderer evidence](../operations/verification.md#packaged-windows-terminal-startup-and-quit---20-november-2026) and [native Guard matrix](../operations/verification.md#native-windows-bracketed-paste-restoration---20-november-2026) |
| Windows Terminal, Windows 11 ARM64 | Native Windows Terminal 1.25.3131.0 and PowerShell 7.6.10 ran the `5e5712a` x64 archive under emulation. The workbench returned a read-only `verify-full` result against loopback PostgreSQL 17.12. Ctrl+Q restored console modes, the main-screen crop, all 171 calibrated cursor pixels and the same PowerShell prompt. PowerShell 7 hand use remains open. [Current workbench evidence](../operations/verification.md#cancellation-fix-candidate---20-november-2026) and [cursor calibration](../operations/verification.md#calibrated-windows-11-arm64-cursor-restoration---20-november-2026) |
| Linux xterm 390 | On Ubuntu 24.04.5 x86-64 under Xvfb, the automated renderer displayed PostgreSQL 18.7 query results over verified TLS and over a separate Unix socket. Both runs verified resize recovery and terminal restoration. Linux desktop hand use remains open. [Renderer evidence and resize-gravity limit](../operations/verification.md#linux-xterm-renderer-and-restoration---19-november-2026) |
| `TERM=dumb` | Use `--plain`, which is a full client with no full-screen interface. The full-screen client refuses when stdout is not a terminal (`!is_terminal`, exit 2); when attached to a real TTY with `TERM=dumb`, the `5e5712a` build forces color off (`0` SGR color sequences) rather than refusing full-screen entry |
| Not a terminal (piped) | Full-screen client refuses; `query` works normally |

Minimum size for the full layout is 80x24. Below that a single-pane layout is
used; below 40x8 a message states the current size, the minimum, and the
line-oriented alternative.

Mouse capture defaults to on (`ui.mouse = true`) in the interactive client,
enabling click-to-focus, row selection, header sorting, double-click actions,
split-bar dragging and wheel scrolling. Setting `ui.mouse = false` disables capture
so the terminal owns every pointer gesture. Holding `Shift` while dragging
preserves the terminal's native text selection even when mouse capture is active.
Bracketed paste is enabled during full-screen sessions and cleanly restored on exit,
with pasted payloads capped at 1 MiB.

Unix SIGTERM routes to normal workbench shutdown while a key chord or modal
is active. The owned macOS PTY check confirmed a clean exit, mode restoration
and cooked input on the same terminal after a single signal.

On macOS, owned-PTY children prove error-return and caught-panic cleanup through
the actual terminal guard/hook for all mouse/paste/alternate-screen options.
The hook attempts acquired-mode restoration before previous-hook details and
reports failed cleanup truthfully. This is separate from native visual use, a
full uncaught product crash and Windows ConPTY acceptance. Crash details may
contain sensitive information and cannot prove a database write outcome.

At published `7d167a9` on 25 October 2026, the owned Windows ConPTY no-guard,
known-unrestored and acknowledged hung-child controls passed. The initial
Guard test observed raw/mouse and alternate-buffer transitions, then failed
the acquired cursor-visibility assertion. The separate `65cf237` calibration
observed native setters and main-screen VT. Its alternate-screen cursor
reading stayed constant. Matrix admission remained open at that stage.
[Initial ConPTY diagnostic](../operations/verification.md#w07-064-first-windows-capability-harness---2026-10-25)

## Themes and M12-M14 CLI subcommands

Built-in themes accept `"dark"`, `"light"`, `"high-contrast"`, `"field-industrial"`,
`"material"`, `"atelier"`, `"paper"`, or `"auto"` (with optional `[theme]` color token overrides in
config):

- `field-industrial`: dark matte slate surface palette with amber/orange signal
  tokens.
- `material`: Google Material Design 3 Tonal Dark palette with blue primary, cyan
  secondary, and sage semantic tokens meeting WCAG contrast thresholds across
  all four surface tiers.
- `atelier`: warm obsidian-espresso editorial dark palette (`#141210` / `#1B1917` /
  `#24211E` / `#2E2A26`) with terracotta focus (`#E6825C`), warm champagne
  selection (`#D8BE9A`), eucalyptus info (`#8EC5BC`), and antique gold accents,
  WCAG-verified across all four surface tiers.
- `paper`: warm archival rag-paper editorial light palette (`#EAE3D5` / `#F6F1E7` /
  `#E4DCCD` / `#DBD2C1`) with carbon ink (`#1E1B18`), Sienna focus (`#9A3115`),
  and Prussian slate selection (`#25486B`), WCAG-verified across all four surface
  tiers.

Non-interactive M12-M14 subcommands (`ignatius replication`, `cluster`, `browse`,
`plan-diff`, `analytics`, `bench`, `snapshot`, `runbook`) follow the repository
read/write safety contract:

- **Read vs write guards.** `replication`, `cluster`, `browse` reads,
  `plan-diff` (`ROLLBACK`-wrapped `EXPLAIN (ANALYZE, BUFFERS, WAL, SETTINGS)`),
  `bench` (default transaction rollback isolation unless `--no-rollback`, and
  refuses non-read statements against production targets in either rollback mode
  before credential fetch or connection unless `--allow-write` is passed),
  `snapshot capture|check`, and `runbook` (`--dry-run` or live mode without
  `--allow-writes`) refuse or isolate unintended mutations on target databases.
- **Deterministic exit gates.** Gate failures (`bench --fail-on-regression-pct`,
  `snapshot check --fail-on <breaking|warning|any|never>`, `runbook` write-guard
  refusal or `@expect-rows` mismatch) return exit code `1` (`ExitCode::Findings`)
  alongside structured `table`, `plain`, `json`, or `markdown` output.

## Structured values, plan history and fleet health

- [Structured Cell Detail](../design/structured-type-inspector.md) reads
  retained PostgreSQL text with described type metadata. JSONB, ARRAY and
  RANGE are built-in types. Spatial and vector support uses types owned by
  the installed PostGIS and pgvector extensions. The raw view remains available
  when a structured preview or edit is unavailable. Edits require a current
  direct result, a usable primary key and a writable non-production session.
- [Query plan history](../design/query-plan-history.md) captures planner
  estimates and optional read-only benchmark and EXPLAIN metrics. The local
  catalog retains 32 series of 64 observations. Machine-readable JSON and
  offline inspect and forget operations use the same catalog.
- `ignatius fleet` compares one baseline profile and up to three candidate
  profiles. PostgreSQL catalog snapshots, extension versions and a fixed
  settings allowlist use read-only sessions. A partial or unavailable target
  stays visible in the report. Drift and incomplete evidence return exit 1.

## Platforms

| Platform | Status |
| --- | --- |
| macOS on Apple silicon | Hosted tests and the source-built Warp renderer passed. Two exact archived builds replaced one another and rolled back in an isolated install path. Signed Warp hand use remains open. |
| Windows, GitHub `windows-latest` runner | Windows Server 2025 build/tests/startup/refusal passed. Native Windows Terminal and ConPTY restoration passed all 24 Guard cases. Windows 11 hand use remains open. |
| Linux, GitHub-hosted runners | Platform tests use `ubuntu-24.04`. PostgreSQL 14-18 and the Unix-socket contract run on `ubuntu-latest`. The archived x64 client passed automated install and rollback. Desktop hand use remains open. |
| Linux arm64, local containers | Clean `871db73` passed pinned Rust 1.97.1 library/socket/PTY checks and Rust 1.90 compilation; later CLI/export/help changes and native hand use remain open. |
| Any other architecture | Not built anywhere yet |

Historical W07 source `65cf237` separately completed three native ConPTY controls
and cursor calibration on image `win25-vs2026`/`20261021.278.1`. Its retained
sensor did not track alternate-VT visibility, so its original Guard cursor
assertion failed and its later startup smoke step did not run. The unchanged
assertion remains in the explicit-dispatch ConPTY diagnostic workflow; product
CI passing does not close native restoration acceptance.

Claims are stated by runner rather than by target triple, because the runner is
what was actually exercised. Cross-architecture builds are release work and
nothing has been released. The target matrix below is the separate packaging
contract; its rows are not runtime or installation evidence until a workflow
run and the relevant platform checks are retained.

## Release target matrix

The non-publishing release workflow defines the first archive targets. A target
is not inferred from the host that verifies it, and a target outside this table
has no release artefact claim.

The 20 November rehearsal bound source `5e5712a` and build `ci/44721590021-1`.
Archive and executable checksums, CycloneDX inventories, repeat builds and
same-path replacement and rollback checks passed for all three targets.
The two Windows repeat builds shared a runner. Each exact archive passed a
live verified-TLS query, hostname refusal and scoped log-privacy checks.
[Candidate review](../operations/release.md#current-candidate-review)
and [exact rehearsal](https://github.com/tkomane/ignatius/actions/runs/44721590021).

| Target triple | Archive format | Workflow runner | Evidence class | Current state |
| --- | --- | --- | --- | --- |
| `aarch64-apple-darwin` | `tar.gz` | `macos-latest` | Hosted packaging and runtime, native archive checks | macOS 27.2.1 arm64 verified TLS, cancellation recovery, and installed Warp OSC 11/`--plain`/mode restoration against PostgreSQL 18.7. A separate hosted Warp source renderer displayed a live result and restored the shell. Human hand use in installed Warp and VoiceOver auditory review remain open |
| `x86_64-pc-windows-msvc` | `zip` | `windows-latest` | Hosted packaging, runtime and live archive checks | Windows Server 2025 x64 verified TLS against PostgreSQL 17.12. Native Windows 11 Terminal restored its cursor and console modes after running the same x64 archive under emulation. Windows Terminal hand use remains open |
| `x86_64-unknown-linux-gnu` | `tar.gz` | `ubuntu-24.04` | Hosted packaging and runtime, emulated live archive checks | Ubuntu 24.04.5 x86-64 under emulation verified TLS, consecutive cancellations and PTY restoration against PostgreSQL 18.7. The archive requires `GLIBC_2.39` symbols. Linux desktop hand use remains open |

The earlier macOS archive at `24f1c8e` failed its privacy audit when debug
logging persisted synthetic SQL. The current probes use the newer archive
and a separately verified marker. [Earlier run](https://github.com/tkomane/ignatius/actions/runs/33563497933) and
[current archive probe results](../operations/verification.md#cancellation-fix-candidate---20-november-2026).

Evidence classes have distinct meanings:

- **Automated packaging contract** means the source-controlled workflow names
  the target, builds it, records its identity, verifies its exact archive bytes
  and defines run-scoped files to upload. It does not prove that a hosted run
  succeeded, installation or hand use.
- **Hosted build only** means the target binary compiled on the named runner,
  but the archive and retained evidence steps did not complete. It is not
  packaging, installation or hand evidence.
- **Hosted packaging evidence** means the named runner completed the archive,
  record, exact-byte sidecars, blocked readiness assertion and run-scoped
  upload, and an independent download passed those packaging checks. It is not
  runtime, installation, recovery, signature, provenance or publication
  evidence.
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

## Buffered retained-result data

Buffered Session ingestion applies one 33,554,432-byte logical occupied-data
allowance across all result sets in an execution. It charges actual native
ResultSet/name/type/row/Cell slots and UTF-8 payload before application-owned
copies. Descriptions and rows are admitted whole; the first data refusal closes
further data admission while PostgreSQL rows and CommandComplete are consumed.
Existing row caps and zero-row count-only mode remain separate. A row still
eligible for retention is refused if a cell exceeds 1 MiB. A CLI row-cap
override does not disable the 32 MiB allowance.

The driver can buffer incoming frames and queue batches before returning rows.
The allowance covers the current buffered execution and excludes driver
buffers, prior results, metadata caches, spare capacity and control vectors.
Public construction of synthetic result sets sits outside the allowance.
Streaming exports keep complete values and bypass the allowance.
[Operational resource bounds](../../specs/001-foundation-vertical-slice/contracts/operational-resource-bounds.md#incoming-driver-allocation-boundary)

An unavailable essential description is distinct from a known zero-row answer.
It retains only its typed reason, source-column count and delivered-row count.
JSON adds `data_unavailable: "retained_result_limit"` and `source_column_count`
only to that case, alongside empty columns/rows and `truncated: true`. Other
script formats emit no invented data and disclose unavailability on stderr;
NDJSON still refuses multiple result sets before writing. Available-result
encoding remains unchanged.

The TUI keeps a fixed execution-level loss summary, including later result sets,
and shows complete numeric facts or defers them until space is available. An
unavailable description refuses retained-data actions before I/O; a future
complete streaming export must be chosen before a deliberate execution. No SQL
is replayed or cancelled, and no setting or transaction state is changed because
of the client allowance. True errors, cancellation and unknown outcomes retain
their existing precedence.
