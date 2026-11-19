//! Bounded read-only catalog reads for server config, HBA rules, extensions,
//! tablespaces, and foreign data wrappers.
//!
//! Queries target `pg_catalog` views and tables only, use bound `LIMIT`
//! parameters to cap memory and wire transfer, mask sensitive option values at
//! extraction time, and remain compatible across PostgreSQL 14 through 18.

use std::collections::HashMap;

use tokio_postgres::Client;
use tokio_postgres::error::SqlState;

use crate::cluster::{
    AvailableExtension, ClusterReport, FdwEntry, ForeignServerEntry, HbaRule, MAX_EXTENSIONS,
    MAX_FDWS, MAX_FOREIGN_SERVERS, MAX_HBA_RULES, MAX_SETTINGS, MAX_TABLESPACES, SettingEntry,
    TablespaceEntry, mask_options,
};
use crate::diagnostics::{Diagnostic, DiagnosticKind};
use crate::postgres::error::from_query_error;

const SETTINGS_SQL: &str = r#"
SELECT
    name::text AS name,
    COALESCE(setting::text, '') AS setting,
    unit::text AS unit,
    COALESCE(category::text, '') AS category,
    COALESCE(context::text, '') AS context,
    COALESCE(source::text, 'default') AS source,
    boot_val::text AS boot_val,
    reset_val::text AS reset_val,
    COALESCE(pending_restart, false) AS pending_restart,
    (COALESCE(source, 'default') <> 'default') AS is_non_default
FROM pg_catalog.pg_settings
ORDER BY
    COALESCE(pending_restart, false) DESC,
    (COALESCE(source, 'default') <> 'default') DESC,
    name::text ASC
LIMIT $1
"#;

const HBA_SQL: &str = r#"
SELECT
    line_number,
    COALESCE(type::text, '') AS rule_type,
    COALESCE(database, ARRAY[]::text[]) AS databases,
    COALESCE(user_name, ARRAY[]::text[]) AS users,
    CASE
        WHEN address IS NULL THEN NULL
        WHEN netmask IS NULL OR netmask = '' THEN address::text
        ELSE address::text || '/' || netmask::text
    END AS address,
    COALESCE(auth_method::text, '') AS auth_method,
    error::text AS error
FROM pg_catalog.pg_hba_file_rules
ORDER BY line_number ASC NULLS LAST
LIMIT $1
"#;

const EXTENSIONS_SQL: &str = r#"
SELECT
    name::text AS name,
    default_version::text AS default_version,
    installed_version::text AS installed_version,
    comment::text AS comment,
    (
        installed_version IS NOT NULL
        AND default_version IS NOT NULL
        AND installed_version <> default_version
    ) AS upgrade_available
FROM pg_catalog.pg_available_extensions
ORDER BY (installed_version IS NOT NULL) DESC, name::text ASC
LIMIT $1
"#;

const TABLESPACES_SQL: &str = r#"
SELECT
    t.spcname::text AS name,
    pg_catalog.pg_get_userbyid(t.spcowner)::text AS owner,
    NULLIF(pg_catalog.pg_tablespace_location(t.oid), '')::text AS location,
    CASE
        WHEN pg_catalog.has_tablespace_privilege(t.oid, 'CREATE')
          OR pg_catalog.pg_has_role('pg_read_all_stats', 'USAGE')
        THEN pg_catalog.pg_tablespace_size(t.oid)::bigint
        ELSE NULL
    END AS size_bytes,
    COALESCE(t.spcoptions, ARRAY[]::text[]) AS options
FROM pg_catalog.pg_tablespace t
ORDER BY t.spcname::text ASC
LIMIT $1
"#;

const TABLESPACES_NO_SIZE_SQL: &str = r#"
SELECT
    t.spcname::text AS name,
    pg_catalog.pg_get_userbyid(t.spcowner)::text AS owner,
    NULLIF(pg_catalog.pg_tablespace_location(t.oid), '')::text AS location,
    NULL::bigint AS size_bytes,
    COALESCE(t.spcoptions, ARRAY[]::text[]) AS options
FROM pg_catalog.pg_tablespace t
ORDER BY t.spcname::text ASC
LIMIT $1
"#;

