# Tasks: Connection picker

**Input**: Design documents from `/specs/020-connection-picker/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md,
contracts/connection-picker.md, quickstart.md

**Tests**: Included because the specification requires safe summaries,
deterministic picker interaction, resolver reuse, stale-connection protection,
accessible rendering and no-regression evidence.

## Phase 1: Setup

- [x] T001 Register `app::connection_picker` and update `.specify/feature.json` to the Feature 020 directory.
- [x] T002 Add the Feature 020 documentation contract target without changing existing output contracts.

## Phase 2: Safe picker foundation

- [x] T003 [P] Add `ConnectionProfileSummary` and deterministic safe summary construction from known profile fields only.
- [x] T004 [P] Add `Connections` palette purpose, default/profile choice commands and safe picker row construction.
- [x] T005 [P] Add `OpenConnectionPicker` and `ConnectProfile` application/keymap boundaries with redacted debug coverage.
- [x] T006 Add model storage for safe profile summaries and reset helpers for connection-bound tree and completion state.
- [x] T007 [P] Add summary tests for missing fields, environments, TLS, read-only, provider labels, control text and password-shaped unknown fields.

**Checkpoint**: Picker state and rows are pure, searchable, value-free and
renderable before any connection runtime work begins.

## Phase 3: User Story 1 - Choose before connecting

- [x] T008 Add explicit-route detection so only profile-aware implicit startup opens the picker.
- [x] T009 Change interactive startup to render the picker before target resolution, provider authentication or connection effects.
- [x] T010 Add reducer tests proving startup picker visibility, default choice and profile choice produce no effects before selection.
- [x] T011 Add runtime seam tests proving profile selection reuses the existing resolver and reports safe preparation failures without fallback.

## Phase 4: User Story 2 - Read the choice

- [x] T012 [P] Add picker-purpose title, empty state, standing note and compact-safe wording.
- [x] T013 [P] Add connection-picker layout tests for full, compact, ASCII, no-colour and reduced-motion presentations.
- [x] T014 Add documentation contract assertions for the shortcut, chord, profile safety wording and default route.

## Phase 5: User Story 3 - Switch deliberately

- [x] T015 Add reducer selection transition that clears server-derived state while retaining editor text and local preferences.
- [x] T016 Add `Effect::ConnectProfile` runtime preparation and current-target storage without placing a target in `Model`.
- [x] T017 Add connection-generation guards to connection, metadata, schema and completion task messages.
- [x] T018 Add reducer/runtime tests for profile auth posture, no password fallback for cloud routes, reset state and stale response rejection.
- [x] T019 Add `Ctrl+K n`, command-palette and contextual-discovery coverage while preserving printable editor input.

**Checkpoint**: A named connection can be chosen before startup or switched
after a quiet session, with one safe route and no stale state leakage.

## Phase 6: Documentation and verification

- [x] T020 Update keymap, interaction, compatibility, security, local-development, journey, roadmap and experience-roadmap docs.
- [x] T021 Update `CHANGELOG.md` with the picker boundaries and no-secret claim.
- [x] T022 Add CLI contract coverage for explicit profile/default routes and unchanged plain/machine-output behaviour.
- [x] T023 Run focused checks, inspect redaction and changed-file hyphen rules, and record passes/skips/failures.
- [x] T024 Start disposable PostgreSQL, run `cargo --locked xtask verify`, record versions and every skipped gate, tear down and verify status.
- [x] T025 Review `git diff --check`, task/spec consistency and dirty-worktree scope; tick only evidenced tasks.

## Dependencies

- Phase 2 blocks all user stories.
- Startup picker work depends on safe entries and palette commands.
- Runtime switching depends on the reducer transition and generation plan.
- Documentation and full verification depend on all desired user stories.

## Notes

- `[P]` marks work that can proceed on separate files or boundaries.
- Existing Feature 013 through Feature 019 working-tree changes are preserved.
- No task authorizes commit, push, tag, signing, release or publication.
