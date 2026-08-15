# Status

**Updated: 2026-08-15.** This file is the resumption point. Read it, then check
`git log`, `specs/001-foundation-vertical-slice/tasks.md`, and the working tree
before trusting anything else.

## Where the work is

**Current feature**: 001, foundation and proven vertical slice. Implemented and
verified on macOS. Cross-platform verification is the main outstanding gap.

**Product name**: `ignatius` is an internal codename. The naming gate in
`docs/product/landscape.md` has not been run and must be before any public
release. Renaming touches only `src/branding.rs`.

## Last green verification

Run on 2026-08-15, macOS 26.6.1 on Apple silicon, rustc 1.97.1 (Homebrew),
against `postgres:18.4-alpine` in Docker with SCRAM-SHA-256 required.

| Gate | Result |
| --- | --- |
| `cargo fmt --check` | Clean |
| `cargo clippy --all-targets -- -D warnings` | Clean |
| `cargo test --lib` | 199 passed |
| `cargo test --test cli_contract` | 20 passed |
| `cargo test --test postgres_integration` | 16 passed |
| Live pty run | Client rendered, F5 executed a query, Ctrl+Q restored the terminal |

Live evidence recorded in `docs/operations/verification.md`, including the exact
escape sequences captured on entry and exit.

## Known gaps

These are real and none of them is hidden anywhere else:

1. **Only macOS has been verified.** Windows and Linux builds are authored but
   unverified: this machine has only the `aarch64-apple-darwin` standard library
   installed and no rustup to add targets. Everything about those platforms is a
   CI claim until CI runs.
2. **Only PostgreSQL 18.4 has been tested.** The supported window is 14 to 18.
   The CI matrix is written but has not run.
3. **Exit codes 5, 6, 8 and 9 lack subprocess-level evidence.** They are produced
   and asserted at library level. Code 9 has no producer until export lands in
   Feature 004.
4. **`sslmode=verify-ca` is deliberately unimplemented** and refuses with an
   explanation. See ADR-0004.
5. **No credential store, `.pgpass`, or service file support.** `PGSERVICE`,
   `PGSERVICEFILE` and `PGPASSFILE` are reported as unread rather than ignored.
   Feature 002.
6. **Terminal restoration is proven by unit tests and one manual pty run**, not
   yet by an automated test in CI. Warp's own renderer and a live terminal
   resize have not been exercised by hand; the pty run used a forced size.
7. **The CI workflow has never run**, and `cargo-deny` has never been executed
   locally, so the licence allowlist in `deny.toml` is untested policy that may
   need adjusting on its first run.
8. **`rust-toolchain.toml` is inert on the development machine**, which uses a
   Homebrew rustc rather than rustup. This is an environment limitation, not a
   defect.

## Decisions pending the owner

1. **Product name.** Needs the naming gate before any public release. Nothing
   else is blocked by it.
2. **libpq versus the pure-Rust driver.** ADR-0003 chose tokio-postgres and
   accepted that service files, `.pgpass` and `verify-ca` become our work. If the
   unsupported list in `docs/support/compatibility.md` stops shrinking, that
   trade should be revisited before Feature 002 hardens around it.
3. **Whether to publish at all, and where.** Nothing has been pushed. No remote
   is configured.

## Next actions, in order

1. Run the CI workflow and fix what it finds (T050 is written but unproven).
2. Verify on Windows 11 in Windows Terminal with PowerShell 7 (T051).
3. Verify on Linux, including the Unix socket path (T052).
4. Extend the server matrix to PostgreSQL 14 through 18 (T053).
5. Add subprocess evidence for exit codes 5, 6 and 8 (T054).
6. Add an automated terminal-restoration test to CI (T055).
7. Then start Feature 002: profiles, credential store, `.pgpass`, service files.
