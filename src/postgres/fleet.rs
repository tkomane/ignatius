//! PostgreSQL 14-18 catalog queries for the fleet report.

use std::collections::BTreeMap;
use std::future::Future;

use tokio_postgres::{Client, Row};

use crate::fleet::{Health, Issue, MAX_EXTENSIONS, Observation, SETTINGS, Setting, TlsObservation};
use crate::postgres::error::from_query_error;
use crate::postgres::session::SessionInfo;
use crate::postgres::tls::TlsState;
use crate::snapshot::SchemaSnapshot;

const HEALTH_SQL: &str = "
SELECT pg_catalog.current_setting('server_version_num')::int4,
       pg_catalog.pg_is_in_recovery(),
       pg_catalog.current_setting('transaction_read_only')::boolean
";

const SCHEMA_EXISTS_SQL: &str = "
SELECT EXISTS (
    SELECT 1 FROM pg_catalog.pg_namespace WHERE nspname::text = $1
)
";

const EXTENSIONS_SQL: &str = "
SELECT CASE WHEN pg_catalog.octet_length(extname::text) <= 64 THEN extname::text END,
       CASE WHEN pg_catalog.octet_length(extversion) <= 64 THEN extversion END,
       pg_catalog.octet_length(extname::text) > 64 OR pg_catalog.octet_length(extversion) > 64
FROM pg_catalog.pg_extension
ORDER BY extname
LIMIT $1
";

const SETTINGS_SQL: &str = "
SELECT CASE WHEN pg_catalog.octet_length(name) <= 64 THEN name END,
       CASE WHEN pg_catalog.octet_length(setting) <= 32 THEN setting END,
       CASE WHEN pg_catalog.octet_length(unit) <= 8 THEN unit END,
       pg_catalog.octet_length(name) > 64 OR pg_catalog.octet_length(setting) > 32
           OR COALESCE(pg_catalog.octet_length(unit) > 8, false)
FROM pg_catalog.pg_settings
WHERE name = ANY($1::text[])
ORDER BY name
LIMIT $2
";

pub(super) async fn inspect(
    client: &Client,
    info: &SessionInfo,
    sslmode: &'static str,
    schema: Option<&str>,
) -> Result<Observation, Issue> {
    if !info.read_only {
        return Err(Issue::ReadOnlyUnconfirmed);
    }
    client
        .batch_execute("BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .await
        .map_err(issue)?;
    let result = inspect_transaction(client, &info.tls, sslmode, schema).await;
    let rollback = client.batch_execute("ROLLBACK").await.map_err(issue);
    result.and_then(|observation| rollback.map(|()| observation))
}

async fn inspect_transaction(
    client: &Client,
    tls: &TlsState,
    sslmode: &'static str,
    schema: Option<&str>,
) -> Result<Observation, Issue> {
    let row = client.query_one(HEALTH_SQL, &[]).await.map_err(issue)?;
    let health = Health {
        server_version_num: get(&row, 0)?,
        in_recovery: get(&row, 1)?,
        read_only: get(&row, 2)?,
        tls: match tls {
            TlsState::Disabled => TlsObservation::Disabled,
            TlsState::NotNegotiated => TlsObservation::NotNegotiated,
            TlsState::Unknown => TlsObservation::Unknown,
            TlsState::Active { .. } => TlsObservation::Active,
        },
        sslmode,
    };
    if !health.read_only {
        return Err(Issue::ReadOnlyUnconfirmed);
    }
    if !(14..=18).contains(&(health.server_version_num / 10_000)) {
        return Ok(Observation {
            health,
            schema: Err(Issue::UnsupportedVersion),
            extensions: Err(Issue::UnsupportedVersion),
            settings: Err(Issue::UnsupportedVersion),
        });
    }

    Ok(Observation {
        health,
        schema: section(client, capture_schema(client, schema)).await,
        extensions: section(client, capture_extensions(client)).await,
        settings: section(client, capture_settings(client)).await,
    })
}

async fn section<T>(
    client: &Client,
    read: impl Future<Output = Result<T, Issue>>,
) -> Result<T, Issue> {
    client
        .batch_execute("SAVEPOINT fleet_section")
        .await
        .map_err(issue)?;
    let result = read.await;
    let finish = if result.is_ok() {
        "RELEASE SAVEPOINT fleet_section"
    } else {
        "ROLLBACK TO SAVEPOINT fleet_section; RELEASE SAVEPOINT fleet_section"
    };
    let cleanup = client.batch_execute(finish).await.map_err(issue);
    result.and_then(|value| cleanup.map(|()| value))
}

async fn capture_schema(client: &Client, schema: Option<&str>) -> Result<SchemaSnapshot, Issue> {
    if let Some(name) = schema {
        let row = client
            .query_one(SCHEMA_EXISTS_SQL, &[&name])
            .await
            .map_err(issue)?;
        if !get::<bool>(&row, 0)? {
            return Err(Issue::SchemaMissing);
        }
    }
    crate::postgres::snapshot::capture_bounded_schema_snapshot(client, schema)
        .await
        .map_err(|error| Issue::from_diagnostic(&error))
}

async fn capture_extensions(client: &Client) -> Result<BTreeMap<String, String>, Issue> {
    let limit = (MAX_EXTENSIONS + 1) as i64;
    let rows = client
        .query(EXTENSIONS_SQL, &[&limit])
        .await
        .map_err(issue)?;
    if rows.len() > MAX_EXTENSIONS {
        return Err(Issue::Bounded);
    }
    let mut extensions = BTreeMap::new();
    for row in &rows {
        if get::<bool>(row, 2)? {
            return Err(Issue::Bounded);
        }
        extensions.insert(token(row, 0)?, token(row, 1)?);
    }
    Ok(extensions)
}

fn token(row: &Row, index: usize) -> Result<String, Issue> {
    get::<Option<String>>(row, index)?
        .filter(|value| {
            !value.is_empty()
                && value.bytes().all(|byte| {
                    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b'+')
                })
                && crate::diagnostics::redaction::redact_text(value) == *value
        })
        .ok_or(Issue::InvalidCatalogValue)
}

