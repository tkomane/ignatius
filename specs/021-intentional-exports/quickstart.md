# Quickstart: Intentional export shapes

## Focused checks

```bash
cargo fmt --all -- --check
cargo check --locked
cargo test --locked cli::output --lib
cargo test --locked app::palette --lib
cargo test --locked app::update --lib
cargo test --locked ui::layout --lib
cargo test --locked --test documentation_matches_the_build
cargo test --locked --test cli_contract
git diff --check
```

## Manual interactive check

Use only a synthetic disposable profile and a result containing NULL, empty
text, quotes and Unicode. Run a query that returns rows, then:

1. press `Ctrl+K e` and confirm the format palette opens without writing a file;
2. search `json`, choose it, and confirm the path prompt says JSON;
3. type a path with a misleading extension and confirm the chosen format wins;
4. cancel the path and confirm no destination or partial file exists;
5. repeat with CSV, TSV, NDJSON and Markdown;
6. filter the result and confirm the note and file contain retained visible rows;
7. run `query --format insert --insert-table orders` against the synthetic
   server and inspect the quoted output without executing it automatically;
8. try INSERT without a table, with duplicate labels and with `--no-header`,
   and confirm each refuses before a connection attempt.

Do not use a production target or real credential for this check. A generated
INSERT file is data that may be executable; review it before applying it.

## Evidence boundary

The locked verifier proves pure encoders, reducer transitions, layout wording,
CLI contracts and disposable PostgreSQL paths that run. It does not prove
hand rendering in Warp, Windows Terminal, Linux terminals, SSH or a
screen-reader workflow.
