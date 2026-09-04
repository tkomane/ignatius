# Implementation Plan: Schema-aware completion

**Branch**: `codex/012-schema-completion` | **Date**: 2026-09-04 | **Spec**: [spec.md](./spec.md)

## Summary

Implement one schema-aware completion engine for the full-screen editor and
plain mode. A read-only metadata session loads a bounded catalogue snapshot at
the same time as the object tree. A pure query module reuses the existing SQL
lexer and statement boundaries to identify the word being completed, the
relation or schema scope, aliases, CTEs, literals, and comments. The reducer
owns loading, stale, unavailable, and menu state; the runtime only performs the
catalogue read and timestamps the resulting snapshot.

The TUI gets a keyboard-first, cursor-anchored menu with visible selection,
kind/schema/type detail, bounded truthful counts, one-key acceptance, Esc
dismissal, and one undo step for an accepted replacement. Plain mode gets an
explicit `\complete [prefix]` command that prints numbered candidates and accepts
one by number or exact name, preserving its no-control-sequences contract.
Completion is enabled by default, can be disabled for automatic popups through
`[ui] completion = false`, and remains available through explicit `Ctrl+Space`.

## Technical Context

**Language/Version**: Rust 1.90, edition 2024
**Primary Dependencies**: ratatui 0.30.2, crossterm 0.29.0,
tokio-postgres 0.7.18, tokio 1.53.1
**Storage**: PostgreSQL catalogue reads; no completion state is persisted
**Testing**: `cargo test`, focused unit tests, CLI contract tests, PostgreSQL
integration tests, and `cargo --locked xtask verify`
**Target Platform**: macOS terminals, Windows Terminal/ConPTY, Linux
terminals, and plain/screen-reader pipelines
**Project Type**: Rust terminal-native PostgreSQL workbench
**Performance Goals**: no database statement per keystroke; retain one local
catalogue snapshot without silently dropping rows; bound the visible menu and
keep filtering local and immediate
**Constraints**: local-first and private; no network other than the existing
PostgreSQL connection; no credential or SQL buffer persistence; no new runtime
dependency; words must retain meaning with ASCII and colour disabled
**Scale/Scope**: thousands of catalogue objects must remain bounded and report
the true matching/total counts; one editor buffer and one catalogue snapshot per
session

## Constitution Check

*GATE: Pass before Phase 0 research and re-check after Phase 1 design.*

- Delight is truthful: loading, stale, unavailable, unreadable, and truncated
  states are explicit; no false empty list is shown.
- PostgreSQL correctness: catalogue values are read with fixed extended-protocol
  queries and bound parameters; accepted names use the existing unconditional
  identifier quoting helper.
- Local-first/privacy: completion reads only the already-connected database and
  keeps no completion state on disk; keystrokes never send SQL.
- Safe defaults: no candidate is inserted or executed without an explicit
  accept action; dismissing leaves the buffer byte-for-byte unchanged.
- Keyboard-first and discoverable: `Ctrl+Space`, Enter, Esc, arrows, help,
  palette/keymap descriptions, and plain `\complete` cover the paths.
- Accessible: kind, source schema, type, counts, loading, and failure are words;
  ASCII mode has no semantic dependence on colour or Unicode glyphs.
- Cross-platform: the feature uses existing crossterm actions and ratatui
  rendering, with no terminal-specific escape sequences.
- One source of truth: the query module owns lexing/context/ranking; metadata
  maps server rows into its catalogue types; TUI and plain mode consume the same
  candidate and replacement logic.
- Evidence and recoverability: focused tests cover pure behavior and reducer
  transitions, live tests cover catalogue reads, skips remain explicit, and an
  accepted replacement is one editor undo step.
- Architecture: input becomes `Action`, the pure reducer changes model and
  returns effects, the executor performs I/O, and stale responses are ignored.
- Secrets: the catalogue contains names, kinds, and types only; no passwords,
  tokens, or connection strings enter completion state or diagnostics.
- Dependencies: no new crate is required.

## Project Structure

```text
src/
├── app/
│   ├── editor.rs          # range replacement with one undo snapshot
│   ├── message.rs         # Complete action and catalogue message/effect data
│   ├── model.rs           # completion status and active menu state
│   └── update.rs          # pure loading, filtering, accept/dismiss transitions
├── cli/
│   ├── interactive.rs     # catalogue effect and TUI model configuration
│   └── plain.rs           # explicit \complete command and numbered selection
├── config/
│   ├── schema.rs          # ui.completion default and deserialization
│   └── store.rs            # starter configuration
├── postgres/
│   ├── metadata.rs        # fixed catalogue snapshot query
│   └── session.rs         # session facade for the snapshot
├── query/
│   ├── completion.rs      # pure context, scope, ranking, candidates
│   ├── identifiers.rs     # shared PostgreSQL identifier quoting
│   └── highlight.rs       # shared word/keyword lexer helpers
└── ui/
    ├── keymap.rs          # Ctrl+Space and discoverable binding metadata
    └── layout.rs          # anchored completion menu in full/compact layouts

docs/
├── design/keymap.md
├── operations/local-development.md
├── product/experience-roadmap.md
└── status.md

specs/012-schema-completion/
├── spec.md
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/completion.md
└── tasks.md

tests/
├── cli_contract.rs        # plain completion transcript contract
└── postgres_integration.rs # live catalogue snapshot evidence
```

**Structure Decision**: retain the single Rust crate and existing unidirectional
application architecture. Pure completion semantics live under `src/query` so
the TUI, plain mode, and tests cannot grow separate parsers. PostgreSQL maps
catalogue rows into its pure types, while `src/app` owns presentation state.

## Implementation Phases

### Phase 0 - Research

Record the existing lexer, statement-boundary, metadata-session, editor undo,
plain-reader, keymap, and renderer constraints in [research.md](./research.md).
Resolve catalogue query shape, permission wording, identifier quoting, and
plain-mode interaction before changing source.

### Phase 1 - Design

Define the pure catalogue, context, candidate, and UI state in
[data-model.md](./data-model.md). Capture the internal action/message/effect,
catalogue SQL, and plain-mode contracts in
[contracts/completion.md](./contracts/completion.md). Provide operator-facing
examples and verification commands in [quickstart.md](./quickstart.md).

### Phase 2 - Core implementation

1. Add shared lexer helpers and the pure completion analyzer with conservative
   scope detection for relations, aliases, schemas, CTE names, subquery aliases,
   literals, comments, and statement boundaries.
2. Add the fixed catalogue snapshot query and session facade, preserving
   read-only metadata routing, bound parameters, permission facts, and bounded
   response data.
3. Add editor range replacement, reducer state/action/message handling,
   auto/explicit invocation rules, selection movement, accept/dismiss behavior,
   and stale/unavailable catalog handling.
4. Add configuration, keymap/help/palette discoverability, anchored TUI rendering,
   ASCII/no-colour wording, and explicit plain-mode completion.

### Phase 3 - Verification and documentation

Add focused unit, reducer, renderer, CLI, and PostgreSQL tests; update keymap,
configuration, roadmap, and status documentation; run focused checks followed by
the authoritative locked verification. Run the disposable PostgreSQL integration
gate where available and report any unavailable platform or terminal checks as a
skip rather than a pass.

## Complexity Tracking

No constitution violations or new architectural components require an exception.
