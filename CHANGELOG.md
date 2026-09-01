# Changelog

The single source of release notes. Every stable or preview version maps to
exactly one entry here, and a rebuild never creates a new version.

Format: `New`, `Improved`, `Fixed`, `Security`, `Known limitations`, so the same
source can generate user-facing notes.

## Unreleased

No changes have been assigned to a version after the 0.1.0 candidate.

## [0.1.0]

- Security: debug logging now accepts only the levels `off`, `error`, `warn`,
  `info`, `debug` and `trace`, admits only Ignatius-owned trace targets, and
  redacts each complete event before writing it. Earlier development candidates
  enabled PostgreSQL driver debug tracing, which persisted full SQL text despite
  the documented logging boundary. A statement now appears only as its job id
  and character count, and dependency target directives fail closed.
- Removed a dead action. `Newline` had no key, no chord and no producer: Enter
  reaches the editor's line break through `Activate`, and had done since the
  beginning. Its handling arms were unreachable code in four match statements.
- Fixed: five chords were documented and did not exist. `Ctrl+K d`, `y`, `w`,
  `o` and `e` - definitions, dependencies, save, open and write - were listed in
  the keymap, reachable from the palette in the documentation's telling, and
  bound to nothing at all. The features worked; there was no way to reach them.
  Found by a new test that compares the keymap document with the build in both
  directions.
- Fixed: the client's export offered `--force`, a flag that exists only on the
  command line, to someone who has no command line to type it on. The advice a
  refusal gives is now the caller's to word, so it names something the reader
  can actually do.
- Fixed: a path beginning with `~` typed into the client would have created a
  directory called `~`. There is no shell behind that prompt, so the expansion
  is now done here.
- A saved query cannot be named after a device Windows reserves. `CON.sql` opens
  the console rather than a file there, so it is refused everywhere: a saved
  query is meant to travel between platforms.
- `Ctrl+K e` writes the rows on screen to a file. Before it does, it says how
  many rows that is and, when the result was truncated or a filter is on, what
  the file will not contain and how to get all of it. It writes through the same
  machinery a scripted export uses, so the file lands complete or not at all and
  never replaces one that is already there.
- Saved queries. `Ctrl+K w` writes the buffer to a named file and `Ctrl+K o`
  finds one by name and opens it. They are ordinary `.sql` files in the
  directory `config paths` has advertised since the first release and nothing
  had ever written to: openable in any editor, keepable in a repository,
  readable by `psql`. A name is checked before it reaches the filesystem - no
  separators, no `..`, no control characters - so a name is always a name and
  never a path.
- `ignatius config init` writes a starter configuration file: every default,
  written out with the parts worth knowing about as comments, an example profile
  and an example key binding, both commented so a first run connects to nothing
  by surprise. It refuses to replace a file that already exists without
  `--force`. A test parses the template and compares it with this build's
  defaults, so the file handed to someone is always one this build accepts and
  always describes what it actually does.
- Named connections. A `[profiles]` table in `config.toml` says where a database
  is and how it is classified; `connect @orders-prod` or `--profile orders-prod`
  uses it. The point is not the typing saved but the classification remembered:
  `environment = "production"` written down once means the write guard applies
  every time, without anyone having to remember the flag on the day it matters.
  Flags still win over the profile, and a profile can only make a session safer.
  A profile that tries to hold a password is refused by name, with the routes
  that do exist, and the value is never repeated back.
- When the server asks for a password and none was found, the client asks back
  instead of only reporting it, in both surfaces: a masked field in the
  full-screen client, and an unechoed line in plain mode. Neither asks a pipe:
  the question needs a terminal at both ends, because a script that hangs
  waiting for something nobody can type is worse than one that fails. The field shows how many characters have been
  typed and nothing else; what is typed goes into one connection attempt and is
  dropped with it, and is kept in no file, no configuration and no history. Its
  `Debug` prints `<hidden>`, so a panic or a test failure cannot spill it. A
  connection failure that is not about credentials asks nothing, because a
  password cannot fix a firewall.
- Key bindings can be replaced from `config.toml`. Naming an action removes its
  defaults, so the file is the whole answer for it. An action name this build
  does not know, a key it cannot read, and two actions on one key are all errors
  with what to do about them, reported before the terminal is taken: a file whose
  purpose is to say what the keyboard does must not hold a line that quietly does
  nothing.
