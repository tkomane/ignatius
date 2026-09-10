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

**Hosted evidence: 14, 15, 16, 17 and 18**, through plain/TLS integration
contracts on Linux at source `52872ac` in
[CI run 33720098318](https://github.com/tkomane/ignatius/actions/runs/33720098318),
whose executed jobs were rechecked on 2026-09-04. Its PostgreSQL 18 Unix-socket
job also executed successfully. **Local evidence: PostgreSQL 18.4 on macOS**,
including recorded later feature runs in `docs/status.md` with an unconfigured
socket gate. The hosted baseline does not cover the newer local worktree;
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
- **No meta-commands.** No `\d`, `\dt` or `\copy` yet.

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

## Reviewable cell-to-UPDATE

The interactive client supports a deliberately narrow result-cell write flow.
With Results focused, `Ctrl+K u` or the command palette accepts only a direct
single-table `SELECT` with direct projections, optional aliases or `*`. The
relation is resolved in the current session's search path, then PostgreSQL
metadata supplies its kind, read and update privileges, columns and primary-key
membership. Only ordinary and partitioned tables with all primary-key columns
projected and non-NULL in the selected row are eligible; a primary-key cell is
not an editable target.

Joins, subqueries, CTEs, expressions, set operations, views, missing keys,
read-only sessions and production-classified connections are refused. A
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

## Export

`--output` streams a result into a file. Supported formats: **csv, tsv, ndjson
and insert**. json, markdown and the aligned table are refused, because each
needs the whole result before its first byte is correct and buffering it would
give up the bounded memory that makes an export worth having.

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

An interrupted export exits 9, leaves the partial file, and reports how many rows
reached it.

`Ctrl+K e` in the client writes **what is on screen** through a small
save-as flow. A searchable format palette offers CSV, TSV, JSON, NDJSON and
Markdown, names the retained and filtered row scope, and opens the path prompt
only after a format is chosen. The selected format wins even when the path has
no familiar extension; the client never guesses from the filename. No file is
created while the palette is open or while the path is being edited. To write
the whole result, run the statement with `--output`, which streams from the
server and never holds it in memory.

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
| `aarch64-apple-darwin` | `tar.gz` | `macos-latest` | Hosted packaging plus native macOS installation and recovery evidence, rejected by privacy audit | Run `33563497933` packaged exact revision `24f1c8e`. The archive passed record, byte, scope, header, installed-path identity, plain and verified-TLS PostgreSQL 18.4 checks, full-screen pseudo-terminal start/quit, source-revision replacement, state preservation and controlled rollback on arm64 macOS. It is not release-acceptable: `IGNATIUS_LOG=debug` persisted full synthetic SQL through dependency tracing. The corrected build has local modified-worktree evidence only. |
| `x86_64-pc-windows-msvc` | `zip` | `windows-latest` | Hosted packaging evidence | Run `33563497933` packaged exact revision `24f1c8e`, and the downloaded archive passed record, byte, scope and PE32+ x86-64 header checks; Windows runtime, PowerShell, PATH, upgrade and rollback evidence remain open |
| `x86_64-unknown-linux-gnu` | `tar.gz` | `ubuntu-latest` | Hosted packaging evidence | Run `33563497933` packaged exact revision `24f1c8e`, and the downloaded archive passed record, byte, scope and ELF x86-64 GNU/Linux header checks; Linux runtime, installed-path, upgrade and rollback evidence remain open |

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
