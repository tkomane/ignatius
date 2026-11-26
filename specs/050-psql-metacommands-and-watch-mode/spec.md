# Feature Specification: psql Meta-Commands and Bounded Watch Mode (050)

**Feature Branch**: `feat/m12-psql-and-cluster-parity`
**Created**: 2026-11-18
**Status**: Draft
**Authority**: Roadmap Milestone 12 (M12: Complete psql & Server/Replication Parity)

## 1. Context and Problem Statement

PostgreSQL operators moving between `psql` and Ignatius expect muscle-memory
backslash catalog commands (`\d`, `\dt`, `\d+`, `\du`, `\l+`, `\conninfo`),
session toggles (`\x`, `\timing`), built-in help (`\?`), and periodic query
refresh (`\watch [N]`). Without direct meta-command support in the SQL buffer,
users must hand-write `pg_catalog` queries or leave the client for routine
inspection.

At the same time, Ignatius cannot inherit `psql` pitfalls: unescaped pattern
inputs must never allow SQL injection, catalog queries must stay bounded, watch
loops must have explicit interval and iteration bounds, and shell escapes (`\!`)
or unsupported file-side `psql` commands (`\i`, `\copy`, `\set`) must fail
loudly with actionable Ignatius alternatives rather than running arbitrary
programs or silently misbehaving.

## 2. User Scenarios

### Scenario 1: Inspecting tables and relations with backslash commands

An operator types `\dt` or `\d+ billing.orders` in the query buffer and runs it.
Ignatius translates the command into a bounded, read-only `pg_catalog` query
that returns schema, relation, column types, nullability, defaults, sizes,
indexes, and comments without leaving the workbench.

### Scenario 2: Watching a live diagnostic query safely

An operator writes `SELECT state, count(*) FROM pg_stat_activity GROUP BY 1;\n\watch 5`
and executes the buffer. Ignatius runs the SQL once immediately and schedules a
bounded watch loop every 5 seconds (up to `MAX_WATCH_ITERATIONS`), stopping
when the user runs `\watch stop`, `\watch off`, or cancels execution.

A watch belongs to the connection that supplied its SQL. An accepted profile
switch stops the old watch. After the switch, standalone `\watch` requires
SQL from a completed execution on the current connection.

### Scenario 3: Refusing shell escapes and unsafe intervals truthfully

An operator enters `\! rm -rf /` or `\watch 0`. Ignatius refuses the input
before any execution or process spawn with a `Usage` diagnostic explaining why
it was refused and pointing to `\?`, `ignatius query -f`, or `ignatius import`.

## 3. Functional Requirements

- FR-5001: Support read-only catalog meta-commands `\d`, `\d+`, `\dt`, `\dv`,
  `\dm`, `\di`, `\ds`, `\dn`, `\du`, `\dg`, `\df`, `\dx`, `\l`, `\l+`, and
  `\conninfo` (plus `+` detail variants where applicable).
- FR-5002: Support optional `[pattern]` arguments (including `schema.name`
  splitting and double-quoted identifiers), converting glob `*` to `%` and `?`
  to `_` inside SQL `LIKE ... ESCAPE '!'` string literals while escaping SQL
  single quotes (`'` to `''`) and literal `LIKE` special characters (`!`, `%`,
  `_`).
- FR-5003: Bound every synthesized catalog query with `LIMIT 500` and qualify
  system relations and functions with `pg_catalog.`.
- FR-5004: Support `\d <relation>` and `\d+ <relation>` column inspection over
  `pg_attribute`, `pg_type`, `pg_attrdef`, `NOT NULL`, and column descriptions
  (with storage, relation size, and index summary in `\d+`).
- FR-5005: Support session toggle commands `\x [on|off]` and `\timing [on|off]`
  producing typed toggle or explicit boolean intents.
- FR-5006: Support `\?` and `\h` producing a static, in-memory `ResultSet` help
  table listing supported meta-commands and syntax.
- FR-5007: Support standalone `\watch [N]`, companion watch syntax
  (`<SQL>\n\watch [N]` or `<SQL> \watch [N]`), and `\watch stop` / `\watch off`,
  defaulting to `DEFAULT_WATCH_INTERVAL_SECS = 2`, enforcing
  `1 <= interval_secs <= MAX_WATCH_INTERVAL_SECS` (`3600`), and capping
  iterations at `MAX_WATCH_ITERATIONS` (`1000`).
- FR-5008: Never misparse backslashes inside PostgreSQL single-quoted strings
  (`'...'`, `E'...'`), double-quoted identifiers (`"..."`), dollar-quoted
  bodies (`$tag$...$tag$`), line comments (`-- ...`), or nested block comments
  (`/* ... */`).
- FR-5009: Refuse shell escapes (`\! ...`) and unsupported `psql` commands
  (`\i`, `\ir`, `\copy`, `\set`, `\echo`, `\o`, `\g`, `\q`, etc.) with a
  `DiagnosticKind::Usage` diagnostic naming supported alternatives
  (`ignatius query -f`, `ignatius import`, `\?`).
- FR-5010: Refuse nonsensical trailing tokens, invalid boolean arguments,
  unclosed double quotes, and invalid `\watch` intervals (`0`, negative,
  overflow, non-integer) with `DiagnosticKind::Usage`.

- FR-5011: An accepted connection switch MUST retire the watch timer and the
  previous result's reusable SQL together. A pending tick MUST emit no work
  from the retired watch. Opening or dismissing the picker leaves the watch
  available on the current connection.

## 4. Non-Functional Requirements

- Pure lexer and dispatcher in `src/metacommand/` with zero I/O, zero database
  handles, and zero terminal dependencies.
- All outputs, help text, diagnostics, and specifications use plain ASCII.
- Compatible with PostgreSQL 14 through 18 system catalogs.

## 5. Out of Scope

- Arbitrary shell execution (`\!`), client-side file includes (`\i`), client
  variable expansion (`\set`), and `\copy` (covered by `ignatius export` and
  `ignatius import`).
- Unbounded background polling without an explicit iteration ceiling.

## Watch error positions

A failure from standalone `\watch` or a timer tick keeps the server
diagnostic and failed history outcome. A reported position belongs to the
watched query. Display that association and preserve the current editor
caret and focus. The first execution of literal SQL submitted alongside
`\watch` maps to that editor submission while the original buffer and
revision are current, using the server statement ordinal when applicable.
Ordinary editor queries retain their normal position mapping even when a
watch timer is active. Replacing the workspace through crash recovery
retires the prior editor association for a running query.

The watch waits while a named-parameter prompt or run confirmation owns
another submission. A later eligible tick resumes the watched query.
