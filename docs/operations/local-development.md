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

## Trying SQL buffer formatting

The pure formatter and its protected-region, bound, cursor and idempotence
fixtures can be run without a database:

```bash
cargo test --locked query::format --lib
```

In the full-screen client, type a compact query and press `Ctrl+Shift+F`,
`Ctrl+K q`, or choose Format SQL from the command palette. The footer reports
whether it changed, had nothing to do, or refused safely. `Ctrl+Z` undoes one
complete formatting edit. An unterminated or over-limit buffer remains
untouched. Formatting is local and does not run SQL, load metadata, write
history or files, or change machine output.

In plain mode, enter `\\format` before the semicolon. The formatted preview is
written to stderr and the pending SQL remains there until a later terminator;
result rows, when eventually executed, remain on stdout. Use synthetic SQL only
when exercising this path.

## Trying prompted named parameters

The parameter scanner and binder are local and deterministic:

```bash
cargo test --locked query::parameters --lib
cargo test --locked parameter --lib
```

With the disposable database running, use a synthetic value that exercises
quotes, backslashes and a newline. The value is supplied through the environment
and the command line contains only its variable name:

```bash
set -a
source docker/dev.env
set +a
export PGPASSWORD="$POSTGRES_PASSWORD"
export IGNATIUS_PARAMETER_PROBE='quote '\'' slash \\ newline'
# Use the plain URI printed by `cargo xtask db up`.
cargo run -- query 'postgres://ignatius_test@127.0.0.1:55432/ignatius_demo' \
  -c "SELECT :value AS value, :value AS repeated" \
  --param-env value=IGNATIUS_PARAMETER_PROBE --format csv
cargo run -- query 'postgres://ignatius_test@127.0.0.1:55432/ignatius_demo' \
  -c "SELECT :value AS value" \
  --param-env value=IGNATIUS_PARAMETER_PROBE --format csv \
  --output /private/tmp/ignatius-parameter-probe.csv
unset IGNATIUS_PARAMETER_PROBE
```

The first command writes only CSV data to stdout. The second streams to the
atomic export boundary. Try the same statement without `--param-env`, with a
duplicate mapping, or with an unset variable; each must fail before target
resolution and must not wait for input. In the full-screen client, run
`SELECT :value;`, confirm any production gate first, enter a value and verify
that the editor and history keep the template while the prompt shows only a
mask. In plain mode, values are hidden and prompts go to stderr; when no
terminal is available it explains the `--param-env` route instead of hanging.

## Trying interactive cell updates

With the disposable PostgreSQL service running, execute a synthetic single-table
query that includes its primary key, focus a non-key cell, and press `Ctrl+K u`.
The first slice accepts direct projections such as:

```sql
SELECT order_id, note FROM orders ORDER BY order_id;
```

It resolves the live relation and privileges, asks for a replacement value,
and then shows the exact bound `UPDATE` before anything is sent. Empty text is
valid and is not the same as SQL `NULL`; quotes, punctuation, control text and
newlines remain literal data. Enter on the review sends one parameterized
statement. Esc at either prompt sends nothing. The prior `SELECT` is a
snapshot, so the client does not rerun it automatically.

Joins, expressions, CTEs, set operations, views, relations without a primary
key, unprojected or NULL key values, primary-key cells, production-classified
connections and server read-only sessions are refused. Printable `u` remains
editor input; the action is also searchable in the command palette. Use only
synthetic data and inspect the review statement before confirming.

The focused checks are:

```bash
cargo test --locked query::update --lib
cargo test --locked app::update::tests::cell_update --lib
cargo test --locked ui::layout::tests::cell_update --lib
cargo test --locked --test postgres_integration a_result_cell_plan_updates
```

The PostgreSQL test is skipped when `IGNATIUS_TEST_PG_URI` is unset. A skip is
not a pass.

## Trying an explicit retained-result refresh

With a completed synthetic result visible, edit the SQL buffer without running
it, focus Results, and press `F6`. The title should say
`Refreshing retained result`, the editor should remain unchanged, and one deliberate
execution should replace the old result after the server responds. The same
action is available from the command palette and the contextual Results footer.
`Ctrl+Shift+R` is an additional shortcut only when the terminal preserves
enhanced modifier reporting; traditional terminals should use `F6`.

Use a single read such as `SELECT order_id, note FROM orders ORDER BY order_id`.
The refresh refuses a multi-statement source or a write, structural,
destructive or unrecognised source. For `SELECT :value AS value`, the masked
parameter prompt appears again and the previous value is not reused. A zero-row
result remains eligible when its result set is retained. Focus changes, grid
controls, reconnect and a failed or cancelled outcome must not start another
request. Use synthetic values only.

The focused checks are:

```bash
cargo test --locked --lib refresh
cargo test --locked --test documentation_matches_the_build retained_result_refresh
```

The full verifier records the execution and history boundaries. The semantic
Unix-socket gate, actual terminal use and cross-platform checks remain separate
evidence; a skipped socket test is not a pass.

## Trying the connection picker

Put a few non-secret profiles in a throwaway configuration directory:

```toml
[profiles.orders-dev]
host = "127.0.0.1"
port = 55432
dbname = "ignatius_demo"
user = "ignatius_test"
sslmode = "disable"
environment = "development"
description = "synthetic local fixtures"

[profiles.orders-prod]
host = "db.example.net"
dbname = "orders"
user = "app"
sslmode = "verify-full"
environment = "production"
read-only = true
```

Start `ignatius` without a target. The first frame opens a searchable picker
with `Use default connection settings` followed by the named profiles. Search
by profile name or a safe hint such as `production` or `read-only`, then press
Enter. `Ctrl+K n` opens it again when the session is quiet. Esc cancels without
connecting, and a running query or plan keeps the picker unavailable. Never put
passwords, tokens or unknown fields in a profile; the existing password-file,
environment and provider routes remain the only credential boundaries.

The pure and runtime-boundary checks are:

```bash
cargo test --locked app::connection_picker --lib
cargo test --locked app::update --lib
cargo test --locked cli::interactive --lib
```

## Trying intentional exports

With the disposable database running, the existing `xtask sql` helper can
exercise both the interactive-shaped machine route and its safety refusals:

```bash
cargo xtask sql "SELECT order_id, note FROM orders ORDER BY order_id" \
  --format insert --insert-table orders
cargo xtask sql "SELECT order_id, note FROM orders ORDER BY order_id" \
  --format insert --insert-table orders --output /private/tmp/ignatius-orders.sql
cargo run -- query -c "SELECT 1" --format insert
cargo run -- query -c "SELECT 1" --format insert --insert-table orders --no-header
```

The first command prints quoted SQL data to stdout; it does not execute the
generated statements. The second uses the bounded `.partial` and atomic-finish
path, so inspect the file before applying it and remove it after the check. The
last two commands must refuse before any connection attempt. In the full-screen
client, run a synthetic result, press `Ctrl+K e`, choose a format, type a path
with a misleading extension, and confirm that the selected shape wins. Cancel
the path prompt once and verify that neither the destination nor a partial file
was created.

## Trying interactive clipboard copy

Copy is disabled unless the configuration says otherwise:

```toml
[clipboard]
osc52 = false
```

After a retained result is visible, focus a text cell and press `Ctrl+K c`.
With the default setting the client explains that no value was sent. To test
the one-way terminal boundary, change the setting to `true`, confirm the row,
column, and UTF-8 size, then cancel once and confirm once. The client can prove
only that it wrote and flushed an OSC 52 sequence; terminal acceptance is
unconfirmed, and the terminal, SSH path, or multiplexer may retain the value.
Use synthetic data only.

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
