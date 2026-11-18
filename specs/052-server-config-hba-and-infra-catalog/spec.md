# Feature Specification: Server Config, HBA, and Infrastructure Catalog (052)

**Feature Branch**: `feat/m12-psql-and-cluster-parity`
**Created**: 2026-11-18
**Status**: In Progress
**Authority**: Roadmap Milestone 12 (M12: Complete psql & Server/Replication Parity)

## 1. Context and Problem Statement

Database administrators and operators routinely inspect server configuration,
host-based authentication rules, installed and available extensions,
tablespaces, and foreign data wrappers (FDWs) and foreign servers during
incidents, security audits, and environment setup. Without a built-in cluster
catalog surface, operators fall back to ad-hoc queries across `pg_settings`,
`pg_hba_file_rules`, `pg_available_extensions`, `pg_tablespace`,
`pg_foreign_data_wrapper`, and `pg_foreign_server`, often hitting permission
errors as non-superusers or accidentally printing raw credentials embedded in
foreign server options.

Feature 052 introduces a read-only, bounded cluster configuration and
infrastructure catalog domain model (`src/cluster/`) and PostgreSQL extractor
(`src/postgres/cluster.rs`), plus reviewed SQL generation helpers that are never
auto-executed.

## 2. User Scenarios

### Scenario 1: Reviewing non-default settings and pending restarts

An operator opens the cluster catalog filtered to modified settings
(`modified_only = true`). The report highlights settings whose `source` differs
from `'default'` and flags parameters with `pending_restart = true` so the
operator sees which changes require a server restart versus a configuration
reload (`SELECT pg_catalog.pg_reload_conf();`).

### Scenario 2: Auditing `pg_hba.conf` rules and handling restricted roles

A security reviewer inspects `pg_hba_file_rules` and immediately sees:
- `CRITICAL` for parse errors (`error` present) and network `trust` rules
  (`rule_type != "local"` and `auth_method == "trust"`),
- `WARNING` for local `trust`, cleartext `password`, and legacy `md5` rules,
- `OK` for `scram-sha-256`, `peer`, `cert`, and other authenticated methods.

When a non-superuser runs the catalog report and PostgreSQL denies access to
`pg_hba_file_rules` (`42501`), the report records `hba_access_restricted = true`
and renders an explicit notice while returning settings, extensions,
tablespaces, and FDWs normally.

### Scenario 3: Inspecting extensions, tablespaces, and FDWs safely

An operator inspects extensions needing upgrades (`upgrade_available = true`),
tablespace locations and privilege-guarded sizes, and FDW wrappers and foreign
servers. Any sensitive option key (`password`, `secret`, `token`, `key`,
`passphrase`, `credential`) in foreign server or tablespace options is masked
with `[redacted]` in all tables, JSON output, and generated DDL.

## 3. Functional Requirements

- FR-5201: `src/cluster/mod.rs` provides a pure, zero-IO domain model with
  bounded capacity constants: `MAX_SETTINGS = 500`, `MAX_HBA_RULES = 500`,
  `MAX_EXTENSIONS = 500`, `MAX_TABLESPACES = 100`, `MAX_FDWS = 100`, and
  `MAX_FOREIGN_SERVERS = 500`.
- FR-5202: `ClusterSection` enumerates `All`, `Settings`, `Hba`, `Extensions`,
  `Tablespaces`, and `Fdw` with case-insensitive string parsing.
- FR-5203: `SettingEntry` models `name`, `setting`, `unit`, `category`,
  `context`, `source`, `boot_val`, `reset_val`, `pending_restart`, and
  `is_non_default`.
- FR-5204: `HbaRule` and `HbaRisk` (`Ok`, `Warning`, `Critical`) classify HBA
  rules deterministically: `Critical` when `error.is_some()` or
  (`auth_method == "trust"` and `rule_type != "local"`); `Warning` for local
  `trust`, `password`, or `md5`; `Ok` otherwise.
- FR-5205: `AvailableExtension` models `name`, `default_version`,
  `installed_version`, `comment`, and `upgrade_available`.
- FR-5206: `TablespaceEntry` models `name`, `owner`, `location`, `size_bytes`,
  and `options`.
- FR-5207: `FdwEntry` and `ForeignServerEntry` model wrappers, handlers,
  validators, and attached servers with their type, version, and options.
- FR-5208: `ClusterReport` tracks all five bounded slices, per-section
  truncation booleans, `hba_access_restricted`, filtering (`modified_only`,
  `filter_text`), summary counters (`pending_restart_count()`,
  `non_default_settings_count()`, `hba_warnings_count()`,
  `upgradable_extensions_count()`), and deterministic ASCII table, plain-text,
  and JSON renderers (`render_table`, `render_plain`, `render_json`).
- FR-5209: Reviewed SQL generators (`alter_system_sql`, `reload_conf_sql`,
  `create_extension_sql`, `alter_extension_update_sql`, `create_fdw_sql`,
  `create_server_sql`, `create_tablespace_sql`) produce properly quoted,
  reviewable PostgreSQL DDL/commands and NEVER execute them automatically.
- FR-5210: Sensitive FDW server and tablespace option values (`password`,
  `secret`, `token`, `key`, etc.) are masked using
  `crate::diagnostics::redaction::REDACTED` and `redact_text` across catalog
  extraction, rendering, JSON serialization, and DDL generation.
- FR-5211: `src/postgres/cluster.rs` executes bounded, read-only catalog
  queries compatible with PostgreSQL 14 through 18, handling `42501`/`42P01` on
  `pg_hba_file_rules` and guarding `pg_tablespace_size(oid)` via privilege
  checks (`has_tablespace_privilege` / `pg_has_role('pg_read_all_stats', 'USAGE')`).

## 4. Non-Functional Requirements

- Pure `src/cluster/` domain with zero driver, terminal, filesystem, or network
  dependencies.
- Plain ASCII output only; no Unicode box drawing or non-ASCII characters.
- No credentials, connection strings, or raw secrets in any output or SQL preview.
- Zero clippy warnings (`#[warn(clippy::all)]`) and sidecar `src/cluster/tests.rs`.

## 5. Out of Scope

- Automatic execution of `ALTER SYSTEM`, `pg_reload_conf()`, `CREATE EXTENSION`,
  `ALTER EXTENSION`, `CREATE FOREIGN DATA WRAPPER`, `CREATE SERVER`, or
  `CREATE TABLESPACE`.
- Reading `pg_user_mapping` / `pg_user_mappings` credential options.
- Direct editing of `postgresql.conf` or `pg_hba.conf` files on disk.
