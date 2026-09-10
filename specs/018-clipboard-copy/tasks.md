# Tasks: Copy a result value out

**Input**: Design documents from `/specs/018-clipboard-copy/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md,
contracts/clipboard.md, quickstart.md

**Tests**: Included because the specification requires measurable transport,
reducer, renderer, configuration, and no-regression evidence.

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Establish the small integration surfaces without changing existing
query or terminal behavior.

- [x] T001 [P] Register the new clipboard protocol module in `src/lib.rs` and keep its public surface limited to the transport boundary.
- [x] T002 [P] Add the Feature018 configuration and contract test targets to `tests/documentation_matches_the_build.rs` without changing existing output contracts.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Build the bounded payload, preference, messages, and model state
needed by every user story.

**CRITICAL**: No user story work can begin until this phase is complete.

- [x] T003 [P] Add `ClipboardConfig` with an omitted-value-safe `osc52 = false` default to `src/config/schema.rs`, re-export it from `src/config/mod.rs`, and document it in the starter template in `src/config/store.rs`.
- [x] T004 [P] Implement the redacted single-use `ClipboardPayload`, 1 MiB bound, base64 encoder, OSC 52 framing, and write/flush helper in `src/clipboard.rs`.
- [x] T005 [P] Add `CopyValue`, clipboard completion/failure messages, and the `CopyValue` effect to `src/app/message.rs` with no raw value in Debug-visible metadata.
- [x] T006 Add clipboard preference, pending candidate identity, and value-free notice state to `src/app/model.rs`, preserving the retained result as the only raw-value store.
- [x] T007 [P] Add unit tests for protocol framing, exact byte round-trip, control-byte containment, size boundaries, and redacted Debug in `src/clipboard.rs`.
- [x] T008 [P] Add configuration load, default, serialization, unknown-field, and starter-template assertions for `[clipboard]` in `src/config/schema.rs`, `src/config/store.rs`, and `tests/documentation_matches_the_build.rs`.

**Checkpoint**: The bounded transport and backward-compatible state contract
compile and have isolated tests before reducer or UI work begins.

---

## Phase 3: User Story 1 - Copy the value I am looking at (Priority: P1) 🎯 MVP

**Goal**: Let an operator confirm one retained text cell and send its exact
bytes once, with no value shown in the UI or stored in candidate state.

**Independent Test**: Select ordinary, empty, and Unicode text cells, invoke the
copy action, confirm one and cancel one, and assert one exact `CopyValue` effect
for the confirmed cell and none for the cancelled cell.

### Tests for User Story 1

- [x] T009 [P] [US1] Add reducer tests in `src/app/update.rs` for candidate counts, Enter confirmation, Escape/Cancel dismissal, one-effect semantics, empty text, and retained result selection.
- [x] T010 [P] [US1] Add renderer buffer tests in `src/ui/layout.rs` proving the confirmation identifies row/column and counts while excluding the raw cell value.
- [x] T011 [P] [US1] Add an interactive effect-dispatch test seam in `src/cli/interactive.rs` or an adjacent unit-testable helper proving a confirmed payload is written once and flushed.

### Implementation for User Story 1

- [x] T012 [US1] Implement current displayed-cell resolution, candidate creation, fresh source revalidation, confirmation Enter/Escape handling, and stale rejection in `src/app/update.rs`.
- [x] T013 [US1] Wire the valid confirmation to `Effect::CopyValue` and handle `Message::ClipboardSent`/failure transitions in `src/app/update.rs` without mutating SQL, history, exports, or database state.
- [x] T014 [US1] Write the OSC 52 payload to the held interactive terminal and send value-free completion messages in `src/cli/interactive.rs`.
- [x] T015 [US1] Render the topmost copy confirmation and sent/unconfirmed outcome in `src/ui/layout.rs`, including terminal-path exposure and no clipboard read-back wording.

**Checkpoint**: User Story 1 is independently usable for a retained text cell,
including cancellation and exact one-write confirmation.

---

## Phase 4: User Story 2 - Decide whether data may leave through this terminal (Priority: P1)

**Goal**: Make the data-transfer boundary opt-in and truthful for disabled,
NULL, oversized, unsupported, and I/O-failure paths.

**Independent Test**: Run with omitted/false and true settings, exercise NULL,
empty, and over-limit cells, and assert every refusal has no payload while a
successful write says acceptance is unconfirmed.

### Tests for User Story 2

- [x] T016 [P] [US2] Add reducer tests in `src/app/update.rs` for disabled configuration, SQL NULL, 1 MiB boundary/overflow, visible plan, busy query, missing rows, and stale candidate no-payload outcomes.
- [x] T017 [P] [US2] Add renderer tests in `src/ui/layout.rs` for disabled, NULL, oversized, stale, sent-but-unconfirmed, and write-failed wording in compact, narrow, ASCII, and no-colour layouts.
- [x] T018 [P] [US2] Add safe diagnostic assertions in `src/cli/interactive.rs` or `src/diagnostics/diagnostic.rs` proving write errors contain no raw result value and point to terminal output or explicit export.

### Implementation for User Story 2

- [x] T019 [US2] Implement opt-in, NULL, empty, size-bound, plan/busy, and stale guards plus typed value-free notices in `src/app/update.rs` and `src/app/model.rs`.
- [x] T020 [US2] Implement safe write/flush failure conversion and no-automatic-retry behavior in `src/cli/interactive.rs`, reusing the existing redaction and diagnostic conventions.
- [x] T021 [US2] Ensure configuration show/init/validate paths expose the opt-in setting without adding secrets or changing schema compatibility in `src/cli/mod.rs`, `src/config/schema.rs`, and `src/config/store.rs`.

**Checkpoint**: User Story 2 has an auditable no-payload path for every refusal
and never claims clipboard acceptance from a one-way terminal write.

---

## Phase 5: User Story 3 - Copy remains a local, discoverable result action (Priority: P2)

**Goal**: Make the action easy to find from Results and inspector contexts while
preserving printable SQL editing and all non-interactive routes.

**Independent Test**: Find and invoke the action from its chord and palette in
the grid and inspector, including filtered/sorted rows, then verify ordinary
editor, result, history, export, plain, JSON, and NDJSON behavior is unchanged.

### Tests for User Story 3

- [x] T022 [P] [US3] Add keymap contract tests in `src/ui/keymap.rs` for `Ctrl+K c`, the Copy label, printable `c` editor preservation, and visible chord/help text.
- [x] T023 [P] [US3] Add palette and contextual-discovery tests in `src/app/discovery.rs` and `src/app/palette.rs` for Results/inspector availability and truthful prerequisites.
- [x] T024 [P] [US3] Add filtered/sorted source-row and inspector-context reducer tests in `src/app/update.rs` and `src/app/inspect.rs`.
- [x] T025 [P] [US3] Add no-OSC52 regression assertions for plain, JSON, NDJSON, history, and export routes in `tests/cli_contract.rs` and `tests/documentation_matches_the_build.rs`.

### Implementation for User Story 3

- [x] T026 [US3] Add the `Ctrl+K c` chord, short label, and help/keymap entries in `src/ui/keymap.rs` without making printable `c` configurable as CopyValue.
- [x] T027 [US3] Add Copy selected value to palette groups, action availability, prerequisites, and Results contextual hints in `src/app/discovery.rs` and `src/app/palette.rs`.
- [x] T028 [US3] Preserve inspector context and pending-copy precedence over inspector, palette, completion, and editor actions in `src/app/update.rs` and `src/app/inspect.rs`.
- [x] T029 [US3] Verify copy remains interactive-only and does not enter machine output, history, export, SQL editing, or ordinary result rendering in `src/cli/output.rs`, `src/cli/plain.rs`, and `src/app/update.rs`.

**Checkpoint**: All three user stories are independently discoverable and
non-interfering, with filtered/source identity preserved.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Complete evidence, documentation, and release-facing wording.

- [x] T030 [P] Update `docs/security/data-handling.md` with the opt-in OSC 52 exposure, write-only/no-clear boundary, and terminal/SSH/multiplexer retention warning.
- [x] T031 [P] Update `docs/support/compatibility.md`, `docs/operations/local-development.md`, `docs/design/interaction-principles.md`, and `docs/design/keymap.md` with the copy contract and fallback wording.
- [x] T032 [P] Update `docs/product/roadmap.md`, `docs/product/experience-roadmap.md`, `docs/product/journeys.md`, and `CHANGELOG.md` to record Feature018 without claiming terminal acceptance.
- [x] T033 [P] Complete `tests/documentation_matches_the_build.rs` contracts for the config starter, keymap, compatibility, data handling, and release wording.
- [x] T034 Run the focused checks from `specs/018-clipboard-copy/quickstart.md`, record passes/skips/failures, and resolve any implementation or documentation drift.
- [x] T035 Start the disposable PostgreSQL fixture, run `cargo --locked xtask verify`, record PostgreSQL version and every executed/skipped gate, then tear the fixture down and verify `cargo xtask db status` reports `Not running.`.
- [x] T036 Review `git diff --check`, the Feature018 task/spec consistency, raw-value redaction surfaces, and the final dirty-worktree scope; mark only genuinely completed tasks in `specs/018-clipboard-copy/tasks.md`.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No code dependencies; preserves the existing module and test boundaries.
- **Foundational (Phase 2)**: Depends on Setup and blocks all user stories.
- **User Story 1 (Phase 3)**: Depends on the foundational payload, config, messages, and model state; delivers the MVP.
- **User Story 2 (Phase 4)**: Depends on User Story 1's confirmation/effect path so its safety guards cover the real transition.
- **User Story 3 (Phase 5)**: Depends on the copy action existing; discovery and non-interference can then be verified independently.
- **Polish (Phase 6)**: Depends on all desired story behavior and focused tests being complete.

### User Story Dependencies

- **US1 (P1)**: Requires Phase 2 only and is the MVP.
- **US2 (P1)**: Extends US1's effect path with opt-in and failure semantics.
- **US3 (P2)**: Extends US1/US2 with keymap, palette, inspector, and compatibility discovery.

### Parallel Opportunities

- T003, T004, and T005 touch separate foundational files and can proceed in parallel; T006 follows their state types.
- T007 and T008 are independent validation work after the foundational code surfaces exist.
- US1 test tasks T009-T011 can proceed in parallel before their implementation tasks.
- US2 tests T016-T018 and US3 tests T022-T025 are independently partitioned by boundary, subject to existing-file coordination.
- Documentation tasks T030-T033 can proceed in parallel after wording is settled.

## Implementation Strategy

### MVP First

1. Complete Setup and Foundational phases.
2. Implement User Story 1 and run its focused reducer, transport, and renderer checks.
3. Stop for an independent MVP validation: disabled configuration remains safe, confirmed copy sends one exact sequence, and cancellation sends none.

### Incremental Delivery

1. Add User Story 2's refusal and failure states without changing the successful copy contract.
2. Add User Story 3's chord, palette, inspector, and non-interference guarantees.
3. Complete documentation and authoritative repository verification.

## Notes

- `[P]` means tasks use different files or independent boundaries and can be parallelized safely.
- Every task includes its principal repository path; tasks touching existing files must preserve unrelated Feature013-017 work.
- No task authorizes commit, push, tag, release, or publication.