const FDWS_SQL: &str = r#"
SELECT
    w.oid AS fdw_oid,
    w.fdwname::text AS name,
    pg_catalog.pg_get_userbyid(w.fdwowner)::text AS owner,
    CASE
        WHEN w.fdwhandler <> 0 THEN w.fdwhandler::regproc::text
        ELSE NULL
    END AS handler,
    CASE
        WHEN w.fdwvalidator <> 0 THEN w.fdwvalidator::regproc::text
        ELSE NULL
    END AS validator
FROM pg_catalog.pg_foreign_data_wrapper w
ORDER BY w.fdwname::text ASC
LIMIT $1
"#;

const FOREIGN_SERVERS_SQL: &str = r#"
WITH fdws AS (
    SELECT oid, fdwname
    FROM pg_catalog.pg_foreign_data_wrapper
    ORDER BY fdwname::text ASC
    LIMIT $1
)
SELECT
    s.srvfdw AS fdw_oid,
    s.srvname::text AS name,
    pg_catalog.pg_get_userbyid(s.srvowner)::text AS owner,
    s.srvtype::text AS server_type,
    s.srvversion::text AS server_version,
    COALESCE(s.srvoptions, ARRAY[]::text[]) AS options
FROM pg_catalog.pg_foreign_server s
JOIN fdws f ON f.oid = s.srvfdw
ORDER BY f.fdwname::text ASC, s.srvname::text ASC
LIMIT $2
"#;

