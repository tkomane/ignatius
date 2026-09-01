# Tasks: Governed release experience

**Input**: Design documents from `specs/008-release-experience/`

**Prerequisites**: [spec.md](./spec.md), [plan.md](./plan.md),
[research.md](./research.md), [data-model.md](./data-model.md),
[quickstart.md](./quickstart.md)

**Status note**: T001-T010, T012-T020, T022-T025, T029 and T031 record the planning package,
foundational release-record contract, evidence contracts, semantic readiness
gate, target reconciliation and operator recovery contract that now exist in
this directory, `release-notes/`, `release-evidence/`, `xtask/`,
`docs/operations/release.md`, `docs/support/compatibility.md`,
`docs/support/diagnostics.md` and
`tests/release_contract.rs`. All remaining later tasks remain open until their evidence
exists. No task authorizes a live publication, signing operation, push or
release.

## Phase 1: Setup and design package

- [x] T001 Freeze the release requirements and scope boundary in `specs/008-release-experience/spec.md`
- [x] T002 Record the constitution check, ownership boundary and release architecture in `specs/008-release-experience/plan.md`
- [x] T003 Record repository-grounded release identity, artefact, checksum, supply-chain and platform decisions in `specs/008-release-experience/research.md`
- [x] T004 Define release records, artefacts, evidence bundles, installation contracts and support identity bundles in `specs/008-release-experience/data-model.md`
- [x] T005 Add the non-publishing release-readiness rehearsal and evidence matrix in `specs/008-release-experience/quickstart.md`
- [x] T006 Review requirements quality and scope coverage in `specs/008-release-experience/checklists/requirements.md`

## Phase 2: Foundational release contract

**Purpose**: Establish one identity, integrity and evidence vocabulary before
adding packaging or remote publication.

- [x] T007 [P] Define the versioned release-record schema and state transitions in `release-notes/catalog.schema.json`
- [x] T008 [P] Add valid, blocked and mismatched release-record fixtures in `release-notes/fixtures/`

Evidence: `jq empty release-notes/catalog.schema.json` and
`for fixture in release-notes/fixtures/*.json; do jq empty "$fixture"; done`
pass the JSON syntax checks, and the one-record envelope checks pass on
2026-08-16. Cross-record equality and readiness semantics remain validator work
in later tasks. The published-state schema conditional also requires nested
`publication_evidence.status` to be `verified`, matching the executable gate.
- [x] T009 [P] Define target-specific archive names, checksum manifest fields and digest verification rules in `docs/operations/release.md`
- [x] T010 Partition generic release packaging from libpq-specific dependency evidence in `specs/001a-libpq-migration/tasks.md` and `docs/operations/release.md`

Evidence: the release procedure now names the generic candidate, checksum,
inventory, signing/provenance and recovery boundary separately from Feature
001a's native libpq dependency identity, loader, architecture and
missing-dependency evidence. The 001a task list names the handoff without
approving discovery, bundling or an authentication route.

Evidence: the release procedure defines the exact target-triple filename
contract, UTF-8 structured and line-oriented checksum manifests, canonical
ordering, size and source/build identity fields, platform verification commands
and fail-closed mismatch rules. T014 owns the local byte-level sidecar
implementation; T013 owns the separate non-publishing archive workflow.

The executable basename guard now matches the catalogue and manifest schemas:
portable names start with an ASCII letter or digit, use only ASCII
letters/digits/dot/underscore/hyphen, plus `+` where it is the SemVer
build-metadata separator, and reject adjacent dots and reserved sidecar names.
The focused release-contract suite covers leading-punctuation and
non-covered target-row archive-name cases plus the ready-record manifest
boundary, publishable target-matrix completeness and non-covered artefact
binding, with 70 passing cases.

**Checkpoint**: A candidate can be described and rejected consistently before
any archive is published.

## Phase 3: User Story 1 - Prepare an identifiable release candidate (Priority: P1)

**Goal**: Produce a candidate whose version, source, target, artefact and
checksum facts are traceable and reproducible.

**Independent Test**: Prepare two candidates from the same declared source and
inputs, then deliberately change a byte, target, version and source state.
Every mismatch must be detected before readiness.

- [x] T011 [P] [US1] Extend exact build-identity assertions for version, revision, source state, target, compiler and build identity in `tests/cli_contract.rs`

