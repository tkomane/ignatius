//! Server operations for restore drills.
//!
//! These helpers create and drop the scratch database, count user relations and
//! measure its size. They are the only write paths in Feature 047, and every
//! name is validated by the caller and quoted here.

use crate::diagnostics::{Diagnostic, DiagnosticKind};
use crate::postgres::error::from_query_error;
use crate::query::identifiers::quote_identifier;
use tokio_postgres::Client;

/// Whether a database with this exact name already exists.
pub async fn database_exists(client: &Client, name: &str) -> Result<bool, Diagnostic> {
    let row = client
        .query_one(
            "SELECT EXISTS (SELECT 1 FROM pg_catalog.pg_database WHERE datname = $1) AS present",
            &[&name],
        )
        .await
        .map_err(|error| from_query_error(&error, 1))?;
    row.try_get("present").map_err(|error| {
        Diagnostic::new(
            DiagnosticKind::Internal,
            format!("database-existence probe returned an unexpected type: {error}"),
            "checking the scratch database",
        )
    })
}

/// Whether the connected role may create a database.
pub async fn can_create_database(client: &Client) -> Result<bool, Diagnostic> {
    let row = client
        .query_one(
            "SELECT (rolcreatedb OR rolsuper) AS allowed \
             FROM pg_catalog.pg_roles WHERE rolname = current_user",
            &[],
        )
        .await
        .map_err(|error| from_query_error(&error, 1))?;
    row.try_get("allowed").map_err(|error| {
        Diagnostic::new(
            DiagnosticKind::Internal,
            format!("CREATEDB probe returned an unexpected type: {error}"),
            "checking restore-drill privileges",
        )
    })
}

/// Creates an empty scratch database from `template0`.
pub async fn create_database(client: &Client, name: &str) -> Result<(), Diagnostic> {
    let sql = format!(
        "CREATE DATABASE {} TEMPLATE template0",
        quote_identifier(name)
    );
    client.batch_execute(&sql).await.map_err(|error| {
        Diagnostic::new(
            DiagnosticKind::Usage,
            format!("could not create the scratch database: {error}"),
            "creating the restore drill scratch database",
        )
        .likely_cause(
            "the connected role needs CREATEDB, or the name is already taken on this server",
        )
        .next_action("ask a superuser for CREATEDB or pass a different --scratch-database")
    })
}

/// Terminates connections to a scratch database and drops it.
pub async fn drop_database(client: &Client, name: &str) -> Result<(), Diagnostic> {
    client
        .query(
            "SELECT pg_catalog.pg_terminate_backend(pid) \
             FROM pg_catalog.pg_stat_activity \
             WHERE datname = $1 AND pid <> pg_catalog.pg_backend_pid()",
            &[&name],
        )
        .await
        .map_err(|error| from_query_error(&error, 1))?;
    let sql = format!("DROP DATABASE IF EXISTS {}", quote_identifier(name));
    client.batch_execute(&sql).await.map_err(|error| {
        Diagnostic::new(
            DiagnosticKind::Query,
            format!("could not drop the scratch database: {error}"),
            "dropping the restore drill scratch database",
        )
        .likely_cause(
            "another session connected to the scratch database while it was being dropped",
        )
        .next_action("re-run the drill with --keep, inspect the database, and drop it manually")
    })
}

/// Counts user relations in the current database.
///
/// The same filter runs against the source and the restored database so the
/// comparison is like for like. System schemas are excluded.
pub async fn user_relation_count(client: &Client) -> Result<u64, Diagnostic> {
    let row = client
        .query_one(
            "SELECT count(*)::bigint AS relations \
             FROM pg_catalog.pg_class c \
             JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
             WHERE c.relkind IN ('r', 'p', 'v', 'm', 'S', 'f') \
               AND n.nspname !~ '^pg_' AND n.nspname <> 'information_schema'",
            &[],
        )
        .await
        .map_err(|error| from_query_error(&error, 1))?;
    let count: i64 = row.try_get("relations").map_err(|error| {
        Diagnostic::new(
            DiagnosticKind::Internal,
            format!("relation-count probe returned an unexpected type: {error}"),
            "counting relations for the restore drill",
        )
    })?;
    Ok(u64::try_from(count).unwrap_or(0))
}

/// Size of the current database in bytes.
pub async fn database_size(client: &Client) -> Result<u64, Diagnostic> {
    let row = client
        .query_one(
            "SELECT pg_catalog.pg_database_size(pg_catalog.current_database())::bigint AS bytes",
            &[],
        )
        .await
        .map_err(|error| from_query_error(&error, 1))?;
    let bytes: i64 = row.try_get("bytes").map_err(|error| {
        Diagnostic::new(
            DiagnosticKind::Internal,
            format!("database-size probe returned an unexpected type: {error}"),
            "measuring the restored database",
        )
    })?;
    Ok(u64::try_from(bytes).unwrap_or(0))
}
