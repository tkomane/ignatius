# Changelog

The single source of release notes. Every stable or preview version maps to
exactly one entry here, and a rebuild never creates a new version.

Format: `New`, `Improved`, `Fixed`, `Security`, `Known limitations`, so the same
source can generate user-facing notes.

## Unreleased

- A real editor for the SQL buffer. The cursor moves up and down with a
  remembered column, to the start and end of a line or the buffer, by word, and
  by a screenful of whatever the pane can actually show. Delete works forwards,
  backwards and by word. There is undo and redo, one word at a time, including
  for text loaded over the buffer. A new line keeps the indentation of the one
  it left, and the window follows the cursor through a buffer of any length.
- Two ways to read a value the grid can only abbreviate. `Ctrl+K x` lays the
  selected row down the screen, one column per line, the way `psql`'s `\x`
  does, so a thirty-column row is readable on an eighty-column terminal. Enter,
  or `Ctrl+K i`, opens the selected cell in full: what the value is in words,
  every character of it wrapped and scrollable, and the line breaks the value
  actually has. It is the one place SQL NULL, an empty string and the text
  `NULL` are told apart by reading rather than by knowing the convention.
- The help overlay now lists the chords as well as the direct keys, and every
  chord is reachable by name from the command palette.
- A plain, line-oriented client behind `--plain`: no alternate screen, no raw
  mode, no cursor addressing, and nothing that only makes sense to an eye. It
  works in `TERM=dumb`, stays in the scrollback, and can be driven by a pipe.
  The prompt carries the database, the production marker and the transaction
  state as words, and a write to production is confirmed in words too.
- Transaction state, read from the server after every execution rather than
  inferred from the statements sent. It sits in the status line, and a failed
  transaction takes over the results pane to say that nothing else will run
  until ROLLBACK ends it.
- Production-aware safety. A connection classified as production holds back
  anything that is not a read until it is confirmed: `--allow-write` in a
  script, a prompt in the client, and the database's own name typed out for a
  statement that destroys data. The prompt states that the judgement is advisory
  and that database permissions are the real control.
- `--read-only`, which asks PostgreSQL to refuse writes for the session. That is
  enforced by the server rather than guessed at from the SQL.
- Streaming export with `--output`. Rows go from the wire to the file without
  passing through memory: 200,000 rows exported in 13 MB of resident memory.
  An export writes to a `.partial` file and only moves it into place when it is
  complete, never replaces an existing file without `--force`, and on
  interruption keeps what it wrote and says how many rows that was.
- Password files in PostgreSQL's `.pgpass` format, including wildcards, escapes
  and first-match-wins ordering, so a password need never appear on a command
  line. A file that others can read is refused, with the `chmod` that fixes it.
- Service files in `pg_service.conf` format, selected by `service=` or
  `PGSERVICE`, so an existing shared setup works here unchanged. An unknown
  service name lists the ones that exist.
- An object tree: schemas, their object groups with counts, the objects
  themselves, and the columns of relations with type, nullability and primary key
  membership. Children load when a node is opened, and a node waiting for an
  answer says so.
- A command palette over both commands and database objects, matching by
  subsequence so three characters find what you meant. Choosing an object inserts
  its schema-qualified, quoted name where SQL is written.
- A chord popup: `Ctrl+K` lists every key that can follow it, and nothing is on a
  timer.
- Breadcrumbs to the selected object, icons for every object kind, and a filter
  over the tree.
- A visual system with three glyph tiers: ASCII, Unicode, and an opt-in Nerd Font
  tier with icons. Icons decorate words rather than replacing them, so `--plain`
  loses decoration and no meaning.
- Result grid with row numbers, alternating row backgrounds, a scroll indicator,
  and right alignment for values that read as numbers.
- A visible editor caret and line numbers.
- A live activity indicator while a statement runs: a spinner, the elapsed time,
  and a heartbeat meter that shows time passing rather than progress, because
  PostgreSQL reports no progress for a running statement.
- `ui.reduced-motion` and `--glyphs`, so both decoration and motion are the
  user's choice.
- Full-screen PostgreSQL client with a SQL buffer and a result grid, opening on a
  useful starter query rather than a blank screen.
