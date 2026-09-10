# Status

**Updated: 2026-09-11.** This file is the resumption point. Read it, then check
`git log`, `specs/001-foundation-vertical-slice/tasks.md`, and the working tree
before trusting anything else.

## Public repository and guardrails - 2026-09-11

**Decision.** The owner made `tkomane/ignatius` public, for an operational
reason: every CI job was rejected before it started because the private Actions
quota was exhausted and the account's spending limit is zero. Public
standard-runner minutes are free, and the previously blocked runs executed
immediately after the change. `docs/operations/release.md` had deferred public
source until CI was green on all three platforms; that prerequisite is not met
(Windows is red on the active W03 branch), so this is an owner decision that
overrides the deferred route, not completion of it.

**Pre-publication audit.** The full 97-commit history was scanned with the same
digest-pinned Gitleaks image CI uses: no leaks. No employer or client content,
private keys, credential files or internal endpoints were found. The only
credentials in the tree are the documented synthetic container fixtures. Commit
metadata includes the owner's public commit address.

**Enabled the same day.** Secret scanning and push protection, private
vulnerability reporting, CodeQL default setup (auto-detected rust, python and
actions; the setup API's explicit language list does not yet accept rust), an
Actions allowlist with SHA-pinning required, approval for all external fork
contributors, deletion of merged branches, a ruleset requiring a pull request
and all 13 CI checks on `main` with resolved review threads, and a ruleset
protecting `v*` tags from updates and deletion. The free plan does not include
non-provider secret patterns or validity checks; those remain disabled.

**Diagnosed and fixed: the Windows-only failure.** GitHub's Windows runner
image sets `PGPASSWORD` machine-wide for its preinstalled PostgreSQL.
`prepare_profile_target` read the process environment directly, so the picker
test's `target.password.is_none()` assertion failed on Windows for a reason that
had nothing to do with the profile under test. The picker body now takes an
`EnvSnapshot`: production still reads the real environment, and the test supplies
a fixed empty snapshot, matching the existing "resolution is a pure function"
rule in `src/connection/target.rs`. Focused evidence on 2026-09-11: `cargo fmt
--all --check`, workspace clippy with warnings denied, the three
`cli::interactive::tests::a_picker` tests, the 15 documentation-match contracts
and the 7 workflow contracts all pass locally. A fresh hosted Windows run is not
yet recorded. `Cargo.toml` named `github.com/tshiamo/ignatius` in its
`repository` field, which does not resolve to the public source; it now points
at `tkomane/ignatius`, and a later move of accounts can update the one field
again.

## W04 privacy probes and provider redaction fix - 2026-09-11

The W04 runtime-privacy probes found a real leak, and the fix ships with its
reproduction. A provider program that writes `Authorization: Bearer <token>` to
standard error produced a diagnostic in which the token survived, because the
single redaction implementation recognised URI userinfo and libpq keyword/value
secrets but not HTTP authorization headers. The redactor now covers inline
`Authorization` and `Proxy-Authorization` values (unquoted, quoted and
repr-style), keeps a known scheme visible, and removes bare bearer tokens.
Overlapping matches are merged, so a shorter match cannot leave a tail behind.
`docs/security/data-handling.md` gained the explicit redaction boundary, and
the threat-model row names the covered shapes and the residual folded or
unusual-shape limit.

Three probes accompany it: a live `IGNATIUS_LOG=debug` run with a named
parameter proves the expanded value reaches neither the log nor stderr; a
provider that never answers proves the timeout path yields a Connection
diagnostic; and the provider stderr regression test proves the redacted reason
survives rather than being dropped.

| Gate at the final source | Result |
| --- | --- |
| `cargo test --locked --lib diagnostics::redaction` | 15 passed |
| `cargo test --locked --lib connection::cloud` | 16 passed |
| `cargo test --locked --test cli_contract debug_logging_never_captures` | 2 passed, live against PostgreSQL 18.4 |
| `cargo --locked xtask verify` | Pass; full suite, plain and TLS database gates, one Unix-socket skip |

Not proven: the packaged-candidate repeat (W12), the panic path, the generated
update and refresh or clipboard log audits, and any non-macOS platform. No
product behaviour changed outside the redactor.

## W01 reconciliation and W03 reviewed-write evidence - 2026-09-10

**Baseline.** The working tree was clean at `247d1c7` and 21 commits behind
`origin/main`; a fast-forward to `6df522e` ("Record main integration state")
brought in the integrated chain for Features 012 through 024. Every task box in
`specs/012-schema-completion/tasks.md` through
`specs/024-retained-result-refresh/tasks.md` is checked, so the handoff that
named schema completion as the next priority predates the merge. The active
priorities are the delivery plan's W01 to W05 hardening packages, not a new
feature. W01 reconciliation is complete for the source tree: `HEAD` is
`6df522e`, the tree was clean before this session's changes, and the inherited
evidence remains valid only for the revisions it names.

**Fixed: `FROM ONLY` misresolved its relation.** `parse_relation` in
`src/query/update.rs` accepted the keyword `ONLY` as the relation name for
`SELECT ... FROM ONLY relation`, then took the real relation as its alias. With
a relation named `only` on the search path, a generated cell update could be
sent to that relation using row identity read from another table's result. The
source is now refused with a truthful reason; a quoted relation named `"only"`
still works. The new test was watched failing before the fix, and the
independent review confirmed no other accepted shape regressed. The
specification Edge Case, the compatibility wording, a checked T020 task and an
Unreleased CHANGELOG fix record the behaviour.

**W03 evidence: reviewed-write identity and truthfulness.** Four live tests
against disposable PostgreSQL 18.4 prove a composite quoted primary key (two
ordered predicates, exactly one intended row changed, neighbours untouched),
that review does not lock a row (a concurrent delete reports zero rows; a
concurrent non-key change is overwritten and reports one), and that
`restricted_reader` metadata is readable but not writable while the server
itself refuses the generated update with SQLSTATE 42501. A reducer assertion
proves the emitted parameterized effect binds to exactly the reviewed
`bound_sql`. The accepted concurrency semantics are documented in
`compatibility.md`: no lock, no conflict detection, no retry, no rollback.

| Gate at the final source | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Pass |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | Pass |
| `cargo test --locked --lib query::update` | 10 passed |
| `cargo test --locked --lib app::update` | 148 passed |
| `cargo test --locked --test documentation_matches_the_build` | 15 passed |
| `cargo --locked xtask verify` | Pass; 995 workspace tests, 53 PostgreSQL integration tests live |
| Unix-socket integration | Skip; `IGNATIUS_TEST_PG_SOCKET_URI` is not set |

That verify run counted 995 workspace tests: 754 library, 3 archive helper, 51
CLI contract, 4 cloud identity, 15 documentation matches, 3 editor contract, 2
keymap contract, 1 native boundary, 3 password prompt, 53 PostgreSQL
integration, 1 Unix-socket harness returning early (the separate semantic skip
above), 80 release contract, 7 release notes, 2 release runtime, 1 release
schema, 3 terminal restoration, 7 workflow contract and 5 xtask tests, plus 0
documentation tests. No test failed.

Environment: macOS 26.6.1 on Apple silicon, rustc and cargo 1.98.1 (Homebrew),
image `postgres:18.4-alpine` digest
`sha256:9a8afca54e7861fd90fab5fdf4c42477a6b1cb7d293595148e674e0a3181de15`.
The container fixture ran on the deliberate CLI-only colima runtime; Docker
Desktop was uninstalled on 2026-09-10 and must not be reinstalled. A missing
`docker-credential-osxkeychain` made the standard compose pull fail until
`brew install docker-credential-helper` restored it. The standard path was then
re-validated with `cargo xtask db up` and the focused live test, and the
disposable services were removed with `cargo xtask db down`, after which
`cargo xtask db status` reported `Not running.` The runtime rules and the
colima lifecycle are recorded in `docs/operations/agent-orchestration.md`.

**Orchestration.** Project-local roles (explorer, implementer, reviewer),
routing, fallback and dated OpenCode Go offer observations are recorded in
`docs/operations/agent-orchestration.md`, with a schema-only `opencode.json`
and `.opencode/agent/` role files. Only OpenCode Go is authorised; no overage,
auto-reload or new provider was enabled. Independent review of both slices
found no blocking or substantive issues; minor observations are in the review
returns. This session ran its workers on the default `deepseek-v4.1-flash`
because agent-level model routing takes effect only after an opencode restart.

**Not proven.** Manual terminal use of the new refusal, Windows and Linux hand
checks, the Unix-socket route, PostgreSQL 14-17 for the new evidence (18.4
only), live cloud-provider authentication, and anything release-related. The
changes are committed on the `w03-reviewed-write-evidence` branch for
integration; no tag, signing, release or publication was performed.

**Integration state.** The repository became public on 2026-09-11 (owner
decision; see the section above) and hosted CI started executing. The Windows
job failed for an environment reason: the runner image sets `PGPASSWORD`
machine-wide and the picker test read the process environment. Pull request 5
isolated that test from ambient credentials, added the public repository
guardrails and merged as `ba284f9`; this branch is rebased on it. Hosted checks
for the rebased revision are recorded on pull request 4. The local
`cargo --locked xtask verify` result above, with the disposable database
running, remains the strongest local evidence.

## Agent planning handoff - 2026-09-04

The [roadmap](product/roadmap.md) now defines the path from the local feature
chain through integrated correctness, daily-use acceptance, verified release
and maintenance. Its [delivery plan](product/agent-delivery-plan.md) contains
16 bounded work packages with role ownership, dependencies, write boundaries
and exit evidence. Start with **W01 integration reconciliation**, retaining
Feature 024's completed local implementation and verification below.

Read the [dated baseline](product/planning-baseline.md) for all 26 feature
directories, [product acceptance](product/acceptance.md) for six outcomes and
eight reference journeys, and the [agent playbook](operations/agent-playbook.md)
for assignment and handoff. The architecture documents now include provider,
clipboard, persistence and session-identity boundaries. Rejected credential-
store/libpq work is not in the active backlog. No application behaviour or
feature checkbox was changed by this planning work.

GitHub readback on 2026-09-04 confirmed private `tkomane/ignatius`, remote main
`52872acd7c0e0e38c6b810723ccda5eee99a28bf` and unprotected main. Its
[CI run 33720098318](https://github.com/tkomane/ignatius/actions/runs/33720098318)
passed the executed OS, PostgreSQL 14-18 plain/TLS, PostgreSQL 18 socket,
MSRV, workflow, security and generated-output gates. The scheduled fixture-drift
job skipped. This source-bound hosted evidence does not cover local HEAD
`3168e7c` or later uncommitted features. Planning verification is separate from
the application runs below.
The private-vulnerability-reporting API returned HTTP 404, so that route's
availability or access remains unverified in W13; it is not labelled disabled.

**Planning verification:** all 15 existing
`cargo test --locked --test documentation_matches_the_build` contracts passed.
The roadmap's named Feature 024 entry was restored after that contract caught
its omission. A concurrent terminal-key documentation update was allowed to
settle and the final contract run passed against the updated files. Local link,
feature-inventory, package/outcome/journey coverage, prose and whitespace checks
passed across 25 documents and 87 local links. The 16-package dependency graph
is acyclic; all 26 feature directories, six outcomes and eight journeys are
covered. All five Mermaid blocks were reviewed as source; graphical rendering
was not performed. No fresh application full-suite, database, cloud-provider
or hand-terminal result is claimed by this documentation-only planning task.
The existing application results below retain their original scope.

| Planning deliverable | Verified repository evidence |
| --- | --- |
| Analyse product objectives and users | `product/planning-baseline.md` and six measurable outcomes in `product/acceptance.md`, grounded in the vision, personas and experience brief |
| Architect subsequent agent work | Updated context, component and runtime documents, with trust/data flows, identity contracts and extension rules |
| Build a thorough execution roadmap | Six milestones, all 26 feature directories, 16 scoped packages, dependencies, role ownership, risks, decisions, release and maintenance gates |
| Make agent assignments repeatable | `operations/agent-playbook.md`, synchronized AGENTS/CLAUDE entry points and an assignment/return template |
| Preserve evidence and work ownership | Existing feature implementation retained, current hosted baseline re-read, rejected work excluded, no feature checkbox or application source changed by the planning task |

The planning handoff is complete. Product implementation acceptance and release
readiness remain the work described in W01-W16. No commit, push, tag, signing,
release or publication operation was performed by this planning task.

## Where the work is

**Current continuation**: Feature 024 explicit retained-result refresh and the
first W02 identity-transition hardening slice are integrated into `main` at
merge commit `a8a2b19664699b46a2edc3fff9478eb38f60ae40`, pushed to
`origin/main` on 2026-09-04. The integrated source is now the resumption point;
the older local feature branch was deleted after the merge. With Results
focused, portable `F6`, the
command palette and the contextual footer request one deliberate refresh from
the retained source; terminals with enhanced modifier reporting also accept
`Ctrl+Shift+R`. Only one read-classified statement is eligible; the editor stays
unchanged, named parameters are prompted again, outcomes are visible, and
focus, filtering, sorting, reconnect or failure never replays the query.
Focused, live and locked-verifier evidence is recorded below; manual terminal
and screen-reader checks and the semantic Unix-socket gate remain separate
evidence boundaries. Feature 023 reviewable cell-to-UPDATE and Feature 022
prompted named parameters are integrated and have their evidence below.
Feature 020 connection picking and Feature 019 formatting are integrated as
well. Feature 012 schema-aware completion is in ancestor commit `3168e7c`;
Feature 017
connection trust remains implemented and was previously pushed to `main` as
`9f80bef3040ba80d017dae3d5d0ae120a4cfc264` on 2026-09-03. No tag, signing,
release or publication operation was performed for this integration. Feature 008 release evidence
remains the separate release-readiness boundary. Its documentation was first pushed at
`b1e032f0344b88c0002c8fb32c08be13d72d73d9`; the hosted candidate
evidence is bound to source revision
`259ac76de6477719b8a3515777f3623b24237dd6`. That revision hardens the hosted
workflow's deterministic archive builds (`/Brepro` for Windows) and corrects
the exact runtime bundle layout. The pre-push `cargo --locked xtask verify` gate
passed formatting, lints, 515 unit/layout tests, 39 CLI contract tests and 38
PostgreSQL integration tests against disposable PostgreSQL 18.4 services, with
no skips.

Owner-authorized non-publishing workflow run `33689674455` passed all target
archive, repeat-build comparison, runtime smoke and aggregate jobs. The
macOS, Windows and Linux repeat archives were byte-identical. Each runtime job
passed identity, diagnostics, configuration initialisation and validation,
non-interactive refusal, side-by-side candidate validation, controlled invalid
configuration exit 3 and known-good rollback validation. The retained
aggregate contains exactly nine ordinary files: three archives, one aggregate
record, one combined manifest, `SHA256SUMS` and three runtime sidecars. The
downloaded allowlist, record, combined manifest and archive checksums passed
independent local verification. Artifacts are retained through 2026-09-09.

The candidate is still correctly blocked. The readiness check names eight
issues: the canonical evidence path is not materialised, the source is detached
and untagged, the record is incomplete and non-publishable, and verified
signature and provenance evidence are absent. The runtime sidecars are
automated smoke evidence only and do not exercise a database query or the
direct `IGNATIUS_LOG=debug` privacy audit. The historical hosted candidate's
privacy failure and the local remediation remain documented in
`docs/operations/verification.md`.

T034 is complete. T033 remains open for Windows/Linux hand-terminal checks,
direct runtime privacy review of the corrected binaries, canonical evidence
materialisation and the remaining supply-chain evidence. T035 remains open
because the accepted release criteria are not all evidenced; the roadmap stays
in planning. No tag, signing, release or publication operation was performed.

The merged CI-hardening work preserves that hosted aggregate as historical
evidence while adding exhaustive local verification, Rust 1.90 and PostgreSQL
14-18 coverage, a real Linux Unix-socket job, immutable action and
workflow-security contracts, bounded jobs, semantic workflow linting, and
independent dependency and secret controls. Its release preflight freezes the
source and runs the same security and verifier gates before retaining target
evidence; the aggregate job rechecks transport, scope, target, product, source
and build identity before retaining its six-file package.

Those changes first passed locally in an isolated integration worktree, including
Actionlint and YAML parsing, 80 release contracts, 7 workflow contracts,
Cargo-deny 0.20.2, digest-pinned Gitleaks history scanning and Rust 1.90. The
full verifier passed PostgreSQL 18.4 plain and TLS; the Unix-socket check was
explicitly skipped because its URI was not configured. The later main CI
readback above now proves PostgreSQL 15/17, the socket job, Rust 1.90 and
workflow linting at that source. It does not prove the separate hardened
release preflight or a new release candidate's runtime evidence.
An explicit macOS Docker socket attempt is not counted as a pass: the bind
mount rejected PostgreSQL's socket-permission change before startup. The
socket job deliberately targets Ubuntu, where the runner filesystem supports
the required Unix-domain socket boundary.

**Earlier implemented product feature**: cloud identity authentication and its
read-only connection trust surface, implemented and pushed in
`9f80bef3040ba80d017dae3d5d0ae120a4cfc264`. The
owner asked for Entra ID first and for a first-class experience on other clouds,
and checking the other two turned that instruction from a generalisation into the
obvious shape: Azure, AWS and Google Cloud authenticate a PostgreSQL connection
identically. A command-line tool returns a short-lived bearer token, the token
is presented as the password, and the transport must be encrypted. Only the
command differs, so the command is data: three built-ins ship and
`[auth.providers]` defines a fourth without waiting for a release.

The interactive client now makes that route inspectable from the command
palette with `Connection and auth details`. The surface names the target,
environment, server posture, observed TLS state, provider command summary,
credential lifetime caveat, and provider remedy. It is display-only: opening it
does not execute a provider, refresh a token, or perform network I/O; arguments
and token values are never rendered. A password-authenticated session says that
no cloud token was requested instead of guessing which password route won.

Two refusals are the security of it. Nothing is fetched for a target whose
`sslmode` would permit an unencrypted connection - the check runs before the
program does, so the token was never asked for rather than asked for and
discarded. And nothing falls back to the password prompt: a token was accepted
or refused on its own terms, so asking a person would describe the wrong problem
and take a real password on the way. `postgres::connect` refuses outright if a
target names a provider and carries no credential, which turns a forgotten call
site into a loud defect rather than a quiet prompt.

Proven end to end against the TLS container with no cloud account: a provider
whose program prints the container's synthetic password, through the subprocess,
the extraction, the `SecretString` and a real authenticated session. What is
**not** proven is that `az`, `aws` and `gcloud` behave as documented - that needs
an account, and `docs/support/compatibility.md` carries a separate column per
provider rather than letting one word cover both.

Also: `query --format json` no longer writes `[]` for a run that failed, which
to a stdout-only reader was indistinguishable from a query that matched nothing.

**Feature 012 implementation**: the TUI and plain mode now consume one shared,
lexer-backed completion engine and one PostgreSQL catalogue snapshot with bounded
visible candidate lists.
The editor offers keywords, schema objects, relations, functions, columns,
aliases and CTEs at the cursor; acceptance quotes identifiers and is one undo
step, while dismissal leaves the buffer and cursor unchanged. Loading, stale,
unavailable, unreadable and bounded states are written in the UI, automatic
popups can be disabled without removing explicit `Ctrl+Space`, and plain mode
uses `\complete` plus `\use` without executing a partial buffer. The focused
completion run, locked verifier, and live catalogue test pass against disposable
PostgreSQL 18.4. The Unix-socket gate is explicitly skipped because its URI is
not configured; cross-platform terminal evidence remains pending.

**Feature 013 implementation**: query failures now retain the server's
statement ordinal and structured schema, table, column, and constraint fields.
For a valid position in the unchanged buffer, the TUI moves the caret to the
server's answer, marks the line with `!`, emphasizes the smallest known token,
and keeps the error explanation visible. A changed buffer, missing position, or
unmappable position is labelled instead of guessed. Plain mode renders the same
statement, line, column, source excerpt, and caret without terminal control
sequences; failed JSON queries keep stdout empty and place the structured
diagnostic on stderr. The live PostgreSQL suite covers syntax, unique,
foreign-key, not-null, and check failures. Focused and full evidence is recorded
below; cross-platform terminal, screen-reader, and Unix-socket evidence remain
separate gaps.

**Feature 014 implementation**: the interactive grid is now a local, reversible
view over retained result rows. Sort cycles through ascending, descending, and
original server order without an execution effect and preserves the selected
source row. A searchable `Ctrl+K g` palette controls source-column visibility,
bounded widths, optional server type labels, and a frozen first visible column;
duplicate labels remain distinct by source position, the last visible column is
protected, and reset or a new execution clears result-specific shape. The
existing filter, inspector, expanded row, editor, history, plain, JSON, and
streaming-export contracts remain separate.

**Feature 015 implementation**: guided discovery is now derived from the current
model and the active configured keymap. The first frame names the connection
state and a valid next action; empty and blocked editor, result and object states
explain their boundary; and the footer presents no more than five contextual
action/key pairs. The general command palette identifies itself, searches by
intent, groups useful actions, and keeps prerequisites beside unavailable chord
entries. Opening, searching, rendering and dismissing these surfaces are
presentation-only and do not execute SQL, reload metadata, write files, record
history, transfer clipboard data, or persist onboarding state.

**Feature 018 implementation**: copying a retained text cell is now an explicit,
opt-in terminal action. `Ctrl+K c`, the inspector and the command palette stage
only the result job, source row, source column and UTF-8 counts; confirmation
revalidates that identity before creating the redacted payload. The interactive
runtime writes one base64-framed OSC 52 sequence and flushes it. Disabled,
NULL, empty, oversized, stale, unsupported and failed paths remain value-free,
and success reports bytes and characters while leaving terminal acceptance
unconfirmed. The terminal, SSH path, or multiplexer may observe or retain the
value; the client never reads or clears the destination. Plain, JSON, NDJSON,
history, export and SQL editing remain outside the transport boundary.

**Feature 019 implementation**: the editor and plain mode now share one bounded,
deterministic SQL formatter. `Ctrl+Shift+F`, `Ctrl+K q`, the command palette,
and plain `\format` format the complete local buffer without contacting
PostgreSQL or creating any effect. Major clauses, boolean predicates and
eligible lists receive readable layout; strings, quoted identifiers,
dollar-quoted bodies and comments remain byte-for-byte exact. A changed buffer
is one undoable edit with logical cursor recovery, while empty, comment-only,
malformed and over-limit input remains untouched with value-free next-action
wording. Plain mode keeps the formatted statement pending, writes its sanitized
preview to stderr, and leaves result stdout unchanged.

**Previous**: the four open decisions are closed, and the product has a
direction rather than a finished roadmap. The owner named the real requirement
on 2026-08-16 - Microsoft Entra ID against Azure Database for PostgreSQL, for
the COI application - and it turns out not to need libpq at all: Entra
authenticates with a token presented as the password over TLS, which this
adapter already does. ADR-0012 records that and supersedes ADR-0009. The
credential store is rejected (ADR-0011), copy goes through OSC 52 opt-in
(ADR-0013), and `docs/product/experience-roadmap.md` sets out what "the best
experience anyone has used" means in concrete features, first of which is
completion that knows the schema. Specifications were written for cloud identity
(since renamed to `specs/011-cloud-identity/`) and completion
(`specs/012-schema-completion/`); neither was implemented at that point.

Also fixed: the two Feature 005 editor defects the audit of `555203f` found.
Enter now reaches `Editor::insert_newline()` so indentation survives a line
break through the real key path, and every cursor movement ends the undo
coalescing run so typing either side of a move is two undo steps. Both are
proven by tests that were watched failing with the fix removed - and the first
version of the movement test passed without the fix, so it was rewritten until
it could fail.

That handoff also unblocked `tests/editor_contract.rs`, the acceptance contract
for the same feature, which had been held behind `#[ignore]` waiting
for it. Two steps in it needed correcting and both are the same mistake in
different clothes: it removed inherited indentation with forward-delete, which
was a no-op that happened to leave the right text behind while Enter still
inserted a bare newline. Backspace is what a person would press and what the
implemented behaviour requires. The pty half also asserted only that
`FROM orders AS o` appeared somewhere, which a line still carrying four
inherited spaces satisfies; it now says the clause is top level. The three
contracts are active and pass in the normal focused test run.

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

## Feature 024 verification - 2026-09-04

Feature 024 is integrated into `main` and remains separate from release
readiness. The interactive client exposes an explicit
portable `F6` action, an enhanced-terminal `Ctrl+Shift+R` alias, a command-
palette entry and a contextual Results hint. Refresh reads only the retained
`last_sql` source, requires exactly one
read-classified statement and a usable non-failed session, leaves the editor
unchanged, and refuses multi-statement, write, plan, missing-result and
unavailable states without an execution effect. Named parameters are prompted
again without retaining entered values. The old snapshot stays visible while
the refresh runs, and success, cancellation, failure and unknown connection
outcomes are distinct. Passive result actions never trigger a refresh.

| Focused gate | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Pass |
| `cargo check --locked` | Pass |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | Pass |
| `cargo test --locked --lib refresh` | 14 passed |
| `cargo test --locked --lib app::discovery::tests::retained_result_refresh` | 1 passed |
| `cargo test --locked --lib a_retained_result_refresh` | 1 passed |
| `cargo test --locked --lib a_refused_retained_result_refresh` | 1 passed |
| focused keymap reachability, configured-key and printable-input tests | 3 passed |
| `cargo test --locked --test cli_contract retained_result_refresh_stays_out_of_noninteractive_machine_output` | 1 passed |
| `cargo test --locked --test documentation_matches_the_build` | 15 passed |
| `git diff --check` | Pass |

The focused evidence proves retained-source execution rather than editor-buffer
execution, editor text and revision preservation, named-parameter re-prompting
including empty input, source classification and multi-statement refusal,
zero-row eligibility, failed-transaction and connection refusal, visible
in-flight and outcome wording, cancellation without retry, connection loss with
the old snapshot preserved, ordinary history behaviour, no passive replay,
configured-key discoverability, printable `r` preservation, ASCII output and a
narrow refusal layout. The non-interactive contract confirms that refresh state
and a synthetic parameter value remain outside machine output.

The final authoritative `cargo --locked xtask verify` exited successfully in a
host context that could inspect the disposable services. Formatting, lints, API
documentation, workspace tests and documentation tests all passed. The
workspace test gate ran 990 cases: 753 library tests, 3 archive-helper tests,
51 CLI-contract tests, 4 cloud-identity tests, 15 documentation matches, 3
editor-contract tests, 2 keymap-contract tests, 1 native-boundary test, 3
password-prompt tests, 49 PostgreSQL integration tests, 1 Unix-socket harness
test, 80 release-contract tests, 7 release-note tests, 2 release-runtime tests,
1 release-schema test, 3 terminal-restoration tests, 7 workflow-contract tests
and 5 xtask tests. Documentation tests ran 0 tests and passed. Plain and TLS
database-backed gates passed against disposable PostgreSQL 18.4 using
synthetic credentials from `docker/dev.env`.

The semantic Unix-socket integration gate remains a skip because
`IGNATIUS_TEST_PG_SOCKET_URI` was not set; the harness returning without that
URI is not live socket evidence. Manual confirmation in Warp, Windows Terminal,
Linux terminals, SSH and multiplexers, screen-reader use, reduced-motion and
live-resize checks remain unverified. The run used Homebrew Rust/Cargo 1.98.0
and PostgreSQL 18.4; broader PostgreSQL matrix, hosted toolchain and
cross-platform evidence remain separate. Release readiness, signing,
provenance, publication and owner authorization remain separate and are not
implied by this green local verifier. The disposable services were torn down
with `cargo xtask db down`, and `cargo xtask db status` reported `Not running.`
No tag, signing, release, push or publication operation was performed.

## W02 identity-transition hardening slice - 2026-09-04

The reducer now treats a connection-loss message as the boundary for every
server-bound prompt and running identity. It discards production confirmations,
analyzed-plan confirmations, named-parameter prompts, update metadata/value/review
state and late execution markers before the runtime's stale completion arrives.
It sets transaction state to `Unknown`, preserves the prior retained result, and
keeps a pending copy available because copying that already-retained snapshot is
local and does not contact the server. Refresh remains bound to its retained
source when the editor changes during the request.

The focused W02 checks passed: refresh-source/editor interleaving, connection
loss across parameter, refresh-parameter, production, plan and cell-update
prompts, local copy after connection loss, and late execution identity cleanup.
The refreshed `cargo --locked xtask verify` exited successfully after this
reducer change: 753 library tests and 990 workspace tests passed, along with
formatting, lints, API documentation, documentation tests and the plain/TLS
PostgreSQL gates. The Unix-socket semantic gate remained skipped because
`IGNATIUS_TEST_PG_SOCKET_URI` was not set. Runtime connection-generation
interleavings beyond this reducer slice, the broader PostgreSQL matrix, native
terminal evidence and hand acceptance remain open under W02 and later
packages.

## Feature 023 verification - 2026-09-04

Feature 023 is integrated into `main` and remains separate from release
readiness. The interactive client now turns one selected
cell from a retained direct single-table `SELECT` into a conservative,
reviewable `UPDATE`. It resolves bare relations through the active session
search path, reads live relation kind, read and update privileges and ordered
primary-key metadata, then keeps the replacement and key values in secret
parameter bindings. Production and server read-only sessions refuse before
replacement input. The exact bound statement is shown in a separate review;
only its Enter emits one parameterized execution, and the prior result remains
a snapshot rather than being rerun.

| Focused gate | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Pass |
| `cargo test --locked --lib query::update` | 9 passed |
| `cargo test --locked --lib app::update::tests::cell_update` | 3 passed |
| stale metadata identity reducer test | 1 passed |
| generated-update snapshot and no-rerun reducer test | 1 passed |
| `cargo test --locked --lib app::discovery::tests::cell_update_is_discoverable_only_for_a_retained_read_write_result_source` | 1 passed |
| `cargo test --locked --lib ui::layout::tests::cell_update` | 2 passed |
| `cargo test --locked --test documentation_matches_the_build` | 14 passed |
| focused live table-update PostgreSQL check | 1 passed against disposable PostgreSQL 18.4 |
| focused live view-kind refusal PostgreSQL check | 1 passed against disposable PostgreSQL 18.4 |
| `git diff --check` | Pass |

The focused evidence proves direct projection and alias mapping, trailing
multiple-statement refusal, joins, expressions, CTEs, set operations,
duplicate output, missing and NULL keys, primary-key target refusal, quoted
identifiers, literal escaping, empty text, NUL and 16 KiB bounds, redacted
debug state, stale result identity, production and read-only refusal, live
search-path resolution, primary-key discovery, update privilege and view-kind
metadata. The renderer checks the value prompt, literal semantics, exact bound
review statement, no-send boundary, narrow output and ASCII output. The live
table check proves no row changes before the execution effect, one intended row
changes after confirmation, and a deliberate read sees the replacement. The
live view check proves that a non-table relation is reported before the
interactive refusal.

The authoritative `cargo --locked xtask verify` exited successfully with the
disposable `postgres:18.4-alpine` plain and TLS fixtures running and synthetic
credentials from `docker/dev.env`. It passed formatting, lints, API
documentation, 737 unit and layout tests, 50 CLI-contract tests, 3
archive-helper tests, 4 cloud-identity tests, 14 documentation matches, 3
editor-contract tests, 2 keymap-contract tests, 1 native-boundary test, 3
password-prompt tests, 49 PostgreSQL integration tests, 80 release-contract
tests, 7 release-note tests, 2 release-runtime tests, 1 release-schema test, 3
terminal-restoration tests, 7 workflow-contract tests, 5 xtask tests, and the
documentation tests. Plain and TLS PostgreSQL database gates passed.

The semantic Unix-socket integration gate remains a skip because
`IGNATIUS_TEST_PG_SOCKET_URI` is not set. The socket harness returned without a
configured URI; that is not live socket evidence. Manual confirmation in Warp,
Windows Terminal, Linux terminals, SSH and multiplexers, screen-reader use,
reduced-motion and live-resize checks remain unverified. The available local run
used Homebrew Rust/Cargo 1.98.0 and PostgreSQL 18.4; broader PostgreSQL matrix,
hosted toolchain and cross-platform evidence remain separate. Release
readiness, signing, provenance, publication and owner authorization remain
separate and are not implied by this green local verifier. The disposable
services were explicitly torn down with `cargo xtask db down`, and `cargo xtask
db status` reported `Not running.` No tag, signing, release, push or publication
operation was performed.

## Feature 022 verification - 2026-09-04

Feature 022 is integrated into `main` and remains separate from release
readiness. The full-screen and plain interactive clients
recognize bounded `:name` placeholders only in executable SQL, prompt once per
distinct name in first-use order, accept empty text, and keep the original
template in the editor and history. Values are held in `SecretString`, shown
only as a mask, and bound as escaped PostgreSQL text literals at the existing
simple-query boundary. Non-interactive `query` accepts only complete
`--param-env NAME=VARIABLE` mappings and validates them before target work.
NUL is refused, and server positions after expansion are reported without a
guessed template caret.

| Focused gate | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Pass |
| `cargo test --locked --lib parameter` | 25 passed |
| `cargo test --locked --test cli_contract named_parameter` | 1 passed |
| `cargo test --locked --test documentation_matches_the_build prompted_parameters` | 1 passed |
| `cargo test --locked --test postgres_integration parameterized` | 3 passed against disposable PostgreSQL 18.4 |
| `cargo test --locked --test password_prompt plain_parameter` | 1 passed through a real macOS pseudo-terminal against disposable PostgreSQL 18.4 |
| CLI `--param-env` probe | Pass; stdout contained only the requested CSV data |
| Missing mapping against an unreachable target | Pass; usage error returned before connection work |
| `git diff --check` | Pass |

The focused evidence proves protected-region scanning, cast exclusion, first-use
ordering, repeated-name reuse, adversarial literal escaping, empty values, NUL
refusal, the 64-name limit, masked prompt rendering, cancellation without an
effect, template-preserving history and diagnostics, redacted effect and log
descriptors, exact environment mapping validation, data-only stdout, and honest
post-expansion error locations. The live PostgreSQL checks also prove execute,
stream, escaped and repeated values, empty text, NUL refusal before send, and
server-confirmed cancellation. The pseudo-terminal check proves that plain
mode accepts a hidden answer and that the returned value appears once as data,
not as prompt echo.

The final authoritative `cargo --locked xtask verify` exited successfully with
the disposable `postgres:18.4-alpine` plain and TLS fixtures running and
synthetic credentials from `docker/dev.env`. It passed formatting, lints, API
documentation, the complete 720-test library suite, 50 CLI-contract tests, 3
archive-helper tests, 4 cloud-identity tests, 13 documentation matches, 3
editor-contract tests, 2 keymap-contract tests, 1 native-boundary test, 3
password-prompt tests, 47 PostgreSQL integration tests, 80 release-contract
tests, 7 release-note tests, 2 release-runtime tests, 1 release-schema test, 3
terminal-restoration tests, 7 workflow-contract tests, 5 xtask tests, and the
documentation tests. Plain and TLS PostgreSQL database gates passed.

The semantic Unix-socket integration gate remains a skip because
`IGNATIUS_TEST_PG_SOCKET_URI` is not set. The socket harness returned without a
configured URI; that is not live socket evidence. Manual confirmation in Warp,
Windows Terminal, Linux terminals, SSH and multiplexers, screen-reader use,
reduced-motion and live-resize checks, and live cloud-provider account routes
remain unverified. The available local run used Homebrew Rust/Cargo 1.98.0 and
PostgreSQL 18.4; the broader PostgreSQL 14-18 matrix and hosted toolchain
evidence remain separate. Release readiness, signing, provenance, publication,
and owner authorization remain separate and are not implied by this green local
verifier. The disposable services were explicitly torn down with `cargo xtask
db down`, and `cargo xtask db status` reported `Not running.` No tag, signing,
release, push, or publication operation was performed.

## Feature 021 verification - 2026-09-04

Feature 021 is integrated into `main` and remains separate from release
readiness. Interactive export now chooses CSV, TSV, JSON,
NDJSON or Markdown in a searchable palette before the destination path. The
choice is carried through the existing retained-row, filter, no-overwrite and
atomic export boundary; the filename extension is never consulted. CLI INSERT
output requires an explicit destination table, quotes table and column
identifiers, preserves NULL, emits escaped PostgreSQL text literals without type
inference, and refuses duplicate labels, NUL text and header suppression.

| Focused gate | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Pass |
| `cargo check --locked` | Pass |
| `cargo test --locked cli::output --lib` | 25 passed |
| `cargo test --locked app::update --lib` | 126 passed |
| `cargo test --locked ui::layout --lib` | 82 passed |
| `cargo test --locked cli::interactive --lib` | 14 passed |
| `cargo test --locked --test documentation_matches_the_build` | 12 passed |
| `cargo test --locked --test cli_contract` without database environment | 48 passed; database cases skipped |
| `git diff --check` | Pass |

The focused evidence proves deterministic format ordering, searchable choice,
no effect before path acceptance, retained and filtered row scope, compact and
ASCII wording, output safety for hostile identifiers and literals, NULL and
empty-text distinction, NUL and duplicate-label refusal, header validation,
buffered/streamed byte equivalence, existing-format stability, and no
decoration on machine output. The interactive JSON route also preserves the
server's original row count and truncation fact while writing only retained
filtered rows. The CLI contract covers validation before connection work,
streamed file output, and the explicit application of generated INSERT data to
a disposable probe table.

The authoritative `cargo --locked xtask verify` exited successfully with the
following environment: `rustc 1.98.0` and `cargo 1.98.0` from Homebrew, against
the local `postgres:18.4-alpine` image
`sha256:db676a0ed906c00f55020fb8999e4fb30c598bf5c3b5c188630aef2812d3f11d`.
It passed formatting, lints, API documentation, the complete 703-test library
suite, 49 CLI-contract tests, 3 archive-helper tests, 4 cloud-identity tests,
12 documentation matches, 3 editor-contract tests, 2 keymap-contract tests, 1
native-boundary test, 2 password-prompt tests, 44 PostgreSQL integration tests,
80 release-contract tests, 7 release-note tests, 2 release-runtime tests, 1
release-schema test, 3 terminal-restoration tests, 7 workflow-contract tests,
5 xtask tests, and the documentation tests. Plain and TLS PostgreSQL database
gates passed. The live INSERT contract generated, streamed, and explicitly
applied the statements to the disposable probe table, confirming three rows.

The semantic Unix-socket integration gate remains a skip because
`IGNATIUS_TEST_PG_SOCKET_URI` is not set. The workspace socket harness safely
returned without a configured URI; that is not live socket evidence. Manual
confirmation in Warp, Windows Terminal, Linux terminals, SSH and multiplexers,
screen-reader use, reduced-motion and live-resize checks, and live
cloud-provider account routes remain unverified. The pinned rustup 1.97.1
toolchain and the broader PostgreSQL 14-18 matrix remain separate hosted or
environment evidence gates; this run used the available Homebrew 1.98.0 and
PostgreSQL 18.4 fixtures. Release readiness, signing, provenance, publication,
and owner authorization remain separate and are not implied by this green local
verifier. The disposable services were explicitly torn down with
`cargo xtask db down`, and `cargo xtask db status` reported `Not running.` No
tag, signing, release, push, or publication operation was performed.

## Feature 020 verification - 2026-09-04

Feature 020 is integrated into `main` and remains separate from release
readiness. Named profiles now produce a searchable
connection picker before implicit startup resolution, with an explicit default
route and safe, bounded profile summaries. `Ctrl+K n` reaches the same picker
after a quiet session. Selection carries only an optional profile name through
the reducer, keeps resolved targets and credentials in runtime-only slots,
retains the editor and local result preferences, clears server-derived state,
and rejects late work from the previous connection generation.

| Focused gate | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Pass |
| `cargo check --locked` | Pass |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | Pass |
| `cargo test --locked app::connection_picker --lib` | 5 passed |
| `cargo test --locked app::palette --lib` | 14 passed |
| `cargo test --locked app::update --lib` | 125 passed |
| `cargo test --locked app::discovery --lib` | 9 passed |
| `cargo test --locked ui::keymap --lib` | 28 passed |
| `cargo test --locked ui::layout --lib` | 81 passed |
| `cargo test --locked cli::interactive --lib` | 13 passed |
| `cargo test --locked --test native_boundary_contract` | 1 passed |
| `cargo test --locked --test documentation_matches_the_build` | 11 passed |
| `cargo test --locked --test cli_contract` | 46 passed |
| `cargo test --locked --lib` | 696 passed |
| `git diff --check` | Pass |

The focused evidence proves deterministic safe summaries, missing-field and
provider wording, bounded derived details, exclusion of password-shaped
unknown values and provider arguments, searchable picker construction, default
and profile choice transitions, quiet-session reset semantics, retained editor
and reading preferences, explicit-route detection, resolver and cloud-auth
boundaries, no-fallback refusal, active-keymap discovery, printable `n`
preservation, full and compact accessible rendering, documentation parity, and
unchanged plain and machine-output routes. The native-boundary check also
confirmed that the new pure picker layer contains no driver or FFI markers.

The authoritative `cargo --locked xtask verify` exited successfully. It passed
formatting, lints, API documentation, the complete 696-test library suite, 46
CLI-contract tests, 3 archive-helper tests, 4 cloud-identity tests, 11
documentation matches, 3 editor-contract tests, 2 keymap-contract tests, 1
native-boundary test, 2 password-prompt tests, 44 PostgreSQL integration tests,
80 release-contract tests, 7 release-note tests, 2 release-runtime tests, 1
release-schema test, 3 terminal-restoration tests, 7 workflow-contract tests,
and 5 xtask tests. The disposable PostgreSQL 18.4 plain and TLS fixtures
supplied live database evidence using synthetic credentials only.

The semantic Unix-socket integration gate remains a skip because
`IGNATIUS_TEST_PG_SOCKET_URI` is not set. The workspace socket harness was
included and safely returned without a configured URI; that is not live socket
evidence. Manual confirmation in Warp, Windows Terminal, Linux terminals, SSH
and multiplexers, screen-reader use, reduced-motion and live-resize checks,
and live cloud-provider account routes remain unverified. Release readiness,
signing, provenance, publication, and owner authorization remain separate and
are not implied by this green local verifier. The disposable services were
torn down and `cargo xtask db status` reported `Not running.` No tag, signing,
release, push, or publication operation was performed.

## Feature 019 verification - 2026-09-04

Feature 019 is integrated into `main` and remains separate from release
readiness. The focused checks cover the bounded lexical
formatter, protected regions, cursor mapping, editor history, reducer,
discovery, keymap, renderer, plain mode, documentation, and machine-output
boundaries.

| Focused gate | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Pass |
| `cargo check --locked` | Pass |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | Pass |
| `cargo test --locked query::format --lib` | 14 passed |
| `cargo test --locked app::editor --lib` | 23 passed |
| `cargo test --locked app::update --lib` | 121 passed |
| `cargo test --locked app::discovery --lib` | 9 passed |
| `cargo test --locked ui::keymap --lib` | 27 passed |
| `cargo test --locked ui::layout --lib` | 80 passed |
| `cargo test --locked cli::plain --lib` | 16 passed |
| `cargo test --locked --test documentation_matches_the_build` | 10 passed |
| `cargo test --locked --test cli_contract` | 45 passed |
| `cargo test --locked --lib` | 678 passed |
| `git diff --check` | Pass |

The focused evidence proves clause, predicate, list, join, nested-query,
operator, cast, array, parameter, custom-operator, numeric-exponent,
multiple-statement and idempotence behavior; exact protected strings,
identifiers, dollar bodies and nested comments; control-looking protected text;
empty and comment-only no-ops; every supported unterminated region refusal;
the inclusive 1 MiB bound; UTF-8-safe cursor mapping; one editor edit with
undo/redo and modified-state preservation; stale error-location invalidation;
active-keymap discovery; accessible outcome wording; plain pending-buffer
replacement; sanitized message-stream preview; and unchanged result stdout.

The authoritative `cargo --locked xtask verify` passed every gate that ran:
formatting, lints, API documentation, 678 library tests, 45 CLI-contract tests,
44 PostgreSQL integration tests against disposable PostgreSQL 18.4 plain and
TLS services, 3 terminal-restoration tests, 10 documentation matches, 4
cloud-identity tests, 2 keymap-contract tests, 3 editor-contract tests, 1
native-boundary test, 2 password-prompt tests, 80 release-contract tests, 7
release-note tests, 2 release-runtime tests, 1 release-schema test, 7
workflow-contract tests, and 5 xtask tests. The run exited successfully.

The semantic Unix-socket gate remains a skip because
`IGNATIUS_TEST_PG_SOCKET_URI` is not set. Manual confirmation in Warp, Windows
Terminal, Linux terminals, SSH and multiplexers, screen-reader use, and
reduced-motion or live-resize hand checks remain unverified. The formatter's
local transformation does not prove terminal rendering on those platforms.
Release readiness, signing, provenance, publication, and owner authorization
remain separate and are not implied by this local verifier. The disposable
services were torn down and `cargo xtask db status` reported `Not running.` No
tag, signing, release, push, or publication operation was performed.

## Feature 018 verification - 2026-09-04

Feature 018 is integrated into `main` and remains separate from release
readiness. The focused checks below cover the bounded
transport, configuration, reducer, discovery, renderer, runtime, documentation,
and non-interactive boundaries.

| Focused gate | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Pass |
| `cargo check --locked` | Pass |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | Pass |
| `cargo test --locked clipboard --lib` | 10 passed |
| `cargo test --locked config --lib` | 36 passed |
| `cargo test --locked app::update --lib` | 118 passed |
| `cargo test --locked app::discovery --lib` | 8 passed |
| `cargo test --locked ui::keymap --lib` | 26 passed |
| `cargo test --locked ui::layout --lib` | 79 passed |
| `cargo test --locked --test documentation_matches_the_build` | 9 passed |
| `cargo test --locked --test cli_contract` | 44 passed |
| `cargo test --locked --lib` | 652 passed |
| `git diff --check` | Pass |

The focused evidence proves exact OSC 52 framing and UTF-8 bytes, control-byte
containment, the inclusive 1 MiB bound, redacted payload and retained-cell
identity, opt-in and refusal paths, confirmation and cancellation, stale
selection rejection, filtered source-row identity, inspector and palette
discovery, printable `c` preservation, safe write failures, value-free
rendering, and no transport in plain, JSON, NDJSON, history, or export routes.

The authoritative `cargo --locked xtask verify` passed every gate that ran:
formatting, lints, API documentation, workspace tests, documentation tests,
652 library tests, 44 CLI-contract tests, 44 PostgreSQL integration tests, 3
terminal-restoration tests, 9 documentation matches, 4 cloud-identity tests,
2 keymap-contract tests, 3 editor-contract tests, 1 native-boundary test, 2
password-prompt tests, 80 release-contract tests, 7 release-note tests, 2
release-runtime tests, 1 release-schema test, 7 workflow-contract tests, and 5
xtask tests. Plain and TLS disposable PostgreSQL 18.4 services supplied the
database evidence. The run exited successfully with no failures.

The semantic Unix-socket gate remains a skip because
`IGNATIUS_TEST_PG_SOCKET_URI` is not set. Manual confirmation in Warp, Windows
Terminal, Linux terminals, SSH and multiplexers, screen-reader use, and
terminal clipboard acceptance remain unverified. The client deliberately cannot
infer acceptance from a successful write and flush. Release readiness, signing,
provenance, publication, and owner authorization remain separate and are not
implied by this local verifier. The disposable services were torn down and
`cargo xtask db status` reported `Not running.` No tag, signing, release, push,
or publication operation was performed.

## Feature 016 verification - 2026-09-04

Feature 016 is integrated into `main` and remains separate from release
readiness. It adds a pure bounded PostgreSQL JSON plan
model, a Results-pane tree, plain and analyzed plan actions, explicit analysis
confirmation, local selection and collapse state, stale-completion protection,
connection-loss recovery, and preserved ordinary result state.

| Focused gate | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Pass |
| `cargo check --locked` | Pass |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | Pass |
| `cargo test --locked query::plan --lib` | 7 passed |
| `cargo test --locked app::plan --lib` | 4 passed |
| `cargo test --locked app::discovery --lib` | 8 passed |
| `cargo test --locked app::update --lib` | 110 passed |
| `cargo test --locked ui::keymap --lib` | 25 passed |
| `cargo test --locked ui::layout --lib` | 77 passed |
| `cargo test --locked --test documentation_matches_the_build` | 8 passed |
| `cargo test --locked --test cli_contract` | 43 passed |
| `cargo test --locked --test postgres_integration --no-run` | Pass |
| `git diff --check` | Pass |

The focused evidence proves bounded parsing, truthful missing metrics, plan
preconditions, confirmation and cancellation, stale completion rejection,
ordinary-result preservation, local navigation, compact/narrow/ASCII/no-colour
rendering, key discovery, documentation contracts, and unchanged plain/JSON/
NDJSON/history boundaries.

The authoritative `cargo --locked xtask verify` passed every gate that ran:
formatting, lints, API documentation, workspace tests, documentation tests,
632 library tests, 43 CLI-contract tests, 44 PostgreSQL integration tests, 3
terminal-restoration tests, 8 documentation matches, 4 cloud-identity tests, 2
keymap-contract tests, 3 editor-contract tests, 1 native-boundary test, 2
password-prompt tests, 80 release-contract tests, 7 release-note tests, 2
release-runtime tests, 1 release-schema test, 7 workflow-contract tests, and 5
xtask tests. The run exited successfully.

The live plan evidence ran against disposable `postgres:18.4-alpine` plain and
TLS services. Plain EXPLAIN did not mutate a guarded UPDATE, analyzed SELECT
returned actual metrics, and the TLS plan ran on the encrypted session. The
fixtures were then removed with `cargo xtask db down`; they are disposable and
recreatable with `cargo xtask db up`. A subsequent `cargo xtask db status`
reported `Not running.`

The Unix-socket integration gate remains a semantic skip because
`IGNATIUS_TEST_PG_SOCKET_URI` is not set. Manual Windows/Linux terminal checks,
live resize, Warp rendering, screen-reader use, and a configured Unix-socket
server remain unverified. Release readiness, signing, provenance, publication,
and owner authorization remain separate and are not implied by this local
verifier. No tag, signing, release, push, or publication operation was
performed.

## Feature 015 verification - 2026-09-04

Feature 015 is integrated into `main` and remains separate from release
readiness. Focused checks cover the pure discovery model,
active keymap snapshot, reducer, palette, renderer, documentation, and the
machine-output boundary.

| Focused gate | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Pass |
| `cargo check --locked` | Pass |
| `cargo test app::discovery --lib` | 7 passed |
| `cargo test app::palette --lib` | 14 passed |
| `cargo test app::update --lib` | 103 passed |
| `cargo test ui::keymap --lib` | 24 passed |
| `cargo test ui::layout --lib` | 72 passed |
| `cargo test --test documentation_matches_the_build` | 7 passed |
| Guided-discovery CLI contract test | Pass |
| `git diff --check` | Pass |

The focused evidence proves that state-specific first-frame and empty-state
guidance, the five-item contextual footer, configured-key labels, intent search,
prerequisite wording, ASCII input cursors, filtered-empty recovery, and command
palette dismissal remain presentation-only. No discovery surface executes SQL,
reloads metadata, records history, writes files, transfers clipboard data, or
persists onboarding state. Plain, JSON, NDJSON, streaming export, history, and
terminal-restoration routes remain outside the discovery path.

The authoritative `cargo --locked xtask verify` passed every gate that ran. The
auto-discovered suite recorded 607 library tests, 43 CLI-contract tests, 41
PostgreSQL integration tests, 3 terminal-restoration tests, 7 documentation
matches, 4 cloud-identity tests, 2 keymap-contract tests, 3 editor-contract
tests, 1 native-boundary test, 2 password-prompt tests, 80 release-contract
tests, 7 release-note tests, 2 release-runtime tests, 1 release-schema test, 7
workflow-contract tests, and 5 xtask tests, with no failures. Disposable
`postgres:18.4-alpine` plain and TLS services supplied the passed database
evidence and were torn down afterward; `cargo xtask db status` reported
`Not running.`

The Unix-socket integration gate remains a semantic skip because
`IGNATIUS_TEST_PG_SOCKET_URI` is not set. Manual Windows/Linux terminal checks,
live resize, Warp rendering, screen-reader use, and a configured Unix-socket
server remain unverified. Release readiness, signing, provenance, publication,
and owner authorization remain separate and are not implied by this local
verifier.

## Feature 013 verification - 2026-09-04

The integrated source on `main` passed
`cargo --locked xtask verify` with the disposable PostgreSQL 18.4 plain and TLS
services running. The verification run was non-publishing and did not create a
release or tag; the later main integration is recorded above.

| Gate | Result |
| --- | --- |
| Formatting | Pass |
| Clippy with warnings denied | Pass |
| API documentation with warnings denied | Pass |
| Workspace tests | 772 passed, 0 failed |
| Library tests | 563 passed |
| CLI contract tests | 42 passed, including failed JSON and constraint diagnostics |
| PostgreSQL integration tests | 40 passed, including syntax and four constraint cases |
| Documentation tests | Pass |
| Database-backed tests | Pass for disposable plain and TLS PostgreSQL |
| Unix-socket integration | Skip: `IGNATIUS_TEST_PG_SOCKET_URI` is not set |

Focused mapper, editor, reducer, renderer, plain-mode, object-context, and JSON
checks are included in those results. The Unix-socket, Windows and Linux hand
terminal checks, screen-reader use, and Warp renderer behavior remain
unverified.

## Feature 014 verification - 2026-09-04

Feature 014 is integrated into `main` and remains separate from release
readiness. The focused checks below exercise the new grid
state and its boundaries. The authoritative verifier and disposable database
evidence are now complete for the available host environment.

| Focused gate | Result |
| --- | --- |
| `cargo check --locked` | Pass |
| `cargo test app::grid --lib` | 14 passed |
| `cargo test query::result --lib` | 10 passed |
| `cargo test postgres::session --lib` | 3 passed |
| `cargo test app::update --lib` | 100 passed |
| `cargo test app::palette --lib` | 13 passed |
| `cargo test ui::keymap --lib` | 23 passed |
| `cargo test ui::layout --lib` | 68 passed |
| `cargo test cli::output --lib` | 20 passed |
| `cargo test --test postgres_integration --no-run` | Pass |
| `git diff --check` | Pass after the final review |

The focused evidence proves local sorting is stable and reversible, selection
keeps source-row identity, filtering remains retained-row scoped, duplicate
column labels remain source-position safe, the last visible column is protected,
widths are bounded, available and unavailable type labels are distinct, frozen
wide views keep the selected column reachable, ASCII and no-colour output retain
meaning, hostile labels and cells cannot emit terminal controls, and machine
output is unchanged by result-view metadata. No focused check emitted an
`Effect::Execute` for a grid action.

The live row-bearing type-description test in `tests/postgres_integration.rs`
passed against disposable PostgreSQL 18.4 in both plain and TLS modes. It
returned `n`, `label`, and `active` as `int4`, `text`, and `bool`, while the
retained values stayed `2`, `ok`, and `NULL`; no second execution was required.

The final `cargo --locked xtask verify` passed formatting, lints with warnings
denied, API documentation, the complete auto-discovered suite, and documentation
tests. The suite reported 591 library tests and 210 integration or contract
cases without harness failures; the Unix-socket case remains a semantic skip,
not live evidence, because `IGNATIUS_TEST_PG_SOCKET_URI` is unset. The verifier
also marked the disposable plain and TLS PostgreSQL gates as passed.

Manual screen-reader use, live resize, Warp rendering, Windows/Linux hand
terminal checks, and a configured Unix-socket server remain unverified. Release
readiness, signing, provenance, publication, and owner authorization remain
separate and are not implied by this green local verifier.

## Previous green verification

Run on 2026-08-16 at committed HEAD `20bb3c3`, macOS 26.6.1 on Apple silicon,
rustc 1.97.1 (Homebrew), against `postgres:18.4-alpine` both plain and with TLS,
with `cargo xtask db up` running. 692 tests, and **one skip**: the Unix-socket
integration check, which needs `IGNATIUS_TEST_PG_SOCKET_URI` and a server
reachable over a socket, which the container does not publish. Everything else
ran. The count of skips is stated rather than the absence of failures, because
an earlier run in this same session reported these numbers with the database
down and the PostgreSQL suites had skipped rather than passed.

| Gate | Result |
| --- | --- |
| Formatting and clippy, `-D warnings` | Pass |
| Library tests | 513 passed |
| CLI contract tests | 38 passed |
| PostgreSQL integration tests | 38 passed, plain and TLS |
| Editor acceptance, reducer and pty | 3 passed |
| Password prompt, in a pty | 2 passed |
| Terminal restoration, in a pty | 3 passed |
| Documentation against the build | 6 passed |
| Cloud identity, against the TLS container | 4 passed |
| Release archive, native, core, notes and schema contracts | 3, 1, 71, 7 and 1 passed |
| CI, all jobs | Green on macOS, Windows and Linux |

Live evidence recorded in `docs/operations/verification.md`.

Separately, and not the same class of evidence: a statically linked x86-64
Linux binary built from `4bec4e3` was run on a bare Alpine container with
nothing installed, connected to PostgreSQL 18.4, and returned rows. From that
same artefact, three refusals and one success that matter:

- it refused a plain connection under `sslmode=verify-full` rather than
  downgrading;
- it opened a TLS session with a credential supplied by a provider program;
- it refused `sslmode=disable` for that provider without asking for the
  credential at all.

Those are the claims this feature rests on, made by something distributable
rather than by a test binary. A Windows binary exists from CI run
`31962001053` at the same revision and has done nothing but start and report
its own identity. No human has used either interactively yet.

Both are in `~/Downloads/ignatius-handoff-2026-08-16/` on the development
machine with their digests and a README, for hand-verification on 2026-08-17.

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
2. **Hosted database integration runs on Linux.** PostgreSQL 14-18 and the
   PostgreSQL 18 socket passed at remote main `52872ac`, rechecked on
   2026-09-04. The macOS and Windows CI jobs do not connect to a server.
   Newer local features have recorded macOS PostgreSQL 18.4 evidence and
   require a fresh candidate-source hosted matrix and native platform checks.
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
5a. **Cloud identity is implemented, and no cloud has been connected to.** The
   mechanism is proven against a real PostgreSQL server; `entra`, `aws` and
   `gcp` are transcriptions of vendor documentation checked on 2026-08-16 and
   nobody has run any of them against that cloud's own database. Treat the
   built-in commands as unverified until a dated row says otherwise.
5b. **The cloud provider tests are Unix-only.** They name real programs by
   absolute path. The feature is not Unix-only; the coverage is.
5c. **`--auth entra` is suspected not to work on Windows-native**, and this is
   the first thing to check there. The Azure CLI installs as `az.cmd`;
   `CreateProcessW` does no `PATHEXT` appending and Rust's `Command` appends
   only `.exe` when searching the path, so `Command::new("az")` will probably
   report the program as missing on a machine that has it - a legible message
   that is false, which is the worst kind. `gcloud` has the same shape; `aws`
   ships a real `.exe` and should be fine. Confirmed from the Rust
   documentation on 2026-08-16, not yet confirmed against a Windows machine.

   Not fixed tonight on purpose: the fix is Windows-only, untested, and
   collides with FR-1108's "no shell is involved at any point", which is a
   security property rather than a style rule. A workaround exists today and
   needs no release, because providers are data:

   ```toml
   [auth.providers.entra]
   command = ["cmd", "/C", "az", "account", "get-access-token",
              "--resource-type", "oss-rdbms", "--output", "json"]
   json-field = "accessToken"
   ```

   If Windows confirms it, the fix is a `cfg(windows)` built-in plus an explicit
   carve-out in the spec - safe for `entra` and `gcp` in particular because
   neither command carries a substituted argument.
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
9. **Main is not protected.** GitHub readback on 2026-09-04 reports
   `protected: false`. Local hooks and CI do not enforce server-side required
   checks. W13 prepares the governance proposal against current hosting
   capabilities; no hosting-plan entitlement is assumed by this observation.
10. **`rust-toolchain.toml` is inert on the development machine**, which uses a
   Homebrew rustc rather than rustup. This is an environment limitation, not a
   defect.
11. **Feature 005 editor is closed.** Enter routes to
    `Editor::insert_newline()` and every movement ends the undo coalescing run,
    both proven by tests confirmed to fail without the fix. The acceptance
    scenario in `tests/editor_contract.rs` is active: an eight-line statement is
    typed, corrected, navigated and run through the reducer, and the same
    journey runs through a real pty at 100 by 30. All three tests pass, along
    with the two focused keymap contracts.

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
- **Repository**: public at `tkomane/ignatius` since 2026-09-11, an owner
  decision taken to restore CI after the private Actions quota was exhausted.
  Route options remain in `docs/operations/release.md`; distribution beyond
  source is still deferred.

## Decisions still pending

1. **Trademark search and a domain**, before publishing only. Neither blocks
   development.
2. **Release trust choices in ADR-0008:** inventory/SBOM, signing identity and
   custody, provenance verification and retention. W11 prepares a concrete
   decision package before seeking the owner's choice.
3. **Public distribution/support and repository-governance actions:** W13
   prepares the routes and changes; W14 obtains authority for the exact
   candidate and external operation. Historical non-publishing authority is
   not a public release decision.

The credential-store, driver and initial clipboard decisions are closed.
Controlled provider accounts and native/assistive-technology access are
evidence dependencies, not reasons to reopen those decisions.

## Next actions, in order

1. **W02:** the reducer transition matrix is covered; no runtime test harness
   exists for `event_loop`, so decide between a message-injection seam and
   extended pty or live evidence before adding tests. Do not rewrite the state
   architecture to make a harness convenient.
2. **W03 continued:** decide the `standard_conforming_strings` boundary. The
   bootstrap query already reads server facts once per session; reading the
   setting there and refusing parameter binding when it is `off` is the
   smallest design that satisfies "never silently bind under incompatible
   semantics". A specification change comes first. Also collect live TUI
   evidence for the update review and run the new evidence on PostgreSQL 14-17.
3. **W04:** the parameter-expansion, provider stderr and provider timeout
   probes are done, along with the authorization-header redaction fix they
   found. The generated update bound-SQL log audit, refresh, clipboard
   confirmation and the panic path remain.
4. **W05:** run the current source against the PostgreSQL 14-18 matrix with
   plain and TLS fixtures and a real Linux socket; the Unix-socket gate is still
   a skip on this machine.
5. **W06-W07:** blocked on a controlled cloud account and a Windows host; the
   portable subprocess portions can proceed locally.
6. **W08-W10:** hand-verify terminals, accessibility and daily use once the
   relevant behaviour is stable; measure resource and recovery boundaries.
7. **W11/W13:** prepare the release trust, support, naming and governance
   decisions. **W12/W14:** assemble the candidate and obtain the owner's
   decision. No release claim is earned by waiting.
