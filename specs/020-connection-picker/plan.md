# Implementation Plan: Connection picker

**Branch**: `020-connection-picker` | **Date**: 2026-09-04 | **Spec**: [spec.md](spec.md)

## Summary

Turn the existing named-profile configuration into a visible, searchable
connection choice. When profiles exist and the user supplied no explicit route,
the interactive client opens a connection-purpose palette before resolving a
target. A selected profile or the default row is then sent through the existing
resolver and cloud-authentication boundary. Switching after a quiet session
clears server-derived state while retaining the editor, and runtime generation
guards reject late work from the old connection.

## Technical Context

**Language/Version**: Rust 2024 edition, MSRV Rust 1.90, pinned local toolchain 1.97.1

**Primary Dependencies**: Existing standard library, `ratatui`, `crossterm`,
Tokio and repository profile/cloud boundaries; no new dependency

**Storage**: No new persistence. Safe summaries live in the interactive model;
resolved targets and credentials remain in runtime connection slots only

**Testing**: Pure summary and palette tests, reducer transition tests, layout
buffer tests, runtime preparation and generation tests, documentation contracts,
CLI contracts, and the locked verifier with disposable PostgreSQL

**Target Platform**: macOS in Warp, Windows Terminal, Linux terminals, SSH,
multiplexers and plain mode; manual terminal evidence remains a separate gate

**Constraints**: No secret or unknown profile value in model/UI/debug; no
fallback after selection failure; no SQL or metadata before choice; no resolver
duplication; no stale async state after switching; normal hyphen in all prose

## Constitution Check

| Gate | Result | Evidence |
| --- | --- | --- |
| I. Delight remains truthful | PASS | Rows distinguish configured hints from defaults, invalid profiles say so, and connection state remains Connecting until the server answers. |
| II. PostgreSQL correctness | PASS | The picker delegates target resolution, TLS, profile classification and cloud authentication to existing PostgreSQL connection boundaries. |
| III. Local-first and private | PASS | Picker search is local; selection uses only the existing database credential route and adds no telemetry or persistence. |
| IV. Safe by default | PASS | The default row is explicit, production/read-only profile facts are visible, and failures never fall back. |
| V. Keyboard-first, never keyboard-only | PASS | Palette search, Enter/Escape, `Ctrl+K n` and existing command-palette access all reach the same choice. |
| VI. Accessible meaning survives styling loss | PASS | Environment, TLS, provider, read-only, default and invalid wording are textual and tested in ASCII and compact layouts. |
| VII. Cross-platform criterion | PASS with evidence boundary | The state and rendering use existing cross-platform primitives; hand terminal evidence remains unverified until exercised. |
| VIII. One source of truth | PASS | Config owns profile fields, the resolver owns precedence, the palette owns search, the reducer owns transitions and the runtime owns secrets. |
| IX. Evidence over confidence | PASS | Pure, reducer, renderer, runtime, contract and database evidence are recorded separately. |
| X. Recoverability | PASS | Escape leaves the current session unchanged, switching retains the editor, and stale connection work is dropped. |

No constitution violation requires complexity tracking.

## Project Structure

```text
src/app/connection_picker.rs       # safe profile summaries and row details
src/app/message.rs                 # picker and connection-choice actions/effects
src/app/model.rs                   # summary list and connection reset state
src/app/palette.rs                 # Connections purpose and typed commands
src/app/discovery.rs               # picker availability and prerequisites
src/app/update.rs                  # pure picker transitions and switch reset
src/cli/interactive.rs             # pre-choice startup and runtime target generation
src/cli/mod.rs                     # explicit-route detection helper
src/ui/keymap.rs                   # Ctrl+K n discovery
src/ui/layout.rs                  # connection-purpose palette wording

tests/cli_contract.rs
tests/documentation_matches_the_build.rs
```

## Implementation Phases

### Phase 0: Research

- Confirm profile validation and resolver ownership.
- Define allowlisted summary fields and default wording.
- Define the runtime generation boundary for late connection and metadata work.

### Phase 1: Pure interaction

- Add safe summaries, connection palette purpose and typed commands.
- Add model/action/discovery/keymap wiring and renderer coverage.
- Keep opening/searching entirely effect-free.

### Phase 2: Runtime connection route

- Gate implicit startup on the picker when profiles exist.
- Resolve and authenticate choices in the runtime, retaining targets outside the
  model and preserving the existing password/cloud boundaries.
- Add generation checks and reset state for profile switches.

### Phase 3: Evidence and handoff

- Update product, security, support, operations, journey and release wording.
- Run focused tests, the locked verifier and disposable PostgreSQL evidence.
- Record manual terminal and screen-reader gaps separately; do not publish.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
| --- | --- | --- |
| Runtime connection generation | A switch can leave old metadata or connection tasks in flight; without an identity, a late answer can look like a fact about the new profile. | Clearing slots alone does not prevent a previously cloned session from sending a result later. |