Evidence on 2026-08-31: the authoritative CLI contract now invokes the real
binary and compares the product version with Cargo, the embedded revision with
`HEAD`, the embedded source state with the current clean/modified state, the
build identity with the compile-time value, the target with `rustc -vV`, and
the compiler identity with `rustc --version`. The focused exact test passes.
`build.rs` also watches tracked source state, the index, packed refs and the
active branch ref, while ignoring absent Git paths, so those identity inputs
invalidate the build metadata.
- [x] T012 [US1] Add release-record generation and validation behind the existing development-task boundary in `xtask/src/release.rs`
- [x] T013 [P] [US1] Add target-specific candidate archive jobs without publication in `.github/workflows/release.yml`
- [x] T014 [US1] Generate and verify a SHA-256 checksum manifest tied to filenames, targets, source revision, byte size and exact archive bytes in `xtask/src/release.rs`
- [x] T015 [US1] Block candidate readiness for dirty source, reused version, missing target, missing checksum or failed required gate in `xtask/src/release.rs` and `.github/workflows/release.yml`
- [x] T016 [US1] Record reproducibility and candidate-identity evidence in `docs/operations/verification.md`

Evidence: `cargo xtask release generate` captures the manifest version, Git
revision/state, exact release-tag state, explicit target and build identity,
emits candidate or blocked state history, binds schema fields and refuses
output overwrite. `cargo xtask release validate` accepts internally consistent
candidate/blocked records without calling them ready; `cargo xtask release check`
remains the strict readiness gate. The release contract suite covers these
commands and current T012-T016/T019-T020 cases.

T013 implementation evidence is the new non-publishing release workflow with
explicit Linux, macOS and Windows target rows, pinned toolchain and action
references, target-specific archive naming, exact archive size capture, local
sidecar generation and exact-byte verification, plus run-scoped retained
workflow evidence. The upload root is now an exact allowlist of the archive,
record and two verified sidecars, checked by
`cargo xtask release evidence-scope` before upload. It has no release, signing,
provenance or publication step; each matrix job emits a per-target record, so a
later aggregation is still required before a multi-target candidate can be
called complete. Publishable records now also fail closed when any supported
target row is missing.
Rerun identity is attempt-specific in both the embedded
IGNATIUS_BUILD_IDENTITY and the candidate record/evidence path; the focused
workflow contract test rejects a return to the pre-attempt identity.
The release-note entry remains derived from the product version only, so a CI
retry changes technical build/evidence identity without creating a second
release-note identity for the same version.
On Windows, the shared Bash packaging path now fails closed when `cygpath` is
unavailable instead of mixing native and POSIX temporary-root forms; the
workflow contract test asserts that guard.
The target matrix is an implementation rehearsal until platform and
reproducibility evidence is recorded by T016 and the support-matrix work.

T015 implementation evidence is the fail-closed readiness reporting in
`xtask/src/release.rs`: dirty or detached source state and declared candidate
blockers are surfaced, while duplicate versions, missing targets, missing
checksums, incomplete evidence and unverified signature or provenance states
remain named blocking conditions. The workflow asserts that each per-target
candidate fails `release check` until the remaining gates exist. The focused
release-contract suite has 70 passing cases; this does not authorize publication
or replace T016's cross-platform evidence.

The workflow rehearsal also asserts the actionable readiness output for incomplete
evidence and unverified signature and provenance states, so a future regression
cannot reduce the release gate to an unexplained generic failure.

T016 implementation evidence is the release-specific identity and
reproducibility matrix in `docs/operations/verification.md`. The workflow now
uses the repository-owned `scripts/create-release-archive.py` helper, which
fixes archive member metadata and installs the completed archive with a
same-directory hard link, refuses a symlinked output parent, and therefore
cannot replace or redirect a destination silently after the initial check. The
focused test repeats both supported archive formats, compares their exact bytes,
confirms existing outputs remain unchanged and checks the symlinked-parent
refusal. Hosted repeat-build, cross-runner, hosted-platform and
multi-target-aggregation evidence remain open. This makes no
production-readiness or publication claim.

The quickstart now installs failure cleanup for disposable database services
and the temporary staging root before work begins, so an interrupted rehearsal
does not silently retain those local resources.

T023 implementation evidence is the separate release target matrix in
`docs/support/compatibility.md`. It maps the three workflow target triples to
archive formats, runners and evidence classes without upgrading a packaging
definition into a support or installation claim. Unlisted architectures remain
outside the release boundary.

