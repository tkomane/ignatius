# Ignatius

[![CI](https://github.com/tkomane/ignatius/actions/workflows/ci.yml/badge.svg)](https://github.com/tkomane/ignatius/actions/workflows/ci.yml)

**The PostgreSQL client you can point at production.** A full-screen terminal
workbench that is pleasant enough for daily exploration and honest enough for
the connections that matter, in one small local binary that is also a
scriptable CLI.

> **Status: early. Not released.** Verified on macOS against PostgreSQL 18.4.
> Windows and Linux have CI evidence, but full-screen hand verification is not
> yet recorded. Nothing here is signed, notarised, or published, and this README
> makes no claim that it is.

## Why this exists

There are two kinds of PostgreSQL client, and neither is the one you want when
the database is real. `psql` is complete, scriptable and always available, and
it is a wall of text with nothing but your shell prompt standing between you
and the wrong `DELETE`. The full-screen clients are pleasant, and almost none
of them will tell you what protection your connection actually has, as
distinct from what you asked for.

So people use `psql` for the connections that matter and something nicer for
the rest, which means the tool they are least fluent in is the one they use
under the most pressure. Ignatius exists to end that split.

The emotional target is calm confidence: oriented, in control, slightly
delighted. Never noisy, cryptic, brittle, or smug.

## Honest by construction

What makes it different is not the grid or the editor. It is what the tool
tells you about itself:

- **The TLS state it shows is the one the server reports**, read back from
  `pg_stat_ssl`, not the one you asked for. `require` and `verify-full` are
  never described as the same thing, and a TLS failure is never retried
  without TLS.
- **A connection parameter it cannot honour fails the connection.** Ignoring
  `sslrootcert` would give you weaker protection than you asked for while
  looking like it worked.
- **Production is a word on the screen**, declared by you, never guessed from
  a host name, and legible with colour switched off.
- **It says what it knows.** `Cancellation requested` until the server
  confirms. `Connection lost - query outcome unknown` when that is the truth.
- **Every claim in this repository is either verified or labelled.** The same
  rule the client follows on screen, applied to its own development.

## What you get

**Writing SQL.** An editor with syntax colouring, undo that thinks in words,
and schema-aware completion that knows your tables, columns, aliases and CTEs
(`Ctrl+Space`, automatic popups optional). One key formats the buffer
deterministically without touching strings or comments. `:name` parameters are
prompted for at run time, masked, and never written anywhere.

**Reading answers.** A result grid with local sort, filter, a frozen first
column and column controls. One key lays a row out vertically; Enter opens any
cell in full, where NULL, empty text and the literal word `NULL` are three
visibly different things. Query plans render as a readable tree with the
expensive node marked. A failed query moves the caret to the server's answer
and underlines the token it meant.

**Acting deliberately.** A selected cell can become a reviewed, parameterised
`UPDATE`: you see the exact bound statement and its key predicates before
anything is sent, and production or read-only sessions refuse before asking.
`F6` re-runs exactly the retained read behind the visible result, never the
editor buffer, and never on its own. Exports state their row count and
truncation before writing, write atomically, and never overwrite.

**Finding your way.** A searchable command palette, contextual footer hints
that follow your actual state and your actual keymap, an object tree with
definitions (`Ctrl+K d`) and dependency edges (`Ctrl+K y`), searchable
statement history (`Ctrl+K s`), and saved queries as ordinary `.sql` files.
No workflow assumes you know Vim.

## Cloud identity, without passwords

Azure Database for PostgreSQL with Entra ID, Amazon RDS with IAM, and Cloud
SQL with a Google identity all authenticate the same way: a short-lived token
from the cloud's own CLI, presented as the password over an encrypted
connection. Ignatius treats that as data, not as three features:

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
silent fallback to a password prompt, and the palette's
`Connection and auth details` surface shows the whole route without executing
anything. A fourth cloud is one `[auth.providers.<name>]` table away.

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

## From a script

The same binary, with data on stdout and diagnostics on stderr:

```bash
ignatius query "$DATABASE_URL" -c "SELECT count(*) FROM orders" --format csv
ignatius query "$DATABASE_URL" -f report.sql --format json | jq '.[0].rows'
echo "SELECT 1" | ignatius query "$DATABASE_URL" --format ndjson

# Stream a large result straight to a file. The rows never pass through
# memory, so the size of the result does not matter.
ignatius query "$DATABASE_URL" -c "SELECT * FROM orders" --format csv -o orders.csv
```

An export never replaces an existing file unless you pass `--force`, and it
writes to `orders.csv.partial` until it is complete. Interrupt it and the rows
already written stay in the partial file, with the count on stderr and exit
code 9, rather than a truncated file at the destination that looks finished.

Exit codes tell you which kind of failure happened: 2 usage, 3 configuration,
4 connection, 5 authentication, 6 TLS, 7 query, 8 cancelled, 9 interrupted
export, 70 internal. Formats: csv, tsv, json, ndjson, markdown, table, and
generated `INSERT` statements with an explicit destination table.

## Without a full-screen interface

```bash
ignatius --plain connect "$DATABASE_URL"
```

One line in, one answer out. No alternate screen, no raw mode, no escape
sequences at all, so it works in `TERM=dumb`, stays in your scrollback, can
be driven by a pipe, and gives a screen reader plain words to work with
(hand verification with VoiceOver and NVDA is still an open evidence gate,
and the status file says so). The
prompt carries the database, the production marker and the transaction state
as words, and `\complete` offers the same schema completion without ever
running a partial statement.

Accessibility is not a mode bolted on afterwards: meaning survives no-colour,
ASCII, narrow terminals and reduced motion by design, and the test suite
renders every combination to prove it.

## Named connections

```bash
ignatius config init      # a starter file, every default written out
ignatius connect @orders-prod        # or --profile orders-prod
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

There is no OS credential store by decision: password files remain compatible
with `psql` and keep the binary dependency-light. GSSAPI, Kerberos and Windows
SSPI are unsupported. Copying a value out goes through confirmed, opt-in
OSC 52 only, because it moves data beyond this process. No second database
engine is planned: depth on PostgreSQL is the product.
`docs/support/compatibility.md` is the authoritative list, and the client
tells you at the point of use rather than failing quietly.

## Where this is going

The next specified milestone is
[the GUI-grade experience](specs/025-gui-grade-experience/spec.md): painted
surfaces and real elevation, user-owned and runtime-switchable themes with
colour-depth awareness, first-class mouse support with full keyboard parity,
a workable grid with position always stated in words, named connection steps
under a fully painted first frame, and the paste fix. Specified, not yet
implemented; the specification says exactly which claims exist today.

## Documentation

- `docs/status.md` - what is done, what is verified, what is next
- [Product roadmap](docs/product/roadmap.md) - milestones, evidence gates and agent assignments
- [Agent playbook](docs/operations/agent-playbook.md) - ownership, architecture review and handoff
- `docs/product/landscape.md` - the other tools, and why this one
- `docs/architecture/decisions/` - why it is built this way
- `docs/security/threat-model.md` - what is protected and what is not
- `docs/operations/local-development.md` - getting set up
- `.specify/memory/constitution.md` - the rules that govern changes

## Licence

Apache-2.0. See `LICENSE`.
