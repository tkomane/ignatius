# Implementation Plan: Interactive SQL Notebook Runner (058)

## 1. Pure Domain Model, Parser, Planner and Renderers (`src/runbook/mod.rs`)

- **Bounds**:
  - `MAX_RUNBOOK_CELLS: usize = 200`
  - `MAX_RUNBOOK_BYTES: usize = 1024 * 1024`
- **Core Data Model** (`Debug, Clone, PartialEq, Eq, Serialize, Deserialize`):
  - `RunbookCell { index, name, heading, start_line, sql, is_write, is_destructive, readonly_hint, expect_rows }`
  - `SqlRunbook { title, cells, truncated }`
  - `CellStatus { Planned, Passed, Failed, Skipped, BlockedByWriteGuard }` (`#[serde(rename_all = "kebab-case")]`, `Copy`)
  - `CellExecutionRecord { index, name, heading, is_write, status, rows_returned, rows_affected, duration_ms, message }`
  - `RunbookReport { title, dry_run, allow_writes, cells, passed_count, failed_count, blocked_count, skipped_count, total_duration_ms }`
- **Markdown Parser (`parse_markdown_runbook`)**:
  - Rejects `markdown.len() > MAX_RUNBOOK_BYTES` with `DiagnosticKind::Usage`.
  - Strips optional UTF-8 BOM and scans line by line with `\r\n` normalization,
    tracking 1-based line numbers.
  - Outside fences: tracks ATX headings (`#`, `##`, `###`, up to `######`) and
    detects CommonMark opening fences (` ``` ` or `~~~`, length >= 3, up to 3
    leading spaces; backtick info strings containing `` ` `` are rejected so
    inline prose spans are ignored).
  - Extracts blocks whose language token (case-insensitive) is `sql`,
    `postgresql`, `postgres`, or `pgsql`. Ignores non-SQL fenced blocks
    (including unclosed non-SQL blocks at EOF) safely without updating heading
    state from code comments inside them.
  - Refuses unclosed SQL fenced blocks at EOF with `DiagnosticKind::Usage`.
  - Skips comment-only/whitespace-only SQL blocks; classifies executable blocks
    using `crate::query::statements::split` and `crate::query::classify_all`.
  - Extracts `-- @name: <slug>`, `-- @readonly`, and `-- @expect-rows: <N>`
    from top-level `TokenKind::Comment` lines using `crate::query::highlight::tokens`.
  - Caps cells at `MAX_RUNBOOK_CELLS` (`truncated = true` when extra executable
    cells exist) and refuses zero-cell documents with `DiagnosticKind::Usage`.
- **SQL Buffer Parser (`parse_buffer_runbook`)**:
  - Bounds input to `MAX_RUNBOOK_BYTES` on a UTF-8 char boundary (`truncated = true`
    if cut).
  - Scans top-level line comments via `crate::query::highlight::tokens` for
    `-- @cell <name>` markers, ignoring markers inside string literals, dollar
    quotes, or block comments.
  - Splits by `-- @cell` sections when present (capturing any non-empty SQL
    preamble before the first marker), or delegates to `statements::split` when
    no markers exist, preserving exact 1-based `start_line` and cell directives.
  - Caps retained cells at `MAX_RUNBOOK_CELLS` (`truncated = true`).
- **Dry-Run Planner and Summarizer**:
  - `plan_dry_run(runbook, allow_writes, selected_cell)` produces a
    `RunbookReport` with `dry_run = true`, flagging unselected cells `Skipped`,
    write cells under `!allow_writes` as `BlockedByWriteGuard`, and runnable
    cells as `Planned` (annotating `@readonly` hints that conflict with write SQL).
  - `summarize_report(title, dry_run, allow_writes, records)` redacts `title`
    and record messages, sums `duration_ms` with saturating addition, and
    tallies `passed_count`, `failed_count`, `blocked_count`, and `skipped_count`.
  - `RunbookReport::has_failures` returns `failed_count > 0 || blocked_count > 0`,
    paired with `has_execution_failures()`, `has_blocked_writes()`, and
    `planned_count()`.
- **Plain-ASCII Renderers**:
  - `render_table`: aligned plain-ASCII summary and cell table.
  - `render_plain`: line-oriented `key=value` output for scripts.
  - `render_json`: pretty JSON escaped to strict 7-bit ASCII (`\uXXXX`).
  - `render_markdown`: GitHub-Flavored Markdown table with pipe/control escaping
    and strictly plain ASCII text.

## 2. Sidecar Unit Tests (`src/runbook/tests.rs`)

- Boundary tests: `MAX_RUNBOOK_BYTES` rejection, `MAX_RUNBOOK_CELLS` cap and
  `truncated = true`, empty/prose-only refusal, unclosed SQL fence refusal.
- Markdown parsing: `#`/`##`/`###` headings, ` ```sql `, ` ```postgresql `,
  ` ~~~sql `, 4-backtick fences containing ` ``` `, inline code span ignoring,
  unclosed non-SQL fence ignoring, CRLF handling, and directives (`-- @name:`,
  `-- @readonly`, `-- @expect-rows:`).
- Classification: read vs write vs structural vs destructive statements, CTE
  writes (`WITH ... DELETE`), and `-- @readonly` never bypassing `is_write`.
- Buffer parsing: `-- @cell` markers, preamble before first marker, markerless
  `statements::split` fallback, `-- @cell` inside strings/dollar-quotes ignored,
  exact 1-based `start_line`.
- Dry-run planning, `summarize_report` counts/helpers, and strictly plain-ASCII
  checks + JSON round-trip across `render_table`, `render_plain`, `render_json`,
  and `render_markdown`.

## 3. Phase 2 Coordinator Integration (Deferred to Coordinator)

- Export `pub mod runbook;` in `src/lib.rs`.
- Add PostgreSQL cell executor, CLI `ignatius runbook` subcommand, and TUI
  notebook runner overlay in `src/postgres/*`, `src/cli/*`, `src/app/*`, and
  `src/ui/*`.
- Run `cargo xtask verify` and live PostgreSQL contract tests once wired.
