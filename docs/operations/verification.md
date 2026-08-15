# Verification

## The full command

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

## Evidence recorded on 2026-08-15

macOS 26.6.1 on Apple silicon, rustc 1.97.1 (Homebrew), against
`postgres:18.4-alpine` with SCRAM-SHA-256 required.

**Automated**: `cargo fmt --check` clean; `cargo clippy --all-targets -D
warnings` clean; 199 library tests, 20 CLI contract tests and 16 PostgreSQL
integration tests passed.

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
NULL, an empty string and the text `NULL` were each displayed distinctly.

## What has not been verified

- **Windows and Linux.** Nothing has run there. The development machine has only
  the `aarch64-apple-darwin` standard library installed.
- **PostgreSQL 14 through 17.** Only 18.4 has been exercised.
- **Exit codes 5, 6, 8 and 9 at subprocess level.** Asserted in library tests
  only; 9 has no producer until export exists.
- **Terminal restoration after a panic or SIGTERM, automatically.** The escape
  sequence balance is unit-tested and the normal exit path is proven above, but
  the panic and signal paths have not been driven end to end in CI.

These are tracked in `docs/status.md`.

## Adding a test

Name it as the claim it makes: `a_stale_result_cannot_overwrite_a_newer_query`
rather than `test_update_3`. A failure message should tell someone what broke
without them reading the test body.

Assert meaning, not pixels. A layout test checks that the production marker and
the row count are present, not that a 3 KB buffer is byte-identical, because the
second kind passes while the interface silently loses its meaning.