async fn capture_settings(client: &Client) -> Result<BTreeMap<String, Setting>, Issue> {
    let names = SETTINGS.as_slice();
    let limit = (names.len() + 1) as i64;
    let rows = client
        .query(SETTINGS_SQL, &[&names, &limit])
        .await
        .map_err(issue)?;
    if rows.len() > names.len() {
        return Err(Issue::Bounded);
    }
    let mut settings = BTreeMap::new();
    for row in &rows {
        if get::<bool>(row, 3)? {
            return Err(Issue::Bounded);
        }
        let name = token(row, 0)?;
        let value = token(row, 1)?;
        let unit: Option<String> = get(row, 2)?;
        if !names.contains(&name.as_str()) || !valid_setting(&name, &value, unit.as_deref()) {
            return Err(Issue::InvalidCatalogValue);
        }
        settings.insert(name, Setting { value, unit });
    }
    if settings.len() != names.len() {
        return Err(Issue::InvalidCatalogValue);
    }
    Ok(settings)
}

fn valid_setting(name: &str, value: &str, unit: Option<&str>) -> bool {
    let integer = || value.parse::<u64>().is_ok();
    match name {
        "jit" => matches!(value, "on" | "off") && unit.is_none(),
        "random_page_cost" | "seq_page_cost" => {
            value
                .parse::<f64>()
                .is_ok_and(|number| number.is_finite() && number >= 0.0)
                && unit.is_none()
        }
        "effective_cache_size" | "shared_buffers" => {
            integer()
                && unit.is_some_and(|unit| {
                    unit.strip_suffix("kB")
                        .and_then(|size| size.parse::<u8>().ok())
                        .is_some_and(|size| size.is_power_of_two() && size <= 32)
                })
        }
        "work_mem" => integer() && unit == Some("kB"),
        _ => integer() && unit.is_none(),
    }
}

fn issue(error: tokio_postgres::Error) -> Issue {
    Issue::from_diagnostic(&from_query_error(&error, 1))
}

fn get<'a, T: tokio_postgres::types::FromSql<'a>>(row: &'a Row, index: usize) -> Result<T, Issue> {
    row.try_get(index).map_err(|_| Issue::InvalidCatalogValue)
}