- `Ctrl+K y` answers "what breaks if I drop this". It lists what an object is
  used by and what it depends on, in both directions, with the reason for each
  edge, and choosing one puts its quoted, qualified name where SQL is written.
  It follows the two edges PostgreSQL actually records - view rewrite rules and
  foreign keys - and says so where the answer is read, because what a function
  body reads is not recorded anywhere and a dependency list people trust has to
  admit what it cannot see.
- The object tree gets a connection of its own, so a long query can no longer
  delay it. It is opened from the same resolved target as the session, so it
  reaches the same server by the same route with the same credentials and the
  same protection; the two deliberate differences are visible on the server, in
  `pg_stat_activity`: its `application_name` says it is the tree, and the
  session is read-only, because reading the catalogue is all it does. If a
  second connection cannot be opened - a connection limit, a pooler - the tree
  shares the session's and the header says so.
- Indexes appear under the relation they belong to, after its columns, and the
  extensions installed in the database sit at the root of the tree beside the
  schemas rather than inside one of them. Opening a relation now asks for its
  columns and its indexes together, so the node fills in one step.
- `Ctrl+K d` shows what an object actually is. A view, an index and a function
  are rendered by PostgreSQL itself, so what is shown is what will run; a table
  is assembled from its columns, constraints and indexes, and the panel says
  which of the two you are reading rather than passing a description off as a
  script. It is coloured by the same lexer that colours the editor, scrolls, and
  is safe for an object named to break a client that interpolates identifiers.
- A filter over the result rows, on the same key that filters the object tree.
  It narrows what is drawn and nothing else: the row numbers stay the rows' own,
  the inspector and the expanded view follow the selection to the real row, and
  the count says what was searched - `matching 3 of 10000 retained rows, of
  200000 returned`. A filter over a truncated result never implies it searched
  what was never received.
- A statement history, kept on this machine and nowhere else. `Ctrl+K s` searches
  what has run and puts a statement back in the editor; `ignatius history list`
  prints it; `ignatius history clear --yes` deletes it. A statement that mentions
  a credential is never written to it, recording can be paused for a session with
  `Ctrl+K v` or `--no-history` and switched off entirely with
  `history.enabled = false`, and a session that keeps no record says so in its
  header. Scripted `query` runs are not recorded at all.
- Syntax colouring in the editor: keywords, string and dollar-quoted literals,
  numbers, comments, quoted identifiers and `$1` placeholders. It shares the
  statement lexer's quoting rules rather than having its own, so what is
  coloured as a string is what will be sent as one. A bar in the gutter marks
  the statement `Ctrl+T` would run, so which one that is no longer has to be
  guessed. All of it is decoration: with colour off, the buffer reads the same.
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
- Security-relevant connection parameters this build does not implement,
  including `sslcrl`, `channel_binding` and `gssencmode`, fail the connection
  rather than being silently ignored.
- Control characters in database values and identifiers are escaped before
  display, so a hostile value cannot emit terminal control sequences.
- Passwords are held in types that do not print themselves, and all displayable
  text passes through one redaction implementation.

### Known limitations

- Statement classification reads leading keywords only. `SELECT wipe_all()` is
  classified as a read, because it is a `SELECT`. It is a usability feature, not
  a security boundary, and every place it appears says so.

- Windows and Linux build and pass their applicable CI suites, but the
  full-screen client has not been used there by hand. Warp rendering, live
  resize, VoiceOver, NVDA and Windows ConPTY restoration are also unverified.
- PostgreSQL 14, 16 and 18 are exercised in CI and 18.4 locally on macOS.
  Versions 15 and 17 are inside the supported window but have no recorded
  server-backed run.
- Built-in Entra, AWS and Google Cloud token commands have no live cloud
  database evidence. The provider mechanism is proven against the disposable
  TLS server; the built-in command transcriptions remain unverified on their
  own services. Windows-native `az` and `gcloud` command discovery also remains
  unverified.
- GSSAPI, Kerberos and Windows SSPI are unsupported by decision. An OS
  credential store is rejected by ADR-0011; supported credential routes are a
  password file, environment injection, the connection string and the prompt.
- Copying a selected value through opt-in OSC 52 is planned but not implemented.
- The driver reports a row count rather than the full command tag, so the
  interface says "3 rows affected" rather than "INSERT 0 3".
- Export supports csv, tsv and ndjson. json, markdown and table need the whole
  result before the first byte is correct, so they are refused for export rather
  than quietly buffering the result they were meant to avoid holding.
- Release archives, manifests and evidence gates have a non-publishing workflow
  contract only. No hosted candidate run is retained, and nothing is signed,
  notarised or published.