T014 implementation evidence is the non-publishing `release manifest generate`
and `release manifest verify` command pair, the canonical sidecar schema in
`release-notes/release-manifest.schema.json`, exact-byte hashing, atomic
two-sidecar publication and 18 focused sidecar mismatch cases in
`tests/release_contract.rs`. The shared all-target compile, 497 library tests,
38 CLI tests and clippy gate pass on 2026-08-16.

**Checkpoint**: User Story 1 is complete only when a candidate can be traced to
one version and source revision and all mismatch scenarios are rejected.

## Phase 4: User Story 2 - Review supply-chain evidence before publication (Priority: P1)

**Goal**: Make dependency inventory, signature status and provenance status
visible without selecting an unapproved provider or overstating evidence.

**Independent Test**: Review complete evidence and then remove or mismatch one
inventory, signature or provenance item; readiness must become blocked.

- [x] T017 [P] [US2] Define the dependency-inventory input, retention and verification contract in `release-evidence/inventory.md`

Evidence: the provider-neutral inventory contract defines direct and
transitive inputs, build identity, retention, verification and fail-closed
privacy rules. It explicitly does not claim that executable generation exists.
- [x] T018 [P] [US2] Define provider-neutral signature and provenance evidence fields and failure states in `release-evidence/README.md`

Evidence: the contract separates checksum, signature and provenance facts,
defines explicit missing and invalid states, binds evidence to the exact
artefact identity, and sets the provider/authorization decision gate.
- [x] T019 [US2] Add the publication-readiness gate for missing or mismatched evidence in `xtask/src/release.rs`
- [x] T020 [P] [US2] Add checksum, target, source, inventory and evidence mismatch cases in `tests/release_contract.rs`

Evidence: `cargo check -p xtask`, `cargo clippy -p xtask --all-targets -- -D warnings`,
focused formatting checks and 70 release-contract cases cover generation,
validation, blocked, evidence-binding, synthetic ready/published records and
exact-byte sidecar mismatch cases. The gate checks declared checksums and
evidence identity, and a non-ready candidate now names incomplete evidence and
unverified signature or provenance states rather than only saying it is not
publishable. T014 now owns sidecar parsing, byte hashing, size re-read and
fail-closed verifier exit behavior. T013 owns the archive portion of the new
release workflow; T015 owns its readiness assertion. Both remain separate from
the shared CI workflow.

Manifest generation also reapplies the ready/published readiness invariants,
so a schema-invalid record cannot reach sidecar generation through the
candidate/blocked compatibility path.

Additional T020 evidence on 2026-08-16: the release verifier now rejects an
evidence bundle reached through a symlink, not only textual `..` traversal, so
the controlled evidence root cannot silently resolve to unrelated files. It
also rejects malformed blocker arrays, unsafe identity values, duplicate JSON
object members and schema-invalid nullable or uncontrolled reference fields.
The release matrix is enforced as the exact three declared target triples, and
the evidence and sidecar paths reject symlinked roots as well as symlinked
files. A published record with only observed publication evidence is also
covered and remains blocked until the evidence is verified.
The catalogue and manifest schemas now reject internal `..` in target, archive,
evidence and publication tokens, matching the executable path-safety checks.
Evidence references additionally reject empty, dot and parent components and
trailing separators, so the executable validator and catalogue schema agree on
canonical controlled paths.
Reserved sidecar names are rejected at catalogue validation as well as by the
manifest parser, so a record cannot validate and then fail only during sidecar
generation. Both record and manifest output creation use symlink metadata, and
manifest verification rejects symlinked archive and sidecar inputs, so a
symlinked record parent, dangling output-directory symlink or symlinked
manifest parent is refused before writing. The catalogue and
manifest schemas now exclude path separators from archive basenames, matching
runtime validation. Catalogue validation also refuses a symlinked record input,
so the ordinary-file evidence rule applies before any release identity is
loaded. The focused release contract now has 70 passing cases, including the
non-covered target artefact binding guard and the conflicting `name`/
`artefact_name` evidence-alias rejection.
The latter closes a real fail-open case where a ready evidence bundle could
carry the correct primary `name` alongside a conflicting alias; validation now
rejects that mismatch before readiness.
The release schemas now apply the same SemVer rule as the executable validator
for numeric prerelease identifiers, including rejection of leading zeroes. The
scope schema also rejects internal `.` and `..` path components, empty
components and trailing separators, matching the executable allowlist checker.
Nullable checksum fields reject empty strings in the schema as well as the
executable validator.
- [x] T021 [US2] Add release-evidence secret and artifact-scope checks to `.github/workflows/ci.yml` without uploading user data

