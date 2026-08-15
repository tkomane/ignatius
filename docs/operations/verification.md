# Verification

## The full command

```bash
cargo xtask verify
```

It runs every gate below in the order that fails fastest, continues past a
failure so one run surfaces every problem, prints a summary, and exits non-zero
if any gate failed. Underneath it is exactly this:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --lib
cargo test --test cli_contract
cargo test --test postgres_integration     # needs IGNATIUS_TEST_PG_URI
```

Run them in that order: the fast gates fail fastest. Without
`IGNATIUS_TEST_PG_URI` the integration tests skip and say so on stderr. **A skip
is not a pass.** Report it as a skip.

## Required gates before merging

| Gate | Command | Blocking |
| --- | --- | --- |
| Formatting | `cargo fmt --check` | Yes |
| Lints | `cargo clippy --all-targets -- -D warnings` | Yes |
| Unit and layout tests | `cargo test --lib` | Yes |
| CLI contract | `cargo test --test cli_contract` | Yes |
| PostgreSQL integration | `cargo test --test postgres_integration` | Yes |
| Compilation on every target | `cargo check --target ...` in CI | Yes |
| Dependency advisories and licences | `cargo deny check` in CI | Yes |
| Secret scan | gitleaks in CI | Yes |
| Documentation and release notes updated | Review | Yes, when behaviour changes |

## Evidence recorded on 2026-08-16

macOS 26.6.1 on Apple silicon, rustc 1.97.1 (Homebrew), against both the plain
and TLS disposable services from `postgres:18.4-alpine`, at committed HEAD
`42aae86`.

**Automated**: formatting clean, `cargo clippy --all-targets -- -D warnings`
clean, 475 library tests, 38 CLI contract tests, 38 PostgreSQL integration tests
and 3 pseudo-terminal tests, all passing. That run covers the editor and its
undo, syntax colouring, the expanded row view and cell inspector, result
filtering, the statement history and its refusals, object definitions and
dependencies, the object tree's own read-only connection, connection profiles,
the password prompt in both surfaces, and configurable key bindings.

**Run directly rather than through `cargo xtask verify`.** The workspace's xtask
crate was being edited concurrently by another tool at the time, so the five
gates were run against the `ignatius` package individually. The gates are the
same commands the xtask runs; what is missing is only the convenience of one
invocation.

**Terminal lifecycle, captured from a real pseudo-terminal.** The client was run
under `script`, sent Ctrl+Q, and the byte stream captured. In order:

```text
entry:  ESC[?1049h   enter alternate screen
        ESC[?2004h   enable bracketed paste
        ESC[?25l     hide cursor
exit:   ESC[?25h     show cursor
        ESC[?2004l   disable bracketed paste
        ESC[?1049l   leave alternate screen
```

Every mode enabled was disabled, in reverse order, with the alternate screen left
last. No mouse capture sequence appears, because mouse capture is off by default.
The process exited 0 and the shell was usable afterwards.

**Interactive execution.** With the pty sized to 110x30, the client rendered the
header (`[PROD] [read-write]`, the target, `TLS not negotiated`), the editor with
its starter query, an empty-state message in the results pane, and the footer
hints. Sending F5 produced `Completed`, `1 row`, and the value `ignatius_demo` in
the grid. Ctrl+Q exited cleanly.

**Live CLI behaviour.** `connect --check` reported five passing stages and warned
truthfully that the connection was not encrypted. A query against a missing
relation reported SQLSTATE 42P01 with a cause and a next action, exited 7, and
wrote nothing to stdout. A value containing `ESC[2J` was rendered as `\x1B[2J`.
NULL, an empty string and the text `NULL` were each displayed distinctly. The
plain-mode subprocess checks also proved line-oriented input,
production-write confirmation, server-confirmed cancellation, failed-
transaction recovery wording and screen-reader-safe output.

## What has not been verified

- **Windows and Linux by hand.** CI builds and tests both platforms, but nobody
  has opened the full-screen client there. The same is true of Warp's renderer
  and a live terminal resize.
- **PostgreSQL 15 and 17.** CI exercises 14, 16 and 18; the local server is 18.4.
- **Windows terminal restoration.** The Unix pty test passes; the equivalent
  ConPTY test remains open work.
- **Panic and signal restoration as a process-level claim.** The RAII and panic
  paths are covered in code and unit tests; only the normal Unix pty path is
  exercised end to end here.
- **Feature 005 editor acceptance.** The editor unit tests pass, but the
  committed key path still calls `Editor::insert('\n')` instead of the
  indentation-preserving newline method, and cursor movement does not end an
  undo coalescing run. The focused keymap and real eight-line terminal
  acceptance checks remain open with Claude's source follow-up.

These are tracked in `docs/status.md`.

## Adding a test

Name it as the claim it makes: `a_stale_result_cannot_overwrite_a_newer_query`
rather than `test_update_3`. A failure message should tell someone what broke
without them reading the test body.

Assert meaning, not pixels. A layout test checks that the production marker and
the row count are present, not that a 3 KB buffer is byte-identical, because the
second kind passes while the interface silently loses its meaning.
