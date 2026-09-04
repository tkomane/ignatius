---

description: "Implementation tasks for prompted named parameters"
---

# Tasks: Prompted Named Parameters

**Input**: Design documents from `specs/022-parameters-prompted/`

**Prerequisites**: `spec.md`, `plan.md`, `research.md`, `data-model.md`, `contracts/parameters.md`, and `quickstart.md`.

**Tests**: The specification explicitly requires independent scanner, prompt, CLI, privacy, and live database evidence. Test tasks are therefore listed before their implementation tasks within each story.

## Phase 1: Setup

**Purpose**: Confirm the selected feature slice and preserve the repository's existing delivery boundaries.

- [x] T001 [P] Confirm Feature 022 is the active feature in `.specify/feature.json` and that all design artifacts under `specs/022-parameters-prompted/` agree on the simple-query, secret-value, and 64-name boundaries.

## Phase 2: Foundational

**Purpose**: Establish the shared parameter model and safe execution message boundary required by every user story.

- [x] T002 [P] Add the `parameters` query module declaration in `src/query/mod.rs` and define the public crate-internal API shape described by `specs/022-parameters-prompted/contracts/parameters.md`.
- [x] T003 [P] Add the parameterized execution effect and model state seams in `src/app/message.rs` and `src/app/model.rs`, retaining the existing ordinary execution effect for compatibility.
- [x] T004 [P] Add redacted diagnostic and debug-boundary coverage for parameter metadata in `src/diagnostics/logging.rs` and the relevant model/message tests.

**Checkpoint**: The shared model and message boundaries exist without exposing parameter values; user story work can begin.

## Phase 3: User Story 1 - Reusable TUI parameter template (Priority: P1)

**Goal**: Let the full-screen client discover named placeholders, prompt once per distinct name, and execute the bound statement while preserving the original template.

**Independent Test**: In the TUI, run a statement containing repeated named placeholders and verify one prompt per distinct name, correct binding, cancel behavior, safe history, and an honest error location.

### Tests for User Story 1

- [x] T005 [P] [US1] Add scanner and binder tests for quoted text, quoted identifiers, dollar-quoted bodies, line and nested block comments, casts, repeated names, first-use ordering, escaping, empty values, NUL rejection, and the 64-name limit in `src/query/parameters.rs`.
- [x] T006 [P] [US1] Add reducer tests for prompt opening, one-field-at-a-time acceptance, empty-value acceptance, cancellation, repeated-name reuse, and parameterized execution effects in `src/app/update.rs`.
- [x] T007 [P] [US1] Add layout tests for prompt title, active parameter, progress, masked input, empty values, and redacted rendering in `src/ui/layout.rs`.

### Implementation for User Story 1

- [x] T008 [US1] Implement placeholder discovery, deterministic binding, PostgreSQL-safe escaped text literals, and redacted `Debug` behavior in `src/query/parameters.rs`.
- [x] T009 [US1] Implement `ParameterPrompt`, secret typed and accepted values, and the parameterized execution message in `src/app/model.rs` and `src/app/message.rs`.
- [x] T010 [US1] Integrate parameter discovery, modal precedence, prompt navigation, cancel behavior, execution start, and truthful parameterized error-location handling in `src/app/update.rs`.
- [x] T011 [US1] Render the parameter prompt as a first-class full-screen modal with masked values, clear progress, and accessible action hints in `src/ui/layout.rs`.

**Checkpoint**: US1 is independently usable in the TUI and no parameter value is present in query history, diagnostics, or debug output.

## Phase 4: User Story 2 - Plain and automated parameter input (Priority: P1)

**Goal**: Support hidden prompts in plain interactive mode and deterministic environment-variable mapping in non-interactive CLI mode without placing secrets in command arguments.

**Independent Test**: Run the plain client with a terminal and verify hidden prompts, then run the query command with `--param-env NAME=VARIABLE` and verify valid, missing, extra, duplicate, unset, and malformed mappings fail safely before connection.

### Tests for User Story 2

- [x] T012 [P] [US2] Add hidden-input tests covering empty values, cancellation, EOF, backspace, and raw-mode restoration in `src/cli/prompt.rs`.
- [x] T013 [P] [US2] Add CLI contract tests for repeatable `--param-env`, exact name-to-variable mapping, missing or extra mappings, duplicate mappings, malformed mappings, unset variables, and refusal to prompt in non-terminal mode in `tests/cli_contract.rs`.
- [x] T014 [P] [US2] Add plain-mode tests for safe template history, hidden prompt text, cancellation, and parameterized error reporting in `src/cli/plain.rs`.

### Implementation for User Story 2

- [x] T015 [US2] Add reusable hidden input handling with safe terminal cleanup while preserving existing password semantics in `src/cli/prompt.rs`.
- [x] T016 [US2] Add `--param-env` parsing, preconnection environment resolution, validation diagnostics, and query request plumbing in `src/cli/mod.rs`.
- [x] T017 [US2] Add plain interactive parameter discovery, hidden prompting, cancellation, and template-preserving history/error output in `src/cli/plain.rs`.
- [x] T018 [US2] Wire parameter bindings through interactive execution and export entry points while keeping stdout data-only in `src/cli/interactive.rs`.

