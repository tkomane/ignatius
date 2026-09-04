# Implementation Plan: Intentional export shapes

**Branch**: `021-intentional-exports` | **Date**: 2026-09-04 | **Spec**: [spec.md](spec.md)

## Summary

Make interactive export feel like a small "Save as..." flow by choosing a
format before the path, and add an explicit, safely quoted `INSERT` output
format for the CLI. Reuse the existing palette, result model, output writer
and atomic export boundary.

## Technical Context

**Language/Version**: Rust 2024 edition, MSRV Rust 1.90, pinned local toolchain 1.97.1

**Primary Dependencies**: Existing Rust standard library, `ratatui`, Tokio,
`clap` and current output/export helpers; no new dependency

**Storage**: No new persistence. Format choice is ephemeral model state; output
options remain command invocation state.

**Testing**: Literal encoder and output tests, reducer/palette tests, layout
buffer tests, CLI and documentation contracts, and the locked verifier with
disposable PostgreSQL

**Target Platform**: macOS in Warp, Windows Terminal, Linux terminals, SSH,
multiplexers and plain mode; hand evidence remains separate

**Constraints**: no output values in diagnostics or model additions, no path
extension guessing, no ambiguous generated SQL, no overwrite, normal hyphen in
all prose

## Constitution Check

| Gate | Result | Evidence |
| --- | --- | --- |
| I. Delight remains truthful | PASS | Format is chosen before the path and every export states retained-row scope. |
| II. PostgreSQL correctness | PASS | INSERT preserves NULL and emits escaped text without guessing source types. |
| III. Local-first and private | PASS | Interactive choice is local and CLI output adds no telemetry or remote work. |
| IV. Safe by default | PASS | Table identifiers are quoted, ambiguous columns are refused and files are never overwritten. |
| V. Keyboard-first, never keyboard-only | PASS | Existing export chord opens a searchable palette with Enter/Escape behavior. |
| VI. Accessible meaning survives styling loss | PASS | Format names, row scope, refusal and recovery remain words. |
| VII. Cross-platform criterion | PASS with evidence boundary | The implementation uses existing terminal and file primitives; hand checks remain pending. |
| VIII. One source of truth | PASS | Palette owns choice, model owns transition, output owns serialization and Export owns file safety. |
| IX. Evidence over confidence | PASS | Buffered, streamed, reducer, renderer, CLI and database evidence are separate. |
| X. Recoverability | PASS | Dismissal and validation refusal leave results and destinations unchanged. |

## Project Structure

```text
src/app/model.rs                 # interactive export format state
src/app/message.rs               # typed export format commands/effects
src/app/palette.rs               # format-purpose palette
src/app/update.rs                # format choice and path transition
src/cli/output.rs                # INSERT format and shared literal encoder
src/cli/mod.rs                   # --insert-table validation and wiring
src/cli/interactive.rs           # selected interactive format writer
src/ui/layout.rs                 # format and path prompt wording

tests/cli_contract.rs
tests/documentation_matches_the_build.rs
```

## Implementation Phases

### Phase 0: Research

- Confirm existing output and atomic-file ownership.
- Define literal, duplicate-column and header invariants.
- Define interactive state so no output value or path is copied before choice.

### Phase 1: Interactive format choice

- Add application export-format enum and typed palette purpose.
- Route `ExportRows` through the format palette, then retain the selected shape
  in the existing name prompt and effect.
- Render format, retained-row and refusal wording in compact and ASCII layouts.

### Phase 2: CLI INSERT shape

- Add `Format::Insert`, `--insert-table` and pre-connection validation.
- Implement one shared encoder for buffered and streamed rows.
- Refuse NUL, duplicate labels and `--no-header` without partial output.

### Phase 3: Evidence and documentation

- Add output, reducer, layout, CLI and documentation regression tests.
- Update compatibility, interaction, journey, local-development, roadmap and
  changelog docs.
- Run focused checks, the locked verifier with plain/TLS PostgreSQL, record the
  Unix-socket skip, tear down fixtures and review dirty scope.

## Risks and Mitigations

| Risk | Mitigation |
| --- | --- |
| Generated SQL is mistaken for a safe migration | Require an explicit table, quote all identifiers, state text-literal semantics and never execute generated output. |
| Interactive output drifts from the screen | Build only from current retained/filter rows and reuse the existing result values. |
| A format change breaks scripts | Keep existing formats byte-stable and add focused machine-output contracts. |
| Streamed and buffered encoders diverge | Use one literal/row encoder and compare bytes in tests. |
| Format choice leaks values or writes early | Palette carries only enum state; file work starts after path acceptance. |
