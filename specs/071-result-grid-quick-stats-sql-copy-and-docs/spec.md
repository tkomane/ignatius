# Feature Specification: Result-Grid Quick-Stats, Query-Ready SQL & JSON Copy, and Docs Alignment

**Feature Branch**: `feat/m25-grid-stats-sql-copy` (PR 3A) and `docs/m25-roadmap-and-readme-sync` (PR 3B)
**Created**: 2026-11-27
**Status**: Complete (PR 3A #203 `de0728e` merged on `main`; PR 3B ready on `docs/m25-roadmap-and-readme-sync`)
**Milestone**: M25 - Result Grid Quick-Stats, Query-Ready SQL Copy & Docs Sync

## Problem Statement

Operators inspecting query results in Ignatius often need two immediate, zero-round-trip workflows:

1. **Instant column summary metrics while navigating results.** Calculating row counts, non-null vs. NULL counts, distinct cardinality, and numeric min/max/sum/avg currently requires writing another SQL aggregation query or opening the full analytics modal.
2. **Safe, query-ready SQL predicates and row JSON without re-typing.** Turning a selected result cell into a `WHERE "col" = '...'` / `WHERE "col" IS NULL` predicate, turning a visible column into `"col" IN (...)`, or copying an entire row as compact JSON through the opt-in OSC 52 confirmation review currently requires manual formatting and risks escaping mistakes.
3. **Milestone documentation alignment.** M23, M24, and M25 milestones must be reflected consistently across roadmap, status, compatibility, changelog, and README documentation in Slice 2.

## Principles This Feature Must Keep

- **Principle I (Delight must remain truthful).** Quick-stats report only over the retained, currently visible rows. Numeric aggregates (`min`, `max`, `sum`, `avg`) are computed ONLY when `non_null > 0` and EVERY non-null cell parses as a finite `f64` (never silently skipping non-numeric values). Distinct counts above `MAX_QUICK_STATS_DISTINCT` (10,000) are marked capped (`+`).
- **Principle III (Local-first and private by default).** Quick-stats, `WHERE`/`IN (...)` SQL generation, and row JSON serialization run purely in memory with zero database queries. Row JSON clipboard copy strictly requires `[clipboard] osc52 = true`, explicit confirmation (`pending_copy`), and `MAX_OSC52_BYTES` (1 MiB) enforcement; nothing is ever written unconfirmed.
- **Principle IV (Safe by default).** `InsertCellWhereClause` and `InsertColumnInList` insert query-ready SQL into the active editor buffer without executing anything; identifiers are quoted via `quote_identifier`, SQL string literals escape single quotes cleanly, and NUL bytes or payloads exceeding 1 MiB are refused.
- **Principle V (Keyboard-first, never keyboard-only).** All four actions are configurable via `[keys]`, reachable from the Command Palette, and `ToggleColumnStats` is integrated into `Ctrl+K g` Result Grid Controls.
- **Principle VI (Accessible meaning survives styling loss).** The quick-stats bar uses `Theme`/`Token` semantics only, degrades cleanly in `Basic16`, `NO_COLOR`, and ASCII modes, and omits itself gracefully when inner pane height has no spare room.

## User Scenarios & Testing

### User Story 1 - Live Result-Grid Column Quick-Stats (Priority: P1, Slice 1 / PR 3A)

While navigating cells in the Results grid, an operator sees a 1-line summary bar at the bottom of the Results pane for the selected column over the currently visible/retained rows:
- Shows `col: N rows | M non-null | K null | D distinct` (or `10,000+ distinct` when capped).
- Appends `| min .. max | sum | avg ..` when every non-null cell parses cleanly as a finite `f64`.
- Can be toggled on/off with `Action::ToggleColumnStats` (`toggle-column-stats`) via the Command Palette, `[keys]`, or `Ctrl+K g` Result Grid Controls, and persists across local sort/column changes and new result sets in the session.

### User Story 2 - Query-Ready SQL Insertion and Reviewable Row JSON Copy (Priority: P1, Slice 1 / PR 3A)

With a retained result cell or column selected:
- `Action::InsertCellWhereClause` (`insert-where-clause`) inserts `WHERE "col" = 'escaped'` (or `WHERE "col" IS NULL`) into the active SQL editor as a single undo step, syncs `BufferManager`, sets a clear notice, and never executes SQL.
- `Action::InsertColumnInList` (`insert-in-list`) inserts `"col" IN ('a', 'b', ...)` over distinct non-NULL visible/retained values in encounter order (bounded at 200 values) as a single undo step, syncs `BufferManager`, reports truncation when capped at 200, refuses when zero non-NULL values exist, and never executes SQL.
- `Action::CopyRowAsJson` (`copy-row-json`) builds compact deterministic JSON distinguishing SQL `NULL` (`null`) from `""` and routes through the existing `pending_copy` OSC 52 review gate (`[clipboard] osc52 = true` and 1 MiB limit).

### User Story 3 - M23-M25 Documentation & Roadmap Sync (Priority: P2, Slice 2 / PR 3B)

Reconcile `docs/product/roadmap.md`, `docs/status.md`, `docs/support/compatibility.md`, `CHANGELOG.md`, `README.md`, and `tests/documentation_matches_the_build.rs` for M23, M24, and M25.

## Functional Requirements

### Slice 1 - Quick-Stats & Query-Ready SQL/JSON Copy (PR 3A)

- **FR-001**: Implement `MAX_QUICK_STATS_DISTINCT = 10_000`, `NumericQuickStats`, `ColumnQuickStats::compute`, `GridCommand::ToggleColumnStats`, and `ResultGridState::show_quick_stats` (default `true`) in `src/app/grid.rs`.
- **FR-002**: Render the 1-line quick-stats summary bar at the bottom of the Results pane in `src/ui/widgets/results.rs` (synced with `grid_capacity` and `results_region` hit-testing) using `Theme`/`Token` only, preserving compact/minimum pane rendering.
- **FR-003**: Implement `src/query/copy_sql.rs` with `build_where_clause`, `build_in_list`, and `build_row_json`, enforcing NUL-byte rejection, distinct-value capping (`200`), and 1 MiB bounds.
- **FR-004**: Wire `ToggleColumnStats`, `InsertCellWhereClause`, `InsertColumnInList`, and `CopyRowAsJson` into `Action`, `Action::all()`, `CONFIGURABLE`, `discovery.rs`, `open_palette`, `grid_control_entries`, `update.rs`, `review.rs`, and `docs/design/keymap.md`.

### Slice 2 - M23-M25 Docs & Roadmap Sync (PR 3B)

- **FR-005**: Update `docs/product/roadmap.md`, `docs/status.md`, `docs/support/compatibility.md`, `CHANGELOG.md`, and `README.md` with M23-M25 delivery evidence and contract tests.

## Non-Functional Requirements

- **NFR-001 (UI Token Discipline)**: Zero raw `Color::Rgb(` or `Color::Indexed(` literals in `src/ui/widgets/`.
- **NFR-002 (Repository Hygiene)**: Sidecar `tests.rs` files only, `AGENTS.md == CLAUDE.md`, zero Unicode em/en dashes (`\u{2014}` / `\u{2013}`) in prose.
- **NFR-003 (Verification)**: Passes `cargo --locked xtask verify`.
