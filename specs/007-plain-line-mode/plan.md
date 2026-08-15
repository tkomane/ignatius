# Implementation Plan: Plain line-oriented terminal mode

**Branch**: `007-plain-line-mode` | **Date**: 2026-08-15 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/007-plain-line-mode/spec.md`

## Summary

Provide an opt-in, line-oriented client that preserves the existing PostgreSQL
connection, result, cancellation, safety and diagnostic contracts while removing
the full-screen terminal requirement. The plain transcript keeps data on the
data stream and prompts, notices and diagnostics on the message stream. The
default invocation remains the full-screen client.

The implementation is present in committed Feature 007 slice `b094d27`. This
plan records the authority, acceptance gates and remaining verification so that
the implementation does not become an unspecified side branch of Feature 001.

## Technical Context

**Language/Version**: Rust 2024, rustc 1.97.1

**Primary Dependencies**: Existing Ignatius connection resolution, PostgreSQL
session, statement splitter, diagnostics, output writer and CLI command tree.
The plain mode does not add a new database driver or a second rendering stack.

**Storage**: Existing versioned TOML configuration and PostgreSQL session; the
plain mode persists no new local state.

**Testing**: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
`cargo test --lib`, `cargo test --test cli_contract`,
`cargo test --test postgres_integration`, and `cargo xtask verify` with the
disposable plain and TLS PostgreSQL services.

**Target Platform**: macOS, Windows and Linux terminals, including `TERM=dumb`,
piped stdin and screen-reader-oriented line output.

**Project Type**: Single Rust CLI and terminal application.

**Performance Goals**: The first prompt appears after the existing connection
establishes; plain mode adds no full-screen redraw or terminal mode overhead.
Input remains line-oriented and one statement is executed at a time.

**Constraints**: No alternate-screen, raw-mode, cursor-addressing or other
terminal-control sequences; no secret output; no silent retry or TLS downgrade;
no change to the default full-screen path; no broad `psql` meta-command promise.

**Scale/Scope**: One interactive connection per plain-mode process, one
statement buffer at a time, existing result row cap and output formats retained.

## Constitution Check

*GATE: Must pass before and after design.*

| Principle | Check | Result |
| --- | --- | --- |
| I. Delight must remain truthful | Cancellation, transaction state and connection loss use existing server-confirmed wording | PASS |
| II. PostgreSQL correctness | Input uses the existing PostgreSQL-aware statement boundaries and session | PASS |
| III. Local-first and private by default | No telemetry, history, upload or new persistence is introduced | PASS |
| IV. Safe defaults | Production writes use existing impact classification and textual confirmation; no replay or TLS downgrade | PASS |
| V. Keyboard-first | SQL, help, connection information, cancellation and quit are available from input | PASS |
| VI. Accessible meaning | Status, posture, environment and recovery are words and survive loss of colour and visual layout | PASS |
| VII. Cross-platform | The contract covers macOS, Windows, Linux, pipes and `TERM=dumb`; hand verification remains explicitly open | PASS |
| VIII. One source of truth | CLI contract, compatibility claims, spec and tasks point to the existing command and exit-code authorities | PASS |
| IX. Evidence over confidence | Focused contract tests and the full `cargo xtask verify` gate are required; hand gaps are named | PASS |
| X. Recoverability | Ctrl+D, Ctrl+C, failed-transaction recovery and existing terminal guard behaviour remain explicit | PASS |

No constitution violation or complexity exception is required.

## Phase 0: Research Decisions

See [research.md](./research.md). The design reuses existing repository
boundaries and does not require external technology research.

## Phase 1: Design Artifacts

- [data-model.md](./data-model.md) defines the transcript, session summary,
  confirmation and execution outcome concepts.
- [contracts/plain-mode.md](./contracts/plain-mode.md) defines invocation,
  stream, command, safety and exit-code behaviour.
- [quickstart.md](./quickstart.md) gives runnable validation scenarios and
  states which platform evidence remains open.

## Project Structure

### Documentation (this feature)

```text
specs/007-plain-line-mode/
├── spec.md
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   └── plain-mode.md
├── checklists/
│   └── requirements.md
└── tasks.md
```

### Source Code (repository root)

```text
src/
├── cli/
│   ├── mod.rs                 # global --plain routing
│   ├── interactive.rs         # shared cancellable execution
│   └── plain.rs               # line reader, prompts and plain session
├── connection/                # existing target resolution and posture
├── diagnostics/               # existing redaction and layered errors
├── postgres/                  # existing session and transaction state
└── query/                     # existing splitter, results and output model

tests/
└── cli_contract.rs            # subprocess stream, safety and plain-mode checks
```

**Structure Decision**: Keep plain mode inside the existing `cli` boundary and
reuse the existing connection, query, diagnostic and output authorities. No
new crate, service, persistence layer or alternate database abstraction is
needed.

## Verification Gates

1. Focused unit tests cover statement completion, command parsing, prompts,
   production confirmation and outcome wording.
2. Subprocess contract tests cover `TERM=dumb`, stream separation, absence of
   terminal-control sequences, production-write cancellation and exit status.
3. `cargo xtask verify` passes with the disposable plain and TLS databases
   running.
4. Documentation records automated evidence separately from the remaining
   VoiceOver, NVDA, Windows Terminal, Linux terminal and ConPTY hand checks.

## Complexity Tracking

None. The feature reuses the existing ports and result contracts and introduces
no new persistence, service or dependency boundary.
