# Status

**Updated: 2026-08-16.** This file is the resumption point. Read it, then check
`git log`, `specs/001-foundation-vertical-slice/tasks.md`, and the working tree
before trusting anything else.

## Where the work is

**Current feature**: the four open decisions are closed, and the product has a
direction rather than a finished roadmap. The owner named the real requirement
on 2026-08-16 - Microsoft Entra ID against Azure Database for PostgreSQL, for
the COI application - and it turns out not to need libpq at all: Entra
authenticates with a token presented as the password over TLS, which this
adapter already does. ADR-0012 records that and supersedes ADR-0009. The
credential store is rejected (ADR-0011), copy goes through OSC 52 opt-in
(ADR-0013), and `docs/product/experience-roadmap.md` sets out what "the best
experience anyone has used" means in concrete features, first of which is
completion that knows the schema. Specifications exist for Entra
(`specs/011-entra-authentication/`) and completion
(`specs/012-schema-completion/`); neither is implemented.

Also fixed: the two Feature 005 editor defects the audit of `555203f` found.
Enter now reaches `Editor::insert_newline()` so indentation survives a line
break through the real key path, and every cursor movement ends the undo
coalescing run so typing either side of a move is two undo steps. Both are
proven by tests that were watched failing with the fix removed - and the first
version of the movement test passed without the fix, so it was rewritten until
it could fail.

**Previous**: consolidation, and the bug it found. Five chords - the
definition panel, dependencies, save, open and write - were documented, palette-
listed and bound to nothing: the features worked and no key reached them. A new
suite compares the keymap document, the bindable action names, the profile
fields and the exit codes against the build in both directions, and it fails when
either side drifts. That is checked by making each side drift on purpose.

**Previous**: consolidation. The password prompt's happy path is now
proven under a real pseudo-terminal against the container - typed, connected,
and the password absent from the transcript afterwards - which was the largest
gap in recent work. Four features had been built without specifications, against
the constitution's own rule; `specs/010-asking-and-keeping/spec.md` records them
and says plainly that it was written after the fact.

**Previous**: writing the rows on screen to a file, and saved queries.
The export from the client says how many rows it will write and what it will not
contain before it writes anything, because what is on screen is not what the
query returned whenever the result was truncated.

**Previous**: saved queries. `Ctrl+K w` and `Ctrl+K o` write and open
ordinary `.sql` files in the directory `config paths` has advertised since the
first release and nothing had ever used. A name is checked before it reaches the
filesystem, and that check is tested as the security boundary it is.

**Previous**: `config init`, a password prompt in plain mode, and named
connections. ADR-0011 proposes where a stored credential would live and is
waiting on the owner: `.pgpass` already exists and is shared with `psql`, so
whether to carry a keyring dependency at all is a decision, not a default.
Plain mode now asks without echoing when the server demands a password, on the
same rule the client follows: a terminal at both ends or no question at all.

**Previous**: named connections. A `[profiles]` table says where a
database is and how it is classified, reached as `@name` or `--profile name`. The
classification is the reason it exists: written down once, the production write
guard applies every time. A profile that tries to hold a password is refused by
name. Specified in `specs/009-connection-profiles/spec.md`.

**Previous**: a password prompt. When the server refuses for want of a
password, the client asks for one and retries; the characters are masked, held
only until the attempt is made, and printed by nothing. That closes half of the
"no password prompting" gap - the full-screen client. Plain mode still relies on
the other password routes.

**Previous**: configurable key bindings. A `[keys]` table in
`config.toml` replaces the built-in binding for an action; an unknown action
name, an unreadable key and two actions on one key are all errors reported before
the terminal is taken. That completes roadmap Feature 007 apart from
hand-verification, which is evidence rather than code.

**Previous**: dependency navigation. `Ctrl+K y` lists what an object is
used by and what it depends on, with the reason for each edge, following the two
edges PostgreSQL records - view rewrite rules and foreign keys - and saying that
a function body's reads are not among them. That completes roadmap Feature 005.

**Previous**: the object tree has a connection of its own, so a long
query cannot delay it. Same resolved target, moved rather than re-derived, with
two deliberate differences the server can see: an `application_name` saying it is
the tree, and a read-only session. If it cannot be opened the tree shares the
session's connection and the header says so.

