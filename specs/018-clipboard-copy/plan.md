# Implementation Plan: Copy a result value out

**Branch**: `018-clipboard-copy` | **Date**: 2026-09-04 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/018-clipboard-copy/spec.md`

## Summary

Add an explicitly confirmed, opt-in copy action for one retained PostgreSQL
result text cell. The reducer records only the result identity and counts while
the confirmation is open, then re-derives the current value before producing a
write-only OSC 52 effect. The interactive runtime writes a base64-framed
sequence to the terminal and reports bytes written without claiming that the
receiving clipboard accepted the value. NULL, stale selections, disabled
configuration, over-limit values, cancellation, and I/O failures remain
truthful no-payload outcomes. The action is available from the Results context,
inspector, keymap, and command palette without taking printable letters from SQL
editing. Ordinary results, history, exports, and machine output remain separate.

## Technical Context

**Language/Version**: Rust 2024 edition, MSRV Rust 1.90, pinned local toolchain 1.97.1

**Primary Dependencies**: Existing `ratatui` 0.30.2, `crossterm` 0.29.0,
`serde`/`toml`, and standard-library I/O; OSC 52 framing and base64 are
implemented locally with no new dependency

**Storage**: In-memory configuration preference, copy candidate identity, and
user-visible outcome only; no clipboard persistence, read-back, telemetry, or
result-value duplication

**Testing**: `cargo test`, pure OSC 52 framing and payload-redaction tests,
configuration and keymap contracts, reducer/effect assertions, renderer buffer
checks, CLI/documentation contracts, and the existing disposable PostgreSQL
integration suite to prove ordinary result behavior is unchanged

**Target Platform**: Interactive terminal client on macOS/Warp, Windows
Terminal, Linux terminals, SSH, and multiplexers; full, compact, narrow, ASCII,
no-colour, and reduced-motion presentation boundaries remain supported

**Project Type**: Local-first Rust terminal application and scriptable CLI

**Performance Goals**: Copy validation is local and bounded; no database round
trip occurs; values are rejected above 1 MiB of raw UTF-8 bytes before terminal
transport; one confirmed request produces at most one terminal write sequence

**Constraints**: OSC 52 is disabled unless `[clipboard] osc52 = true` is set.
The client writes and flushes the sequence but cannot verify terminal acceptance,
read the clipboard, or clear it. Raw values must not enter Debug output,
diagnostics, logs, status text, or confirmation UI. Existing job identity,
redaction, terminal sanitisation, production protection, and no-automatic-retry
rules remain authoritative.

**Scale/Scope**: One text cell from the currently retained result per request,
including source-row identity behind filtering/sorting. No native clipboard
dependency, clipboard read-back, auto-clear, bulk copy, file export redesign,
machine-output route, or database mutation.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Gate | Result | Evidence |
| --- | --- | --- |
| I. Delight remains truthful | PASS | Confirmation identifies location and byte/character counts; success reports terminal write and explicitly leaves acceptance unconfirmed. |
| II. PostgreSQL correctness | PASS | The action reads the already retained result cell and never transforms SQL NULL, re-runs a statement, or changes database state. |
| III. Local-first and private | PASS | No network call, persistence, telemetry, clipboard read-back, or credential route is added; transport is opt-in and write-only. |
| IV. Safe by default | PASS | Configuration defaults to off, explicit confirmation is required, a 1 MiB bound refuses oversized values, and stale candidates send nothing. |
| V. Keyboard-first, never keyboard-only | PASS | A non-printable key chord, contextual hints, inspector entry, and command-palette entry expose the action while printable `c` remains SQL input. |
| VI. Accessible meaning survives styling loss | PASS | Confirmation, disabled, stale, NULL, size, and unconfirmed outcomes use plain text that is testable in compact, narrow, ASCII, and no-colour layouts. |
| VII. Cross-platform criterion | PASS with evidence boundary | The sequence is terminal-protocol-only and has no OS-specific clipboard API; automated tests cover bytes and renderer fallbacks, while real terminal acceptance remains unverified by design. |
| VIII. One source of truth | PASS | `Config` owns the preference, the retained result owns the value, the reducer owns confirmation/effects, and the transport module owns framing. |
| IX. Evidence over confidence | PASS | Pure transport, configuration, reducer, renderer, documentation, and no-regression checks are separated; terminal acceptance is never inferred. |
| X. Recoverability | PASS | Cancel, stale, NULL, oversized, disabled, unsupported, and I/O-failure paths retain the result and provide a next action without retrying automatically. |

No constitution violation requires complexity tracking.

## Project Structure

### Documentation (this feature)

```text
specs/018-clipboard-copy/
├── plan.md              # This implementation plan
├── research.md          # Phase 0 decisions and protocol evidence
├── data-model.md        # Preference, candidate, and outcome model
├── quickstart.md        # Focused, live, and manual validation guide
├── contracts/
│   └── clipboard.md     # Interactive clipboard contract
└── tasks.md             # Dependency-ordered implementation and evidence tasks
```

### Source Code (repository root)

```text
src/
├── clipboard.rs          # Bounded payload, redacted Debug, OSC 52 framing
├── config/
│   ├── schema.rs         # Backward-compatible [clipboard] preference
│   └── store.rs          # Starter configuration documentation
├── app/
│   ├── discovery.rs      # Palette, prerequisite, and contextual discovery
│   ├── inspect.rs        # Existing selected-cell context
│   ├── message.rs        # Copy action, effect, and completion messages
│   ├── model.rs          # Candidate identity and copy outcome state
│   └── update.rs         # Confirmation, stale check, and effect transition
├── cli/
│   └── interactive.rs    # Single terminal write/flush boundary
├── ui/
│   ├── keymap.rs         # Ctrl+K c chord and visible copy label
│   └── layout.rs         # Confirmation and outcome rendering
└── lib.rs                # Clipboard module registration

tests/
├── cli_contract.rs                    # No machine-output or export leakage
├── documentation_matches_the_build.rs # Config and keymap documentation
└── postgres_integration.rs            # Ordinary result regression boundary
```

**Structure Decision**: Keep the existing unidirectional state flow. A small
root `clipboard` module owns only protocol-safe byte framing and the redacted
payload type. `Config` owns the opt-in preference; `app::update` owns all
confirmation and stale-identity decisions; `cli::interactive` is the only
runtime writer; and `ui::layout` renders safe, value-free status. The retained
query result remains the sole value store.

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified**

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| None | N/A | The feature stays within the existing config, reducer, terminal, and UI boundaries. |
