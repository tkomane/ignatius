# Quickstart: Connection picker

## Focused checks

```bash
cargo fmt --all -- --check
cargo test --locked app::connection_picker --lib
cargo test --locked app::palette --lib
cargo test --locked app::update --lib
cargo test --locked app::discovery --lib
cargo test --locked ui::keymap --lib
cargo test --locked ui::layout --lib
cargo test --locked cli::interactive --lib
cargo test --locked --test documentation_matches_the_build
cargo test --locked --test cli_contract
git diff --check
```

## Manual interactive check

Use a disposable configuration directory containing only synthetic profiles:

```toml
[profiles.orders-dev]
host = "127.0.0.1"
port = 55432
dbname = "ignatius_demo"
user = "ignatius_test"
sslmode = "disable"
environment = "development"
description = "safe local fixtures"

[profiles.orders-read]
host = "127.0.0.1"
port = 55432
dbname = "ignatius_demo"
user = "ignatius_test"
sslmode = "disable"
environment = "test"
read-only = true
description = "read-only checks"
```

Start the interactive client without a target and confirm:

1. the picker appears before a connection attempt;
2. the default row and both profiles are visible;
3. typing `read` finds the read-only profile;
4. the row says `test`, `read-only` and `TLS disable` in ASCII mode;
5. Enter connects only after a choice;
6. `Ctrl+K n` opens the picker after connection;
7. switching clears the old Results and Objects state but keeps the editor;
8. `Ctrl+K p` can still find Choose a connection;
9. Escape leaves the current session unchanged.

Do not use real credentials or a production target for this check. A real
database connection proves the selected route; it does not prove Windows,
Linux, SSH, multiplexer, Warp or screen-reader behaviour.

## Evidence boundary

The locked verifier proves the reducer, renderer, resolver integration, plain
and machine-output regressions, and disposable PostgreSQL paths that run. A
picker being rendered in a unit buffer is not hand evidence for a real terminal;
record those checks separately.
