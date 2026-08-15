# Tasks: Plain line-oriented terminal mode

**Input**: Design documents from `/specs/007-plain-line-mode/`

**Prerequisites**: [spec.md](./spec.md), [plan.md](./plan.md),
[research.md](./research.md), [data-model.md](./data-model.md),
[contracts/plain-mode.md](./contracts/plain-mode.md)

**Tests**: Included because the feature contract is user-visible, safety-critical
and stream-sensitive. Existing implementation tasks are marked complete only
where the current worktree and the combined verification run provide evidence.

**Organization**: Tasks are grouped by user story. The current implementation is
staged in the shared worktree; unchecked tasks are remaining release evidence or
documentation follow-up, not permission to overwrite Claude's active files.

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Establish the feature contract and its traceability boundary.

- [x] T001 Define the invocation, stream, command, safety and exit-code contract in `specs/007-plain-line-mode/contracts/plain-mode.md`

**Checkpoint**: The plain mode has a single user-visible contract before further
implementation or verification work is merged.

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Preserve existing PostgreSQL, diagnostics, cancellation and output
authorities rather than creating parallel implementations.

- [x] T002 [P] Reuse PostgreSQL-aware statement boundaries in `src/cli/plain.rs` and `src/query/statements.rs`
- [x] T003 [P] Reuse the cancellable session execution path in `src/cli/interactive.rs` and `src/postgres/session.rs`
- [x] T004 [P] Preserve stream separation and existing result/diagnostic contracts in `src/cli/plain.rs` and `src/cli/output.rs`
- [x] T005 Validate the constitution and feature assumptions in `specs/007-plain-line-mode/plan.md` and `specs/007-plain-line-mode/research.md`

**Checkpoint**: Shared safety, PostgreSQL correctness, cancellation and output
contracts are identified and no new driver or persistence boundary is introduced.

## Phase 3: User Story 1 - Use the client without a full-screen terminal (Priority: P1) MVP

**Goal**: Connect, enter SQL and read a complete line-oriented transcript without
alternate-screen, raw-mode or cursor-control behaviour.

**Independent Test**: Run the plain-mode subprocess contract in
`tests/cli_contract.rs` with the disposable PostgreSQL services and confirm data
and messages are separated and free of terminal-control sequences.

### Tests for User Story 1

- [x] T006 [P] [US1] Add plain-mode subprocess coverage for `TERM=dumb`, stream separation and control-sequence absence in `tests/cli_contract.rs`
- [x] T007 [P] [US1] Add line-reader unit coverage for blank input, continuation and statement completion in `src/cli/plain.rs`

### Implementation for User Story 1

- [x] T008 [US1] Implement the line-oriented session, connection summary, prompt and result/message stream split in `src/cli/plain.rs`
- [x] T009 [US1] Route `--plain` for `connect` and the default interactive entry point in `src/cli/mod.rs`
- [x] T010 [US1] Keep the default full-screen route unchanged when `--plain` is absent in `src/cli/mod.rs`
- [x] T011 [US1] Document plain-mode invocation and scrollback behaviour in `README.md`, `docs/support/compatibility.md` and `CHANGELOG.md`

**Checkpoint**: A user who cannot use a full-screen terminal can connect, run a
query and leave while reading a plain transcript.

## Phase 4: User Story 2 - Enter PostgreSQL statements naturally (Priority: P1)

**Goal**: Accumulate complete statements safely and provide discoverable local
commands without claiming full `psql` compatibility.

**Independent Test**: Exercise the reader unit tests and the plain-mode contract
with a multi-line statement containing quoted semicolons and the `\\?`, `\\c` and
`\\q` commands.

### Tests for User Story 2

- [x] T012 [P] [US2] Test quoted semicolons and dollar-quoted bodies in `src/cli/plain.rs`
- [x] T013 [P] [US2] Test command recognition only at statement start and unknown-command guidance in `src/cli/plain.rs`
- [x] T014 [US2] Test the multi-line transcript and continuation prompt in `tests/cli_contract.rs`

### Implementation for User Story 2

- [x] T015 [US2] Implement the buffered input reader and local command parser in `src/cli/plain.rs`
- [x] T016 [US2] Implement help, connection information, quit and end-of-input behaviour in `src/cli/plain.rs`
- [x] T017 [US2] Keep user-facing query advice readable in full-screen, plain and scripted contexts in `src/postgres/error.rs`

**Checkpoint**: A user can enter multi-line PostgreSQL statements without
premature execution and can discover the supported local commands.

## Phase 5: User Story 3 - Keep safety and state visible in plain text (Priority: P1)

**Goal**: Preserve production confirmation, cancellation, diagnostic layering and
transaction recovery in a presentation that carries meaning without colour.

**Independent Test**: Run the production-write and failed-transaction contract
tests against the disposable database, then inspect the transcript for the
required confirmation, state and recovery wording.

### Tests for User Story 3

- [x] T018 [P] [US3] Test production-write confirmation and cancellation in `tests/cli_contract.rs`
- [x] T019 [P] [US3] Test textual prompt and outcome state wording in `src/cli/plain.rs`
- [x] T020 [P] [US3] Test failed-transaction recovery with an attached diagnostic in `src/ui/layout.rs`
- [x] T021 [US3] Test server-backed transaction state, cancellation and recovery in `tests/postgres_integration.rs`

### Implementation for User Story 3

