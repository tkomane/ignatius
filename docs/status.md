# Status

**Updated: 2026-08-16.** This file is the resumption point. Read it, then check
`git log`, `specs/001-foundation-vertical-slice/tasks.md`, and the working tree
before trusting anything else.

## Where the work is

**Current feature**: indexes and extensions in the object tree, and object
definitions. `Ctrl+K d` on an object in the tree
shows what it is, coloured by the same lexer that colours the editor. Views,
indexes and functions come from PostgreSQL's own renderers; a table is assembled
from its columns, constraints and indexes and is labelled as a description rather
than a script. Specified in `specs/007-object-definitions/spec.md`.

**Previous**: a filter over the result rows, on the same key that filters
the object tree. It searches retained rows, which is all there is to search, and
the count line says so: `matching 3 of 10000 retained rows, of 200000 returned`.
Row numbers stay the rows' own, and the inspector and expanded view follow the
selection to the real row. That leaves copying a value out as the only unbuilt
part of Feature 004, and it is waiting on the clipboard decision.

**Previous**: the statement history. Interactive sessions record what ran,
`Ctrl+K s` searches it and puts a statement back in the editor, and every control
over it is explicit: a credential-bearing statement is never written, recording
pauses for a run (`--no-history`) or a session (`Ctrl+K v`), configuration can
switch it off entirely, and `ignatius history list` and `clear` show and remove
what is kept. Scripted `query` runs are not recorded. Specified in
`specs/006-statement-history/spec.md`.

**Previous**: syntax colouring, and a real editor for the SQL buffer.
Keywords, literals, numbers, comments, quoted identifiers and placeholders are
coloured from the statement lexer's own rules rather than a second set, and a bar
in the gutter marks the statement `Ctrl+T` would run. Vertical movement with a
remembered column, line and buffer ends, word movement and deletion, movement by
a screenful, and undo and redo a word at a time including over loaded text. The
window follows the cursor rather than being stored, so there is one source of
truth for where the cursor is. Specified in `specs/005-sql-editing/spec.md`.
Syntax highlighting and query history are the rest of roadmap Feature 003.

**Previous**: reading a result. `Ctrl+K x` lays one row down the screen,
one column per line; Enter on a cell opens it in full, wrapped, scrollable, with
what the value is stated in words. Specified in
`specs/004-result-inspection/spec.md`. That is two of the four items Feature 004
had left. Result filtering and copying a value out remain; copying waits on a
decision about the clipboard route, not on time, because it moves data out of
this process.

**Previous**: a plain, line-oriented client behind `--plain`. No
alternate screen, no raw mode, no cursor addressing: it works in `TERM=dumb`,
stays in the scrollback, and can be driven by a pipe. It is the accessibility
item that had been unchecked in the Feature 001 quality checklist since it was
written.

**Previous**: transaction state, read from the server rather than inferred.

**Previous**: 006, production-aware safety. A production-classified
connection holds back writes until they are confirmed, and `--read-only` asks
the server to enforce it.

**Previous**: transport hardening. `verify-ca` and client certificates
are implemented natively and tested against a TLS server, which closes the
largest honesty gap the project had: the headline differentiator had never been
exercised against a server that speaks TLS.

**Previous**: 004, result export. Streaming export is implemented and
verified with a 200,000-row result in 13 MB of resident memory.

**Previous**: 003, credential routes. Password files and service files
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

Run on 2026-08-16 at committed HEAD `49df20a`, macOS 26.6.1 on Apple silicon,
rustc 1.97.1 (Homebrew), against `postgres:18.4-alpine` both plain and with TLS.

| Gate | Result |
| --- | --- |
| `cargo xtask verify` | All five gates pass |
| Library tests | 433 passed |
| CLI contract tests | 33 passed |
| PostgreSQL integration tests | 33 passed |
| Terminal restoration, in a pty | 3 passed |
| CI, all jobs | Green: macOS, Windows and Linux, plus PostgreSQL 14, 16 and 18 |

Live evidence recorded in `docs/operations/verification.md`.

## What is proven, and by what

| Claim | Evidence |
| --- | --- |
| A query runs and renders | Integration tests, plus a pty capture of the client |
| A long statement is cancelled | Server confirms SQLSTATE 57014, in under a second |
| Results stay bounded | 5000 rows capped at 10 with the true count kept |
| An export stays bounded | 200,000 rows exported in 13 MB resident memory |
| An interrupted export keeps its rows | Subprocess test: exit 9, `.partial` file, count reported |
| verify-full checks the name | Refuses a certificate that does not cover the address used |
| verify-ca does not | Accepts that same certificate, against the same server |
| A client certificate authenticates | Connects as `cert_user` with no password at all |
| TLS is never silently dropped | A server refusing TLS exits 6, not 4 |
| Hostile values cannot drive the terminal | Escapes rendered as text, in values and object names |
| Hostile object names cannot alter SQL | A table named to break interpolation is listed and queried |
| Passwords never leak | Subprocess tests over failing connections |
| A failed transaction is reported | Read from the server, with ROLLBACK named as the way out |
| Plain mode emits nothing screen-reader-hostile | Subprocess test under `TERM=dumb`: no escape sequences at all |
| Plain mode still guards production | Subprocess test: a write to a production target is confirmed in words |
| An object's definition is what the server renders | Integration tests over a view, a function and a table |
| A hostile object name is safe to describe | Integration test: the definition is read, `orders` still exists |
| A credential never reaches the history file | Subprocess test: the statement runs, the file does not hold it |
| A paused session records nothing and says so | Subprocess test over a real session |
| The history file is the owner's alone | Subprocess test asserting 0600 after writing and after trimming |
| Colouring never alters the buffer | Property test rebuilding the text from its tokens |
| A buffer of any length can be navigated | Layout test: the window follows the cursor and the line numbers stay right |
| A deleted word comes back | Reducer test through the real key actions |
| A value is never abbreviated without recourse | The inspector renders it whole, wrapped, with its position stated |
| NULL, empty and the text NULL are distinguishable | Layout test asserting the words for each |
| Every documented exit code | Produced by a real invocation |

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
3. Every documented exit code now has a real producer with subprocess-level
   evidence, including 9 from an interrupted export.
