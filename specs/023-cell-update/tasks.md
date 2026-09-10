---
description: "Implementation tasks for reviewable cell-to-UPDATE"
---

# Tasks: Reviewable cell-to-UPDATE

**Input**: Design documents from `specs/023-cell-update/`.

**Prerequisites**: `spec.md`, `plan.md`, `research.md`, `data-model.md`,
`contracts/update.md`, and `quickstart.md`.

## Phase 1: Setup

- [x] T001 Confirm Feature 023 is the active feature in `.specify/feature.json`
  and that all artifacts agree on single-table source analysis, primary-key
  identity, review confirmation and parameterized execution.

## Phase 2: Pure source and plan foundation

- [x] T002 [P] Add `src/query/update.rs` and export its value-free source,
  refusal, column and plan types.
- [x] T003 [P] Add lexical source analysis for direct projections, aliases,
  star expansion, relation qualification and conservative refusal cases.
- [x] T004 [P] Add bounded parameterized update-template generation with quoted
  identifiers, key predicates, NUL refusal, size refusal and redacted Debug.
- [x] T005 [P] Add unit tests for accepted projections, joins, expressions,
  duplicates, missing keys, NULL keys, key-column edits, escaping and bounds.

**Checkpoint**: Pure analysis and planning can produce either one safe plan or a
truthful refusal without a terminal or database.

## Phase 3: Live relation discovery

- [x] T006 [P] Add PostgreSQL metadata types and a bound relation resolver that
  reports schema, relation kind, read privilege, update privilege and primary
  key membership.
- [x] T007 [P] Add the session adapter and runtime lookup effect with connection
  generation and request identity handling.
- [x] T008 [P] Add disposable PostgreSQL checks for search-path resolution,
  primary-key discovery, update privilege and relation-kind refusal.

**Checkpoint**: The live adapter returns only safe relation metadata and stale
lookup responses can be discarded.

## Phase 4: Interactive workflow

- [x] T009 Add the cell-update action, chord and palette/discovery entries while
  preserving printable editor input.
- [x] T010 Add result SQL retention, update lookup, value prompt and pending
  review state with redacted debug representations.
- [x] T011 Add reducer transitions for source capture, async metadata, value
  input, review confirmation, cancellation and stale identity revalidation.
- [x] T012 Add immediate production/read-only refusal and no-automatic-rerun
  completion wording.
- [x] T013 Add layout surfaces for loading, value entry and exact bound review
  SQL, including narrow, ASCII, no-colour and value-redaction checks.
- [x] T014 Add reducer, keymap, discovery and renderer tests for the complete
  workflow and every refusal boundary.

**Checkpoint**: A valid result cell reaches a review modal and only its explicit
confirmation emits one parameterized execution effect.

## Phase 5: Documentation and evidence

- [x] T015 [P] Update compatibility, security/data handling, threat model,
  journeys, experience roadmap, product roadmap, keymap, operations and
  `CHANGELOG.md` with the supported boundaries.
- [x] T016 [P] Add documentation parity assertions and Feature 023 quickstart
  coverage.
- [x] T017 Run focused formatting, query, reducer, keymap, renderer, docs and
  live PostgreSQL checks; distinguish every skip.
- [x] T018 Run `cargo --locked xtask verify`, then `cargo xtask db down` and
  `cargo xtask db status`.
- [x] T019 Mark the feature implemented only after evidence is recorded in
  `docs/status.md`, update all task checkboxes and verify normal-hyphen prose.

## Dependencies and execution order

- T001 precedes all implementation.
- T002-T005 precede live planning and reducer work.
- T006-T008 precede the async reducer completion path.
- T009-T014 form the user-visible vertical slice.
- T015-T019 follow stable behavior and evidence.

## Implementation strategy

Keep the pure refusal boundary first, then connect live metadata, then add the
prompt and review state. At every stage preserve ordinary execution, source-row
identity, existing parameter binding and no-automatic-rerun behavior.
