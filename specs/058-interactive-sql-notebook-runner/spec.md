# Feature Specification: Interactive SQL Notebook Runner (058)

**Feature Branch**: `feat/m14-bench-snapshot-runbook-polish`
**Created**: 2026-11-19
**Status**: Phase 1 Implemented (Pure Library Module and Sidecar Tests)
**Authority**: Roadmap Milestone 14 (M14: Benchmarks, Schema Snapshots, SQL Notebook Runner and Polish)

## 1. Context and Problem Statement

Engineers maintain PostgreSQL diagnostic playbooks, incident runbooks, and
migration verification checklists as Markdown files with fenced `sql` blocks, as
well as multi-step scratchpads in editor buffers separated by `-- @cell`
markers. Running steps one by one by copying and pasting into a terminal is
error-prone, loses structure between headings and statements, and risks running
unintended write or destructive SQL without an explicit guard.

Feature 058 introduces a pure, local-first SQL notebook parser, write-guard
planner, and multi-format report renderer in `src/runbook/mod.rs`:

1. Markdown runbook parsing (`parse_markdown_runbook`) that tracks active
   headings (`#`, `##`, `###`), extracts fenced SQL blocks (` ```sql `,
   ` ```postgresql `, ` ~~~sql `), parses cell directives (`-- @name: <slug>`,
   `-- @readonly`, `-- @expect-rows: <N>`), ignores prose inline code spans and
   unclosed non-SQL fences, and classifies write/destructive risk via
   `crate::query::classify_all`.
2. SQL editor buffer parsing (`parse_buffer_runbook`) that splits cells by
   top-level `-- @cell <name>` markers (or individual statements via
   `crate::query::statements::split` when no markers exist) with exact 1-based
   `start_line` tracking.
3. Safe-by-default dry-run planning (`plan_dry_run`) that blocks write,
   structural, and destructive cells (`BlockedByWriteGuard`) unless writes are
   explicitly enabled (`allow_writes = true`), and supports single-cell scoping
   via `selected_cell`.
4. Deterministic execution summarization (`summarize_report`) and strictly
   plain-ASCII `table`, `plain`, `json`, and `markdown` report renderers.

Consistent with Constitution IV, heuristic SQL classification via
`crate::query::classify_all` is an advisory safety guardrail and never a
security boundary; PostgreSQL permissions and transaction settings remain the
authoritative controls.

## 2. User Scenarios

### Scenario 1: Parsing a Markdown incident runbook with directives and headings

An operator loads a Markdown runbook containing prose explanations, inline code
spans (such as ```` ```sql SELECT 1;``` ````), non-SQL fenced blocks (including
unclosed example blocks), and fenced ```` ```sql ````, ```` ```postgresql ````,
or ```` ~~~sql ```` cells under `#` / `##` / `###` headings.

`parse_markdown_runbook` enforces the `MAX_RUNBOOK_BYTES` (1 MiB) input limit
with an explicit `DiagnosticKind::Usage` error, refuses empty or SQL-free
documents and unclosed SQL fences with `DiagnosticKind::Usage`, bounds retained
cells at `MAX_RUNBOOK_CELLS` (200) while setting `SqlRunbook.truncated = true`,
and classifies each cell using `crate::query::classify_all` over
`crate::query::statements::split`.

### Scenario 2: Splitting an active SQL buffer into cells

Inside the editor or from a `.sql` script, an operator writes `-- @cell <name>`
markers or bare multi-statement SQL. `parse_buffer_runbook` uses lexical token
scanning (`crate::query::highlight::tokens`) so `-- @cell` text inside string
literals, dollar-quoted PL/pgSQL blocks, or block comments never splits a cell.
When no `-- @cell` markers are present, it delegates to `statements::split` and
records exact 1-based `start_line` values for every cell.

### Scenario 3: Dry-run planning with default write guard

Before touching a live database, the operator runs a dry-run plan
(`plan_dry_run(&runbook, false, None)`). Read-only cells become
`CellStatus::Planned`, whereas any cell whose `is_write` flag is `true`
(covering writes, structural DDL, destructive operations, `DO`/`CALL`, and
unrecognized statements) becomes `CellStatus::BlockedByWriteGuard` with an
explicit diagnostic message. When `selected_cell` is supplied, all other cells
become `CellStatus::Skipped`.

### Scenario 4: Summarizing and rendering strictly plain-ASCII reports

