# Local development

## What you need

- A Rust toolchain. `rust-toolchain.toml` pins the version for rustup users. If
  your `rustc` comes from a package manager instead, the pin is inert: check
  `rustc --version` matches before trusting a build difference to be real.
- Docker, for the disposable PostgreSQL server.
- Git.

Nothing else. There is no code generator, no database to install, and no account.

## The short version

```bash
cargo xtask db up        # start the disposable database, wait for it, print the URI
cargo xtask run          # open the client against it
cargo xtask verify       # every gate, with a summary
cargo xtask db down      # stop it and delete its data
```

`cargo xtask` is a small Rust program in `xtask/`, not a shell script, so the
same commands work on macOS, Windows and Linux. It has no dependencies of its
own, because it runs before anything else is known to work.

## Every task

| Command | What it does |
| --- | --- |
| `cargo xtask db up` | Starts `postgres:18.4-alpine` on port 55432, waits until it is really accepting connections, loads the synthetic fixtures, and prints the URI |
| `cargo xtask db down` | Stops it and deletes its data |
| `cargo xtask db status` | Whether it is running and ready |
| `cargo xtask run [args]` | Opens the full-screen client against it |
| `cargo xtask sql "SELECT 1"` | Runs one statement against it |
| `cargo xtask test` | The whole suite. Says so if the database is absent and the integration tests will skip |
| `cargo xtask verify` | Formatting, lints, unit tests, CLI contract, integration. Runs every gate even after one fails, then prints a summary and exits non-zero if any failed |
| `cargo xtask install [--dir PATH]` | Builds a release binary and copies it to `~/.local/bin`, telling you whether that is on your PATH |

## The disposable database

`cargo xtask db up` starts a throwaway PostgreSQL with SCRAM-SHA-256 required and
loads fixtures designed to exercise the parts that are easy to get wrong: an
`orders` table, a `type_coverage` table with numerics, arrays, ranges, JSON,
`bytea` and intervals, a `rendering_cases` table containing values built to break
a naive renderer, and a `restricted_reader` role for permission tests.

`cargo xtask db up` starts **two** servers: a plain one on port 55432 and one
that really speaks TLS on 55433, with a throwaway certificate authority
generated on demand into `docker/tls/generated` and never committed. The TLS
server exists so the transport claims can be tested rather than asserted, and
its certificate deliberately covers `localhost` and not `127.0.0.1`, so the same
server proves both that verify-full checks the name and that verify-ca does not.

Credentials live in `docker/dev.env`. They are synthetic, deliberately committed,
and exist only inside that container. Never point the test suite at a database
you care about.

Readiness is checked over TCP inside the container, not over the socket. During
initialisation the entrypoint runs a temporary server with `listen_addresses`
empty, so a socket check would report ready before the fixtures exist.

## Running it by hand

If you would rather not use xtask:

```bash
docker compose -f docker/compose.yaml up -d
export IGNATIUS_TEST_PG_URI="postgres://ignatius_test:not-a-real-password-disposable-container@127.0.0.1:55432/ignatius_demo"
cargo run -- connect "$IGNATIUS_TEST_PG_URI"
cargo test
```

## Trying schema completion

The full-screen client loads one schema snapshot alongside the object tree and
bounds only the visible candidate list. In the editor, type a partial table or
column name and press `Ctrl+Space`;
the same menu is also available from the command palette. `Enter` accepts the
visible candidate, `Esc` dismisses it, and `Ctrl+Z` undoes an accepted
replacement in one step. Automatic popups can be disabled without removing
explicit completion:

```toml
[ui]
completion = false
```

Plain mode keeps the workflow line-oriented:

```text
orders => SELECT * FROM ord
orders => \complete
1  orders  table  public  readable
orders => \use 1
Completion inserted: orders
orders => ;
```

The completion catalogue is read once per session, never once per keystroke.
Use the focused completion tests and the PostgreSQL integration test when
working on the analyzer or metadata query:

```bash
cargo test completion
cargo test --test postgres_integration completion_catalog
```

## Keeping your real configuration out of it

Set `IGNATIUS_CONFIG_DIR` and `IGNATIUS_DATA_DIR` to throwaway directories while
developing. The test suite already does this for itself, so tests never touch a
real profile.

```bash
export IGNATIUS_CONFIG_DIR=$(mktemp -d)
```

## Turning on logs

Off by default and writes nothing until asked:

```bash
IGNATIUS_LOG=debug cargo xtask sql "SELECT 1"
```

Logs never contain SQL text or row values. If you need to see a statement while
debugging, print it in your own scratch code and do not commit that. The value is
one of `off`, `error`, `warn`, `info`, `debug` or `trace`; dependency target
directives are deliberately refused.

## Layout of the code

Read `src/lib.rs` first: it is the module map and states the layering rules.
Then read the module doc comment of whatever you are changing; each one explains
what the module is responsible for and, more usefully, what it is not allowed to
do.

The rules that matter: `app` performs no I/O, `ui` opens no connections and
writes no files, and only `postgres` knows which driver is in use.