- [x] T022 [US3] Reuse server-confirmed cancellation and await its final outcome in `src/cli/interactive.rs`
- [x] T023 [US3] Apply production impact confirmation before sending writes in `src/cli/plain.rs`
- [x] T024 [US3] Carry environment, posture, TLS and transaction facts in plain prompts and connection summaries in `src/cli/plain.rs`
- [x] T025 [US3] Preserve a failed diagnostic while showing ROLLBACK recovery in full and compact layouts in `src/ui/layout.rs`
- [x] T026 [US3] Keep control characters and secret-bearing diagnostics safe in `src/postgres/error.rs` and `src/diagnostics/`

**Checkpoint**: Plain mode does not weaken production safety, cancellation truth,
secret handling or transaction recovery guidance.

## Phase 6: Polish and cross-cutting concerns

**Purpose**: Record evidence, keep documentation current and close platform
verification gaps without claiming more than has been proven.

- [x] T027 [P] Add the feature requirements-quality checklist in `specs/007-plain-line-mode/checklists/requirements.md`
- [x] T028 [P] Record combined automated verification and its evidence boundary in `docs/operations/verification.md`
- [x] T029 Run `cargo xtask verify` with both disposable PostgreSQL services and record 358 library, 29 CLI-contract and 33 integration tests
- [x] T030 [US3] Add a plain-mode subprocess cancellation test in `tests/cli_contract.rs`
- [x] T031 [US3] Add a plain-mode failed-transaction transcript test in `tests/cli_contract.rs`
- [ ] T032 [P] Hand-verify plain mode with a screen reader on macOS and Windows, recording the exact environment in `docs/support/compatibility.md`
- [ ] T033 [P] Open and use the full-screen and plain clients on Windows Terminal and a Linux terminal, including `TERM=dumb`, in `docs/operations/verification.md`
- [ ] T034 Implement and evidence ConPTY terminal restoration for Windows in `tests/terminal_restore.rs` and `docs/status.md`
- [x] T035 Update the authoritative current-state entry in `docs/status.md` after the plain-mode slice was committed

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No code dependency; establishes the contract.
- **Foundational (Phase 2)**: Depends on the contract and blocks story work
  that could otherwise duplicate safety or PostgreSQL behaviour.
- **User Stories (Phases 3-5)**: Depend on the shared boundaries; the current
  stories use disjoint conceptual surfaces but integrate through the same CLI.
- **Polish (Phase 6)**: Depends on the implementation and focused tests; hand
  verification remains open and does not block automated local use.

### User Story Dependencies

- **User Story 1 (P1)**: Depends on Phase 2 and provides the MVP transcript.
- **User Story 2 (P1)**: Depends on the statement boundary used by User Story 1.
- **User Story 3 (P1)**: Depends on the session and stream path from User Story 1
  and the input lifecycle from User Story 2.

### Parallel Opportunities

- T002, T003 and T004 can be reviewed in parallel because they preserve separate
  existing authorities.
- T006 and T007 can be tested in parallel because they use separate test scopes.
- T012 and T013 can be tested in parallel within the reader module.
- T018, T019 and T020 can be reviewed in parallel after the shared model exists.
- T027 and T028 can be prepared in parallel with code review, but T029 must run
  after all implementation files are stable.
- T030 and T031 are independent plain-mode contract tests but should not run
  concurrently against the same disposable database process.
- T032, T033 and T034 are independent platform evidence tasks and should not be
  assigned to the same person or terminal session as source changes.

## Parallel Example: User Story 1

```text
Agent A: T006 - subprocess stream and control-sequence contract in tests/cli_contract.rs
Agent B: T007 - reader unit coverage in src/cli/plain.rs
Owner: T008-T011 - integrate the line session and preserve the default route
```

## Implementation Strategy

### MVP First

1. Confirm the contract and shared boundaries in Phases 1 and 2.
2. Validate User Story 1 with the plain-mode subprocess test.
3. Validate User Story 2's statement and command handling.
4. Validate User Story 3's safety and transaction recovery.
5. Run `cargo xtask verify` with the database services running.

### Incremental Delivery

1. Keep the existing full-screen route as the default.
2. Deliver the plain transcript and stream split.
3. Add the safe statement reader and small command set.
4. Add the safety and truthful-state checks.
5. Close platform hand-verification and ConPTY tasks separately.

### Shared-worktree coordination

- Claude Code owns the staged implementation slice in `src/cli/`,
  `src/postgres/error.rs` and its user-facing docs. The existing CLI contract
  tests remain Claude's context except for the explicit T030/T031 reservation
  below.
- Codex owns this feature documentation, the staged disjoint `src/ui/layout.rs`
  regression fix, the separate verification-record correction and T030/T031 in
  `tests/cli_contract.rs` while this handoff stands.
- Before any further edit, inspect `git status --short`; do not stage, reset,
  overwrite or reformat the other agent's paths.

**Live handoff on 2026-08-16**: Claude Code is actively implementing Feature
004 in `src/app/`, `src/query/value.rs`, `src/ui/` and
`specs/004-result-inspection/`. Codex is not editing those paths. The CLI
contract file is currently clean and has no active Claude process, so Codex is
reserving only T030 and T031 there; Claude should not edit those test sections
while this reservation stands. T032 through T034 require platform evidence
that is not available in this macOS session. Re-check this note and
`git status --short` before claiming or taking any remaining task.

## Notes

- `[P]` tasks have disjoint files or are evidence-only and can proceed in
  parallel.
- `[USn]` maps a task to a user story for traceability.
- Checked tasks are backed by current worktree evidence; unchecked tasks remain
  open and must not be described as complete.
- No task authorises a commit, push, release or production change.
