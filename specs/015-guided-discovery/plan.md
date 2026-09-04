# Implementation Plan: Guided discovery

**Branch**: `015-guided-discovery` | **Date**: 2026-09-04 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/015-guided-discovery/spec.md`

**Note**: This template is filled in by the `/speckit-plan` command; its definition describes the execution workflow.

## Summary

Add a derived discovery layer to the existing terminal workbench. It will turn
the current focus and session state into a small, bounded set of hints, richer
empty-state guidance, and a contextual command palette. The existing keymap
remains authoritative for keys, the existing reducer remains authoritative for
effects, and no onboarding state is persisted.

## Technical Context

<!--
  ACTION REQUIRED: Replace the content in this section with the technical details
  for the project. The structure here is presented in advisory capacity to guide
  the iteration process.
-->

**Language/Version**: Rust 2024 edition, package MSRV 1.90, pinned local toolchain 1.97.1

**Primary Dependencies**: Existing `ratatui` 0.30.2, `crossterm` 0.29.0, and application keymap; no new dependency

**Storage**: In-memory derived presentation only; no persisted onboarding state

**Testing**: `cargo test`, renderer buffer assertions, reducer effect assertions, keymap and documentation contracts, and the repository verifier

**Target Platform**: Terminal UI on macOS/Warp, Windows Terminal, and Linux terminals; ASCII, no-colour, narrow, and plain modes remain supported

**Project Type**: Local-first Rust terminal application and scriptable CLI

**Performance Goals**: Derive and render discovery copy during the existing frame with no database, filesystem, subprocess, or network work; palette search remains bounded by the existing visible list

**Constraints**: At most five contextual hints; active configured keys only; truthful prerequisites; no automatic SQL, metadata reload, file, history, clipboard, telemetry, or new persisted state effects

**Scale/Scope**: Existing Editor, Results, Objects, empty/blocked states, general command palette, and related documentation; no new database capability or machine-output change

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Gate | Result | Evidence |
| --- | --- | --- |
| I. Delight remains truthful | PASS | Guidance is derived from current state and names prerequisites instead of inventing availability. |
| III. Local-first and private | PASS | No network, telemetry, persistence, or credential path is added. |
| IV. Safe by default | PASS | Discovery does not execute or replay SQL; unavailable actions are omitted or explained. |
| V. Keyboard-first, never keyboard-only | PASS | Hints and palette use the active keymap and remain usable without a mouse. |
| VI. Meaning survives styling loss | PASS | Copy is tested in ASCII, no-colour, narrow, reduced-motion, and plain-adjacent paths. |
| VIII. One source of truth | PASS | `Keymap` supplies advertised keys and existing actions/effects remain authoritative. |
| IX. Evidence over confidence | PASS | Focused reducer/renderer/keymap tests and the full verifier are planned, with manual platform gaps reported separately. |
| X. Recoverability | PASS | Empty and blocked states provide bounded next actions without changing user data. |

No constitution violation requires complexity tracking.

## Project Structure

### Documentation (this feature)

```text
specs/[###-feature]/
├── plan.md              # This file (/speckit-plan command output)
├── research.md          # Phase 0 output (/speckit-plan command)
├── data-model.md        # Phase 1 output (/speckit-plan command)
├── quickstart.md        # Phase 1 output (/speckit-plan command)
├── contracts/           # Phase 1 output (/speckit-plan command)
└── tasks.md             # Phase 2 output (/speckit-tasks command - NOT created by /speckit-plan)
```

### Source Code (repository root)
<!--
  ACTION REQUIRED: Replace the placeholder tree below with the concrete layout
  for this feature. Delete unused options and expand the chosen structure with
  real paths (e.g., apps/admin, packages/something). The delivered plan must
  not include Option labels.
-->

```text
src/
├── app/
│   ├── discovery.rs       # derived focus/state guidance and palette context
│   ├── model.rs
│   ├── palette.rs
│   └── update.rs
├── cli/
├── query/
├── postgres/
└── ui/
    ├── keymap.rs
    └── layout.rs

tests/
├── cli_contract.rs
├── documentation_matches_the_build.rs
└── postgres_integration.rs
```

**Structure Decision**: Extend the existing unidirectional application model
with one pure `app::discovery` module. `ui::layout` renders its derived copy,
`ui::keymap` resolves actual keys, and `app::update` continues to own all
effects. Existing palette and result behaviours are refined in place so the
feature does not create a second command or state system.

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified**

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| [e.g., 4th project] | [current need] | [why 3 projects insufficient] |
| [e.g., Repository pattern] | [specific problem] | [why direct DB access insufficient] |