/// Reads a bounded snapshot of server settings, HBA rules, extensions,
/// tablespaces, and foreign data wrappers without modifying server state.
///
/// If the connected role lacks privileges to inspect `pg_hba_file_rules`
/// (`42501` or `42P01`), [`ClusterReport::hba_access_restricted`] is set to
/// `true` and the remaining catalog sections are still returned.
pub async fn cluster_report(client: &Client) -> Result<ClusterReport, Diagnostic> {
    let settings_limit = (MAX_SETTINGS + 1) as i64;
    let setting_rows = client
        .query(SETTINGS_SQL, &[&settings_limit])
        .await
        .map_err(|error| from_query_error(&error, 1))?;
    let truncated_settings = setting_rows.len() > MAX_SETTINGS;
    let mut settings = Vec::with_capacity(setting_rows.len().min(MAX_SETTINGS));
    for row in setting_rows.iter().take(MAX_SETTINGS) {
        settings.push(SettingEntry {
            name: get(row, "name")?,
            setting: get(row, "setting")?,
            unit: get(row, "unit")?,
            category: get(row, "category")?,
            context: get(row, "context")?,
            source: get(row, "source")?,
            boot_val: get(row, "boot_val")?,
            reset_val: get(row, "reset_val")?,
            pending_restart: get(row, "pending_restart")?,
            is_non_default: get(row, "is_non_default")?,
        });
    }

    let hba_limit = (MAX_HBA_RULES + 1) as i64;
    let mut hba_rules = Vec::new();
    let mut truncated_hba_rules = false;
    let mut hba_access_restricted = false;
    match client.query(HBA_SQL, &[&hba_limit]).await {
        Ok(hba_rows) => {
            truncated_hba_rules = hba_rows.len() > MAX_HBA_RULES;
            hba_rules.reserve(hba_rows.len().min(MAX_HBA_RULES));
            for row in hba_rows.iter().take(MAX_HBA_RULES) {
                hba_rules.push(HbaRule::new(
                    get(row, "line_number")?,
                    get(row, "rule_type")?,
                    get(row, "databases")?,
                    get(row, "users")?,
                    get(row, "address")?,
                    get(row, "auth_method")?,
                    get(row, "error")?,
                ));
            }
        }
        Err(error) if is_hba_restricted_error(&error) => {
            hba_access_restricted = true;
        }
        Err(error) => return Err(from_query_error(&error, 2)),
    }

    let ext_limit = (MAX_EXTENSIONS + 1) as i64;
    let ext_rows = client
        .query(EXTENSIONS_SQL, &[&ext_limit])
        .await
        .map_err(|error| from_query_error(&error, 3))?;
    let truncated_extensions = ext_rows.len() > MAX_EXTENSIONS;
    let mut extensions = Vec::with_capacity(ext_rows.len().min(MAX_EXTENSIONS));
    for row in ext_rows.iter().take(MAX_EXTENSIONS) {
        extensions.push(AvailableExtension {
            name: get(row, "name")?,
            default_version: get(row, "default_version")?,
            installed_version: get(row, "installed_version")?,
            comment: get(row, "comment")?,
            upgrade_available: get(row, "upgrade_available")?,
        });
    }

    let ts_limit = (MAX_TABLESPACES + 1) as i64;
    let ts_rows = match client.query(TABLESPACES_SQL, &[&ts_limit]).await {
        Ok(rows) => rows,
        Err(error) if is_insufficient_privilege(&error) => client
            .query(TABLESPACES_NO_SIZE_SQL, &[&ts_limit])
            .await
            .map_err(|fallback_err| from_query_error(&fallback_err, 4))?,
        Err(error) => return Err(from_query_error(&error, 4)),
    };
    let truncated_tablespaces = ts_rows.len() > MAX_TABLESPACES;
    let mut tablespaces = Vec::with_capacity(ts_rows.len().min(MAX_TABLESPACES));
    for row in ts_rows.iter().take(MAX_TABLESPACES) {
        let raw_options: Vec<String> = get(row, "options")?;
        tablespaces.push(TablespaceEntry {
            name: get(row, "name")?,
            owner: get(row, "owner")?,
            location: get(row, "location")?,
            size_bytes: get(row, "size_bytes")?,
            options: mask_options(&raw_options),
        });
    }

    let fdw_limit = (MAX_FDWS + 1) as i64;
    let fdw_rows = client
        .query(FDWS_SQL, &[&fdw_limit])
        .await
        .map_err(|error| from_query_error(&error, 5))?;
    let truncated_fdws = fdw_rows.len() > MAX_FDWS;
    let mut fdws = Vec::with_capacity(fdw_rows.len().min(MAX_FDWS));
    let mut index_by_oid: HashMap<u32, usize> = HashMap::new();
    for row in fdw_rows.iter().take(MAX_FDWS) {
        let oid: u32 = get(row, "fdw_oid")?;
        index_by_oid.insert(oid, fdws.len());
        fdws.push(FdwEntry {
            name: get(row, "name")?,
            owner: get(row, "owner")?,
            handler: get(row, "handler")?,
            validator: get(row, "validator")?,
            servers: Vec::new(),
        });
    }

    if !fdws.is_empty() {
        let server_limit = MAX_FOREIGN_SERVERS as i64;
        let server_rows = client
            .query(FOREIGN_SERVERS_SQL, &[&(MAX_FDWS as i64), &server_limit])
            .await
            .map_err(|error| from_query_error(&error, 6))?;
        for row in &server_rows {
            let fdw_oid: u32 = get(row, "fdw_oid")?;
            let Some(index) = index_by_oid.get(&fdw_oid).copied() else {
                continue;
            };
            let raw_options: Vec<String> = get(row, "options")?;
            fdws[index].servers.push(ForeignServerEntry {
                name: get(row, "name")?,
                owner: get(row, "owner")?,
                server_type: get(row, "server_type")?,
                server_version: get(row, "server_version")?,
                options: mask_options(&raw_options),
            });
        }
    }

    Ok(ClusterReport {
        settings,
        hba_rules,
        extensions,
        tablespaces,
        fdws,
        truncated_settings,
        truncated_hba_rules,
        truncated_extensions,
        truncated_tablespaces,
        truncated_fdws,
        hba_access_restricted,
    }
    .bounded())
}

fn is_insufficient_privilege(err: &tokio_postgres::Error) -> bool {
    err.as_db_error().is_some_and(|db| {
        *db.code() == SqlState::INSUFFICIENT_PRIVILEGE || db.code().code() == "42501"
    })
}

fn is_hba_restricted_error(err: &tokio_postgres::Error) -> bool {
    err.as_db_error().is_some_and(|db| {
        *db.code() == SqlState::INSUFFICIENT_PRIVILEGE
            || *db.code() == SqlState::UNDEFINED_TABLE
            || db.code().code() == "42501"
            || db.code().code() == "42P01"
    })
}

fn get<'a, T>(row: &'a tokio_postgres::Row, column: &str) -> Result<T, Diagnostic>
where
    T: tokio_postgres::types::FromSql<'a>,
{
    row.try_get(column).map_err(|error| {
        Diagnostic::new(
            DiagnosticKind::Internal,
            format!("cluster catalog column '{column}' had an unexpected type: {error}"),
            "reading the cluster catalog",
        )
        .next_action("report this with the server version and the cluster catalog output")
    })
}
