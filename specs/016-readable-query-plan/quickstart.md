# Quickstart: A plan you can read

## Prerequisites

- Rust 1.90 or newer with the repository lockfile.
- A PostgreSQL 14-18 server for live evidence. The repository's disposable
  fixture is sufficient for the integration checks.

## Focused validation

```bash
cargo fmt --all -- --check
cargo check --locked
cargo test --locked query::plan --lib
cargo test --locked app::plan --lib
cargo test --locked app::update --lib
cargo test --locked ui::keymap --lib
cargo test --locked ui::layout --lib
cargo test --locked --test documentation_matches_the_build
cargo test --locked --test cli_contract
git diff --check
```

Expected evidence is separated by boundary:

- parser fixtures prove parent-child order, metrics, mismatch thresholds,
  unknown fields, sanitisation inputs, and resource bounds;
- reducer tests prove plain planning has the expected effect, analysis waits for
  confirmation, cancellation emits no effect, and stale jobs are rejected;
- renderer tests prove full, compact, narrow, ASCII, no-colour, reduced-motion,
  loading, failure, and selected-node wording;
- CLI and documentation tests prove machine output, history, and terminal
  restoration remain unchanged.

## Live PostgreSQL evidence

```bash
cargo xtask db up
cargo test --locked --test postgres_integration plan
cargo xtask db down
cargo xtask db status
```

The live check must record the PostgreSQL version and whether plain and TLS
connections passed. A Unix-socket result is a skip unless
`IGNATIUS_TEST_PG_SOCKET_URI` is configured. `cargo xtask db status` must report
`Not running.` after teardown.

## Manual terminal evidence

In a real interactive session:

1. Connect to a disposable database and put a nested SELECT under the cursor.
2. Open the command palette, search `plan`, choose the plain plan, and confirm
   the tree is readable without changing the editor or history.
3. Search `analyze`, verify the warning before confirming, then compare actual
   rows, time, loops, and the marked slow node.
4. Move, expand, collapse, and dismiss the plan; verify the prior result returns
   without a rerun.
5. Repeat at full, compact, narrow, ASCII, no-colour, reduced-motion, Warp,
   Windows Terminal, Linux terminal, and with a screen reader.

Manual platform evidence remains separate from automated renderer tests.