- Scriptable `query` command with table, CSV, TSV, JSON, NDJSON and Markdown
  output, documented exit codes, data on stdout and diagnostics on stderr.
- `connect --check`, which tests a target stage by stage: address resolution,
  TCP, PostgreSQL, TLS and session facts.
- `doctor` with human and JSON output, and a next action for every check that is
  not passing.
- `config paths`, `show`, `validate` and `migrate --dry-run`, with atomic writes
  and a backup before any migration.
- Generated completions for zsh, bash, fish and PowerShell.
- Dark, light and high-contrast themes, plus `NO_COLOR`, `--plain` and ASCII
  presentations.
- Statement cancellation through PostgreSQL's own cancellation request.

### Improved

- A server that refuses TLS now exits with the TLS code rather than the
  connection code. The network worked; the answer was no.
- A server that requires a password when none was supplied now exits with the
  authentication code rather than the connection code, and points at the
  password file as the safer route.
- Ctrl+C during a non-interactive query asks the server to cancel and returns the
  cancellation exit code, instead of killing the process mid-statement.
- The advertised run keys are `Ctrl+R` and `Ctrl+T` rather than `F5` and `F9`.
  Function keys are routinely claimed by the operating system or an assistant
  before a terminal program sees them. `F5` and `F9` remain bound, `Ctrl+Enter`
  is bound where the terminal can distinguish it, and help gains `Ctrl+G`.

### Security

- `sslmode=verify-ca` is implemented: the certificate chain is checked and the
  host name deliberately is not, for the setups where a trusted certificate is
  presented under a name that will not match. It is never described as
  verify-full.
- Client certificates are supported through `sslcert` and `sslkey`. Half a
  client certificate is refused rather than silently ignored.
- An explicit `sslrootcert` replaces the system trust store rather than adding
  to it, which is what pinning an internal authority is supposed to mean.
- Every transport claim is now tested against a server that really speaks TLS,
  rather than asserted: verify-full succeeding on a matching name, refusing a
  mismatch, verify-ca accepting that same mismatch, an untrusted chain being
  refused, and a client certificate authenticating without a password.
- Catalogue queries bind every object name as a parameter rather than
  interpolating it. An object name is attacker-controlled input the moment anyone
  can create a table, and a fixture named to exploit that is part of the test
  suite.
- Object names inserted into the editor are quoted, so a hostile name cannot
  alter the statement it is pasted into.
- Objects the current role cannot read are listed and marked rather than hidden
  or fatal, because the catalogue is readable when the contents are not.
- The TLS state shown is read back from the server through `pg_stat_ssl` rather
  than assumed from the requested `sslmode`, and is reported as unknown when it
  cannot be confirmed.
- A TLS failure is never retried without TLS.
- Security-relevant connection parameters this build does not implement
  (`sslrootcert`, `sslcert`, `sslkey`, `channel_binding`, `gssencmode` and
  others) fail the connection rather than being silently ignored.
- Control characters in database values and identifiers are escaped before
  display, so a hostile value cannot emit terminal control sequences.
- Passwords are held in types that do not print themselves, and all displayable
  text passes through one redaction implementation.

### Known limitations

- Statement classification reads leading keywords only. `SELECT wipe_all()` is
  classified as a read, because it is a `SELECT`. It is a usability feature, not
  a security boundary, and every place it appears says so.

- Verified on macOS only. Windows and Linux are exercised in CI but have not been
  used by hand.
- The PostgreSQL adapter is being moved to libpq (ADR-0009) for client
  certificates, password files, service files and enterprise authentication.
  Until then those are unsupported and say so at the point of use.
- Tested against PostgreSQL 18.4 only; the supported window is 14 to 18.
- `sslmode=verify-ca` is not implemented and refuses with an explanation.
- No connection profiles, OS credential store, `.pgpass` or service file support.
  `PGSERVICE`, `PGSERVICEFILE` and `PGPASSFILE` are reported as unread.
- No object explorer, query history or export.
- The driver reports a row count rather than the full command tag, so the
  interface says "3 rows affected" rather than "INSERT 0 3".
- Export supports csv, tsv and ndjson. json, markdown and table need the whole
  result before the first byte is correct, so they are refused for export rather
  than quietly buffering the result they were meant to avoid holding.
- Nothing is signed, notarised or published.
