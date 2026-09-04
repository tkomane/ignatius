# Quickstart: Copy a result value out

## Prerequisites

- Rust 1.90 or newer with the repository lockfile.
- A terminal capable of showing raw output for transport tests. OSC 52 support
  is not assumed and cannot be inferred from a successful write.
- Optional disposable PostgreSQL fixture for regression checks.

## Focused validation

```bash
cargo fmt --all -- --check
cargo check --locked
cargo test --locked clipboard --lib
cargo test --locked config --lib
cargo test --locked app::update --lib
cargo test --locked app::discovery --lib
cargo test --locked ui::keymap --lib
cargo test --locked ui::layout --lib
cargo test --locked --test documentation_matches_the_build
cargo test --locked --test cli_contract
git diff --check
```

Expected evidence is separated by boundary:

- transport tests prove exact OSC framing, base64 round-tripping, control-byte
  containment, one write/flush operation, size bounds, and redacted Debug;
- configuration tests prove omitted and explicit settings and the starter
  configuration contract;
- reducer tests prove disabled, NULL, empty, oversized, stale, cancelled, and
  confirmed paths, including source-row identity after filtering/sorting;
- renderer and keymap tests prove confirmation and outcome wording in full,
  compact, narrow, ASCII, no-colour, and reduced-motion modes, plus palette and
  `Ctrl+K c` discovery;
- CLI and PostgreSQL checks prove plain, JSON, NDJSON, history, export, SQL, and
  ordinary results do not emit OSC 52 output.

## Live terminal evidence

Use a disposable result containing ordinary text, an empty string, SQL NULL,
Unicode, newlines, and control-looking text. With `[clipboard] osc52 = false`,
invoke Copy selected value and verify that the client explains the setting and
writes nothing. With `[clipboard] osc52 = true`, invoke it again, review the
row/column and byte/character confirmation, cancel once, then confirm once.

The expected success wording is that the terminal sequence was sent and
clipboard acceptance is unconfirmed. A terminal that shows no clipboard change
is not a client failure if the write and flush succeeded. Do not paste real
secrets into the test result.

## PostgreSQL regression evidence

```bash
cargo xtask db up
cargo --locked xtask verify
cargo xtask db down
cargo xtask db status
```

The authoritative run must record the executed test counts separately from any
Unix-socket or other environment skips. The post-run status must report
`Not running.` after teardown. Clipboard itself is an interactive terminal
boundary and is not proven by PostgreSQL integration tests.

## Manual platform evidence

Repeat the interactive flow in Warp, Windows Terminal, a Linux terminal, over
SSH, and inside a multiplexer where available. Record only that the sequence
was written and whether the terminal visibly accepted it; acceptance is always
external evidence, not a client guarantee. Verify that the value is not shown
in the confirmation, status, diagnostics, or debug output, and that printable
`c` still inserts into the SQL editor.
