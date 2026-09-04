# Implementation Plan: Reviewable cell-to-UPDATE

**Branch**: `023-cell-update` | **Date**: 2026-09-04 | **Spec**: [spec.md](./spec.md)

## Summary

Add a conservative interactive result-cell workflow that resolves a live
single-table source and primary key, prompts for one replacement value, shows a
fully bound `UPDATE` for review, and emits one existing parameterized execution
only after confirmation. Production-classified and server read-only sessions
are refused before input. Unsupported SQL shapes and stale result identities
fail closed.

## Technical Context

**Language/Version**: Rust 2024 edition, minimum Rust 1.90, local verifier
currently using Rust/Cargo 1.98.0

**Primary Dependencies**: Existing PostgreSQL catalogue adapter, Tokio,
Ratatui, Crossterm, and `secrecy::SecretString`. No new dependency.

**Storage**: None beyond the existing optional statement history. Generated
history stores the private parameter template, never bound values.

**Testing**: Pure source/planner tests, reducer and renderer tests, keymap and
discovery tests, debug/privacy checks, documentation parity, and disposable
PostgreSQL integration evidence followed by `cargo --locked xtask verify`.

**Target Platform**: macOS, Linux and Windows terminal clients. The feature is
interactive TUI only in this slice.

**Project Type**: Terminal-native interactive workbench and scriptable CLI.

**Performance Goals**: Source analysis is linear and bounded; metadata is one
bound catalogue lookup plus the existing ordered-column read; generated SQL is
bounded at 16 KiB; no result rerun or bulk memory path is introduced.

**Constraints**: No full SQL parser, no type inference, no unique-index
guessing, no production bypass, no automatic execution, no automatic rerun,
and no raw replacement or key values in debug output, logging or history.

## Constitution Check

*GATE: Pass before implementation and re-check after verification.*

- Delight remains truthful: unsupported shapes and stale identities are named,
  and the result remains a snapshot after the write.
- PostgreSQL correctness remains explicit: relation and key facts come from the
  live PostgreSQL catalogue, identifiers are quoted, and values use the existing
  simple-query literal binding boundary.
- Local-first and private remain intact: no service or storage is added, and
  generated values are secret-bound until the deliberate review display or
  execution boundary.
- Safe defaults remain intact: production and server read-only sessions refuse,
  and confirmation is separate from value entry.
- Keyboard-first and accessible: the action is palette and chord discoverable,
  the modal carries words, and printable editor input is preserved.
- Cross-platform: no new terminal escape sequence or platform-specific input is
  introduced.
- Evidence remains separate: pure, reducer, renderer, live database and
  terminal/platform checks will be reported independently.

**Gate result**: PASS. The new metadata query, pure planner, prompt and typed
effect are bounded additions to the existing architecture.

## Project Structure

```text
src/
├── app/
│   ├── discovery.rs       # action availability and prerequisite wording
│   ├── message.rs         # lookup message and parameterized write effect
│   ├── model.rs           # result SQL, lookup, prompt and review state
│   └── update.rs          # pure workflow reducer and stale checks
├── postgres/
│   ├── metadata.rs        # bound live relation and privilege discovery
│   └── session.rs         # adapter method
├── query/
│   ├── mod.rs             # feature exports
│   └── update.rs          # conservative source analyzer and planner
└── ui/
    ├── keymap.rs          # Ctrl+K u discovery
    └── layout.rs          # value and review surfaces

tests/
├── documentation_matches_the_build.rs
└── postgres_integration.rs
```

**Structure Decision**: Pure SQL shape and plan generation stays in `query`,
live relation facts stay in `postgres`, and all prompt/review transitions stay
in the existing pure reducer. The runtime only executes returned effects.

## Complexity Tracking

The separate update lookup and review states are required to keep database I/O,
secret values and confirmation boundaries visible. No constitution exception is
needed. Primary-key-only and conservative-source refusal intentionally trade
coverage for a reviewable first write path.

## Implementation Phases

1. Add the Feature 023 artifacts and active feature pointer.
2. Add pure source analysis, identifier parsing, bounded planner and focused
   tests.
3. Add live relation discovery, update messages/effects and runtime plumbing.
4. Add model state, reducer transitions, prompt/review rendering and
   production/read-only/stale protections.
5. Add action, chord, palette, discovery and documentation parity.
6. Run focused and live checks, the locked verifier, disposable database cleanup
   and status evidence capture.
