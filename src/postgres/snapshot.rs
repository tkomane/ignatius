//! Bounded read-only PostgreSQL 14-18 catalog extractor for
//! Feature `057-schema-snapshots-and-drift-guard`.
//!
//! Captures canonical schema structure (`tables`, `columns`, `constraints`,
//! `indexes`, `rls_enabled`, `rls_forced`, `policies`, `views`, `sequences`,
//! and `enums`) using extended-protocol bound parameters and explicit `LIMIT`
//! caps. Never reads table rows, sequence `last_value` runtime counters, or
//! credentials, and calls [`SchemaSnapshot::finalize`] before returning.

use std::collections::BTreeMap;

use tokio_postgres::Client;

use crate::diagnostics::{Diagnostic, DiagnosticKind};
use crate::postgres::error::from_query_error;
use crate::snapshot::{
    ColumnSnapshot, ConstraintSnapshot, EnumSnapshot, IndexSnapshot, MAX_SNAPSHOT_COLUMNS,
    MAX_SNAPSHOT_CONSTRAINTS, MAX_SNAPSHOT_INDEXES, MAX_SNAPSHOT_POLICIES, MAX_SNAPSHOT_TABLES,
    PolicySnapshot, SchemaSnapshot, SequenceSnapshot, TableSnapshot, ViewSnapshot, sha256_hex,
};

const TABLES_SQL: &str = r#"
SELECT
    n.nspname::text AS schema_name,
    c.relname::text AS table_name,
    c.relrowsecurity AS rls_enabled,
    c.relforcerowsecurity AS rls_forced
FROM pg_catalog.pg_class c
JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
WHERE c.relkind IN ('r', 'p')
  AND (
      ($1::text IS NOT NULL AND n.nspname = $1::text)
      OR (
          $1::text IS NULL
          AND n.nspname NOT IN ('pg_catalog', 'information_schema', 'pg_toast')
          AND n.nspname NOT LIKE 'pg_temp_%'
          AND n.nspname NOT LIKE 'pg_toast_temp_%'
      )
  )
ORDER BY n.nspname, c.relname
LIMIT $2
"#;

const COLUMNS_SQL: &str = r#"
SELECT
    n.nspname::text AS schema_name,
    c.relname::text AS table_name,
    a.attname::text AS column_name,
    CASE WHEN pg_catalog.octet_length(expr.data_type) <= $3 THEN expr.data_type END AS data_type,
    (NOT a.attnotnull) AS nullable,
    CASE WHEN pg_catalog.octet_length(expr.definition) <= $3 THEN expr.definition END AS default_expr,
    COALESCE(pg_catalog.octet_length(expr.definition) > $3, false)
        OR pg_catalog.octet_length(expr.data_type) > $3 AS definition_bounded,
    a.attnum AS ordinal
FROM pg_catalog.pg_attribute a
JOIN pg_catalog.pg_class c ON c.oid = a.attrelid
JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
LEFT JOIN pg_catalog.pg_attrdef d ON d.adrelid = a.attrelid AND d.adnum = a.attnum
CROSS JOIN LATERAL (
    SELECT pg_catalog.pg_get_expr(d.adbin, d.adrelid)::text AS definition,
           pg_catalog.format_type(a.atttypid, a.atttypmod)::text AS data_type
    OFFSET 0
) expr
WHERE c.relkind IN ('r', 'p')
  AND a.attnum > 0
  AND NOT a.attisdropped
  AND (
      ($1::text IS NOT NULL AND n.nspname = $1::text)
      OR (
          $1::text IS NULL
          AND n.nspname NOT IN ('pg_catalog', 'information_schema', 'pg_toast')
          AND n.nspname NOT LIKE 'pg_temp_%'
          AND n.nspname NOT LIKE 'pg_toast_temp_%'
      )
  )
ORDER BY n.nspname, c.relname, a.attnum
LIMIT $2
"#;

const CONSTRAINTS_SQL: &str = r#"
SELECT
    n.nspname::text AS schema_name,
    c.relname::text AS table_name,
    con.conname::text AS constraint_name,
    CASE con.contype
        WHEN 'p' THEN 'PRIMARY KEY'
        WHEN 'f' THEN 'FOREIGN KEY'
        WHEN 'u' THEN 'UNIQUE'
        WHEN 'c' THEN 'CHECK'
        WHEN 'x' THEN 'EXCLUSION'
        ELSE con.contype::text
    END AS kind,
    CASE WHEN pg_catalog.octet_length(expr.definition) <= $3 THEN expr.definition END AS definition,
    pg_catalog.octet_length(expr.definition) > $3 AS definition_bounded
