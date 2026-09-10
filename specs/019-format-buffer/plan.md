# Implementation Plan: Format the SQL buffer

**Branch**: `019-format-buffer` | **Date**: 2026-09-04 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/019-format-buffer/spec.md`

## Summary

Add a deterministic, conservative SQL formatter for the complete interactive
editor buffer and the plain-mode pending buffer. A small lexical scanner reuses
PostgreSQL's existing quote and comment boundary helpers, preserves protected
regions byte-for-byte, lays out major clauses and eligible lists with bounded
indentation, and maps the cursor through unchanged token bytes. The reducer
applies the result as one editor edit and reports value-free outcomes; it never
executes SQL or performs I/O.

## Technical Context

**Language/Version**: Rust 2024 edition, MSRV Rust 1.90, pinned local toolchain 1.97.1

**Primary Dependencies**: Existing Rust standard library, `ratatui` 0.30.2,
`crossterm` 0.29.0, and the repository's PostgreSQL boundary helpers; no new
dependency

**Storage**: In-memory formatter result and value-free editor notice; no new
persistence

**Testing**: `cargo fmt`, `cargo check`, `cargo clippy -- -D warnings`, unit
formatter and editor tests, reducer/keymap/palette/layout tests, plain-mode
contract tests, documentation contracts, and the existing disposable
PostgreSQL verifier to prove no query-path regression

**Target Platform**: Interactive terminal client on macOS, Windows Terminal,
Linux terminals, SSH, and multiplexers; plain mode on all supported hosts

**Project Type**: Local-first Rust terminal application and scriptable CLI

**Performance Goals**: Linear bounded scan and render for source up to 1 MiB;
no database round trip; one formatting action produces at most one editor edit

**Constraints**: Never alter protected-region bytes; never rewrite keyword case
or values; fail closed on ambiguous protected regions; preserve UTF-8
boundaries; no SQL execution, network, file, history, telemetry, or credential
effects

**Scale/Scope**: One complete editor or pending plain-mode buffer per request;
fixed two-space continuation indentation; no parser, formatter configuration,
format-on-save, or external formatter process

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Gate | Result | Evidence |
| --- | --- | --- |
| I. Delight remains truthful | PASS | Applied, already-formatted, empty, malformed, and over-limit outcomes are distinct; no semantic-validity claim is made. |
| II. PostgreSQL correctness | PASS | Existing PostgreSQL quote/comment boundaries are reused; formatting changes only outside protected regions and does not parse or rewrite SQL meaning. |
| III. Local-first and private | PASS | The operation is bounded and local; no network, persistence, telemetry, or credential route is added. |
| IV. Safe by default | PASS | Malformed and over-limit input remains unchanged; one undo step provides recovery. |
| V. Keyboard-first, never keyboard-only | PASS | A visible shortcut, chord, palette action, and plain `\\format` expose the same capability. |
| VI. Accessible meaning survives styling loss | PASS | Outcome wording and the plain route do not rely on colour, glyphs, or full-screen layout. |
| VII. Cross-platform criterion | PASS with evidence boundary | The formatter is standard-library text logic; automated platform contracts remain separate from hand terminal evidence. |
| VIII. One source of truth | PASS | The query formatter owns lexical layout, the editor owns history/cursor mutation, and the reducer owns action/outcome transitions. |
| IX. Evidence over confidence | PASS | Protected-byte, idempotence, cursor, undo, no-effect, renderer, plain, and full verifier checks are planned separately. |
| X. Recoverability | PASS | Refusal leaves text untouched and changed formatting is reversible in one edit. |

No constitution violation requires complexity tracking.

## Project Structure

### Documentation (this feature)

```text
specs/019-format-buffer/
├── spec.md
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   └── format-buffer.md
└── tasks.md
```

### Source Code (repository root)

```text
src/
├── query/format.rs             # bounded scanner, layout, cursor mapping
├── app/
│   ├── message.rs              # FormatBuffer action
│   ├── model.rs                # value-free format outcome
│   ├── discovery.rs            # hints, palette availability, prerequisites
│   └── update.rs                # local reducer transition and editor edit
├── app/editor.rs               # one undoable whole-buffer replacement
├── cli/plain.rs                 # \\format command and message-stream output
├── ui/keymap.rs                 # Ctrl+Shift+F and Ctrl+K q discovery
└── ui/layout.rs                 # outcome wording in the footer

tests/
├── cli_contract.rs              # no machine-output or execution regression
└── documentation_matches_the_build.rs
```

**Structure Decision**: Keep the formatter as a pure `query` module and keep
all side effects in the existing reducer/runtime boundary. The editor receives
one replacement and cursor adjustment, while interactive and plain surfaces
only render the safe outcome. No external formatter process or database call is
introduced.

## Implementation Phases

### Phase 0: Research

- Confirm the existing PostgreSQL lexical boundary helpers and define the
  protected-region scanner.
- Lock the deterministic clause, list, operator, comment, and indentation rules.
- Define the source-to-output cursor mapping and the 1 MiB refusal boundary.

### Phase 1: Foundation

- Add the pure formatter module and its bounded result/error types.
- Add the editor action, model notice, and one-edit cursor-aware integration.
- Add the format action to the keymap, palette, discovery, and documentation
  contract surfaces.

### Phase 2: Accessible interaction

- Add the interactive shortcut/chord and safe footer outcome wording.
- Add the plain-mode `\\format` command without contaminating result stdout.
- Preserve existing modal precedence, execution, completion, history, export,
  and machine-output behavior.

### Phase 3: Verification and release evidence

- Run focused pure, reducer, renderer, keymap, plain, and documentation tests.
- Run formatting, compilation, clippy, and the authoritative locked verifier
  with disposable PostgreSQL where available.
- Record manual terminal and screen-reader evidence separately from automated
  results, and leave release publication unauthorized.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
| --- | --- | --- |
| None | N/A | The feature stays within the existing query, editor, reducer, CLI, and UI boundaries. |
