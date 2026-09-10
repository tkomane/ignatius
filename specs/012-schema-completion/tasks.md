---

description: "Implementation tasks for schema-aware completion"
---

# Tasks: Schema-aware completion

**Input**: [spec.md](./spec.md), [plan.md](./plan.md), [research.md](./research.md),
[data-model.md](./data-model.md), and [contracts/completion.md](./contracts/completion.md)

**Tests**: included because the feature specification makes behavior, security,
accessibility, and live PostgreSQL evidence acceptance criteria.

## Phase 1: Foundation

- [x] T001 [P] Add a pure identifier-quoting module under `src/query/` and keep
  `crate::postgres::metadata::quote_identifier` as its compatibility re-export.
- [x] T002 [P] Expose the existing highlight lexer word/keyword helpers for
  reuse, without creating a second string/comment lexer.
- [x] T003 Add pure catalogue, scope, context, candidate, ranking, and bounded
  result types in `src/query/completion.rs`.
- [x] T004 Add focused analyzer tests for prefix ordering, identifier quoting,
  literals/comments, statement boundaries, schema scope, and UTF-8 boundaries.

**Checkpoint**: the pure completion engine can be tested without a terminal or
database and has no dependency on UI state.

## Phase 2: User Story 1 - Discover schema-aware candidates (Priority: P1)

**Goal**: offer useful, truthful keywords, objects, columns, aliases, schemas,
and CTE names from one loaded catalogue snapshot.

**Independent Test**: pure scope tests pass, and a disposable PostgreSQL session
returns a snapshot containing fixture relations, columns, functions, and hostile
names without interpolated identifiers.

### Tests for User Story 1

- [x] T005 [P] [US1] Add analyzer tests for `FROM`/`JOIN` object scope, ordinary
  column scope, alias-qualified columns, schema-qualified objects, CTEs,
  subquery aliases, and incomplete earlier syntax in `src/query/completion.rs`.
- [x] T006 [P] [US1] Add PostgreSQL integration coverage for the fixed catalogue
  snapshot and permission/detail fields in `tests/postgres_integration.rs`.

### Implementation for User Story 1

- [x] T007 [US1] Implement conservative token/context extraction and candidate
  ranking in `src/query/completion.rs`, including bounded true counts.
- [x] T008 [US1] Implement the fixed, bound-parameter catalogue snapshot query
  in `src/postgres/metadata.rs` and map rows into the pure query types.
- [x] T009 [US1] Add `Session::completion_catalog` in `src/postgres/session.rs`
  and load it through the existing metadata connection in
  `src/cli/interactive.rs`.
- [x] T010 [US1] Add completion loading state, request identity, message, and
  effect handling in `src/app/model.rs`, `src/app/message.rs`, and
  `src/app/update.rs`, rejecting stale responses.

**Checkpoint**: the model has a truthful ready/loading/unavailable catalogue
state and candidate computation is local after the initial read.

## Phase 3: User Story 2 - Accept or dismiss safely (Priority: P1)

**Goal**: make completion feel like a first-class editor operation while
preserving exact buffer safety, quoting, and one-step undo.

**Independent Test**: open a menu in a seeded model, navigate, accept a hostile
identifier, undo once, and dismiss another menu; assert exact buffer/cursor and
no effects for all local menu operations.

### Tests for User Story 2

- [x] T011 [P] [US2] Add editor range-replacement tests proving UTF-8-safe range
  replacement, cursor placement, redo invalidation, and one undo snapshot in
  `src/app/editor.rs`.
- [x] T012 [P] [US2] Add reducer tests for explicit/automatic invocation,
  printable filtering, Up/Down selection, Enter acceptance, Esc dismissal, and
  no implicit insertion in `src/app/update.rs`.
- [x] T013 [P] [US2] Add the automatic-off property test over representative SQL
  prefixes, asserting the same buffer as completion-on until acceptance.

### Implementation for User Story 2

- [x] T014 [US2] Add an editor range replacement operation and completion menu
  state that preserves the replacement range through selection.
- [x] T015 [US2] Add `Action::Complete`, keymap configuration/default `Ctrl+Space`,
  exhaustive labels, help entries, and `docs/design/keymap.md` updates.
- [x] T016 [US2] Implement reducer menu precedence and acceptance/dismissal
  behavior in `src/app/update.rs`, reusing the editor and query completion
  operations and closing the menu before unrelated actions.
- [x] T017 [US2] Render a cursor-anchored bounded menu in full and compact TUI
  layouts in `src/ui/layout.rs`, with visible selection, kind/schema/type
  details, loading/stale/unavailable wording, and ASCII/no-colour semantics.

