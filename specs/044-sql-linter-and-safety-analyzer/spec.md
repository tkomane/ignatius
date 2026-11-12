# Feature Specification: SQL Linter and Safety Analyzer (044)

## 1. Context and Problem Statement

Ignatius already guards writes to production at execution time, but a
migration or query file can reach a release pipeline without anyone reading it.
Reviewers need a deterministic, offline check that names destructive or
unreviewable SQL before it runs. The same findings should be visible while the
query is still in the editor.

This feature adds a pure SQL linter, a scriptable `ignatius lint` command and a
Results-style findings overlay. It never connects to a database and never
rewrites the SQL.

## 2. User Scenarios

### Scenario 1: A migration file fails CI

An engineer runs `ignatius lint migrations/ --fail-on error --format sarif
--output lint.sarif`. The report names `DELETE FROM orders;` at line 14 as
`delete-without-where` with error severity, the command exits 1, and CI blocks
the merge.

### Scenario 2: A reviewer checks the open buffer

The reviewer presses `Ctrl+K !`. A findings overlay lists every problem in the
active buffer with line and column. `Enter` moves the editor caret to the
finding. `r` re-lints after an edit. `Esc` closes the overlay.

### Scenario 3: The rule set is honest about what it cannot know

A statement uses a CTE that may filter rows. The linter reports a warning, not
an error, and says it cannot prove the predicate is absent. It does not guess
that a query is safe.

## 3. Functional Requirements

- FR-4401: Lint a single SQL file, standard input, or a directory tree of
  `.sql` files. Directory traversal is bounded to 1000 files and each file to
  1 MiB; larger inputs are refused with a clear diagnostic.
- FR-4402: Split input with the existing statement splitter and mask string
  literals, quoted identifiers, dollar bodies and comments before matching, so
  text inside them cannot trigger a rule.
- FR-4403: Rules, each with a stable code and severity:
  - `delete-without-where` (error)
  - `update-without-where` (error)
  - `truncate-table` (error)
  - `drop-object` (warning)
  - `grant-all` (warning)
  - `create-index-without-concurrently` (warning)
  - `reindex-without-concurrently` (warning)
  - `vacuum-full` (warning)
  - `select-star` (info)
  - `leading-wildcard-like` (warning)
  - `non-sargable-predicate` (warning)
  - `implicit-cross-join` (warning)
  - `missing-transaction` (info; file level when DDL appears outside an
    explicit transaction)
- FR-4404: Every finding carries the rule code, severity, one-line message,
  statement index, line and column, and a bounded excerpt. It never prints the
  whole file.
- FR-4405: `ignatius lint [PATH...]` reads files sequentially; `-` reads
  standard input. `--format table|json|sarif` defaults to table.
- FR-4406: `--min-severity info|warning|error` filters the report;
  `--fail-on never|info|warning|error` defaults to `error` and exit code 1
  means findings met the threshold. A clean run exits 0.
- FR-4407: `--output PATH` writes atomically and refuses an existing file
  unless `--force` is given.
- FR-4408: The interactive overlay (`Ctrl+K !`, bindable name `lint-sql`)
  lints only the active buffer, is presentation-only, never executes SQL,
  never writes files, and maps `Enter` to the existing editor-position path.
- FR-4409: Findings are deterministic for the same input and rule version.
- FR-4410: The rule catalogue is documented in `docs/design/lint-rules.md` and
  compared against the build by the documentation contract.

## 4. Non-Functional Requirements

- Pure `src/lint/` domain with no driver or process calls.
- ASCII output and stable ordering by line, column, rule code.
- Bounded excerpt length; no terminal control characters from source text.
- JSON and SARIF output are machine-only: no decoration, no colour.
- The linter never opens a connection, reads a profile or touches history.

## 5. Out of Scope

- Semantic analysis that requires a live catalog or server.
- Automatic fixes or rewriting SQL.
- A full PostgreSQL grammar. The rules are lexical and deliberately
  conservative; `missing-transaction` is advisory.
