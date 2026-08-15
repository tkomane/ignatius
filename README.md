# Ignatius

A terminal-native PostgreSQL workbench: a full-screen client for exploring, and
the same binary as a scriptable CLI for automating.

> **Status: early. Not released.** Verified on macOS against PostgreSQL 18.4.
> Windows and Linux support is written but not yet verified. Nothing here is
> signed, notarised, or published, and this README makes no claim that it is.

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
cursor, **Ctrl+C** cancels, **F1** shows help, **Ctrl+Q** quits.

Navigation: **Ctrl+B** shows the object tree, **Tab** moves between panes,
**Ctrl+P** opens the palette, and **Ctrl+K** lists the chords. In the tree,
arrows navigate, `/` filters, and **Enter** puts the selected object's quoted
name where you are writing SQL.

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

It is never chosen automatically: whether your font carries the icon range cannot
be detected from inside a terminal, and guessing wrong fills the screen with
replacement characters. Every icon decorates a word rather than replacing it, so
`--plain` loses decoration and no meaning. See `docs/design/icons-and-motion.md`.

## From a script

```bash
ignatius query "$DATABASE_URL" -c "SELECT count(*) FROM orders" --format csv
ignatius query "$DATABASE_URL" -f report.sql --format json | jq '.[0].rows'
echo "SELECT 1" | ignatius query "$DATABASE_URL" --format ndjson
```

Data goes to stdout, diagnostics to stderr, and the exit code tells you which
kind of failure happened: 2 usage, 3 configuration, 4 connection,
5 authentication, 6 TLS, 7 query, 8 cancelled, 9 interrupted export, 70 internal.

```powershell
ignatius query $env:DATABASE_URL -c "SELECT 1" --format csv | Select-Object -First 5
```

## Other commands

```bash
ignatius doctor --json          # is this machine set up correctly
ignatius connect --check URI    # test a target, stage by stage
ignatius config paths           # where configuration and logs live
ignatius completion zsh         # zsh, bash, fish, powershell
ignatius version --verbose      # version, revision, build identity, target
```

## What it does not do yet

Connection profiles, the OS credential store, `.pgpass`, service files,
`sslmode=verify-ca`, client certificates, an object explorer, query history and
export. `docs/support/compatibility.md` is the authoritative list, and the client
tells you at the point of use rather than failing quietly.

## Documentation

- `docs/status.md` - what is done, what is verified, what is next
- `docs/product/landscape.md` - the other tools, and why this one
- `docs/architecture/decisions/` - why it is built this way
- `docs/security/threat-model.md` - what is protected and what is not
- `docs/operations/local-development.md` - getting set up
- `.specify/memory/constitution.md` - the rules that govern changes

## Licence

Apache-2.0. See `LICENSE`.
