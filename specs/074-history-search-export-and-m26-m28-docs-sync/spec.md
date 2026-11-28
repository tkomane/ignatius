# Feature Specification: Local History Search/Export and M26-M28 Docs Sync

**Feature Branch**: `feat/m28-history-search-and-stats` (PR 3A) and `docs/m28-roadmap-and-readme-sync` (PR 3B)
**Created**: 2026-11-28
**Status**: In Progress
**Milestone**: M28 - Rich History Search/Stats/Export & M26-M28 Docs Sync

## Problem Statement

1. **Offline History Search, Statistics, and Export (Slice 1, PR 3A).** Local history (`history.jsonl`) currently supports only `history list [--limit] [--json]`, `history clear --yes`, and `history path`. Operators auditing past queries or troubleshooting intermittent slow statements offline cannot search history by substring/database/environment/status/time window, inspect aggregated latency/outcome summaries, export sanitized history to JSONL/CSV/SQL, or preview full history SQL and filter by `@db:`/`@env:`/`@status:` in the TUI History palette.
2. **Documentation & Roadmap Reconciliation (Slice 2, PR 3B).** Across M26-M28 (`072`, `073`, `074`), specs, roadmap, agent delivery plan, operational status, compatibility reference, `CHANGELOG.md`, `README.md`, and `tests/documentation_matches_the_build.rs` must stay synchronized.

## Principles This Feature Must Keep

- **Principle III (Local-first and private by default).** `history search`, `history stats`, and `history export` operate strictly offline on the local history file without resolving connections or contacting PostgreSQL. Credential-mentioning SQL (`mentions_a_credential`) is excluded and targets/SQL pass through `redact_text` defense-in-depth on every read/export/preview route.
- **Principle IV (Safe by default).** File exports (`history export --output`) reject symlinks, refuse overwriting existing destinations unless `--force` is passed, and publish atomically. CSV export escapes RFC 4180 quotes/newlines safely. Output/query limits are bounded (`MAX_HISTORY_QUERY_LIMIT = 10_000`).
- **Principle V (Accessible & truthful UI).** TUI History Spotlight preview uses `Theme`/`Token` only, strips control characters with `sanitize_for_display`, and preserves full meaning in `Basic16`, `NO_COLOR`, and ASCII modes.
- **Documentation Integrity.** Zero Unicode em/en dashes (`\u{2014}` / `\u{2013}`), `AGENTS.md == CLAUDE.md`, sidecar `tests.rs` only, and owner-gated release tasks remain open.

## User Scenarios & Testing

### User Story 1 - Offline History CLI & TUI History Preview (Priority: P1, Slice 1 / PR 3A)

- `ignatius history search [QUERY] [--database DB] [--environment ENV] [--status STATUS] [--since TS] [--limit N] [--json]` filters newest-first within `MAX_HISTORY_QUERY_LIMIT = 10_000`.
- `ignatius history stats [--database DB] [--environment ENV] [--since TS] [--json]` summarizes total count, outcome counts (`ok`, `failed`, `cancelled`, `unknown`), per-database/environment counts, and median/p95/max elapsed latency.
- `ignatius history export --format jsonl|csv|sql [--output PATH] [--force] [filters...]` writes sanitized output to stdout or safely to disk (`Export::create` symlink/overwrite/atomic protections).
- TUI `open-history` uses `Palette::from_history_entries` to enable `@db:`, `@env:`, `@status:` inline tokens and Spotlight multi-line SQL + metadata preview.

### User Story 2 - M26-M28 Docs Sync (Priority: P1, Slice 2 / PR 3B)

- Reconcile `specs/072-*`, `specs/073-*`, `specs/074-*`, `docs/product/roadmap.md`, `docs/product/agent-delivery-plan.md`, `docs/status.md`, `docs/support/compatibility.md`, `CHANGELOG.md`, `README.md`, and `tests/documentation_matches_the_build.rs`.

## Non-Functional Requirements

- **NFR-001 (Secret Discipline)**: Never log or print raw passwords; omit credential-bearing entries and redact via `diagnostics::redaction::redact_text`.
- **NFR-002 (Verification)**: Passes `cargo --locked xtask verify` and `cargo --locked xtask security`.
