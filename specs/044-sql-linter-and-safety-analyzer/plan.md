# Implementation Plan: SQL Linter and Safety Analyzer (044)

## 1. Domain (`src/lint/`)

- `mod.rs`:
  - `LintSeverity` (`Info`, `Warning`, `Error`) with `rank()` and ordering.
  - `LintCode` enum with `as_str()`, `severity()`, `message()`.
  - `Finding` (`code`, `severity`, `message`, `statement_index`, `line`,
    `column`, `excerpt`).
  - `LintReport` (`findings`, `files`, `counts`), with `has_at_or_above()`.
  - `lint_sql(sql: &str) -> LintReport`.
  - `lint_file(path)`, `lint_paths(paths)` with bounds and deterministic order.
  - `render_table`, `render_json`, `render_sarif`.
- Masking: reuse `query::highlight::tokens` to blank out `Literal`, `Comment`
  and quoted `Identifier` spans before rule matching, preserving byte offsets.
- Statement splitting: reuse `query::statements::split`; each finding maps back
  to the statement and source line/column.
- Rules operate on uppercase-normalised masked text per statement. Avoid
  false positives with word-boundary matching.

## 2. CLI (`src/cli/`)

- Add `Command::Lint(LintArgs)` and `run_lint`.
- `LintArgs`: `paths: Vec<PathBuf>`, `format`, `min_severity`, `fail_on`,
  `output`, `force`.
- Exit code: add `ExitCode::Findings = 1` and document it in
  `docs/support/compatibility.md` and the README exit-code table.
- Refuse paths that are not regular files or directories; skip symlinks;
  sort directory entries for determinism.
- `-` reads stdin; at most one `-`.

## 3. TUI Integration

- `Action::LintBuffer`, `Model.lint_findings: Option<LintFindingsState>`.
- `Ctrl+K !` (`lint-sql`) is a configurable chord; no global key conflict.
- Overlay lists code, severity, line, column, message; `Enter` moves the
  editor caret through the existing `Action::Move`/error-location path;
  `r` re-lints; `Esc`/`q` dismiss. No SQL effect and no file write.
- Discovery: available with a non-empty active buffer; palette group
  `Files/History` or `Tools`.
- Renderer follows the existing hardware-capsule style, ASCII fallback.

## 4. Tests

- `src/lint/tests.rs`: one test per rule, protected-region immunity, line and
  column mapping, ordering, bounds, severity filter, SARIF/JSON shapes.
- `src/app/update/tests.rs`: overlay opens, Enter moves the caret, r re-lints,
  Esc dismisses, no `Effect::Execute`.
- `src/ui/widgets/*/tests.rs`: overlay rendering, narrow and ASCII forms.
- `tests/lint_contract.rs`: real binary over temp files, stdin, directory,
  fail-on thresholds, atomic output/`--force`, exit code 1 and 0.
- `tests/documentation_matches_the_build.rs`: rule catalogue parity.

## 5. Documentation

- README feature section and CLI block.
- `docs/design/lint-rules.md`: every rule, severity, why it matters, and its
  limits.
- Keymap table and bindable action list.
- CHANGELOG, status, roadmap.

## 6. Verification

- `cargo fmt --all -- --check`
- `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`
- focused lint, update and widget tests
- `cargo test --locked --test lint_contract`
- `cargo test --locked --test documentation_matches_the_build`
- `cargo xtask verify` with disposable plain and TLS PostgreSQL
