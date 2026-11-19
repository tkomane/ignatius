# Tasks: Interactive SQL Notebook Runner (058)

- [x] T001: Author `specs/058-interactive-sql-notebook-runner/spec.md`, `plan.md`, and `tasks.md`.
- [x] T002: Implement `src/runbook/mod.rs` pure Markdown/buffer SQL notebook parsers (`parse_markdown_runbook`, `parse_buffer_runbook`), classification via `crate::query::classify_all`, dry-run planner (`plan_dry_run`), and report summarizer (`summarize_report`).
- [x] T003: Implement `RunbookReport::{render_table, render_plain, render_json, render_markdown}` in `src/runbook/mod.rs` with strictly plain-ASCII formatting, secret redaction, and control-character sanitization.
- [x] T004: Implement `src/runbook/tests.rs` sidecar unit tests covering Markdown fences/headings/directives, buffer `-- @cell` and statement splitting, write-guard dry-run planning, truncation/size limits, and all four renderers.
- [x] T005: Coordinator wiring in `src/lib.rs`, `src/postgres/*`, `src/cli/*`, `src/app/*`, and `src/ui/*`.
- [x] T006: Run `cargo xtask verify` and live PostgreSQL contract tests once wired.
