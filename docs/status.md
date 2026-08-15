# Status

**Updated: 2026-08-15.** This file is the resumption point. Read it, then check
`git log`, `specs/001-foundation-vertical-slice/tasks.md`, and the working tree
before trusting anything else.

## Where the work is

**Current feature**: 003, credential routes. Password files and service files
are implemented and verified against a real server.

**Previous**: 002, object navigation. Implemented and verified on macOS
and in CI. Feature 001 is complete apart from the hand-verification gaps below.

The owner asked for navigation ahead of the libpq migration in ADR-0009. The
reorder is safe because this feature touches no credential route: it reads the
catalogue over an existing session.

**Product name**: Ignatius, confirmed by the owner on 2026-08-15. The
availability check is recorded in `docs/product/landscape.md`. A trademark search
and a domain remain outstanding before publishing; neither blocks development.

## Last green verification

Run on 2026-08-15, macOS 26.6.1 on Apple silicon, rustc 1.97.1 (Homebrew),
against `postgres:18.4-alpine` in Docker with SCRAM-SHA-256 required.

| Gate | Result |
| --- | --- |
| `cargo fmt --check` | Clean |
| `cargo clippy --all-targets -- -D warnings` | Clean |
| `cargo test --lib` | 288 passed |
| `cargo test --test cli_contract` | 20 passed |
| `cargo test --test postgres_integration` | 22 passed |
| Live pty run | Client rendered, the run key executed a query, Ctrl+Q restored the terminal |
| CI, all jobs | Green on 2026-08-15: macOS, Windows and Linux, plus PostgreSQL 14, 16 and 18 |
| Object tree, live | Expanded schemas and groups in a pty against the demo database, including a table named to break identifier interpolation |

Live evidence recorded in `docs/operations/verification.md`, including the exact
escape sequences captured on entry and exit.

## Known gaps

These are real and none of them is hidden anywhere else:

1. **Windows and Linux are CI-verified, not hand-verified.** CI builds them and
   runs the unit, layout and CLI contract tests plus a startup smoke test on
   both. Nobody has yet opened the full-screen client on either platform and
   used it. Warp's own renderer and a live terminal resize are likewise
   unverified anywhere.
2. **Database integration runs on Linux only.** PostgreSQL 14, 16 and 18 all
   pass there. The macOS and Windows jobs do not connect to a server, so the
   protocol claims rest on the Linux matrix and the local macOS runs.
3. **Exit code 9 has no producer** until export lands in Feature 004. Codes 5, 6
   and 8 now have subprocess-level evidence.
4. **`sslmode=verify-ca` is deliberately unimplemented** and refuses with an
   explanation. See ADR-0004.
5. **No OS credential store and no password prompting.** Password files and
   service files are supported; the credential store and profiles are not.
6. **Terminal restoration is proven automatically on Unix**, by a test that runs
   the client under a real pty and reads the bytes. The Windows equivalent needs
   ConPTY and has not been written. Warp's own renderer and a live terminal
   resize have not been exercised by hand; the pty runs use a forced size.
7. **The secret scan was previously scanning nothing.** It walks the commit
   range of a push, which the default shallow checkout could not resolve, so it
   reported no leaks after scanning zero bytes. Fixed on 2026-08-15 by fetching
   full history for that job. Every earlier green run of that gate should be read
   as "did not run".
8. **Branch protection is unavailable.** Required status checks need a paid
   GitHub plan on a private repository. `cargo xtask install-hooks` runs the same
   gates before every push as the local stand-in, and CI runs on every push
   regardless, but nothing prevents a push that skips the hook.
9. **`rust-toolchain.toml` is inert on the development machine**, which uses a
   Homebrew rustc rather than rustup. This is an environment limitation, not a
   defect.

## Decisions taken

- **Name**: Ignatius, confirmed 2026-08-15.
- **Driver**: moving to libpq, confirmed 2026-08-15. See ADR-0009, which
  supersedes ADR-0003 and records the costs: an `unsafe` exception scoped to the
  adapter, a changed Windows distribution story, and a concurrency model that
  needs its own ADR.
- **Repository**: private, at `tkomane/ignatius`. Publishing beyond that is
  deferred; the options are in `docs/operations/release.md`.

## Decisions still pending

1. **Trademark search and a domain**, before publishing only. Neither blocks
   development.
2. **How libpq is bundled on Windows**, which ADR-0009 leaves open until the
   migration is scoped.

## Next actions, in order

1. Migrate the PostgreSQL adapter to libpq (ADR-0009). The driver-independent
   credential routes are now done, which is what that ADR sequenced first.
2. Open the client by hand on Windows 11 in Windows Terminal, and on Linux,
   including the Unix socket path (T051, T052). CI proves it builds and its
   tests pass; it does not prove the interface is usable there.
3. A terminal-restoration test for Windows, which needs ConPTY (T055a).
4. Migrate the PostgreSQL adapter to libpq (ADR-0009), before profiles harden
   on the current model.
5. Then connection profiles, credential store, `.pgpass`, service files.
6. Object explorer hardening: DDL inspection, dependencies, indexes and
   extensions in the tree, and a dedicated metadata connection so a long query
   cannot delay it.
