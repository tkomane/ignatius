# Quickstart: Errors that point at the problem

## Focused pure and reducer checks

```bash
cargo test error_location
cargo test diagnostic
cargo test app::editor
cargo test app::update
cargo test ui::layout
cargo test --test cli_contract failed_json_query
git diff --check
```

Expected evidence is separate: pure mapping proves character-safe offsets,
reducer tests prove stale-job and stale-buffer behavior, and renderer tests prove
what is visible. A green focused run does not prove a live PostgreSQL position.

## Disposable PostgreSQL evidence

```bash
cargo xtask db up
cargo test --test postgres_integration a_server_error_carries_sqlstate_and_a_position
cargo test --test postgres_integration constraint
```

The live tests prove PostgreSQL 18.4 behavior in the disposable environment.
They do not prove PostgreSQL 14 through 17 or platform terminal behavior. The
Unix-socket test remains a skip unless `IGNATIUS_TEST_PG_SOCKET_URI` is set.

## Full repository gate

```bash
cargo --locked xtask verify
```

Record formatting, lints, unit and contract tests, PostgreSQL plain/TLS/socket
results, dependency/security checks, and documentation checks separately. A
socket or platform check that is unavailable is an explicit evidence gap.

## Manual TUI check

1. Start the disposable PostgreSQL service and connect with the normal TUI.
2. Enter a multi-line buffer with a malformed second statement, for example:
   `SELECT 1;\nSELECT * FRM orders;`.
3. Run the buffer and confirm the editor caret and `!` marker land on `FRM`, the
   error says statement 2 with line and column, and the SQL remains unchanged.
4. Edit the marked statement and confirm the marker is removed and the error is
   labelled as belonging to the previous submission.
5. Trigger a fixture constraint violation and confirm object context is visible
   before technical detail. Reload or disable the catalogue and confirm the
   limitation is stated without replacing server fields.
6. Repeat with `--ascii --color never` and in a narrow terminal. Confirm words,
   line numbers, and caret meaning survive.

## Plain and JSON check

```bash
printf 'SELECT 1;\nSELECT * FRM orders;\n' | ignatius --plain connect "$DATABASE_URL"
ignatius query "$DATABASE_URL" -c 'SELECT * FRM orders' --format json >result.json 2>error.txt
```

Review `error.txt` for the line-oriented location and `result.json` for the
existing failed-query stdout contract. Review all output for control sequences,
credentials, tokens, and unintended SQL or result data before treating it as
support evidence.