FROM pg_catalog.pg_constraint con
JOIN pg_catalog.pg_class c ON c.oid = con.conrelid
JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
CROSS JOIN LATERAL (
    SELECT COALESCE(pg_catalog.pg_get_constraintdef(con.oid)::text, '') AS definition OFFSET 0
) expr
WHERE c.relkind IN ('r', 'p')
  AND con.contype IN ('p', 'f', 'u', 'c', 'x')
  AND (
      ($1::text IS NOT NULL AND n.nspname = $1::text)
      OR (
          $1::text IS NULL
          AND n.nspname NOT IN ('pg_catalog', 'information_schema', 'pg_toast')
          AND n.nspname NOT LIKE 'pg_temp_%'
          AND n.nspname NOT LIKE 'pg_toast_temp_%'
      )
  )
ORDER BY n.nspname, c.relname, con.conname
LIMIT $2
"#;

const INDEXES_SQL: &str = r#"
SELECT
    n.nspname::text AS schema_name,
    c.relname::text AS table_name,
    ic.relname::text AS index_name,
    i.indisunique AS is_unique,
    i.indisprimary AS is_primary,
    CASE WHEN pg_catalog.octet_length(expr.definition) <= $3 THEN expr.definition END AS definition,
    pg_catalog.octet_length(expr.definition) > $3 AS definition_bounded
FROM pg_catalog.pg_index i
JOIN pg_catalog.pg_class c ON c.oid = i.indrelid
JOIN pg_catalog.pg_class ic ON ic.oid = i.indexrelid
JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
CROSS JOIN LATERAL (
    SELECT COALESCE(pg_catalog.pg_get_indexdef(i.indexrelid)::text, '') AS definition OFFSET 0
) expr
WHERE c.relkind IN ('r', 'p')
  AND i.indislive
  AND (
      ($1::text IS NOT NULL AND n.nspname = $1::text)
      OR (
          $1::text IS NULL
          AND n.nspname NOT IN ('pg_catalog', 'information_schema', 'pg_toast')
          AND n.nspname NOT LIKE 'pg_temp_%'
          AND n.nspname NOT LIKE 'pg_toast_temp_%'
      )
  )
ORDER BY n.nspname, c.relname, ic.relname
LIMIT $2
"#;

const POLICIES_SQL: &str = r#"
SELECT
    n.nspname::text AS schema_name,
    c.relname::text AS table_name,
    p.polname::text AS policy_name,
    CASE p.polcmd
        WHEN '*' THEN 'ALL'
        WHEN 'r' THEN 'SELECT'
        WHEN 'a' THEN 'INSERT'
        WHEN 'w' THEN 'UPDATE'
        WHEN 'd' THEN 'DELETE'
        ELSE p.polcmd::text
    END AS cmd,
    p.polpermissive AS permissive,
    CASE
        WHEN p.polroles = ARRAY[0::oid] THEN ARRAY['PUBLIC'::text]
        ELSE ARRAY(
            SELECT r.rolname::text
            FROM pg_catalog.unnest(p.polroles) AS roid
            JOIN pg_catalog.pg_roles r ON r.oid = roid
            ORDER BY r.rolname
            LIMIT $4
        )
    END AS roles,
    CASE WHEN pg_catalog.octet_length(expr.qual) <= $3 THEN expr.qual END AS qual,
    CASE WHEN pg_catalog.octet_length(expr.with_check) <= $3 THEN expr.with_check END AS with_check,
    COALESCE(pg_catalog.octet_length(expr.qual) > $3, false)
        OR COALESCE(pg_catalog.octet_length(expr.with_check) > $3, false) AS definition_bounded
FROM pg_catalog.pg_policy p
JOIN pg_catalog.pg_class c ON c.oid = p.polrelid
JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
CROSS JOIN LATERAL (
    SELECT pg_catalog.pg_get_expr(p.polqual, p.polrelid)::text AS qual,
           pg_catalog.pg_get_expr(p.polwithcheck, p.polrelid)::text AS with_check
    OFFSET 0
) expr
WHERE c.relkind IN ('r', 'p')
  AND (
      ($1::text IS NOT NULL AND n.nspname = $1::text)
      OR (
          $1::text IS NULL
          AND n.nspname NOT IN ('pg_catalog', 'information_schema', 'pg_toast')
          AND n.nspname NOT LIKE 'pg_temp_%'
          AND n.nspname NOT LIKE 'pg_toast_temp_%'
      )
  )
