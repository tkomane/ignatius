# Tasks: Local History Search/Export and M26-M28 Docs Sync

**Input**: Design documents from `/specs/074-history-search-export-and-m26-m28-docs-sync/`
**Prerequisites**: `spec.md`, `plan.md`

## Phase 1: PR 3A - Local History Search, Stats, Export & TUI Preview (`feat/m28-history-search-and-stats`)

- [x] T001 Extend `src/history.rs` and `src/history/tests.rs` with `MAX_HISTORY_QUERY_LIMIT`, `Outcome::parse`, `parse_since_timestamp`, `HistoryFilter`, `History::search`, `HistoryStats::compute`, `HistoryExportFormat`, and `format_history_export` (excluding `mentions_a_credential` and re-redacting via `redact_text`).
- [x] T002 Add offline `ignatius history search|stats|export` CLI subcommands (`src/cli/mod.rs`) with atomic `--output` / `--force` / symlink protection and CLI contract coverage (`tests/cli_contract.rs`).
- [x] T003 Upgrade TUI History palette (`src/app/palette.rs`, `src/app/update.rs`, `src/app/palette/tests.rs`, `src/ui/widgets/palette/tests.rs`) with `Palette::from_history_entries`, Spotlight SQL + metadata preview, and `@db:`/`@env:`/`@status:` tokens, verified with `cargo --locked xtask verify`.

## Phase 2: PR 3B - M26-M28 Docs Sync (`docs/m28-roadmap-and-readme-sync`)

- [ ] T004 Reconcile `specs/072-*`, `specs/073-*`, `specs/074-*`, `docs/product/roadmap.md`, `docs/product/agent-delivery-plan.md`, `docs/status.md` (Waves 122-124), `docs/support/compatibility.md`, `CHANGELOG.md`, `README.md`, and `tests/documentation_matches_the_build.rs` (zero em/en dashes, owner-gated tasks left open).
- [ ] T005 Verify `AGENTS.md == CLAUDE.md`, `cargo --locked xtask verify`, and `cargo --locked xtask security`.