Evidence on 2026-08-31: shared CI runs the focused release contracts, builds a
synthetic four-file upload root, validates that exact root with `cargo xtask
release evidence-scope`, and scans only the validated root with the Gitleaks
v8.30.0 container pinned by digest and denied container network access. The
machine-readable allowlist is `release-evidence/scope.schema.json`; focused
contracts reject missing, unexpected, duplicate, traversal, symlinked and
non-regular entries, including a symlinked allowlist parent. A workflow contract
asserts the checked root is the scanner mount and forbids widening it to the
workspace. Ruby parsed the YAML and the focused workflow contract passed.
Hosted CI run `33445418179` passed this job at revision
`247d1c7e829aa5b642a5047e56e53d402e386dbb` on 2026-09-01. This is hosted
evidence for the synthetic four-file scope and secret scan, not a candidate
archive, complete evidence bundle or publication result.

**Checkpoint**: User Story 2 is complete only when an evidence reviewer can
identify the exact missing or mismatched item and the candidate cannot be called
ready by omission.

## Phase 5: User Story 3 - Install and upgrade with a recoverable path (Priority: P1)

**Goal**: Give each supported platform a truthful install, verify, upgrade and
rollback contract.

**Independent Test**: Follow the contract with a candidate on macOS, Windows
and Linux, recording platform-specific prerequisites and recovery outcomes.

- [x] T022 [P] [US3] Document platform-specific prerequisites, archive selection, integrity verification and first start in `docs/operations/release.md`

Evidence: the platform installation section names the exact macOS, Windows and
Linux target rows, archive formats, host prerequisites, pre-extraction digest
and size checks, binary-only extraction shape and first-start commands. It
keeps target selection separate from runtime and hand evidence, states the
current unsigned and unpublished boundary, and does not prescribe a security
control bypass.
- [x] T023 [P] [US3] Reconcile supported targets, architectures and evidence classes in `docs/support/compatibility.md`
- [x] T024 [US3] Document configuration/data preservation, failed upgrade repair and rollback in `docs/operations/release.md`

Evidence: the recovery section binds backups to the paths printed by
`ignatius config paths`, distinguishes configuration, saved queries, history
and logs, documents dry-run and timestamped migration backups, keeps versions
side by side, and gives non-destructive failed-upgrade and rollback steps. It
does not claim that a platform rollback rehearsal has already run; T025 owns
those evidence rows.
- [x] T025 [US3] Add clean-install, upgrade and rollback evidence rows to `docs/operations/verification.md`

Evidence: the verification record now has nine target/scenario rows covering
clean install and first start, upgrade/data preservation, and failed-upgrade
rollback for macOS Apple silicon, Windows x86_64 and Linux x86_64 GNU. Every
row is explicitly marked unverified on 2026-08-16 and names the evidence needed
without turning the packaging workflow into platform hand evidence.
- [x] T026 [US3] Resolve missing-dependency and wrong-architecture diagnostics at the appropriate platform/release boundary without weakening the existing driver rules in `src/platform/` and `docs/operations/release.md`

Evidence on 2026-08-31: ADR-0012 closed Feature 001a without a libpq migration,
so there is no native database dependency, discovery route or driver fallback
for `src/platform/` to diagnose. The release guide now gives pre-execution host
architecture checks and repairable macOS, Linux and Windows loader outcomes.
It explains that a wrong-architecture binary cannot emit its own diagnostic,
requires the exact host message in support evidence, and forbids substituting
another target row. The existing `tokio-postgres` and TLS failure boundaries
remain unchanged.

**Checkpoint**: User Story 3 is complete only when each claimed platform has
install and recovery evidence, not only a CI compile result.

## Phase 6: User Story 4 - Support a released version with truthful notes (Priority: P2)

**Goal**: Keep release notes, compatibility claims and support identity tied to
the exact artefact while remaining privacy-safe.

**Independent Test**: Trace one candidate from its release record to notes,
compatibility, diagnostics and rollback guidance without exposing secrets.

- [x] T027 [P] [US4] Add the structured release catalogue and its changelog-reference rules in `release-notes/catalog.json`
- [x] T028 [US4] Add catalogue/changelog consistency validation and examples in `release-notes/validate.sh`

