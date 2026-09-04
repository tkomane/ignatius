# Implementation Plan: Errors that point at the problem

**Branch**: `codex/013-errors-at-cursor` | **Date**: 2026-09-04 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/013-errors-at-cursor/spec.md`

## Summary

Use PostgreSQL's structured statement ordinal and one-based character position
to produce one validated error location. The pure query layer will map that
position through the existing statement-boundary lexer and UTF-8-safe editor
offsets, identify a token span for the editor marker, and render a source caret
for plain output. The reducer will remember the editor revision and the source
span of a statement run from a larger buffer, so a late result cannot target
changed text. A valid location moves the editor caret and focus without touching
the buffer or undo history; an invalid or stale location is stated explicitly.

Structured PostgreSQL schema, table, column, and constraint fields will be
promoted into a shared diagnostic object context. The TUI will optionally match
that context against the already loaded completion snapshot and state when the
snapshot is unavailable or does not match. No catalogue query is added to the
error path, and no SQL is stored in diagnostics or logs. Plain mode will add a
line-oriented location excerpt from the SQL it already owns, while JSON retains
the numeric server position and structured fields.

## Technical Context

**Language/Version**: Rust 1.90, edition 2024

**Primary Dependencies**: ratatui 0.30.2, crossterm 0.29.0,
tokio-postgres 0.7.18, tokio 1.53.1

**Storage**: in-memory editor revision and error location only; existing
PostgreSQL diagnostics and completion snapshot

**Testing**: focused Rust unit, reducer, renderer, plain-mode and live
PostgreSQL integration tests, followed by `cargo --locked xtask verify`

**Target Platform**: macOS terminals, Windows Terminal/ConPTY, Linux terminals,
and plain/screen-reader pipelines

**Project Type**: Rust terminal-native PostgreSQL workbench

**Performance Goals**: location mapping is linear in the submitted statement and
does no database work; rendering adds no per-keystroke query or background task

**Constraints**: local-first, private, no new dependency, no SQL in diagnostics
or logs, UTF-8-safe byte boundaries, truthful no-colour and ASCII output

**Scale/Scope**: one active editor buffer, one in-flight job, one mapped error
location, and the existing bounded schema snapshot per session

## Constitution Check

*GATE: Pass before Phase 0 research. Re-check after Phase 1 design.*

- Delight is truthful: a location is applied only when the server position,
  statement source, and editor revision all match; otherwise the limitation is
  visible.
- PostgreSQL correctness: server character positions and structured fields remain
  authoritative; the client does not parse or rewrite SQL to manufacture a fix.
- Local-first/privacy: no new network call is made when an error arrives, and
  source SQL remains in the editor or execution path rather than diagnostic/log
  state.
- Safe defaults: mapping an error never retries or writes; users press the
  existing run action to try again.
- Keyboard-first and discoverable: the caret arrives at the problem, existing
  error detail and dismissal keys remain usable, and the error surface names the
  next action.
- Accessible: line, column, statement, object context, and unavailable reasons
  are words; markers survive ASCII, no-colour, narrow, and high-contrast modes.
- Cross-platform: no new terminal control sequences or platform-specific input
  paths are introduced.
- One source of truth: one pure mapper feeds TUI marker, plain excerpt, and
  reducer location state; the existing redaction implementation remains the
  only diagnostic redaction boundary.
- Evidence and recoverability: pure mapping, reducer, renderer, plain, JSON,
  and live PostgreSQL checks are separate evidence; skips stay explicit.
- Architecture and secrets: mapping stays pure, async job identity is retained,
  and no source SQL or credentials enter logs, snapshots, or diagnostic JSON.
- Dependencies: no new crate is required.

## Project Structure

```text
src/
├── app/
│   ├── editor.rs          # revision and non-undo cursor positioning
│   ├── model.rs           # running source and error location state
│   └── update.rs          # pure stale-buffer mapping and focus transition
├── diagnostics/
│   └── diagnostic.rs      # object context and stable position wording
├── cli/
│   └── plain.rs           # line-oriented location excerpt
├── postgres/
│   ├── error.rs           # statement ordinal and object context mapping
│   └── session.rs         # existing execution source and live behavior
├── query/
│   ├── error_location.rs  # pure UTF-8-safe statement/token mapping
│   ├── statements.rs      # existing PostgreSQL statement boundaries
│   └── highlight.rs       # existing token spans
└── ui/
    └── layout.rs          # editor marker, caret, context, and error panel

tests/
├── cli_contract.rs         # plain and JSON diagnostic contract
├── postgres_integration.rs # live syntax and constraint fields
└── editor_contract.rs      # terminal/editor behavior where applicable
```

**Structure Decision**: retain the existing single Rust crate and unidirectional
application architecture. Pure error-location semantics belong under `src/query`
so TUI and plain mode cannot drift. The PostgreSQL adapter continues to translate
driver facts, the reducer owns revision and focus state, and renderers remain
read-only consumers of that state.

## Implementation Phases

### Phase 0 - Research

Record the current PostgreSQL driver fields, statement splitting offsets, editor
revision requirements, existing diagnostic redaction, completion catalogue shape,
plain output ownership, and full/compact renderer constraints in
[research.md](./research.md). Confirm that constraint context can be enriched
from the current snapshot without an error-path query.

### Phase 1 - Design

Define `StatementSource`, `ErrorLocation`, diagnostic object context, and the
reducer state transitions in [data-model.md](./data-model.md). Capture the
driver, reducer, TUI, plain, JSON, catalogue, and security contracts in
[contracts/errors.md](./contracts/errors.md). Provide reproducible focused and
full verification commands in [quickstart.md](./quickstart.md).

### Phase 2 - Core implementation

1. Add the pure error-location mapper, token-span selection, absolute line and
   column calculation, and source caret renderer with UTF-8 and boundary tests.
2. Extend diagnostics and PostgreSQL error mapping with statement identity and
   redacted structured object context without changing the server's facts.
3. Add editor revision and safe cursor positioning, then update the reducer to
   retain the running source span, reject stale results, invalidate markers on
   edit, and focus the editor only for a valid query location.
4. Render the location and object context in the full and compact TUI, including
   explicit ASCII/no-colour wording and the existing technical expansion path.
5. Add the plain-mode excerpt and preserve the current JSON and stdout contracts.

### Phase 3 - Verification and documentation

Add focused unit, reducer, renderer, CLI, and PostgreSQL tests; update diagnostics
support guidance, journeys, key product roadmaps, status, and the Unreleased
release notes. Run focused checks, then the authoritative locked verifier with
the disposable PostgreSQL services where available. Record Unix-socket,
cross-platform, and screen-reader gaps separately.

## Complexity Tracking

No constitution violations or new architectural components require an exception.
