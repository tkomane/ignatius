# Implementation Plan: A result grid you can work

**Branch**: `014-workable-result-grid` | **Date**: 2026-09-04 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/014-workable-result-grid/spec.md`

## Summary

Turn the existing bounded result pane into a local investigation surface. A
small application-owned grid state will keep sort, visibility, width, type-label,
freeze, and horizontal-viewport decisions separate from the server-owned
`ResultSet`. Stable source-row indices will preserve selection and filtering
meaning while rows are displayed in a different order. A command-palette-based
controls surface and a searchable column chooser will make every action
discoverable without turning printable editor keys into global shortcuts.

The current execution path deliberately uses PostgreSQL's simple query protocol
so arbitrary extension values remain in the server's text representation. That
protocol exposes column names but not type OIDs through `tokio-postgres`'s
`SimpleColumn`. For a successful row-bearing statement, the session will make a
non-executing parse/describe request after the result has been received and use
the returned server type names only when the described column names and count
match the result. If description is unavailable, the grid says so and never
infers a PostgreSQL type from cell text. This preserves value fidelity and
keeps the type boundary honest.

## Technical Context

**Language/Version**: Rust 1.90, edition 2024

**Primary Dependencies**: ratatui 0.30.2, crossterm 0.29.0,
tokio-postgres 0.7.18, tokio 1.53.1, unicode-width 0.2.x; no new dependency

**Storage**: In-memory `ResultGridState` scoped to the current interactive
session and retained result; optional server-described type labels on
`ResultSet`; no files, clipboard, or database view state

**Testing**: Rust unit tests for state and ordering, reducer tests, ratatui
buffer tests at full/compact/narrow ASCII sizes, command-palette and keymap
contract tests, live PostgreSQL 18.4 integration, then `cargo --locked xtask
verify`

**Target Platform**: macOS terminals including Warp, Windows Terminal/ConPTY,
Linux terminals, and unchanged plain/JSON/scripted interfaces

**Project Type**: Local-first terminal-native PostgreSQL workbench

**Performance Goals**: Sorting is O(R log R) over at most the existing retained
row cap; filtering and viewport calculation remain bounded by retained rows and
columns; grid rendering does no database work; type description adds at most
one parse/describe exchange for a row-bearing statement

**Constraints**: No query rewrite, server-side sort, automatic rerun, page
fetch, clipboard/OSC 52 transfer, result mutation, new dependency, or SQL/value
logging; values and identifiers pass through existing sanitisation before
rendering; type labels are server-described or explicitly unavailable

**Scale/Scope**: One active result set, the existing configured row cap, wide
results with duplicate labels and up to terminal-sized horizontal windows; view
state is disposable and is reset when a new execution replaces the result

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 research and design.*

- **Truthful delight**: Pass. Sort scope, retained-row limits, type availability,
  hidden columns, frozen state, and narrow-terminal fallbacks are written in the
  UI rather than implied by styling.
- **PostgreSQL correctness**: Pass. The server remains authoritative for values,
  type labels, row counts, and NULLs. Local sorting is explicitly not described
  as PostgreSQL collation or query semantics.
- **Local-first and private**: Pass. Grid state is in memory, type description
  carries no result values into state, and no output or clipboard route changes.
- **Safe defaults**: Pass. Controls never execute SQL or alter transaction
  state; export and scripted output retain their existing source-result contract.
- **Keyboard-first and discoverable**: Pass. The controls surface is reachable
  by the command palette and a chord, with a searchable column chooser and
  contextual state labels.
- **Accessible meaning**: Pass. Sort direction, type availability, visibility,
  freeze state, source-row identity, and truncation are words that survive
  colour, Unicode, narrow, and reduced-motion presentation.
- **Cross-platform**: Pass. The feature uses existing keymap, layout, and
  terminal abstractions and adds no platform-specific input or escape sequence.
- **One source of truth**: Pass. `ResultGridState` owns local view choices,
  `ResultSet` owns server result facts, and the renderer consumes both without
  duplicating sorting or selection rules.
- **Evidence over confidence**: Pass. Pure ordering, reducer, renderer,
  discoverability, live type metadata, and full-suite evidence are separate;
  unavailable socket and platform gates remain explicit.
- **Recoverability and secrets**: Pass. Reset returns a result to automatic
  layout, new executions discard stale view state, and no credential, SQL, or
  result value is added to persistent state or logs.

## Project Structure

### Documentation (this feature)

```text
specs/014-workable-result-grid/
├── spec.md
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   └── result-grid.md
├── checklists/
│   └── requirements.md
└── tasks.md
```

### Source Code (repository root)

```text
src/
├── app/
│   ├── grid.rs             # local view state, commands, row ordering
│   ├── message.rs          # result-controls intent
│   ├── model.rs            # grid state on the application model
│   ├── palette.rs          # grid controls and column chooser entries
│   └── update.rs           # pure grid transitions and selection rules
├── postgres/
│   └── session.rs          # non-executing result-column description
├── query/
│   └── result.rs           # optional server type labels with retained rows
├── ui/
│   ├── keymap.rs           # controls chord and discoverable binding data
│   └── layout.rs           # headers, types, hidden columns, freeze and window
└── docs/
    └── ...                 # repository documentation is updated at root

