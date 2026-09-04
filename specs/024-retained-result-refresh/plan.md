# Implementation Plan: Explicit retained-result refresh

**Branch**: `024-retained-result-refresh` | **Date**: 2026-09-04 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/024-retained-result-refresh/spec.md`

## Summary

Add one explicit, discoverable refresh action for a retained interactive result.
The reducer validates that the retained source is exactly one read-classified
statement with a visible result, then passes the unchanged source through the
existing execution and named-parameter paths. The editor remains independent,
and no refresh is introduced into any passive state transition.

## Technical Context

<!--
  The following context records the selected implementation constraints for this
  feature and is part of the plan reviewed before implementation.
-->

**Language/Version**: Rust 2024 edition, minimum Rust 1.90, local verifier
currently using Rust/Cargo 1.98.0

**Primary Dependencies**: Existing reducer, PostgreSQL execution adapter,
Ratatui, Crossterm, named-parameter scanner and advisory SQL classifier. No
new dependency.

**Storage**: None. The existing retained source and optional history entry are
ephemeral or governed by existing history settings.

**Testing**: Reducer eligibility and lifecycle tests, discovery and palette
tests, keymap reachability tests, narrow/ASCII renderer tests, documentation
parity, focused formatter and full `cargo --locked xtask verify` checks. No new
database adapter is introduced, so existing disposable PostgreSQL execution
evidence remains the live boundary.

**Target Platform**: macOS, Linux and Windows terminal clients. Interactive TUI
only in this slice; plain and scripted routes remain unchanged.

**Project Type**: Terminal-native interactive workbench and scriptable CLI.

**Performance Goals**: Eligibility is bounded by the retained source length and
the existing statement splitter. Refresh adds no polling, background refresh,
second query, or unbounded retained data. It uses the existing execution
latency path.

**Constraints**: No full SQL parser, no replay of multiple statements, no
write-classified source, no automatic retry or rerun, no editor mutation, no
new network call, and no secret parameter values outside the existing boundary.

**Scale/Scope**: One retained result source and one explicit refresh request at
a time. The feature is limited to the interactive Results pane and reuses the
existing bounded result cap and history limits.

## Constitution Check

*GATE: Pass before Phase 0 research and after Phase 1 design.*

- Delight remains truthful: the action is explicit, says when it is running,
  distinguishes refusal from execution, and does not claim freshness after a
  failed request.
- PostgreSQL correctness remains explicit: source splitting and advisory impact
  classification are reused, and server permissions and transaction state stay
  authoritative.
- Local-first and private remain intact: no service, storage or telemetry is
  added; parameter prompts continue to use their existing secret boundary.
- Safe defaults remain intact: multiple statements and non-read classifications
  are refused, while a read on a production or server read-only session keeps
  the existing read path.
- Keyboard-first and accessible: the action has a palette entry, a direct
  configurable shortcut, contextual wording and no colour-only meaning.
- Cross-platform remains unchanged: no new terminal escape sequence or
  platform-specific I/O is added.
- Evidence remains separate: focused reducer/UI checks, full verifier output,
  live database evidence and manual terminal evidence will not be conflated.

**Gate result**: PASS. The feature extends existing state and execution
contracts without introducing a new effect type, storage route or permission
boundary.

## Project Structure

### Documentation (this feature)

```text
specs/024-retained-result-refresh/
├── plan.md              # This file (/speckit-plan command output)
├── research.md          # Phase 0 output (/speckit-plan command)
├── data-model.md        # Phase 1 output (/speckit-plan command)
├── quickstart.md        # Phase 1 output (/speckit-plan command)
├── contracts/           # Phase 1 output (/speckit-plan command)
└── tasks.md             # Phase 2 output (/speckit-tasks command - NOT created by /speckit-plan)
```

### Source Code (repository root)
<!--
  The tree below lists only the repository paths touched by this feature.
-->

```text
src/
├── app/
│   ├── discovery.rs       # refresh availability and prerequisite wording
│   ├── message.rs         # RefreshResult intent
│   └── update.rs          # eligibility and explicit refresh transition
├── query/
│   ├── classify.rs        # existing advisory impact check
│   └── statements.rs      # existing bounded statement split
└── ui/
    ├── keymap.rs          # direct shortcut and reachability
    └── layout.rs          # refreshing state and contextual result hint

tests/
└── documentation_matches_the_build.rs
```

**Structure Decision**: Keep the eligibility decision and the new intent in
the pure application layer. Reuse the current statement splitter, classifier,
parameter prompt, execution effects and result lifecycle. The keymap and
renderer only expose the action and state; they do not execute SQL.

## Complexity Tracking

No constitution violations. A separate refresh-specific effect or cache is
intentionally not introduced because it would duplicate the existing execution
boundary and create a second source of truth.

## Implementation Phases

1. Add the pure eligibility predicate and `RefreshResult` action, including
   palette, discovery and direct-key reachability.
2. Route an eligible retained source through the existing execution and
   parameter-prompt paths without changing the editor.
3. Make the running state and refusal wording visible in the Results surface
   and footer while preserving ordinary result lifecycle rules.
4. Add reducer, discovery, keymap, renderer and documentation parity tests.
5. Update user-facing documentation, release notes and status evidence, then
   run focused checks and the locked verifier with all skips called out.
