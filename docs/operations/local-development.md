# Local development

## What you need

- A Rust toolchain. `rust-toolchain.toml` pins the version for rustup users. If
  your `rustc` comes from a package manager instead, the pin is inert: check
  `rustc --version` matches before trusting a build difference to be real.
- Docker, for the disposable PostgreSQL server.
- Git.

Nothing else. There is no code generator, no database to install, and no account.

## Getting a server

```bash
docker compose -f docker/compose.yaml up -d
docker compose -f docker/compose.yaml logs -f postgres   # wait for "ready"
```

This starts `postgres:18.4-alpine` on port 55432 with SCRAM-SHA-256 required, and
loads synthetic fixtures: an `orders` table, a `type_coverage` table exercising
the types the renderer has to handle, a `rendering_cases` table containing values
designed to break a naive renderer, and a `restricted_reader` role for permission
tests.

Every credential in that file is synthetic and exists only inside the container.
Never point the test suite at a database you care about.

```bash
export IGNATIUS_TEST_PG_URI="postgres://ignatius_test:not-a-real-password-disposable-container@127.0.0.1:55432/ignatius_demo"
```

Tear it down with `docker compose -f docker/compose.yaml down -v`.

## Running it

```bash
cargo run -- doctor
cargo run -- connect --check "$IGNATIUS_TEST_PG_URI"
cargo run -- query "$IGNATIUS_TEST_PG_URI" -c "SELECT * FROM orders" --format csv
cargo run -- "$IGNATIUS_TEST_PG_URI"        # the full-screen client
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
IGNATIUS_LOG=debug cargo run -- doctor
cat "$(cargo run -q -- config paths | grep '^Logs:' | cut -d' ' -f2)/ignatius.log"
```

Logs never contain SQL text or row values. If you need to see a statement while
debugging, print it in your own scratch code and do not commit that.

## Layout of the code

Read `src/lib.rs` first: it is the module map and states the layering rules.
Then read the module doc comment of whatever you are changing; each one explains
what the module is responsible for and, more usefully, what it is not allowed to
do.

The rules that matter: `app` performs no I/O, `ui` opens no connections and
writes no files, and only `postgres` knows which driver is in use.
