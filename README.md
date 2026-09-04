# Ignatius

A terminal-native PostgreSQL workbench: a full-screen client for exploring, and
the same binary as a scriptable CLI for automating.

> **Status: early. Not released.** Verified on macOS against PostgreSQL 18.4.
> Windows and Linux have CI evidence, but full-screen hand verification is not
> yet recorded. Nothing here is signed, notarised, or published, and this README
> makes no claim that it is.

## Why this exists

`psql` is the reference client and is not pleasant to explore with. The
full-screen clients are pleasant and are not what you would point at production.
This tries to be both, by being honest about the things that matter when the
database is real:

- **The TLS state it shows is the one the server reports**, read back from
  `pg_stat_ssl`, not the one you asked for. `require` and `verify-full` are never
  described as the same thing, and a TLS failure is never retried without TLS.
- **A connection parameter it cannot honour fails the connection.** Ignoring
  `sslrootcert` would give you weaker protection than you asked for while looking
  like it worked.
- **Production is a word on the screen**, declared by you, never guessed from a
  host name, and legible with colour switched off.
- **It says what it knows.** `Cancellation requested` until the server confirms.
  `Connection lost - query outcome unknown` when that is the truth.

## Try it

```bash
cargo xtask db up      # disposable PostgreSQL with synthetic fixtures
cargo xtask run        # open the client against it
cargo xtask db down    # stop it and delete its data
```

`cargo xtask install` builds a release binary and puts `ignatius` on your PATH.
Everything the demo needs is synthetic and lives in `docker/dev.env`; no real
database is ever required.

In the client: **Ctrl+R** runs the buffer, **Ctrl+T** runs the statement at the
cursor, **Ctrl+Space** opens schema completion, **Ctrl+C** cancels, **F1** shows
help, **Ctrl+Q** quits.

Navigation: **Ctrl+B** shows the object tree, **Tab** moves between panes,
**Ctrl+P** opens the palette, and **Ctrl+K** lists the chords. In the tree,
arrows navigate, `/` filters, and **Enter** puts the selected object's quoted
name where you are writing SQL. Completion is local to the loaded schema: type
normally, choose with the arrows, accept with **Enter**, or dismiss with **Esc**.

The run keys are chords rather than function keys, because operating systems and
assistants routinely claim `F5` before a terminal program sees it. `F5` and `F9`
stay bound anyway, for muscle memory from other tools.

If you use a Nerd Font, turn the icons on:

```bash
ignatius --glyphs nerd-font connect "$DATABASE_URL"
```

or permanently, in `config.toml`:

```toml
[ui]
glyphs = "nerd-font"
```

Automatic schema completion is on by default. If you prefer a quiet editor,
keep explicit **Ctrl+Space** and turn off only automatic popups:

```toml
[ui]
completion = false
```

It is never chosen automatically: whether your font carries the icon range cannot
be detected from inside a terminal, and guessing wrong fills the screen with
replacement characters. Every icon decorates a word rather than replacing it, so
`--plain` loses decoration and no meaning. See `docs/design/icons-and-motion.md`.

## From a script

```bash
ignatius query "$DATABASE_URL" -c "SELECT count(*) FROM orders" --format csv
ignatius query "$DATABASE_URL" -f report.sql --format json | jq '.[0].rows'
echo "SELECT 1" | ignatius query "$DATABASE_URL" --format ndjson

# Stream a large result straight to a file. The rows never pass through memory,
# so the size of the result does not matter.
ignatius query "$DATABASE_URL" -c "SELECT * FROM orders" --format csv -o orders.csv
```

An export never replaces an existing file unless you pass `--force`, and it
writes to `orders.csv.partial` until it is complete. Interrupt it and the rows
already written stay in the partial file, with the count on stderr and exit
code 9 - rather than a truncated file at the destination that looks finished.

Data goes to stdout, diagnostics to stderr, and the exit code tells you which
kind of failure happened: 2 usage, 3 configuration, 4 connection,
5 authentication, 6 TLS, 7 query, 8 cancelled, 9 interrupted export, 70 internal.

```powershell
ignatius query $env:DATABASE_URL -c "SELECT 1" --format csv | Select-Object -First 5
```

## Without a full-screen interface

```bash
ignatius --plain connect "$DATABASE_URL"
```

One line in, one answer out. No alternate screen, no raw mode, nothing that only
makes sense to an eye, so it works in `TERM=dumb`, stays in your scrollback, and
can be driven by a pipe. The prompt carries the database, the production marker
and the transaction state as words.

Completion is explicit in plain mode and does not run a partial statement:

```text
orders => SELECT * FROM ord
orders => \complete
1  orders  table  public  readable
orders => \use 1
Completion inserted: orders
orders => ;
```

Use `\complete [prefix]` to narrow the list and `\use <number|exact-name>` to
accept one printed candidate. A schema snapshot is loaded once per session;
loading, stale, unavailable, and bounded-list states are printed as words.

## Named connections

```bash
ignatius config init      # a starter file, every default written out
```

```toml
# ~/.config/ignatius/config.toml
[profiles.orders-prod]
host = "db.example.net"
port = 6432
dbname = "orders"
user = "app"
sslmode = "verify-full"
environment = "production"
```

```bash
ignatius connect @orders-prod        # or --profile orders-prod
```

The classification is the point. Written down once, the production write guard
applies every time, without anyone having to remember a flag on the day it
matters. Flags still win over the profile, and a profile can only make a session
safer. A profile never holds a password: use a password file, the environment, or
type it when the client asks.

## Saved queries

`Ctrl+K w` names the buffer and writes it; `Ctrl+K o` finds one and opens it.
They are ordinary `.sql` files in `~/.config/ignatius/queries`, so a query worth
keeping can be opened in an editor, committed to a repository, or run by `psql`.

## What it remembers

Interactive sessions record the statements that run, on this machine only.
`Ctrl+K s` searches them and puts one back in the editor.

```bash
ignatius history list          # exactly what is kept
ignatius history clear --yes   # remove it
ignatius --no-history connect  # a session that records nothing
```

A statement that mentions a credential is never written, `history.enabled =
false` switches recording off entirely, and a session that keeps no record says
so in its header. Scripted `ignatius query` runs are never recorded.

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
SSPI are unsupported; the required Entra, AWS and Google Cloud routes use a
short-lived token as the password over TLS. Copying a value through confirmed,
opt-in OSC 52 is implemented locally; hand-terminal acceptance remains
unverified. When a server asks for a password, both the full-screen
client and plain mode prompt safely when a terminal is available; a script fails
rather than waiting for input nobody can provide.
`docs/support/compatibility.md` is the authoritative list, and the client tells
you at the point of use rather than failing quietly.

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