tests/
├── cli_contract.rs         # unchanged scripted output contract checks
└── postgres_integration.rs # live type metadata and result invariants
```

**Structure Decision**: Keep the existing single crate and unidirectional state
model. Grid behaviour that must be deterministic and shared belongs in
`src/app/grid.rs`; PostgreSQL type acquisition remains in the session adapter;
the TUI only renders the state. Plain mode, JSON, export, history, and the SQL
editor are deliberately outside the grid-state path.

## Implementation Phases

### Phase 0 - Research

Record the existing result retention, selection/filter semantics, simple-query
column metadata boundary, type-description API, terminal width helpers, keymap
discoverability contracts, and export/privacy boundaries in [research.md](./research.md).
Resolve how type labels can be shown without changing arbitrary-value fidelity
or claiming that a local sort is server ordering.

### Phase 1 - Design

Define `ResultGridState`, stable source-row ordering, column descriptors, sort
rules, selection transitions, overlay commands, and reset behaviour in
[data-model.md](./data-model.md). Define the interactive, metadata, export, and
security contracts in [contracts/result-grid.md](./contracts/result-grid.md),
and provide reproducible checks in [quickstart.md](./quickstart.md).

### Phase 2 - Core implementation

1. Add deterministic result-column metadata and a post-execution server
   description sidecar that never executes the statement again and falls back
   explicitly when unavailable.
2. Add the pure grid state and stable row ordering, including numeric-looking
   comparison, NULL placement, duplicate labels, visible-column invariants,
   bounded widths, and source-selection preservation.
3. Add reducer actions and palette overlays for grid controls and the searchable
   column chooser. Reset per-result state on execution while preserving only
   documented session preferences.
4. Render a responsive grid with type lines, sort markers, hidden-column count,
   frozen first column, horizontal viewport wording, and safe ASCII/no-colour/
   narrow fallbacks.
5. Keep filtering, inspection, expanded rows, export, JSON, history, and the
   editor on their existing contracts, adjusting only their source-row or
   visible-column lookup where the new view state requires it.

### Phase 3 - Verification and documentation

Add focused unit, reducer, palette/keymap, renderer, and live PostgreSQL tests;
run formatting and the locked verifier with disposable plain/TLS PostgreSQL;
record pass, live, skip, and unverified evidence in `docs/status.md`; update the
roadmaps, journeys, interaction principles, keymap, diagnostics/support notes,
and `CHANGELOG.md`.

## Complexity Tracking

| Addition | Why needed | Simpler alternative rejected because |
|---|---|---|
| Post-execution parse/describe exchange | `tokio-postgres` simple-query column events expose names but not type metadata, while the product must not lose arbitrary extension values | Guessing from text would mislabel PostgreSQL types; switching the value path to binary would regress unknown-type fidelity |
| Dedicated grid state and controls palette | Sorting, hiding, widths, freeze, and type visibility must remain discoverable and must not consume printable editor keys | Global single-key bindings would steal normal SQL typing and make the terminal mode context-dependent |
| Stable source-row mapping | Selection, filtering, inspection, expanded rows, and export must not silently change records after local sorting | Mutating `ResultSet.rows` would mix a view concern with server-owned result order and make reset/export semantics ambiguous |