ORDER BY n.nspname, c.relname, p.polname
LIMIT $2
"#;

const VIEWS_SQL: &str = r#"
SELECT
    n.nspname::text AS schema_name,
    c.relname::text AS view_name,
    (c.relkind = 'm') AS materialized,
    CASE WHEN pg_catalog.octet_length(expr.definition) <= $3 THEN expr.definition END AS definition,
    pg_catalog.octet_length(expr.definition) > $3 AS definition_bounded
FROM pg_catalog.pg_class c
JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
CROSS JOIN LATERAL (
    SELECT COALESCE(pg_catalog.pg_get_viewdef(c.oid, true)::text, '') AS definition OFFSET 0
) expr
WHERE c.relkind IN ('v', 'm')
  AND (
      ($1::text IS NOT NULL AND n.nspname = $1::text)
      OR (
          $1::text IS NULL
          AND n.nspname NOT IN ('pg_catalog', 'information_schema', 'pg_toast')
          AND n.nspname NOT LIKE 'pg_temp_%'
          AND n.nspname NOT LIKE 'pg_toast_temp_%'
      )
  )
ORDER BY n.nspname, c.relname
LIMIT $2
"#;

const SEQUENCES_SQL: &str = r#"
SELECT
    n.nspname::text AS schema_name,
    c.relname::text AS sequence_name,
    pg_catalog.format_type(s.seqtypid, NULL)::text AS data_type,
    s.seqstart AS start_value,
    s.seqincrement AS increment_by,
    s.seqcycle AS cycle
FROM pg_catalog.pg_sequence s
JOIN pg_catalog.pg_class c ON c.oid = s.seqrelid
JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
WHERE c.relkind = 'S'
  AND (
      ($1::text IS NOT NULL AND n.nspname = $1::text)
      OR (
          $1::text IS NULL
          AND n.nspname NOT IN ('pg_catalog', 'information_schema', 'pg_toast')
          AND n.nspname NOT LIKE 'pg_temp_%'
          AND n.nspname NOT LIKE 'pg_toast_temp_%'
      )
  )
ORDER BY n.nspname, c.relname
LIMIT $2
"#;

const ENUMS_SQL: &str = r#"
SELECT
    n.nspname::text AS schema_name,
    t.typname::text AS enum_name,
    ARRAY(
        SELECT e.enumlabel::text
        FROM pg_catalog.pg_enum e
        WHERE e.enumtypid = t.oid
        ORDER BY e.enumsortorder
        LIMIT $3
    ) AS labels
FROM pg_catalog.pg_type t
JOIN pg_catalog.pg_namespace n ON n.oid = t.typnamespace
WHERE t.typtype = 'e'
  AND (
      ($1::text IS NOT NULL AND n.nspname = $1::text)
      OR (
          $1::text IS NULL
          AND n.nspname NOT IN ('pg_catalog', 'information_schema', 'pg_toast')
          AND n.nspname NOT LIKE 'pg_temp_%'
          AND n.nspname NOT LIKE 'pg_toast_temp_%'
      )
  )
ORDER BY n.nspname, t.typname
LIMIT $2
"#;

/// Captures a bounded, canonical [`SchemaSnapshot`] from PostgreSQL 14-18
/// catalog tables.
///
/// When `schema` is `None`, system schemas (`pg_catalog`, `information_schema`,
/// `pg_toast`, and temporary schemas) are excluded by default.
pub async fn capture_schema_snapshot(
    client: &Client,
    schema: Option<&str>,
) -> Result<SchemaSnapshot, Diagnostic> {
    capture(client, schema, i32::MAX, i64::MAX - 1).await
}

/// Adds wire bounds for raw definitions and nested arrays before fleet comparison.
pub(super) async fn capture_fleet_schema_snapshot(
    client: &Client,
    schema: Option<&str>,
) -> Result<SchemaSnapshot, Diagnostic> {
    capture(client, schema, 4096, 128).await
}

