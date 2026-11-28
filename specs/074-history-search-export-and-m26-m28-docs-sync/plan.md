# Implementation Plan: Local History Search/Export and M26-M28 Docs Sync

**Branch**: `feat/m28-history-search-and-stats` (PR 3A) -> `docs/m28-roadmap-and-readme-sync` (PR 3B)
**Spec**: `specs/074-history-search-export-and-m26-m28-docs-sync/spec.md`

## Slice 1 (PR 3A): History Search, Stats, Export, and TUI Preview

1. **`src/history.rs` & `src/history/tests.rs`**: Add `MAX_HISTORY_QUERY_LIMIT = 10_000`, `Outcome::parse`, `parse_since_timestamp`, `HistoryFilter`, `History::search`, `HistoryStats::compute`, `HistoryExportFormat`, and `format_history_export` (re-applying `redact_text` and skipping `mentions_a_credential` defense-in-depth).
2. **`src/cli/mod.rs` & `tests/cli_contract.rs`**: Add `HistoryAction::{Search, Stats, Export}` offline commands reusing safe atomic file publication for `--output [--force]`.
3. **`src/app/palette.rs`, `src/app/update.rs` & sidecar tests**: Add `Palette::from_history_entries`, `Purpose::History` Spotlight preview, and `@db:`/`@env:`/`@status:` inline tokens while keeping `Palette::over_history` backwards-compatible.

## Slice 2 (PR 3B): Documentation & Verification Sync

1. Reconcile specs `072`/`073`/`074`, `docs/product/roadmap.md`, `docs/product/agent-delivery-plan.md`, `docs/status.md` (Waves 122-124), `docs/support/compatibility.md`, `CHANGELOG.md`, `README.md`, and `tests/documentation_matches_the_build.rs`.
2. Run `diff -u AGENTS.md CLAUDE.md`, `cargo --locked xtask verify`, and `cargo --locked xtask security`.
