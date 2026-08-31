# Tasks: Enterprise PostgreSQL authentication

**Input**: [spec.md](./spec.md), [plan.md](./plan.md), [research.md](./research.md),
[data-model.md](./data-model.md), [contracts/libpq-adapter.md](./contracts/libpq-adapter.md)

**Closed without implementation by ADR-0012 on 2026-08-16.** The checked tasks
record planning and boundary work that exists. The unchecked native tasks are
retained as rejected historical scope, not as an active backlog: the actual
enterprise requirement is cloud token authentication over the existing TLS
driver, and GSSAPI, Kerberos and SSPI are not required.

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

Preparation evidence on 2026-08-16: `cargo --locked test --test
native_boundary_contract` passes a standard-library-only guard over the pure
layers. It rejects direct selected-driver and FFI markers while allowing public
`crate::postgres` contract types. This is a static boundary guard only; T026
remains open until the approved adapter seam has a build/test comparison and
the owner-approved migration gates exist.

## Phase 5: User Story 3 - Installation and release (P1)

**Independent test**: Install the release candidate on a clean macOS, Windows
and Linux environment and either start successfully or receive one repairable
dependency diagnostic.

### Feature 008 release boundary

Feature 008 owns generic release identity, target-specific archive naming,
archive byte checksums, provider-neutral inventory/signing/provenance states,
and versioned installation and rollback guidance. Feature 001a owns only the
native libpq-specific evidence that is required after its owner decision:
library and TLS identity, loader trust and search path, architecture/runtime
closure, selected discovery or bundling route, and repairable missing-
dependency diagnostics. A generic checksum or successful Rust build does not
prove any of those native facts.

Accordingly, T027 implements only the selected native discovery or bundle
check; T030 records the native clean-install and missing-dependency scenarios
without duplicating Feature 008's generic archive checks; T031 documents the
approved native route alongside, rather than inside, the generic installation
contract; and T032 binds native identity to the generic artifact and checksum
identity without replacing either contract. These tasks remain decision-gated
and unchecked until the owner approves the route.

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
- [x] T037 Update data-handling and support guidance for native credentials in `docs/security/data-handling.md` and `docs/support/diagnostics.md`

Evidence: the security and support documents now define native credentials,
tickets, tokens, connection strings and provider diagnostics as sensitive or
untrusted; require the existing non-printing secret, redaction and terminal
sanitisation boundaries; and limit support identity to safe facts and repair
actions. The text makes no authentication, dependency or packaging choice, and
does not claim native implementation evidence.

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

## Closure boundary

- ADR-0012 closes this package without a native driver or FFI implementation.
- The unchecked tasks remain historical rejected scope and must not be treated
  as current implementation work.
- Any future concrete GSSAPI, Kerberos or SSPI requirement must reopen the
  architecture, security, packaging and platform-evidence decisions together.
