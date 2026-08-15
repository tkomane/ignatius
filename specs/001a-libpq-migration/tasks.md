# Tasks: Enterprise PostgreSQL authentication

**Input**: [spec.md](./spec.md), [plan.md](./plan.md), [research.md](./research.md),
[data-model.md](./data-model.md), [contracts/libpq-adapter.md](./contracts/libpq-adapter.md)

This is a decision-gated task list. The checked planning tasks document the
package that now exists. No implementation task is checked until the owner
approves the migration gates and the evidence exists.

## Phase 1: Planning package

- [x] T001 Create the decision-gated Feature 001a specification in `specs/001a-libpq-migration/spec.md`
- [x] T002 Record the proposed serialized-session concurrency model in `docs/architecture/decisions/0010-libpq-concurrency-model.md`
- [x] T003 Resolve repository-grounded migration questions in `specs/001a-libpq-migration/research.md`
- [x] T004 Define the adapter entities and lifecycle states in `specs/001a-libpq-migration/data-model.md`
- [x] T005 Define the connect, execute, cancel and close contract in `specs/001a-libpq-migration/contracts/libpq-adapter.md`
- [x] T006 Add the constitution and requirements quality checklist in `specs/001a-libpq-migration/checklists/requirements.md`
- [x] T007 Add the design and release-gate validation guide in `specs/001a-libpq-migration/quickstart.md`

## Phase 2: Decision gates

- [ ] T008 Record the owner's confirmation that GSSAPI, Kerberos or Windows SSPI is required in `docs/architecture/decisions/0009-libpq-migration.md`
- [x] T009 [P] Compare maintained wrappers and binding approaches for license, maintenance, authentication and platform coverage in `specs/001a-libpq-migration/research.md`
- [ ] T010 [P] Decide whether each supported platform bundles or discovers the native dependency in `docs/architecture/decisions/0010-libpq-concurrency-model.md`
- [ ] T011 Approve the session actor, cancellation, shutdown and back-pressure invariants in `docs/architecture/decisions/0010-libpq-concurrency-model.md`
- [x] T012 Update the threat model for native loading, FFI ownership and enterprise credentials in `docs/security/threat-model.md`

## Phase 3: User Story 1 - Enterprise authentication (P1)

**Independent test**: Connect to a controlled server for every approved route,
identify the authenticated role, run a read query and verify no weaker fallback.

- [ ] T013 [P] Add the selected native dependency and platform feature gates in `Cargo.toml` and `Cargo.lock`
- [ ] T014 [P] Add the adapter ownership boundary and safety invariants in `src/postgres/adapter.rs`
- [ ] T015 Implement enterprise route resolution without changing existing target precedence in `src/connection/target.rs`
- [ ] T016 Implement the approved enterprise authentication path in `src/postgres/adapter.rs`
- [ ] T017 Preserve server identity, TLS posture and connection facts in `src/postgres/session.rs`
- [ ] T018 Add controlled server-backed success and unsupported-route tests in `tests/postgres_libpq.rs`
- [ ] T019 Add redacted authentication failure and missing-dependency tests in `tests/postgres_libpq.rs`

## Phase 4: User Story 2 - Compatibility and lifecycle parity (P1)

**Independent test**: Run the complete pre-migration contract suite and compare
target precedence, TLS, results, notices, transaction state, cancellation,
connection loss, redaction and exit codes.

- [ ] T020 [P] Adapt session execution behind the existing contract in `src/postgres/session.rs`
- [ ] T021 [P] Preserve native and server error mapping in `src/postgres/error.rs`
- [ ] T022 Preserve TLS policy and refusal of silent downgrade in `src/postgres/tls.rs`
- [ ] T023 Preserve ordered execution and cancellation confirmation in `src/postgres/adapter.rs`
- [ ] T024 Add transaction, notice, connection-loss and cancellation parity tests in `tests/postgres_libpq.rs`
- [ ] T025 Add CLI stream and exit-code parity coverage in `tests/cli_contract.rs`
- [ ] T026 Verify pure application and query layers do not depend on the native adapter in `src/app/`, `src/query/` and `src/ui/`

## Phase 5: User Story 3 - Installation and release (P1)

**Independent test**: Install the release candidate on a clean macOS, Windows
and Linux environment and either start successfully or receive one repairable
dependency diagnostic.