async fn capture(
    client: &Client,
    schema: Option<&str>,
    definition_bytes: i32,
    nested_items: i64,
) -> Result<SchemaSnapshot, Diagnostic> {
    if let Some(filter) = schema
        && filter.trim().is_empty()
    {
        return Err(Diagnostic::new(
            DiagnosticKind::Usage,
            "schema filter cannot be empty",
            "capturing schema snapshot",
        )
        .next_action("provide a non-empty schema name or omit the schema filter"));
    }

    let mut snapshot = SchemaSnapshot::new(schema.map(str::to_owned));
    let nested_limit = nested_items + 1;

    let table_limit = (MAX_SNAPSHOT_TABLES + 1) as i64;
    let table_rows = client
        .query(TABLES_SQL, &[&schema, &table_limit])
        .await
        .map_err(|error| from_query_error(&error, 1))?;
    if table_rows.len() > MAX_SNAPSHOT_TABLES {
        snapshot.truncated = true;
    }

    let mut table_map: BTreeMap<(String, String), TableSnapshot> = BTreeMap::new();
    for row in table_rows.iter().take(MAX_SNAPSHOT_TABLES) {
        let schema_name: String = get(row, "schema_name")?;
        let table_name: String = get(row, "table_name")?;
        table_map.insert(
            (schema_name.clone(), table_name.clone()),
            TableSnapshot {
                schema: schema_name,
                name: table_name,
                columns: Vec::new(),
                constraints: Vec::new(),
                indexes: Vec::new(),
                rls_enabled: get(row, "rls_enabled")?,
                rls_forced: get(row, "rls_forced")?,
                policies: Vec::new(),
            },
        );
    }

    if !table_map.is_empty() {
        let col_limit = (MAX_SNAPSHOT_COLUMNS + 1) as i64;
        let col_rows = client
            .query(COLUMNS_SQL, &[&schema, &col_limit, &definition_bytes])
            .await
            .map_err(|error| from_query_error(&error, 2))?;
        if col_rows.len() > MAX_SNAPSHOT_COLUMNS {
            snapshot.truncated = true;
        }
        for row in col_rows.iter().take(MAX_SNAPSHOT_COLUMNS) {
            let key = (
                get::<String>(row, "schema_name")?,
                get::<String>(row, "table_name")?,
            );
            if let Some(table) = table_map.get_mut(&key) {
                snapshot.truncated |= get::<bool>(row, "definition_bounded")?;
                table.columns.push(ColumnSnapshot {
                    name: get(row, "column_name")?,
                    data_type: get::<Option<String>>(row, "data_type")?.unwrap_or_default(),
                    nullable: get(row, "nullable")?,
                    default_expr: get(row, "default_expr")?,
                    ordinal: get(row, "ordinal")?,
                });
            }
        }

        let con_limit = (MAX_SNAPSHOT_CONSTRAINTS + 1) as i64;
        let con_rows = client
            .query(CONSTRAINTS_SQL, &[&schema, &con_limit, &definition_bytes])
            .await
            .map_err(|error| from_query_error(&error, 3))?;
        if con_rows.len() > MAX_SNAPSHOT_CONSTRAINTS {
            snapshot.truncated = true;
        }
        for row in con_rows.iter().take(MAX_SNAPSHOT_CONSTRAINTS) {
            let key = (
                get::<String>(row, "schema_name")?,
                get::<String>(row, "table_name")?,
            );
            if let Some(table) = table_map.get_mut(&key) {
                snapshot.truncated |= get::<bool>(row, "definition_bounded")?;
                table.constraints.push(ConstraintSnapshot {
                    name: get(row, "constraint_name")?,
                    kind: get(row, "kind")?,
                    definition: get::<Option<String>>(row, "definition")?.unwrap_or_default(),
                });
            }
        }

        let idx_limit = (MAX_SNAPSHOT_INDEXES + 1) as i64;
        let idx_rows = client
            .query(INDEXES_SQL, &[&schema, &idx_limit, &definition_bytes])
            .await
            .map_err(|error| from_query_error(&error, 4))?;
        if idx_rows.len() > MAX_SNAPSHOT_INDEXES {
            snapshot.truncated = true;
        }
        for row in idx_rows.iter().take(MAX_SNAPSHOT_INDEXES) {
            let key = (
                get::<String>(row, "schema_name")?,
                get::<String>(row, "table_name")?,
            );
            if let Some(table) = table_map.get_mut(&key) {
                snapshot.truncated |= get::<bool>(row, "definition_bounded")?;
                table.indexes.push(IndexSnapshot {
                    name: get(row, "index_name")?,
                    is_unique: get(row, "is_unique")?,
                    is_primary: get(row, "is_primary")?,
                    definition: get::<Option<String>>(row, "definition")?.unwrap_or_default(),
                });
            }
        }

        let pol_limit = (MAX_SNAPSHOT_POLICIES + 1) as i64;
        let pol_rows = client
            .query(
                POLICIES_SQL,
                &[&schema, &pol_limit, &definition_bytes, &nested_limit],
            )
            .await
            .map_err(|error| from_query_error(&error, 5))?;
        if pol_rows.len() > MAX_SNAPSHOT_POLICIES {
            snapshot.truncated = true;
        }
        for row in pol_rows.iter().take(MAX_SNAPSHOT_POLICIES) {
            let key = (
                get::<String>(row, "schema_name")?,
                get::<String>(row, "table_name")?,
            );
            if let Some(table) = table_map.get_mut(&key) {
                let mut roles: Vec<String> = get(row, "roles")?;
                snapshot.truncated |=
                    get::<bool>(row, "definition_bounded")? | trim_nested(&mut roles, nested_items);
                table.policies.push(PolicySnapshot {
                    name: get(row, "policy_name")?,
                    cmd: get(row, "cmd")?,
                    permissive: get(row, "permissive")?,
                    roles,
                    qual: get(row, "qual")?,
                    with_check: get(row, "with_check")?,
                });
            }
        }
    }

    snapshot.tables = table_map.into_values().collect();

    let view_limit = (MAX_SNAPSHOT_TABLES + 1) as i64;
    let view_rows = client
        .query(VIEWS_SQL, &[&schema, &view_limit, &definition_bytes])
        .await
        .map_err(|error| from_query_error(&error, 6))?;
    if view_rows.len() > MAX_SNAPSHOT_TABLES {
        snapshot.truncated = true;
    }
    for row in view_rows.iter().take(MAX_SNAPSHOT_TABLES) {
        snapshot.truncated |= get::<bool>(row, "definition_bounded")?;
        let definition = get::<Option<String>>(row, "definition")?.unwrap_or_default();
        snapshot.views.push(ViewSnapshot {
            schema: get(row, "schema_name")?,
            name: get(row, "view_name")?,
            materialized: get(row, "materialized")?,
            definition_sha256: sha256_hex(definition.trim().as_bytes()),
        });
    }

    let seq_limit = (MAX_SNAPSHOT_TABLES + 1) as i64;
    let seq_rows = client
        .query(SEQUENCES_SQL, &[&schema, &seq_limit])
        .await
        .map_err(|error| from_query_error(&error, 7))?;
    if seq_rows.len() > MAX_SNAPSHOT_TABLES {
        snapshot.truncated = true;
    }
    for row in seq_rows.iter().take(MAX_SNAPSHOT_TABLES) {
        snapshot.sequences.push(SequenceSnapshot {
            schema: get(row, "schema_name")?,
            name: get(row, "sequence_name")?,
            data_type: get(row, "data_type")?,
            start_value: get(row, "start_value")?,
            increment_by: get(row, "increment_by")?,
            cycle: get(row, "cycle")?,
        });
    }

    let enum_limit = (MAX_SNAPSHOT_TABLES + 1) as i64;
    let enum_rows = client
        .query(ENUMS_SQL, &[&schema, &enum_limit, &nested_limit])
        .await
        .map_err(|error| from_query_error(&error, 8))?;
    if enum_rows.len() > MAX_SNAPSHOT_TABLES {
        snapshot.truncated = true;
    }
    for row in enum_rows.iter().take(MAX_SNAPSHOT_TABLES) {
        let mut labels: Vec<String> = get(row, "labels")?;
        snapshot.truncated |= trim_nested(&mut labels, nested_items);
        snapshot.enums.push(EnumSnapshot {
            schema: get(row, "schema_name")?,
            name: get(row, "enum_name")?,
            labels,
        });
    }

    Ok(snapshot.finalize())
}

fn trim_nested(values: &mut Vec<String>, max: i64) -> bool {
    let bounded = values.len() as i64 > max;
    if bounded {
        values.truncate(max as usize);
    }
    bounded
}

fn get<'a, T>(row: &'a tokio_postgres::Row, column: &str) -> Result<T, Diagnostic>
where
    T: tokio_postgres::types::FromSql<'a>,
{
    row.try_get(column).map_err(|error| {
        Diagnostic::new(
            DiagnosticKind::Internal,
            format!("snapshot catalog column '{column}' had an unexpected type: {error}"),
            "reading schema snapshot catalog",
        )
        .next_action("report this with the PostgreSQL server version")
    })
}

#[cfg(test)]
mod tests;