Evidence on 2026-08-31: `release-notes/catalog.json` contains the one 0.1.0
record, binds it to the exact `[0.1.0]` changelog heading, the rehearsal source
revision, three declared target rows and one deterministic local macOS archive.
Its retained state is `blocked`: Windows and Linux artefacts, inventory,
signature, provenance, platform recovery evidence and owner authorization are
explicit blockers. Rebuilding after the catalogue update reproduced the exact
3,010,634-byte archive and SHA-256
`c7ba67b568ea803d9f6f7d8e1de802e25cdf539d1377618c76825d1683aa380d`.

The read-only `release-notes/validate.sh` wrapper delegates semantic record
validation to `cargo --locked xtask release
validate`, then requires every single-line `notes_reference.entry` to name its
exact semantic-version token and occur exactly once as a `## <entry>` heading
in the selected `CHANGELOG.md`. Its focused contract tests accept a synthetic
matching changelog and reject body-only mentions, stable-version matches inside
preview versions, near-version headings and duplicate headings.
The wrapper also refuses symlinked input ancestors, including nested
non-immediate parents, for the catalogue or changelog before canonicalizing the
input path, so a transport path cannot silently redirect validation to another
directory. Its focused contract coverage exercises both immediate and nested
ancestor symlinks. The repository catalogue and changelog now have a focused
passing contract in addition to the negative fixtures.
- [x] T029 [P] [US4] Document the privacy-safe support identity bundle in `docs/support/diagnostics.md`

Evidence: the support identity section defines the minimum product, source,
build, target, platform, optional server and failure facts, gives a copyable
bundle template, and explicitly excludes passwords, tokens, credential files,
full URIs, SQL text, result data and unrestricted environment dumps. It also
warns that path-bearing `doctor --json` details and local names require review
before sharing.
- [x] T030 [US4] Reconcile stale release-facing capability claims in `README.md` and `docs/support/compatibility.md`

Evidence on 2026-08-31: the README and compatibility authority now agree that
both interactive surfaces prompt safely, the credential-store route was
rejected by ADR-0011, GSSAPI/Kerberos/SSPI are unsupported, and cloud identity
uses short-lived tokens over TLS. The compatibility record retains separate
automated, hand, server, live-cloud and unverified evidence classes, and its
release target matrix still says that no hosted archive run or publication has
been observed.

**Checkpoint**: User Story 4 is complete only when a support reader can identify
what is known, what is unverified and what to do next from the same release
identity.

## Phase 7: Polish and completion evidence

- [x] T031 [P] Record approved packaging, inventory, signing and provenance choices in `docs/architecture/decisions/0008-packaging-and-release.md`

T031 evidence on 2026-08-16: ADR-0008 now records the accepted archive-first
and SHA-256 contract, the provider-neutral dependency-inventory contract, the
separate signature and provenance evidence classes, their fail-closed states,
and the unresolved provider, retention, verification and owner-authorization
gates. This completes the decision-boundary documentation; it does not claim
that an inventory, signature, provenance statement or publication exists.
- [x] T032 [P] Align the release threat model and data-handling guidance with the final evidence and trust path in `docs/security/threat-model.md` and `docs/security/data-handling.md`

Evidence on 2026-08-31: the provider-neutral sections describe the complete
current source-to-build-to-archive-to-evidence trust path, safe retained
identity facts and the explicit exclusion of credentials, SQL, results,
configuration, environment dumps and unrestricted logs. They cross-reference
ADR-0008, the inventory/signature/provenance contract and the schema-1 staging
allowlist while keeping file-set scope, secret scanning and verification as
separate controls. Provider selection remains an explicit future authorization
gate rather than missing alignment. The retained Feature 001a addendum now
states that ADR-0012 closed the native route without implementation.

- [ ] T033 Run the full release quickstart and `cargo xtask verify`, recording every gate and evidence class in `docs/operations/verification.md`
- [ ] T034 Perform a non-publishing dry run of the release workflow and record the owner authorization gate in `docs/operations/release.md`
- [ ] T035 Update `docs/product/roadmap.md` and `docs/status.md` only after all accepted release criteria have evidence

Current local evidence on 2026-08-31: `cargo --locked xtask verify` passed
formatting, lints, 513 unit/layout tests, 38 CLI contract tests and 38
PostgreSQL integration tests against the disposable PostgreSQL 18.4 plain/TLS
services. Teardown removed both containers, their network and data; a separate
status check reported `Not running`. The focused release run passed 3
archive-helper, 1 native-boundary, 71 release-contract, 7 release-notes and 1
release-schema tests, and the repository catalogue validated as one blocked
record with one exact changelog heading.

