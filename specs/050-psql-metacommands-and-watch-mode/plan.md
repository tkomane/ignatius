# Implementation Plan: psql Meta-Commands and Bounded Watch Mode (050)

## 1. Pure Domain Engine (`src/metacommand/`)

- `mod.rs`:
  - Constants:
    - `DEFAULT_WATCH_INTERVAL_SECS: u64 = 2`
    - `MAX_WATCH_INTERVAL_SECS: u64 = 3600`
    - `MAX_WATCH_ITERATIONS: u64 = 1000`
    - `CATALOG_ROW_LIMIT: usize = 500`
  - `MetaCommand` enum:
    - `CatalogQuery { command: String, title: String, sql: String }`
    - `ToggleExpanded(Option<bool>)`
    - `ToggleTiming(Option<bool>)`
    - `StartWatch { interval_secs: u64, companion_sql: Option<String> }`
    - `StopWatch`
    - `Help`
  - `contains_metacommand(input: &str) -> bool`:
    - Fast path for trimmed input starting with `\`, plus quote/comment-aware
      detection of a trailing `\watch` line.
  - `parse_input(input: &str) -> Option<Result<MetaCommand, Diagnostic>>`:
    - Standalone mode: parses backslash commands, strips a single trailing
      unquoted semicolon cleanly, rejects trailing garbage, refuses `\!` and
      unsupported `psql` commands with `DiagnosticKind::Usage`.
    - Companion watch mode: reuses `crate::query::statements` lexical skippers
      (`skip_single_quoted`, `skip_double_quoted`, `skip_dollar_quoted`,
      `skip_line_comment`, `skip_block_comment`) so backslashes inside literals,
      identifiers, dollar quotes, or comments are never treated as `\watch`.
  - Pattern compiler:
    - Tokenizes optional single pattern argument respecting `"..."` double-quoted
      identifiers and unquoted `.` schema separators.
    - Converts `*` -> `%` and `?` -> `_`, escapes `!`, `%`, `_`, and `'`, and
      emits `LIKE '...' ESCAPE '!'` SQL literals.
  - `help_result() -> ResultSet`:
    - Builds an in-memory `ResultSet` documenting every supported meta-command.

## 2. Application, Session, and UI Wiring (Coordinator Integration)

- Register `pub mod metacommand;` in `src/lib.rs`.
- Dispatch `contains_metacommand` / `parse_input` from the query submission path
  in `src/app/`:
  - `CatalogQuery`: run synthesized read-only SQL through the normal execution
    path and present with the command title.
  - `ToggleExpanded` / `ToggleTiming`: update session display state.
  - `Help`: display `help_result()` without a database round trip.
  - `StartWatch` / `StopWatch`: manage bounded timer ticks up to
    `MAX_WATCH_ITERATIONS` (`1000`) and stop cleanly on user cancel, `\watch stop`,
    or error.

### Retire the watch when a profile switch is accepted

Clear the active watch and the prior result's SQL in the existing connection
transition. Keep the busy-state refusal ahead of the reset. The runtime already
clears its deadline when the model holds no watch. Exercise a pending old tick,
an attempted bare watch before a new execution, a new local query and a rejected
switch. Drive the ordinary picker and watch commands in an owned PTY against
two disposable database profiles.

## 3. Verification and Documentation

- `src/metacommand/tests.rs`: sidecar unit tests covering every catalog command,
  `\d` and `\d+` relation inspection, optional and schema-qualified patterns,
  SQL injection safety (`O'Reilly; DROP TABLE x; --`), quote/comment masking for
  `\watch`, valid and invalid watch intervals (`0`, negative, overflow), `\x`,
  `\timing`, `\?`/`\h`, and explicit refusal of `\!` and unsupported commands.
- Integration tests and documentation updates once wired into `src/app/` and
  `src/cli/`.

## W02 retain the source of watched SQL

Carry the origin with the submitted job across any interactive confirmation
or parameter prompt. Standalone watch and periodic ticks carry the watched
query origin. Literal companion SQL carries the submitting editor span
and identity. Use the existing diagnostic and history paths and stop the
watch on a failed iteration.

Verify standalone and timer failures, a positioned failure in the second
statement of the initial companion SQL, and an ordinary query with a
watch armed. Exercise different tabs at the same revision, workspace
replacement through crash recovery, and timer deferral for a parameterized
refresh and a run confirmation. Reproduce a failing tick through an
owned terminal and disposable PostgreSQL. Run the full local, PR and
post-merge gates.
