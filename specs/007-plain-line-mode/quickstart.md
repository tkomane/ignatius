# Quickstart: Plain line-oriented terminal mode

## Prerequisites

- Rust 1.97.1 and the repository toolchain available.
- Docker Desktop running for the disposable PostgreSQL services.
- No real credentials. The compose fixtures are synthetic.

## Start the disposable services

```bash
cargo xtask db up
```

The command prints the plain and TLS connection targets. Use the printed plain
target for the examples below.

## Run a readable multi-line session

```bash
printf 'SELECT order_id\nFROM orders\nORDER BY order_id;\n\\q\n' \\
  | cargo run --quiet -- --plain connect \
      'postgres://ignatius_test@127.0.0.1:55432/ignatius_demo'
```

Expected evidence:

- result data appears on stdout;
- prompts and connection messages appear on stderr;
- the output contains no ANSI escape sequences;
- the query completes only after the terminating statement boundary;
- the process exits successfully after `\\q`.

## Exercise quoting and commands

```text
\?
\c
SELECT 'a;b';
\q
```

The semicolon in the string must not end the statement early. `\\?` lists the
supported commands and `\\c` repeats the connection summary.

## Exercise the production guardrail

Use a synthetic target with the existing environment option, then enter a write
and answer the confirmation incorrectly or with an empty line. The expected
result is a textual cancellation and no statement sent to the database.

## Run focused tests

```bash
cargo test --lib cli::plain::tests
cargo test --lib ui::layout::tests::a_failed_transaction_with_an_error_still_shows_recovery_in_the_results -- --exact
cargo test --test cli_contract plain_mode_is_a_line_oriented_client_that_a_screen_reader_can_follow -- --exact
cargo test --test cli_contract plain_mode_asks_before_writing_to_a_production_target -- --exact
```

## Run the full gate

```bash
cargo xtask verify
```

With both disposable services running, the integration gate must run rather
than skip. The current local evidence is 358 library tests, 29 CLI-contract
tests and 33 PostgreSQL integration tests, plus formatting and lints.

## Evidence boundary

These commands prove automated stream, parser, safety and server behaviour on
the local macOS development environment. They do not prove a person has used
VoiceOver or NVDA, opened the full-screen client in Windows Terminal or Linux,
or exercised the Windows ConPTY restoration path. Those remain explicit
release follow-up actions.
