# Implementation Plan: Prompted query parameters

**Branch**: `022-parameters-prompted` | **Date**: 2026-09-04 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/022-parameters-prompted/spec.md`

## Summary

Add a bounded PostgreSQL-aware named-placeholder scanner and an explicit
parameter prompt boundary. The full-screen and plain interactive clients will
collect one masked secret value per distinct `:name` in first-use order. The
non-interactive query command will require repeatable `--param-env
NAME=VARIABLE` mappings and will never prompt. The runtime will expand values as
escaped text literals immediately before using the existing simple-query path,
so server-rendered values and the current result model remain unchanged. The
template SQL remains the only history and editor value; parameterized server
positions are reported without an editor marker because they refer to expanded
SQL.

## Technical Context

**Language/Version**: Rust 2024 edition, minimum Rust 1.90, local verifier currently using Rust/Cargo 1.98.0

**Primary Dependencies**: Existing `tokio-postgres` 0.7.18 simple-query adapter,
Tokio, Ratatui, Crossterm, Clap, `secrecy::SecretString`, and the existing
diagnostic, history and terminal prompt boundaries. No new dependency.

**Storage**: None. Parameter values are process-memory data for one execution
attempt only. The template may remain in the editor and statement history.

**Testing**: Focused `cargo test --locked` unit, reducer, renderer, plain-mode,
CLI contract and PostgreSQL integration tests, followed by
`cargo --locked xtask verify`.

**Target Platform**: macOS, Linux and Windows terminal clients; plain and
non-interactive routes; disposable PostgreSQL 14-18 evidence where available.

**Project Type**: Terminal-native interactive workbench and scriptable CLI.

**Performance Goals**: Scan a statement once in linear time, keep at most 64
distinct names, and add no result-row or history overhead after the binding is
consumed. Prompt rendering remains bounded by the existing modal layout.

**Constraints**: No raw parameter value in process arguments, history, logs,
diagnostics, debug output or terminal decoration. No SQL parser or type
inference. No auto-execution after prompt completion beyond the one explicit run
already requested. Existing simple-query text fidelity, cancellation,
transaction state, row cap and exit-code contracts remain authoritative.

## Constitution Check

*GATE: Pass before Phase 0 research and re-check after Phase 1 design.*

- Delight remains truthful: placeholder recognition follows the existing
  PostgreSQL string, identifier, dollar-quote and comment lexer rules; unknown
  or over-limit input is refused rather than guessed.
- PostgreSQL correctness remains explicit: user SQL continues through the
  existing simple-query protocol, values become escaped literal data, and no
  type is inferred from a prompt string.
- Local-first and privacy remain intact: prompting and binding contact no new
  service, history stores only the safe template, and secret values use the
  existing secret type boundary.
- Safe defaults remain intact: a production write confirms before prompting,
  cancellation discards incomplete values, and non-interactive routes fail
  rather than hang.
- Keyboard-first and accessible: existing actions drive the TUI modal, plain
  mode uses hidden terminal input, and names, progress and cancellation are
  stated in words.
- Cross-platform: no platform-specific SQL or terminal API is introduced; the
  current Crossterm raw-mode and runtime abstractions remain the only platform
  boundaries.
- Evidence and recoverability: focused and locked verification, live database
  checks, and remaining terminal/platform skips are recorded separately in
  `docs/status.md`.

**Gate result**: PASS. The design introduces one new typed effect and prompt
state, but it does not add a project, dependency, storage system or credential
route.

## Project Structure

### Documentation (this feature)

```text
specs/022-parameters-prompted/
├── spec.md
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   └── parameters.md
├── checklists/
│   └── requirements.md
└── tasks.md
```

### Source Code

```text
src/
├── app/
│   ├── message.rs       # parameterized execution effect
│   ├── model.rs         # masked parameter prompt state
│   └── update.rs        # prompt precedence and run transition
├── cli/
│   ├── interactive.rs   # parameterized runtime execution/export
│   ├── mod.rs           # --param-env validation and wiring
│   ├── plain.rs         # accessible prompt flow
│   └── prompt.rs        # reusable hidden input boundary
├── postgres/
│   └── session.rs       # bind at the database boundary
└── query/
    ├── mod.rs
    └── parameters.rs    # protected-region scanner and literal binder

tests/
├── cli_contract.rs
├── documentation_matches_the_build.rs
└── postgres_integration.rs
```

**Structure Decision**: Keep the feature inside the existing single Rust
library and binary. Pure placeholder discovery and binding live in `query`, UI
state stays in `app`, I/O remains in the CLI and PostgreSQL adapters, and the
existing tests remain the public contract boundary.

## Complexity Tracking

No constitution violation requires a complexity exception. The separate
`ExecuteParameterized` effect is deliberate: adding secret-bearing fields to
the ordinary `Execute` variant would make all existing debug and history paths
look parameterized and would weaken the unparameterized contract.

## Design Phases

### Phase 0 - Research and decisions

1. Confirm the existing driver decision: extended protocol binding would require
   generic decoding to preserve arbitrary PostgreSQL text values, while the
   simple protocol preserves the current fidelity and streaming behavior.
2. Confirm the existing statement lexer is the source for protected regions and
   that named placeholders can be recognized without a full SQL parser.
3. Confirm the existing secret and plain terminal prompt boundaries can hold
   values without echo and without changing the process argument surface.
4. Record the error-position limitation and the explicit environment mapping
   contract rather than creating a false editor mapping or a hanging script.

### Phase 1 - Design and contracts

1. Define the placeholder, template, secret binding and parameterized execution
   state transitions in `data-model.md`.
2. Define scanner, binding, TUI, plain and CLI contracts in
   `contracts/parameters.md`.
3. Define unit, reducer, renderer, plain, CLI and disposable PostgreSQL checks
   in `quickstart.md`.
4. Re-evaluate the constitution gate before implementation.

### Phase 2 - Implementation sequence

1. Add the pure scanner and literal binder with adversarial and protected-region
   tests.
2. Add the secret-bearing effect and masked TUI prompt, then connect it to the
   existing production confirmation and execution lifecycle.
3. Add runtime/session binding while preserving history template text and
   disabling only parameterized error-location mapping.
4. Add hidden plain-mode input and `--param-env` validation before connection
   work; preserve machine-output stream boundaries.
5. Update documentation, compatibility claims, release notes and status.

### Phase 3 - Verification

Run focused tests first, then the complete locked verifier with disposable
PostgreSQL where available. Record database evidence, terminal evidence and
remaining skips independently. Confirm the database is explicitly stopped
after live checks and leave the worktree uncommitted unless the owner authorizes
publication.
