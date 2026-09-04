# Implementation Plan: A plan you can read

**Branch**: `016-readable-query-plan` | **Date**: 2026-09-04 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/016-readable-query-plan/spec.md`

## Summary

Add a safe, local query-plan workflow to the interactive workbench. A plain plan
request asks PostgreSQL for structured EXPLAIN output without executing the
statement; an explicit analyze request warns and confirms before execution.
The server response is parsed into a bounded plan tree, rendered in the existing
Results pane, and navigated locally with the existing keyboard and reducer
boundaries. Normal result, history, plain, machine-output, and export paths stay
unchanged.

## Technical Context

**Language/Version**: Rust 2024 edition, MSRV Rust 1.90, pinned local toolchain 1.97.1

**Primary Dependencies**: Existing `serde_json` 1.0.145 for structured plan
responses, `tokio-postgres` 0.7.18 simple-query protocol, `ratatui` 0.30.2,
`crossterm` 0.29.0; no new dependency

**Storage**: In-memory plan document and view state only; PostgreSQL is queried
through the existing session; no plan persistence

**Testing**: `cargo test`, pure JSON parser fixtures, reducer effect assertions,
renderer buffer checks, CLI/documentation contracts, and disposable PostgreSQL
14-18-compatible integration checks where available

**Target Platform**: Interactive terminal client on macOS/Warp, Windows Terminal,
and Linux; full, compact, narrow, ASCII, no-colour, reduced-motion, and plain
boundaries remain supported

**Project Type**: Local-first Rust terminal application and scriptable CLI

**Performance Goals**: Plan parsing and rendering remain local and bounded;
plans up to 500 nodes are fully navigable, with a 1 MiB structured-response cap
and a maximum displayed depth of 64; local expansion never performs a round trip

**Constraints**: Plain EXPLAIN never executes the target statement. EXPLAIN
ANALYZE always requires an explicit one-shot confirmation and states that
side-effects are possible. Existing PostgreSQL permissions, transaction state,
production protection, diagnostic redaction, terminal sanitisation, stale-job
rejection, and no-automatic-retry rules remain authoritative.

**Scale/Scope**: One statement under the cursor per request; one visible plan tree
in the Results pane; estimated and actual row/time/cost facts, local selection,
collapse/expand, bounded overflow and recovery copy. No plan persistence, diff,
visual diagram, auto-analysis, or machine-output route.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Gate | Result | Evidence |
| --- | --- | --- |
| I. Delight remains truthful | PASS | Cost units, actual timings, unavailable fields, estimate mismatch basis, and ANALYZE side effects are named explicitly. |
| II. PostgreSQL correctness | PASS | The parser follows the documented structured EXPLAIN response and preserves server-owned values without generic SQL parsing. |
| III. Local-first and private | PASS | No non-database network call, telemetry, persistence, clipboard path, or credential route is added. |
| IV. Safe by default | PASS | Plain planning is non-executing; analysis has a one-shot confirmation and inherits the existing impact and transaction safeguards. |
| V. Keyboard-first, never keyboard-only | PASS | Plan entry is palette and chord discoverable; movement and expansion reuse the visible keymap. |
| VI. Meaning survives styling loss | PASS | Node names, metrics, warnings, selection, and exit wording are tested in ASCII, no-colour, narrow, and compact layouts. |
| VII. Cross-platform criterion | PASS with evidence boundary | Automated renderer and restoration contracts remain; Windows/Linux/Warp hand checks remain unclaimed. |
| VIII. One source of truth | PASS | `query::plan` owns plan facts, `Keymap` owns keys, and the existing reducer owns effects. |
| IX. Evidence over confidence | PASS | Pure fixtures, reducer tests, renderer tests, machine-output contracts, and disposable PostgreSQL evidence are planned separately. |
| X. Recoverability | PASS | Failed, cancelled, truncated, and connection-lost plans provide explicit recovery without retry or data mutation. |

No constitution violation requires complexity tracking.

## Project Structure

### Documentation (this feature)

```text
specs/016-readable-query-plan/
├── plan.md              # This file (/speckit-plan command output)
├── research.md          # Phase 0 output (/speckit-plan command)
├── data-model.md        # Phase 1 output (/speckit-plan command)
├── quickstart.md        # Phase 1 output (/speckit-plan command)
├── contracts/           # Phase 1 output (/speckit-plan command)
└── tasks.md             # Dependency-ordered implementation and evidence tasks
```

### Source Code (repository root)
```text
src/
├── app/
│   ├── message.rs       # plan actions, result messages and effects
│   ├── model.rs         # plan session state
│   ├── plan.rs          # local plan view state and selection helpers
│   └── update.rs        # confirmation, job identity and no-side-effect rules
├── cli/
│   └── interactive.rs  # session plan effect execution
├── postgres/
│   └── session.rs       # structured EXPLAIN request over the existing session
├── query/
│   └── plan.rs          # bounded JSON plan parser and derived metrics
└── ui/
    ├── keymap.rs        # plan actions and visible chord labels
    └── layout.rs        # readable plan tree and selected-node detail

tests/
├── cli_contract.rs
├── documentation_matches_the_build.rs
└── postgres_integration.rs
```

**Structure Decision**: Keep one unidirectional state flow. `query::plan` parses
and derives facts without I/O; the PostgreSQL session is the only new database
boundary; `app::update` controls confirmation, job identity and effects; and
`ui::layout` renders the plan over the existing Results pane. The previous
retained result remains in the model while the plan view is open, so dismissal
restores it without execution.

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified**

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| None | N/A | The feature stays within the existing application, query, session, and renderer boundaries. |