- [ ] T027 [P] Implement the selected native dependency discovery or bundle check in `src/platform/`
- [ ] T028 [P] Include the dependency identity in build and diagnostic evidence in `build.rs` and `src/cli/`
- [ ] T029 Update cross-platform build and service prerequisites in `.github/workflows/ci.yml`
- [ ] T030 Add clean-install and missing-dependency scenarios to `docs/operations/verification.md`
- [ ] T031 Document the supported installation and native dependency route in `docs/operations/release.md` and `docs/support/compatibility.md`
- [ ] T032 Verify artifact architecture, checksums and dependency identity in `docs/operations/release.md`

## Phase 6: User Story 4 - Safe failure and security (P1)

**Independent test**: Exercise authentication failure, TLS refusal, cancellation,
connection loss and shutdown without secret leakage or automatic replay.

- [ ] T033 [P] Add native credential and ticket redaction cases in `src/diagnostics/redaction.rs`
- [ ] T034 [P] Add native-driver failure mapping cases in `src/postgres/error.rs`
- [ ] T035 Verify native handles and worker shutdown on every outcome in `tests/postgres_libpq.rs`
- [ ] T036 Verify no SQL replay and truthful unknown outcomes in `tests/postgres_libpq.rs`
- [ ] T037 Update data-handling and support guidance for native credentials in `docs/security/data-handling.md` and `docs/support/diagnostics.md`

## Phase 7: Polish and completion evidence

- [ ] T038 Run `cargo xtask verify` with the migration build and record every gate in `docs/operations/verification.md`
- [ ] T039 [P] Update the architecture component boundary and trust zones in `docs/architecture/containers-and-components.md`
- [ ] T040 [P] Update the roadmap and current status only after all approved authentication and packaging gates pass in `docs/product/roadmap.md` and `docs/status.md`
- [ ] T041 Confirm the migration remains reversible or supersede ADR-0009 if the owner rejects the cost in `docs/architecture/decisions/0009-libpq-migration.md`

## Dependencies and execution order

Phase 1 is complete and provides the contract. Phase 2 blocks every source
change. User Story 1 establishes the adapter capability; User Story 2 depends
on its seam and proves parity; User Story 3 can proceed in parallel after the
native dependency choice; User Story 4 spans all adapter outcomes and must pass
before release evidence. Phase 7 follows all approved story gates.

## Parallel opportunities

- T009 and T010 can proceed in parallel as decision research, but neither can
  be treated as approved without the owner gate.
- T013 and T014 can proceed in parallel after T008-T011.
- T018 and T019 can proceed in parallel against separate controlled fixtures.
- T020, T021 and T022 can proceed in parallel only after the adapter seam is
  stable and each change remains inside `src/postgres`.
- T027, T030 and T031 can proceed in parallel after the packaging choice.
- T033 and T034 can proceed in parallel because they have separate authorities.
- T032 and T038 must wait for all platform and server evidence to be stable.

## Shared-worktree coordination

- This package changes only `docs/architecture/decisions/0010-*`, the new
  `specs/001a-libpq-migration/` directory and explicitly named roadmap/status/
  verification documentation at planning time.
- Live handoff on 2026-08-16: the Feature 005 SQL-editing and syntax-colouring
  slice is integrated in `060380c`, including the editor, query lexer and UI
  paths. Claude now has an uncommitted query-history/configuration slice across
  `src/app/`, `src/cli/`, `src/config/`, `src/ui/`, `tests/cli_contract.rs`,
  `src/lib.rs` and untracked `src/history.rs`. Those paths must not be staged,
  reset, reformatted or edited by 001a while Claude continues.
- `specs/008-release-experience/` is currently untracked in the shared worktree
  and contains Codex's new Feature 008 specification and requirements checklist.
  Codex owns the remaining planning artifacts in that directory. Claude owns
  the active source slice listed above and must not edit or reset the Feature
  008 planning package without an explicit handoff.
- Codex also owns the Feature 005 planning artifacts under
  `specs/005-sql-editing/` and the 001a threat-model addendum in
  `docs/security/threat-model.md`. The completed Feature 007 T030/T031
  reservation has ended.
- Do not create `.specify/feature.json` for this package while another feature
  is active; its explicit directory is the source of truth for this planning
  handoff.
- No task authorises a commit, push, release or production change.

## Implementation strategy

1. Keep the decision gate visible and resolve the remaining requirement.
2. Prove one adapter seam and one approved authentication route.
3. Prove compatibility and lifecycle parity before changing the default route.
4. Prove clean installation on every supported platform.
5. Update roadmap and status only from release-grade evidence.