After planning or Phase 2 execution, `summarize_report` tallies `passed_count`,
`failed_count`, `blocked_count`, `skipped_count`, and `total_duration_ms`, and
exposes `has_failures()`. `RunbookReport::{render_table, render_plain,
render_json, render_markdown}` format deterministic, secret-redacted,
control-character-sanitized, strictly plain-ASCII outputs suitable for terminals,
pipes, CI logs, and Markdown artifacts.

## 3. Functional Requirements

- **FR-5801**: Define `MAX_RUNBOOK_CELLS = 200` and
  `MAX_RUNBOOK_BYTES = 1024 * 1024` in `src/runbook/mod.rs`.
- **FR-5802**: Define `RunbookCell`, `SqlRunbook`, `CellStatus` (`Planned`,
  `Passed`, `Failed`, `Skipped`, `BlockedByWriteGuard`), `CellExecutionRecord`,
  and `RunbookReport` deriving `Debug, Clone, PartialEq, Eq, Serialize,
  Deserialize`.
- **FR-5803**: `parse_markdown_runbook(markdown: &str, title: &str) -> Result<SqlRunbook, Diagnostic>`:
  - Refuses inputs larger than `MAX_RUNBOOK_BYTES` with `DiagnosticKind::Usage`.
  - Refuses unclosed SQL fences and runbooks containing zero executable SQL
    cells with `DiagnosticKind::Usage`.
  - Caps retained cells at `MAX_RUNBOOK_CELLS` and sets `truncated = true` when
    more executable SQL cells follow.
  - Tracks current Markdown heading (`#`, `##`, `###`, up to `######`) outside
    fenced blocks.
  - Extracts fenced `sql`, `postgresql`, `postgres`, and `pgsql` blocks using
    backtick (```` ``` ````) or tilde (`~~~`) fences while ignoring inline prose
    code spans and unclosed non-SQL fences safely.
  - Extracts `-- @name: <slug>`, `-- @readonly`, and `-- @expect-rows: <N>`
    directives from top-level line comments inside each cell.
  - Classifies each cell via `crate::query::classify_all(&crate::query::statements::split(&sql))`,
    setting `is_write = impact.needs_confirmation()` and
    `is_destructive = impact.needs_typed_confirmation()`. Passing `-- @readonly`
    sets `readonly_hint = true` but never suppresses `is_write`.
- **FR-5804**: `parse_buffer_runbook(buffer_sql: &str, title: &str) -> SqlRunbook`
  splits cells at top-level `-- @cell <name>` markers (ignoring strings,
  dollar quotes, and block comments) or falls back to `statements::split`,
  recording 1-based `start_line` and bounding bytes/cells to `MAX_RUNBOOK_BYTES`
  and `MAX_RUNBOOK_CELLS` (`truncated = true` if exceeded).
- **FR-5805**: `plan_dry_run(runbook: &SqlRunbook, allow_writes: bool, selected_cell: Option<&str>) -> RunbookReport`
  deterministically plans cells without database execution, marking unselected
  cells `Skipped`, write cells when `!allow_writes` as `BlockedByWriteGuard`,
  and runnable cells as `Planned`.
- **FR-5806**: `summarize_report(title: String, dry_run: bool, allow_writes: bool, records: Vec<CellExecutionRecord>) -> RunbookReport`
  aggregates status counts and saturated total duration, with
  `RunbookReport::has_failures(&self) -> bool` returning `true` when
  `failed_count > 0 || blocked_count > 0` alongside `has_execution_failures()`,
  `has_blocked_writes()`, and `planned_count()` helpers.
- **FR-5807**: `RunbookReport::{render_table, render_plain, render_json,
  render_markdown}` emit strictly plain-ASCII output with redacted secrets and
  escaped terminal control characters.

## 4. Non-Functional Requirements

- Pure domain module in `src/runbook/mod.rs` with zero filesystem, network,
  process, database, or terminal I/O.
- Zero `unsafe` code, zero new Cargo dependencies, and Rust 1.90+ (edition
  2024) compatibility.
- Strictly plain ASCII comments and output (normal `-` hyphens only).
- All unit tests placed in sidecar `src/runbook/tests.rs` (CodeQL rule).
- Clean under `rustfmt --edition 2024 --check`, `clippy --all-targets -- -D warnings`,
  and `RUSTDOCFLAGS="-D warnings" cargo doc`.

## 5. Out of Scope (Phase 1)

- Live PostgreSQL execution of runbook cells (`src/postgres/runbook.rs` or
  `Session` runner).
- CLI subcommand (`ignatius runbook`) and TUI notebook runner overlay
  (`src/cli/*`, `src/app/*`, `src/ui/*`) and `src/lib.rs` export wiring
  (owned by Phase 2 coordinator integration).
