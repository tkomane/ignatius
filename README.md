# Ignatius

[![CI](https://github.com/tkomane/ignatius/actions/workflows/ci.yml/badge.svg)](https://github.com/tkomane/ignatius/actions/workflows/ci.yml)

```
 ___   ___  _  _    _    _____  ___  _   _  ___
|_ _| / __|| \| |  /_\  |_   _||_ _|| | | |/ __|
 | | | (_ || .` | / _ \   | |   | | | |_| |\__ \
|___| \___||_|\_|/_/ \_\  |_|  |___| \___/ |___/
------------------------------------------------
the PostgreSQL client you can point at production
```

A terminal-native PostgreSQL workbench: a full-screen client pleasant enough
for daily exploration, careful enough for production, and the same small
binary when you want a script. No account, no telemetry, no cloud in the loop.

> **Early. Not released.** The full-screen client is hand-verified on macOS
> against PostgreSQL 18.4. Windows and Linux build and pass their suites in CI,
> but nobody has opened the full-screen client there in person. Nothing is
> signed, notarised or published. `docs/status.md` records what is proved,
> where, and what is not.

## The split this ends

PostgreSQL clients have always come in two flavours, and neither is the one
you want when the database is real.

`psql` is the reference implementation: complete, scriptable, on every
machine. It is also a wall of text where the only thing between you and the
wrong `DELETE` is your shell prompt.

The full-screen clients are lovely to use. But most were built for a dozen
databases at once, so PostgreSQL is one dialect among many, and few will tell
you what protection your connection actually negotiated, as distinct from what
you asked for.

So the habit forms: `psql` for the connections that matter, something prettier
for the rest. The tool you know least well becomes the one you reach for under
the most pressure.

Ignatius exists to end that split. The ambition is the best PostgreSQL
experience anyone has had in a terminal, with nothing about the connection
taken on faith. The target feeling is calm confidence: oriented, in control,
quietly delighted.

## The workbench

Everything a graphical client does for you that a terminal can carry, done
natively.

**Writing SQL.** Syntax colouring; undo that thinks in words; completion that
knows your schemas, tables, columns, aliases and CTEs (`Ctrl+Space`,
automatic popups optional). One key formats the buffer deterministically
without touching a string or a comment. `:name` parameters are asked for when
the statement runs, masked, and kept out of history: the template is recorded,
the answers are not.

**Reading answers.** A result grid with local sort, filter, a frozen first
column and keyboard column sizing. One key lays a row out vertically, and
Enter opens any cell in full, where NULL, empty text and the literal word
`NULL` are three visibly different things. Query plans render as a readable
tree with the expensive node marked and the basis for the mark stated. A
failed query moves the caret to the server's answer and underlines the token
it meant.

**Acting deliberately.** A selected cell can become a reviewed, parameterised
`UPDATE`: you see the exact bound statement and its key predicates before
anything is sent, and production or read-only sessions refuse before asking.
`F6` re-runs exactly the retained read behind the visible result, never the
editor buffer, and never on its own. Exports name their row count and
truncation before writing, write atomically, and never overwrite.

**Finding your way.** A searchable command palette; footer hints that follow
your actual state and your actual keymap; an object tree with definitions
(`Ctrl+K d`) and dependency edges (`Ctrl+K y`); searchable statement history
(`Ctrl+K s`); saved queries as ordinary `.sql` files. No workflow assumes you
know Vim.

## Honest by construction

A pleasant client that lies to you is worse than `psql`. These are not
policies bolted onto the side; they are why it is built the way it is.

- **Uncertainty is stated.** `Cancellation requested` until the server
  confirms. `Connection lost - query outcome unknown` when that is the truth.
  An interrupted export leaves a `.partial` file, a row count and exit code 9,
  never a truncated file at the destination that looks finished.
- **The TLS state shown is the server's answer**, read back from
  `pg_stat_ssl`, not the one you asked for. `require` and `verify-full` are
  never described as the same thing, and a TLS failure is never retried
  without TLS.
- **An option it cannot honour fails the connection.** A client that quietly
  ignores security-relevant parameters hands you weaker protection than you
  asked for and a screen that looks like success. That is the one failure mode
  a client must never have.
- **Production is a word you declared**, never inferred from a host name, and
  legible with colour switched off. The write guard is advisory and says so;
  `--read-only` is the real control, because PostgreSQL enforces it.
- **Nothing is replayed, retried or silently downgraded.** Not a write, not a
  failure, not a connection.

## One binary, two lives

The thing you explore with is the thing you script.

```bash
ignatius query "$DATABASE_URL" -c "SELECT count(*) FROM orders" --format csv
ignatius query "$DATABASE_URL" -f report.sql --format json | jq '.[0].rows'
echo "SELECT 1" | ignatius query "$DATABASE_URL" --format ndjson

# Stream a large result straight to a file. The rows never pass through
# memory, so the size of the result does not matter.
ignatius query "$DATABASE_URL" -c "SELECT * FROM orders" --format csv -o orders.csv
```

Formats: csv, tsv, json, ndjson, markdown, table, and generated `INSERT`
statements with an explicit destination table; `--output` streams the four
whose first byte is correct before the last row is read. Exit codes tell a
script which kind of failure happened: 2 usage, 3 configuration, 4 connection,
5 authentication, 6 TLS, 7 query, 8 cancelled, 9 interrupted export,
70 internal.

## Plain mode, and the screen reader question

```bash
ignatius --plain connect "$DATABASE_URL"
```

One line in, one answer out. No alternate screen, no raw mode, no escape
sequences at all, so it works in `TERM=dumb`, stays in your scrollback, can be
driven by a pipe, and gives a screen reader plain words to work with. The
prompt carries the database, the production marker and the transaction state
as words, and `\complete` offers the same schema completion without ever
running a partial statement.

Accessibility is not a mode bolted on afterwards: meaning survives no-colour,
ASCII, narrow terminals and reduced motion by design, and the test suite
renders every combination to prove it. Hand verification with VoiceOver and
NVDA is still an open evidence gate, and `docs/status.md` says so rather than
implying otherwise.

## Cloud identity, without passwords

Azure Database for PostgreSQL with Entra ID, Amazon RDS with IAM, and Cloud
SQL with a Google identity authenticate the same way: a short-lived token from
the cloud's own CLI, presented as the password over an encrypted connection.
Ignatius treats that as data, not as three features:

```toml
[profiles.orders-prod]
host = "orders.postgres.database.azure.com"
dbname = "orders"
user = "app@example.com"
sslmode = "verify-full"
environment = "production"
auth = "entra"          # or "aws", or "gcp"
```

Nothing is stored and nothing expires in a file. A target whose `sslmode`
would permit an unencrypted connection is refused before the token is ever
requested, because a bearer token is usable by whoever sees it. There is no
silent fallback to a password prompt, and the palette's `Connection and auth
details` surface shows the whole route without executing anything. A fourth
cloud is one `[auth.providers.<name>]` table away.

The mechanism is proven end to end against a real server with a configured
provider. What has not happened is anyone running each built-in against that
vendor's own database; `docs/support/compatibility.md` keeps that separate
rather than letting one word cover both.

## Try it

```bash
cargo xtask db up      # disposable PostgreSQL with synthetic fixtures
cargo xtask run        # open the client against it
cargo xtask db down    # stop it and delete its data
```

`cargo xtask install` builds a release binary and puts `ignatius` on your
PATH. Everything the demo needs is synthetic and lives in `docker/dev.env`; no
real database is ever required.

| Key | Does |
| --- | --- |
| `Ctrl+R` / `Ctrl+T` | run the buffer / the statement at the cursor |
| `Ctrl+Space` | schema completion |
| `Ctrl+C` | cancel, and say so honestly |
| `Ctrl+B` / `Tab` | object tree / next pane |
| `Ctrl+P` | command palette |
| `Ctrl+K` | chord menu: the rest of the verbs, listed on screen |
| `F1` / `Ctrl+Q` | help / quit |

The run keys are chords rather than function keys, because operating systems
and assistants routinely claim `F5` before a terminal program sees it. `F5`
and `F9` stay bound anyway, for muscle memory from other tools. Every binding
is yours to change in `[keys]`, and a binding that would do nothing is refused
with a reason before the terminal is taken.

If you use a Nerd Font, turn the icons on:

```toml
[ui]
glyphs = "nerd-font"
```

It is never chosen automatically: whether your font carries the icon range
cannot be detected from inside a terminal, and guessing wrong fills the screen
with replacement characters. Every icon decorates a word rather than replacing
it, so `--plain` loses decoration and no meaning.

## Named connections

```bash
ignatius config init                # a starter file, every default written out
ignatius connect @orders-prod       # or --profile orders-prod
```

The classification is the point. Written down once, the production write
guard applies every time, without anyone having to remember a flag on the day
it matters. Flags still win over the profile, and a profile can only make a
session safer. A profile never holds a password: use a password file, the
environment, a cloud identity, or type it when the client asks.

## What it remembers

Interactive sessions record the statements that run, on this machine only.
There is no telemetry, no update check, no account, and no network call that
is not a database connection.

```bash
ignatius history list          # exactly what is kept
ignatius history clear --yes   # remove it
ignatius --no-history connect  # a session that records nothing
```

A statement that mentions a credential is never written, recording can be
switched off entirely, and a session that keeps no record says so in its
header. Scripted `ignatius query` runs are never recorded.

## Other commands

```bash
ignatius doctor --json          # is this machine set up correctly
ignatius connect --check URI    # test a target, stage by stage
ignatius config paths           # where configuration and logs live
ignatius completion zsh         # zsh, bash, fish, powershell
ignatius version --verbose      # version, revision, build identity, target
```

## What it does not do

No OS credential store, by decision: password files remain compatible with
`psql` and the binary stays dependency-light. GSSAPI, Kerberos and Windows
SSPI are unsupported. Copying a value out goes through confirmed, opt-in
OSC 52 only, because it moves data beyond this process. No second database
engine, no migration framework, no visual schema editor, no dashboards, and
nothing that sends your schema or results to an AI provider. Depth on
PostgreSQL is the product.

`docs/support/compatibility.md` is the authoritative list, and the client
tells you at the point of use rather than failing quietly.

## Where this is going

The next milestone is
[the GUI-grade experience](specs/025-gui-grade-experience/spec.md): painted
surfaces and real elevation, user-owned and runtime-switchable themes with
colour-depth awareness, first-class mouse support with full keyboard parity,
movable panes with a density setting, quiet threshold-gated chrome, palette
previews, and an identity mark that materialises over the connecting wait.

In progress. Pasted SQL already reaches the editor, the frame paints its own
surfaces across four colour depths, the grid states its visible range and
resizes from the keyboard, and the connecting wait names each step it can
honestly observe. Themes, mouse, panes and the rest are specified and not yet
merged; the specification says exactly which claims exist today.

> Success is not stars. It is whether a senior engineer reaches for Ignatius
> instead of `psql` when the database is production-adjacent, and does not
> feel they gave anything up to do so.

## Documentation

- `docs/status.md` - what is done, what is verified, what is next
- [Vision](docs/product/vision.md) - the problem, the wedge and the non-goals
- [Product roadmap](docs/product/roadmap.md) - milestones, evidence gates and agent assignments
- [Agent playbook](docs/operations/agent-playbook.md) - ownership, architecture review and handoff
- `docs/product/landscape.md` - the other tools, and why this one
- `docs/architecture/decisions/` - why it is built this way
- `docs/security/threat-model.md` - what is protected and what is not
- `docs/operations/local-development.md` - getting set up
- `.specify/memory/constitution.md` - the rules that govern changes

## Licence

Apache-2.0. See `LICENSE`.