**Checkpoint**: US2 works both with a human at a terminal and in automation using environment-injected secret values; raw values never appear in CLI arguments or stdout diagnostics.

## Phase 5: User Story 3 - Truthful and recoverable execution (Priority: P1)

**Goal**: Bind values only at the database boundary, preserve cancellation and streaming behavior, and keep all release evidence explicit about what was and was not verified.

**Independent Test**: Against the disposable PostgreSQL fixture, execute and stream a parameterized statement, cancel a long-running request, inspect safe history and tracing descriptors, and verify parameterized server positions are not mapped to false template carets.

### Tests for User Story 3

- [x] T019 [P] [US3] Add execution and streaming tests for parameterized statements, repeated values, empty values, escaped values, binding failures, and cancellation behavior in `src/postgres/session.rs` and `tests/postgres_integration.rs`.
- [x] T020 [P] [US3] Add privacy tests proving template text and parameter names may be retained while values are absent from history, diagnostics, tracing fields, model and effect debug output, palette text, result metadata, and terminal decoration in `src/app/update.rs`, `src/cli/plain.rs`, `src/diagnostics/logging.rs`, and `src/ui/layout.rs`.
- [x] T021 [P] [US3] Add error-location tests proving server positions for expanded parameterized requests produce an honest note without a misleading template caret in `src/query/error_location.rs` and the client error renderers.

### Implementation for User Story 3

- [x] T022 [US3] Implement session-level parameter binding and parameterized execute/stream methods at the simple-query boundary in `src/postgres/session.rs`, preserving existing transaction and rendering behavior.
- [x] T023 [US3] Implement parameterized cancellable execution and export streaming in `src/cli/interactive.rs`, retaining the existing ordinary-query wrappers and cancellation cleanup.
- [x] T024 [US3] Complete parameterized error-location, history, tracing, diagnostic, and failure-recovery behavior in `src/app/update.rs`, `src/cli/plain.rs`, and `src/diagnostics/logging.rs`.

**Checkpoint**: US3 has live disposable-PostgreSQL evidence for execute and stream paths, with cancellation and skipped checks reported separately from passes.

## Phase 6: Polish and release maturity

**Purpose**: Make the feature discoverable, auditable, and consistent with the product's GUI-like CLI UX and release evidence rules.

- [x] T025 [P] Update user-facing behavior, security, compatibility, operations, journeys, experience roadmap, product roadmap, and threat-model documentation in `docs/support/compatibility.md`, `docs/security/data-handling.md`, `docs/security/threat-model.md`, `docs/operations/local-development.md`, `docs/product/journeys.md`, `docs/product/experience-roadmap.md`, `docs/product/roadmap.md`, and `CHANGELOG.md`.
- [x] T026 [P] Add or update documentation and schema assertions required by the repository test suite in `tests/documentation_matches_the_build.rs`, `tests/release_schema_contract.rs`, and related focused test files.
- [x] T027 Validate the feature walkthrough and live commands in `specs/022-parameters-prompted/quickstart.md`, including a disposable PostgreSQL execute and stream probe with synthetic values only.
- [x] T028 Run focused formatting, unit, CLI contract, editor, keymap, documentation, and PostgreSQL checks for Feature 022; record each pass, skip, and failure separately.
- [x] T029 Run the authoritative `cargo --locked xtask verify`, then explicitly run `cargo xtask db down` and `cargo xtask db status`; retain the exact verifier and live evidence needed for release readiness.
- [x] T030 Update `specs/022-parameters-prompted/spec.md`, `specs/022-parameters-prompted/tasks.md`, and `docs/status.md` with implementation state and separate local, live, skipped, and pending terminal evidence; verify the diff and normal-hyphen prose rule.

## Dependencies and execution order

### Phase dependencies

- Phase 1 has no dependency.
- Phase 2 depends on the selected Feature 022 artifacts and blocks all user stories.
- User stories depend on Phase 2 and are implemented in priority order for this single working tree.
- Phase 6 depends on all three user stories and their focused checks.

### Within each user story

- Tests are written before implementation and must initially fail for the missing behavior.
- Core data and prompt state precede reducer wiring; reducer wiring precedes rendering and runtime integration.
- Database and automation evidence follows the local implementation and remains distinct from compile or unit evidence.

### Parallel opportunities

- T001-T004 can be reviewed or prepared independently where their files do not overlap.
- T005-T007, T012-T014, and T019-T021 are independent test groups.
- T025-T026 can proceed independently of one another after the behavior is stable.

## Implementation strategy

1. Complete the shared model and message seams.
2. Deliver and validate the TUI prompt as the MVP.
3. Add plain and automated input without changing ordinary query behavior.
4. Add database-bound execution, streaming, cancellation, and truthful diagnostics.
5. Complete documentation and run focused checks followed by the locked verifier and disposable live evidence.