**Checkpoint**: TUI completion is usable with a keyboard, reversible, and
truthful without requiring colour, Unicode, or mouse input.

## Phase 4: User Story 3 - Work through catalogue uncertainty (Priority: P1)

**Goal**: keep completion useful and honest while metadata is loading, stale, or
  unreadable.

**Independent Test**: reducer and renderer tests exercise each catalogue state;
keywords remain available, no false empty object list is shown, and reload is
explicit rather than per-keystroke.

- [x] T018 [P] [US3] Add model/reducer tests for loading, successful refresh,
  unavailable catalogue, stale timestamp text, and stale response rejection.
- [x] T019 [P] [US3] Add renderer tests for bounded counts, ASCII kind/source
  words, and failure/loading messages in `src/ui/layout.rs`.
- [x] T020 [US3] Add `ui.completion` default/serde/starter-file coverage in
  `src/config/schema.rs` and `src/config/store.rs`, and wire it into the
  interactive model in `src/cli/interactive.rs`.
- [x] T021 [US3] Ensure reload and reconnect reset catalogue state without
  clearing the SQL buffer, and keep metadata failure separate from connection
  failure in `src/app/update.rs`.

**Checkpoint**: all uncertainty states are user-visible and no unavailable
  check is reported as a pass.

## Phase 5: User Story 4 - Use completion in plain mode (Priority: P2)

**Goal**: offer the same completion semantics to screen readers, pipes, and
  `TERM=dumb` users as explicit line-oriented commands.

**Independent Test**: a plain-mode transcript requests completion, prints an
  ASCII numbered list, accepts one candidate by number or exact name, and never
  runs a statement or emits control sequences while doing so.

- [x] T022 [P] [US4] Add `Complete` and `Use` command parsing plus reader buffer
  access/range application tests in `src/cli/plain.rs`.
- [x] T023 [P] [US4] Add subprocess coverage in `tests/cli_contract.rs` for the
  plain completion transcript, invalid choice safety, and hostile-name display.
- [x] T024 [US4] Implement plain-mode snapshot loading and explicit
  `\complete [prefix]` / `\use <number|exact-name>` handling in `src/cli/plain.rs`
  using the shared analyzer and quoting operation.
- [x] T025 [US4] Update plain help text and accessibility wording to explain the
  explicit completion flow without weakening existing meta-command behavior.

**Checkpoint**: TUI and plain mode consume one completion engine and preserve
  their separate interaction contracts.

## Phase 6: Polish, documentation, and release evidence

- [x] T026 [P] Update `docs/product/experience-roadmap.md`, `docs/product/roadmap.md`,
  and `docs/status.md` with the implemented gate and remaining platform/live
  evidence.
- [x] T027 [P] Update `docs/operations/local-development.md`, README usage,
  and any generated/configuration documentation for `ui.completion`.
- [x] T028 [P] Add release notes/catalogue entry for Feature 012 if the current
  release-history convention requires it.
- [x] T029 Run focused unit and CLI checks, then disposable PostgreSQL integration
  checks; record results and explicit skips.
- [x] T030 Run `cargo --locked xtask verify`, review the complete failure summary,
  and correct or document every warning, failure, skip, and remaining gate.
- [x] T031 Re-run `git diff --check`, inspect the final diff/status, and leave
  changes unpushed on the feature branch for owner review.

## Dependencies and execution order

- T001-T004 are foundational; T007-T010 depend on the pure types and lexer
  seams, and T014-T017 depend on the query/reducer state.
- US1 must land before T014 because the menu consumes the catalogue and context.
- US2 and US3 can be developed in parallel after T010, but T017 depends on the
  final menu state shape.
- US4 depends on T007 and T014 but is independent of ratatui rendering.
- T026-T031 depend on all desired behavior being implemented.

## Parallel opportunities

- T001, T002, and initial T004 test scaffolding touch different files.
- T005 and T006 are independent evidence paths.
- T011, T012, and T013 are independent focused test suites.
- T018, T019, and T020 are independent state/config/rendering checks.
- T022, T023, and T026-T028 can be prepared in parallel once interfaces settle.

## Implementation strategy

Deliver US1 and US2 as the MVP: a real catalogue-backed TUI completion menu
with reversible acceptance. Validate it with pure tests and a live disposable
PostgreSQL server before adding plain-mode polish. Complete US3 and US4, then
run the full locked verification and update status without implying publication
or production readiness.
