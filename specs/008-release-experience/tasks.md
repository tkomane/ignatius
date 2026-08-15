# Tasks: Governed release experience

**Input**: Design documents from `specs/008-release-experience/`

**Prerequisites**: [spec.md](./spec.md), [plan.md](./plan.md),
[research.md](./research.md), [data-model.md](./data-model.md),
[quickstart.md](./quickstart.md)

**Status note**: T001-T008 and T017-T018 record the planning package, foundational
release-record contract and dependency-inventory contract that now exist in
this directory, `release-notes/` and `release-evidence/`. All later tasks
remain open until their evidence exists. No task authorizes a live publication,
signing operation, push or release.

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

Evidence: `jq empty release-notes/catalog.schema.json
release-notes/fixtures/*.json` and the one-record envelope checks pass on
2026-08-16. Cross-record equality and readiness semantics remain validator work
in later tasks.
- [ ] T009 [P] Define target-specific archive names, checksum manifest fields and digest verification rules in `docs/operations/release.md`
- [ ] T010 Partition generic release packaging from libpq-specific dependency evidence in `specs/001a-libpq-migration/tasks.md` and `docs/operations/release.md`

**Checkpoint**: A candidate can be described and rejected consistently before
any archive is published.

## Phase 3: User Story 1 - Prepare an identifiable release candidate (Priority: P1)

**Goal**: Produce a candidate whose version, source, target, artefact and
checksum facts are traceable and reproducible.

**Independent Test**: Prepare two candidates from the same declared source and
inputs, then deliberately change a byte, target, version and source state.
Every mismatch must be detected before readiness.

- [ ] T011 [P] [US1] Extend exact build-identity assertions for version, revision, source state, target, compiler and build identity in `tests/cli_contract.rs`
- [ ] T012 [US1] Add release-record generation and validation behind the existing development-task boundary in `xtask/src/release.rs`
- [ ] T013 [P] [US1] Add target-specific candidate archive jobs without publication in `.github/workflows/release.yml`
- [ ] T014 [US1] Generate a SHA-256 checksum manifest tied to filenames, targets and source revision in `xtask/src/release.rs`
- [ ] T015 [US1] Block candidate readiness for dirty source, reused version, missing target, missing checksum or failed required gate in `xtask/src/release.rs` and `.github/workflows/release.yml`
- [ ] T016 [US1] Record reproducibility and candidate-identity evidence in `docs/operations/verification.md`

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
- [ ] T019 [US2] Add the publication-readiness gate for missing or mismatched evidence in `xtask/src/release.rs`
- [ ] T020 [P] [US2] Add checksum, target, source, inventory and evidence mismatch cases in `tests/release_contract.rs`
- [ ] T021 [US2] Add release-evidence secret and artifact-scope checks to `.github/workflows/ci.yml` without uploading user data

**Checkpoint**: User Story 2 is complete only when an evidence reviewer can
identify the exact missing or mismatched item and the candidate cannot be called
ready by omission.

## Phase 5: User Story 3 - Install and upgrade with a recoverable path (Priority: P1)

**Goal**: Give each supported platform a truthful install, verify, upgrade and
rollback contract.

**Independent Test**: Follow the contract with a candidate on macOS, Windows
and Linux, recording platform-specific prerequisites and recovery outcomes.

- [ ] T022 [P] [US3] Document platform-specific prerequisites, archive selection, integrity verification and first start in `docs/operations/release.md`
- [ ] T023 [P] [US3] Reconcile supported targets, architectures and evidence classes in `docs/support/compatibility.md`
- [ ] T024 [US3] Document configuration/data preservation, failed upgrade repair and rollback in `docs/operations/release.md`
- [ ] T025 [US3] Add clean-install, upgrade and rollback evidence rows to `docs/operations/verification.md`
- [ ] T026 [US3] Add missing-dependency and wrong-architecture diagnostics to the appropriate platform/release boundary without weakening the existing driver fallback rules in `src/platform/` and `docs/operations/release.md`

**Checkpoint**: User Story 3 is complete only when each claimed platform has
install and recovery evidence, not only a CI compile result.

## Phase 6: User Story 4 - Support a released version with truthful notes (Priority: P2)

**Goal**: Keep release notes, compatibility claims and support identity tied to
the exact artefact while remaining privacy-safe.

**Independent Test**: Trace one candidate from its release record to notes,
compatibility, diagnostics and rollback guidance without exposing secrets.

- [ ] T027 [P] [US4] Add the structured release catalogue and its changelog-reference rules in `release-notes/catalog.json`
- [ ] T028 [US4] Add catalogue/changelog consistency validation and examples in `release-notes/validate.sh`
- [ ] T029 [P] [US4] Document the privacy-safe support identity bundle in `docs/support/diagnostics.md`
- [ ] T030 [US4] Reconcile stale release-facing capability claims in `README.md` and `docs/support/compatibility.md`

**Checkpoint**: User Story 4 is complete only when a support reader can identify
what is known, what is unverified and what to do next from the same release
identity.

## Phase 7: Polish and completion evidence

- [ ] T031 [P] Record approved packaging, inventory, signing and provenance choices in `docs/architecture/decisions/0008-packaging-and-release.md`
- [ ] T032 [P] Align the release threat model and data-handling guidance with the final evidence and trust path in `docs/security/threat-model.md` and `docs/security/data-handling.md`
- [ ] T033 Run the full release quickstart and `cargo xtask verify`, recording every gate and evidence class in `docs/operations/verification.md`
- [ ] T034 Perform a non-publishing dry run of the release workflow and record the owner authorization gate in `docs/operations/release.md`
- [ ] T035 Update `docs/product/roadmap.md` and `docs/status.md` only after all accepted release criteria have evidence

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

- T007-T009 can proceed in parallel because they own separate planning files.
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

## Shared-worktree coordination

- Codex owns this Feature 008 planning directory while the package is under
  active planning.
- For the active Phase 2 slice, Codex also owns only
  `release-notes/catalog.schema.json` and `release-notes/fixtures/`. This does
  not authorize the catalogue validator, CI, publication workflow or shared
  release documentation.
- Codex owns `release-evidence/inventory.md` for T017. The file is a provider-
  neutral contract only; executable inventory generation and shared CI remain
  separately owned tasks.
- Codex owns `release-evidence/README.md` for T018. Provider selection, signing
  operations and CI enforcement remain outside this handoff.
- Claude owns the active source paths in `src/app/`, `src/cli/`, `src/config/`,
  `src/ui/`, `tests/cli_contract.rs`, `src/lib.rs` and `src/history.rs`; no
  task above edits them except the future T026, which is blocked until a source
  handoff.
- Shared authorities such as `CHANGELOG.md`, `docs/operations/release.md`,
  `docs/operations/verification.md` and `.github/workflows/ci.yml` are deferred
  until the relevant task is explicitly selected and ownership is rechecked.
- No task authorizes staging, committing, pushing, tagging, signing, publishing
  or deleting user data.
