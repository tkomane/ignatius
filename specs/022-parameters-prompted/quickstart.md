# Quickstart: Prompted query parameters

This guide validates the feature locally. It does not publish or commit
anything.

## Prerequisites

- Rust/Cargo with the repository lockfile available.
- A disposable PostgreSQL service started through the repository task helper.
- No real credentials in SQL, environment variables, fixtures or command
  arguments. Use only synthetic values.

## Focused checks

```bash
cargo fmt --all -- --check
cargo test --locked query::parameters --lib
cargo test --locked app::update --lib
cargo test --locked ui::layout --lib
cargo test --locked cli::plain --lib
cargo test --locked cli::prompt --lib
cargo test --locked cli --lib
```

Expected: the scanner recognizes only executable named placeholders, binding
escapes adversarial values, prompt state keeps values redacted, and malformed
environment mappings fail before connection work.

## Live database checks

```bash
cargo xtask db up
# Use the plain URI printed by `db up`:
set -a
source docker/dev.env
set +a
export PGPASSWORD="$POSTGRES_PASSWORD"
export IGNATIUS_TEST_PG_URI='postgres://ignatius_test@127.0.0.1:55432/ignatius_demo'
export IGNATIUS_PARAMETER_PROBE='quote '\'' slash \\ newline'
cargo run --locked -- query "$IGNATIUS_TEST_PG_URI" \
  --command "SELECT :value AS value" \
  --param-env value=IGNATIUS_PARAMETER_PROBE \
  --format json
cargo xtask db down
cargo xtask db status
```

Expected: JSON stdout contains one value equal to the synthetic environment
value, stderr contains no prompt or progress decoration, and `db status` says
the disposable service is not running after cleanup.

Also check a missing mapping against an unreachable target and confirm the
error is a usage diagnostic with no connection failure:

```bash
cargo run --locked -- query \
  'postgres://app@127.0.0.1:1/does-not-connect' \
  --command 'SELECT :missing' \
  --format json
```

## Manual TUI and plain checks

1. Open the full-screen client against the disposable service and enter
   `SELECT :first AS first, :first AS repeated, :second AS second;`.
2. Run it. Confirm the prompt names `first`, then `second`, and accepts an
   empty answer without altering the editor text.
3. Enter a synthetic value containing a quote, backslash, newline and Unicode;
   confirm the result is data and not a new clause or statement.
4. Cancel halfway through; confirm nothing runs and the SQL remains.
5. Run the same statement in `--plain` mode. Confirm answers are not echoed by
   the client and prompts stay on stderr.
6. Force a parameterized syntax failure. Confirm the interface states that the
   server position belongs to expanded SQL and leaves the editor caret alone.
7. Run with `IGNATIUS_LOG=debug`; inspect only for absence of the synthetic
   value. Do not copy real secrets into the test.

## Full gate

```bash
cargo --locked xtask verify
git diff --check
```

Record full-suite passes, PostgreSQL plain/TLS results, explicit Unix-socket
skips, terminal hand-check gaps, and any unavailable platform or server matrix
separately in `docs/status.md`.