**Previous**: indexes and extensions in the object tree, and object
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
| Every action has a way in | A test over the list of actions rather than the list of keys |
| The documentation matches the build | A suite comparing keys, actions, profile fields and exit codes both ways |
| A typed password opens a session and is not echoed | pty test against the container: typed, connected, absent from the transcript |
| An export from the client says what it leaves out | Reducer test over a truncated and filtered result |
| A saved query's name can never escape its directory | Unit tests over separators, `..`, drive letters and control characters |
| A pipe is never asked for a password | Subprocess test with stdin closed: it exits 5 rather than waiting |
| A profile classified as production guards it | Subprocess test: the write is refused with no flag typed |
| A profile cannot hold a password | Subprocess test: refused by name, and the value is not echoed |
| A key binding that would do nothing is refused | Subprocess test: three broken files, each exiting 3 with what to fix |
| Dependencies are read in both directions | Integration test over a view and the table it reads |
| The tree's connection is separate and read-only | Integration test: two backend pids, and a write refused with SQLSTATE 25006 |
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
4. **GSSAPI, Kerberos and Windows SSPI are unsupported**, and stay that way.
   ADR-0012 closed the libpq migration: the enterprise requirement that actually
   exists is Entra ID, which is a token over TLS and needs no driver capability
   this adapter lacks. If a real Kerberos requirement appears the decision
   reopens on the same terms.
5. **No OS credential store, by decision.** ADR-0011 is rejected. The routes are
   a password file, the environment, the connection string and the prompt, and
   `.pgpass` is shared with `psql`.
5a. **Entra ID is specified and not implemented.** `specs/011-entra-authentication/`
   describes it; no code exists. The COI server cannot be reached by this build.
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
11. **Feature 005 editor: the two source defects are fixed, the terminal
    acceptance scenario is not written.** Enter now routes to
    `Editor::insert_newline()` and every movement ends the undo coalescing run,
    both proven by tests confirmed to fail without the fix. What remains from
    that audit is an eight-line terminal acceptance scenario driving the editor
    at a real small size; until it exists, Feature 005 is corrected but not
    complete.

## Decisions taken

- **Name**: Ignatius, confirmed 2026-08-15.
- **Driver**: `tokio-postgres` stays. ADR-0012 supersedes ADR-0009 on
  2026-08-16 and closes Feature 001a unimplemented: the enterprise requirement
  is Entra ID, which is a token presented as a password over TLS. The `unsafe`
  exception ADR-0009 granted is withdrawn, ADR-0010 is moot, and the binary
  stays self-contained.
- **Credential store**: rejected, ADR-0011, 2026-08-16.
- **Copying a value out**: OSC 52, off unless configuration turns it on, no new
  dependency. ADR-0013, 2026-08-16. Decided on the environment it will be used
  in: a Linux clipboard crate needs a display that WSL does not have, and
  Windows Terminal implements the write half of OSC 52 and deliberately not the
  read half.
- **Repository**: private, at `tkomane/ignatius`. Publishing beyond that is
  deferred; the options are in `docs/operations/release.md`.

## Decisions still pending

1. **Trademark search and a domain**, before publishing only. Neither blocks
   development.

Nothing else is waiting on the owner. The four that were are closed.

## Next actions, in order

1. **Implement Entra ID authentication** (`specs/011-entra-authentication/`).
   It is what stands between this build and the database the owner actually
   wants to use it against.
2. **Implement completion that knows the schema**
   (`specs/012-schema-completion/`). First of the experience roadmap, and the
   one that decides whether the product's claim about cognitive load is true.
3. Hand-verification on Windows and in WSL, using the artefact built on
   2026-08-16. A static musl binary covers WSL and Linux including the Unix
   socket path; a Windows-native `.exe` is a separate artefact and separate
   evidence, and neither substitutes for the other.
4. Implement copy through OSC 52 (ADR-0013), which closes Feature 004.
5. The remaining experience roadmap in order: errors that point at the problem,
   a result grid that can be worked, finding your way without being told, a
   readable plan.
6. An eight-line terminal acceptance scenario for the editor, the last item from
   the Feature 005 audit.
7. A terminal-restoration test for Windows, which needs ConPTY (T055a).
8. Drive `--plain` with VoiceOver on macOS and NVDA on Windows by hand. The
   absence of escape sequences is proven; the experience is not.
9. Release packaging (Feature 008). Codex is working on this; the planning
   package is in `specs/008-release-experience/` and the uncommitted work in the
   tree is theirs.