The same rehearsal built and extracted the real Apple silicon archive, verified
its 3,010,634 bytes and SHA-256 sidecars, repeated identical archive bytes,
validated the exact four-file upload scope and scanned that scope with the
network-disabled, digest-pinned Gitleaks container. First-start identity and
configuration checks passed; `doctor --json` reported seven OK, three skipped
and no failed checks. Readiness exited 1 with all 13 blockers named.

Follow-up evidence on 2026-09-01: hosted non-publishing run `33561901626`
checked out exact revision
`247d1c7e829aa5b642a5047e56e53d402e386dbb`. The macOS, Windows and Linux jobs
all built their declared target binary, then failed in the common packaging
step before the archive helper ran. The pinned Cargo 1.97 package-ID spelling
did not satisfy the workflow's exact `#ignatius@` parser, leaving the version
guard empty; no archive or run-scoped candidate evidence was retained.

The local repair reads the version from its `Cargo.toml` authority and gives
the version, missing-binary and symlink refusals explicit diagnostics. Six
workflow-focused contracts pass. The complete focused release run passes 3
archive-helper, 1 native-boundary, 72 release-contract, 7 release-notes and 1
release-schema tests. The full Docker-backed verifier also passes formatting,
lints, 513 unit/layout tests, 38 CLI contracts and 38 PostgreSQL 18.4 plain/TLS
integration tests; teardown removed the disposable containers, network and
data, and the final status was `Not running`.

T033 remains open because the full quickstart still lacks a successful hosted
target run, Windows/Linux hand verification, macOS upgrade and rollback, and
cross-runner reproducibility. T034 remains open because its first hosted run
failed and the locally verified repair has not been committed, pushed or
re-run; the owner-authorization gate also remains absent. T035 stays open: the
roadmap still says `In planning`, and the status record does not call the
candidate complete or ready. Those completion claims remain forbidden until
every accepted release criterion has evidence.

## Dependencies and execution order

- **Phase 1** is complete for the planning package and has no source dependency.
- **Phase 2** blocks candidate generation because identity, checksum and
  evidence semantics must be stable first.
- **User Story 1** depends on Phase 2 and is the MVP for a reviewable candidate.
- **User Story 2** depends on the candidate identity from User Story 1, but
  inventory and provider-neutral evidence documentation can proceed in parallel.
- **User Story 3** depends on target rows from User Story 1 and evidence status
  from User Story 2; its documentation can be drafted in parallel with both.
- **User Story 4** depends on the stable release record and support contract.
- **Phase 7** depends on all required stories and owner decisions; it must not
  convert a rehearsal into a live publication.

## Parallel opportunities

- T007-T008 and T017-T018 can proceed in parallel because they own separate
  planning or evidence files. T009 edits a section of shared release procedure
  documentation and therefore requires its explicit section-scoped handoff.
- T011, T013 and T017-T018 can proceed in parallel after Phase 2, subject to
  CI and source ownership review.
- T022-T025 can proceed in parallel as separate platform/documentation scopes.
- T027 and T029 can proceed in parallel; T028 waits for the catalogue shape.
- T031-T032 can proceed in parallel after the corresponding provider and trust
  decisions are approved.

## Implementation strategy

### MVP first

1. Complete the planning package and Phase 2 contract.
2. Deliver User Story 1 only: a non-publishing candidate record, target rows and
   checksum validation.
3. Stop and review one complete and one blocked candidate before enabling any
   release automation.

### Incremental delivery

1. Add supply-chain evidence review and mismatch blocking.
2. Add platform install, upgrade and rollback evidence.
3. Add catalogue and privacy-safe support tracing.
4. Run the full evidence gates and obtain explicit owner authorization before
   any live publication task.

## Current worktree boundary

- Feature 008 owns its specification package, release schemas and validators,
  blocked catalogue, exact 0.1.0 changelog heading, release evidence contracts,
  xtask release commands, non-publishing workflow, narrow shared CI gate and the
  named release, verification, compatibility, diagnostics and security sections.
- Application behaviour remains outside the feature. T011 changes only the
  authoritative CLI identity assertion; T026 adds operator guidance because a
  wrong-architecture binary cannot run its own diagnostic.
- The retained catalogue points to an intentionally incomplete canonical
  evidence path. The workflow upload tree is a transport envelope, not complete
  evidence, and cannot make the record ready by omission.
- No task authorizes staging, committing, pushing, tagging, signing, publishing,
  distribution or deletion of user data.
