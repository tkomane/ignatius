# Implementation Plan: Server Config, HBA, and Infrastructure Catalog (052)

## 1. Pure Domain Model (`src/cluster/mod.rs`)

- **Bounds**:
  - `MAX_SETTINGS = 500`
  - `MAX_HBA_RULES = 500`
  - `MAX_EXTENSIONS = 500`
  - `MAX_TABLESPACES = 100`
  - `MAX_FDWS = 100`
  - `MAX_FOREIGN_SERVERS = 500`
- **Types**:
  - `ClusterSection`: `All`, `Settings`, `Hba`, `Extensions`, `Tablespaces`, `Fdw`
    with `ACCEPTED`, `label()`, and `from_str_loose()`.
  - `SettingEntry`: configuration GUC metadata, `pending_restart`, `is_non_default`,
    `display_value()`, and `matches_filter()`.
  - `HbaRisk`: `Ok`, `Warning`, `Critical` with `label()`.
  - `HbaRule`: HBA rule metadata with `classify_hba_risk(rule_type, auth_method, error)`.
  - `AvailableExtension`: extension availability and `upgrade_available`.
  - `TablespaceEntry`: tablespace name, owner, optional location, optional size,
    and redacted options.
  - `ForeignServerEntry` and `FdwEntry`: wrapper handler/validator and nested servers
    with redacted options.
  - `ClusterReport`: bounded slices, truncation flags, `hba_access_restricted`,
    deterministic `bounded()` sorting/truncation/redaction, summary counters,
    filtering helpers, ASCII `render_table()`, `render_plain()`, and redacted
    `render_json()`.
- **Reviewed SQL Generators** (exposed both as module functions and `ClusterReport`
  associated functions; never auto-executed):
  - `alter_system_sql(name, value)`
  - `reload_conf_sql()` -> `"SELECT pg_catalog.pg_reload_conf();"`
  - `create_extension_sql(name)`
  - `alter_extension_update_sql(name, version)`
  - `create_fdw_sql(fdw)`
  - `create_server_sql(server, fdw_name)`
  - `create_tablespace_sql(ts)`
- **Redaction**:
  - Reuse `crate::diagnostics::redaction::{REDACTED, redact_text}` in `mask_option()`
    and `mask_options()` for keys containing `password`, `passwd`, `secret`,
    `token`, `key`, `passphrase`, or `credential`.

## 2. Read-Only PostgreSQL Extractor (`src/postgres/cluster.rs`)

- `pub async fn cluster_report(client: &Client) -> Result<ClusterReport, Diagnostic>`:
  1. **Settings**: query `pg_catalog.pg_settings` ordered by
     `pending_restart DESC, (source <> 'default') DESC, name ASC LIMIT $1`
     using `MAX_SETTINGS + 1`.
  2. **HBA rules**: query `pg_catalog.pg_hba_file_rules` ordered by
     `line_number ASC NULLS LAST LIMIT $1` using `MAX_HBA_RULES + 1`. Catch
     SQLSTATE `42501` (`INSUFFICIENT_PRIVILEGE`) and `42P01` (`UNDEFINED_TABLE`)
     gracefully and set `hba_access_restricted = true`.
  3. **Extensions**: query `pg_catalog.pg_available_extensions` ordered by
     `(installed_version IS NOT NULL) DESC, name ASC LIMIT $1` using
     `MAX_EXTENSIONS + 1`.
  4. **Tablespaces**: query `pg_catalog.pg_tablespace` with
     `pg_tablespace_location(oid)` and guarded `pg_tablespace_size(oid)` using
     `has_tablespace_privilege(t.oid, 'CREATE') OR pg_has_role('pg_read_all_stats', 'USAGE')`,
     with a fallback query omitting `pg_tablespace_size` if `42501` is raised.
  5. **FDWs & Foreign Servers**: query `pg_catalog.pg_foreign_data_wrapper`
     (`MAX_FDWS + 1`) and `pg_catalog.pg_foreign_server` (`MAX_FOREIGN_SERVERS + 1`),
     masking all option values via `mask_options()`.
- All queries use extended protocol with bound `LIMIT` parameters and explicit
  `::text` casts compatible with PostgreSQL 14 through 18.

## 3. Sidecar Tests (`src/cluster/tests.rs`)

- Cover HBA risk classification (`Critical` parse error, `Critical` remote
  `trust`, `Warning` local `trust`/`password`/`md5`, `Ok` `scram-sha-256`/`peer`/`cert`).
- Cover `ClusterReport` bounds and truncation booleans, deterministic ordering,
  filtering (`modified_only` and `filter_text`), summary counters, and
  `hba_access_restricted` rendering.
- Cover ASCII `render_table`, `render_plain`, and `render_json` output across
  all `ClusterSection` variants.
- Cover all 7 reviewed SQL generators including identifier/literal quoting and
  dot-separated GUC names.
- Cover option redaction across extraction/sanitization, table/plain rendering,
  JSON serialization, and `create_server_sql`/`create_tablespace_sql`.

## 4. Coordinator Integration (Deferred to Coordinator)

- Export `pub mod cluster;` in `src/lib.rs` and `pub mod cluster;` in
  `src/postgres/mod.rs`.
- Wire `Session::cluster_report`, CLI subcommand, and TUI overlay in coordinator
  follow-up.