4. **GSSAPI, Kerberos and Windows SSPI are unsupported**, and are now the only
   remaining reason to adopt libpq. See the addendum in ADR-0009: the other four
   capabilities it was accepted for have been implemented natively.
5. **No OS credential store and no password prompting.** Password files and
   service files are supported; the credential store and profiles are not.
6. **No screen reader has been used with this.** `--plain` is built and proven
   to emit no escape sequences under `TERM=dumb`, which is the mechanical part.
   Whether it is pleasant with VoiceOver or NVDA is unknown, because neither has
   been driven by hand.
7. **Terminal restoration is proven automatically on Unix**, by a test that runs
   the client under a real pty and reads the bytes. The Windows equivalent needs
   ConPTY and has not been written. Warp's own renderer and a live terminal
   resize have not been exercised by hand; the pty runs use a forced size.
8. **The secret scan was previously scanning nothing.** It walks the commit
   range of a push, which the default shallow checkout could not resolve, so it
   reported no leaks after scanning zero bytes. Fixed on 2026-08-15 by fetching
   full history for that job. Every earlier green run of that gate should be read
   as "did not run".
9. **Branch protection is unavailable.** Required status checks need a paid
   GitHub plan on a private repository. `cargo xtask install-hooks` runs the same
   gates before every push as the local stand-in, and CI runs on every push
   regardless, but nothing prevents a push that skips the hook.
10. **`rust-toolchain.toml` is inert on the development machine**, which uses a
   Homebrew rustc rather than rustup. This is an environment limitation, not a
   defect.
11. **Feature 005 editor acceptance is not complete despite the committed
    implementation.** A direct audit of `555203f` found that Enter routes to
    `Editor::insert('\n')` rather than `Editor::insert_newline()`, so the
    indentation requirement is not wired through the real key path. Cursor
    movement also does not end an undo coalescing run, so typing on both sides
    of a move can share one undo step. Focused keymap coverage and a real
    eight-line terminal acceptance scenario are still absent. Claude owns the
    source follow-up in `src/app/` and `src/ui/`; do not claim Feature 005
    complete until those paths are corrected and reverified.

## Decisions taken

- **Name**: Ignatius, confirmed 2026-08-15.
- **Driver**: ADR-0009 accepted the move to libpq on 2026-08-15. The
  implementation remains decision-gated in Feature 001a because four of the
  five original capability gaps are now implemented natively; the remaining
  GSSAPI, Kerberos or Windows SSPI requirement must still justify the cost.
  ADR-0009 supersedes ADR-0003 and records the costs: an `unsafe` exception
  scoped to the adapter, a changed Windows distribution story, and a
  concurrency model that needs its own ADR.
- **Repository**: private, at `tkomane/ignatius`. Publishing beyond that is
  deferred; the options are in `docs/operations/release.md`.

## Decisions still pending

1. **Trademark search and a domain**, before publishing only. Neither blocks
   development.
2. **Whether the remaining enterprise authentication route justifies libpq**,
   after the T009 wrapper comparison in Feature 001a. If approved, decide how
   libpq is bundled on Windows before implementation.

## Next actions, in order

1. **Decision needed**: ADR-0009 adopted libpq for five capabilities; four are
   now implemented without it. The remaining one is GSSAPI and Windows SSPI.
   Whether the migration is still worth its cost is the owner's call.
2. Open the client by hand on Windows 11 in Windows Terminal, and on Linux,
   including the Unix socket path (T051, T052). CI proves it builds and its
   tests pass; it does not prove the interface is usable there.
3. A terminal-restoration test for Windows, which needs ConPTY (T055a).
4. Drive `--plain` with VoiceOver on macOS and NVDA on Windows by hand. The
   absence of escape sequences is proven; the experience is not.
5. **Decision needed**: how a value gets copied out. The system clipboard needs a
   crate and platform support; OSC 52 writes the value into the terminal, where
   it may be logged by the emulator. Neither is obviously right for a tool that
   handles other people's data, so it is the owner's call. Result filtering does
   not depend on it and can go first.
6. Migrate the PostgreSQL adapter to libpq (ADR-0009), before profiles harden
   on the current model.
7. Then connection profiles and an OS credential store. Password files and
   service files are already done.
8. Object explorer hardening: DDL inspection, dependencies, indexes and
   extensions in the tree, and a dedicated metadata connection so a long query
   cannot delay it.
