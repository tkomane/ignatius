# Quickstart: Explicit retained-result refresh

## Prerequisites

- Rust and Cargo from the pinned project toolchain.
- A disposable PostgreSQL instance for the existing integration checks.
- A terminal that can display the interactive client.

## Automated validation

From the repository root:

```text
cargo fmt --all -- --check
cargo test --locked --lib app::update::tests::refreshing_
cargo test --locked --lib app::discovery::tests::retained_result_refresh
cargo test --locked --lib ui::keymap::tests::core_actions_are_reachable_with_named_keys
cargo test --locked --lib ui::keymap::tests::discovery_snapshot_and_hints_use_configured_keys
cargo test --locked --lib ui::layout::tests::a_retained_result_refresh
cargo test --locked --test documentation_matches_the_build
cargo --locked xtask verify
```

Expected outcomes:

- an eligible single read result emits one ordinary execution effect;
- the editor is unchanged;
- named parameters prompt again and no value is printed by debug or history;
- multi-statement, write-classified, missing-result, busy and failed-transaction
  cases emit no execution effect;
- the portable `F6` shortcut, optional enhanced-terminal alias, palette entry,
  contextual hint and renderer wording stay in sync;
- the locked verifier reports every available gate separately from any
  semantic environment skip.

## Manual interactive check

1. Start the disposable database with `cargo xtask db up`.
2. Open the interactive client against it and run a simple `SELECT` that
   returns at least one row.
3. Focus Results and confirm the footer or command palette names Refresh
   retained result and its active key, normally `F6`. On a terminal with
   enhanced modifier reporting, `Ctrl+Shift+R` is also accepted.
4. Change the row outside the client, then invoke refresh once.
5. Confirm that the Results pane shows a running state, the editor text does
   not change, and the new value appears after completion.
6. Try a multi-statement buffer, a write result source, and a failed
   transaction. Confirm the action explains its boundary and sends nothing.
7. Run a result with a named parameter. Confirm refresh asks for the value
   again and that the history entry contains only the template.

Manual terminal, screen-reader and cross-platform checks remain evidence gates
separate from these automated scenarios. A skipped gate is not a pass.
