//! Bounded catalog reads for the retention and growth audit.
//!
//! The queries select sizes and statistics only. No table rows are read, no
//! statistics are modified, and each query applies its retention bound in SQL
//! so a large catalog cannot force an unbounded transfer.

use crate::diagnostics::{Diagnostic, DiagnosticKind};
use crate::growth::{
    GrowthOptions, GrowthSnapshot, MAX_RELATIONS, MAX_SEQUENCES, RelationGrowth, SequenceHeadroom,
};
use crate::postgres::error::from_query_error;
use tokio_postgres::Client;

const RELATIONS_SQL: &str = r#"
SELECT
    n.nspname::text AS schema,
    c.relname::text AS relation_name,
    (c.relkind = 'p') AS partitioned,
    pg_catalog.pg_total_relation_size(c.oid)::bigint AS total_bytes,
    pg_catalog.pg_relation_size(c.oid)::bigint AS heap_bytes,
    pg_catalog.pg_indexes_size(c.oid)::bigint AS index_bytes,
    pg_catalog.pg_total_relation_size(c.reltoastrelid)::bigint AS toast_bytes,
    CASE WHEN c.reltuples < 0 THEN NULL ELSE c.reltuples::bigint END AS estimated_rows,
    COALESCE(s.n_live_tup, 0)::bigint AS live_tuples,
    COALESCE(s.n_dead_tup, 0)::bigint AS dead_tuples,
    COALESCE(s.n_tup_ins, 0)::bigint AS inserts,
    COALESCE(s.n_tup_upd, 0)::bigint AS updates,
    COALESCE(s.n_tup_del, 0)::bigint AS deletes,
    pg_catalog.to_char(
        COALESCE(s.last_vacuum, s.last_autovacuum),
        'YYYY-MM-DD"T"HH24:MI:SSOF'
    ) AS last_vacuum,
    pg_catalog.to_char(
        COALESCE(s.last_analyze, s.last_autoanalyze),
        'YYYY-MM-DD"T"HH24:MI:SSOF'
    ) AS last_analyze,
    pg_catalog.to_char(db.stats_reset, 'YYYY-MM-DD"T"HH24:MI:SSOF') AS stats_reset,
    (
        CASE
            WHEN db.stats_reset IS NULL THEN NULL
            ELSE EXTRACT(EPOCH FROM (pg_catalog.clock_timestamp() - db.stats_reset)) / 3600.0
        END
    )::float8 AS stats_age_hours
FROM pg_catalog.pg_class c
JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
LEFT JOIN pg_catalog.pg_stat_user_tables s ON s.relid = c.oid
CROSS JOIN (
    SELECT stats_reset
    FROM pg_catalog.pg_stat_database
    WHERE datname = pg_catalog.current_database()
) db
WHERE c.relkind IN ('r', 'p')
  AND n.nspname !~ '^pg_' AND n.nspname <> 'information_schema'
  AND ($1::text IS NULL OR lower(n.nspname::text) = lower($1::text))
ORDER BY pg_catalog.pg_total_relation_size(c.oid) DESC, n.nspname, c.relname
LIMIT $2
"#;

const SEQUENCES_SQL: &str = r#"
SELECT
    schemaname::text AS schema,
    sequencename::text AS sequence_name,
    data_type::text AS data_type,
    last_value,
    min_value,
    max_value,
    increment_by,
    cycle
FROM pg_catalog.pg_sequences
WHERE schemaname !~ '^pg_' AND schemaname <> 'information_schema'
  AND ($1::text IS NULL OR lower(schemaname::text) = lower($1::text))
ORDER BY
    CASE
        WHEN cycle OR last_value IS NULL OR max_value <= min_value THEN 1
        ELSE 0
    END,
    CASE
        WHEN cycle OR last_value IS NULL THEN NULL
        WHEN increment_by > 0 THEN
            (max_value::numeric - last_value::numeric)
                / NULLIF(max_value::numeric - min_value::numeric, 0)
        WHEN increment_by < 0 THEN
            (last_value::numeric - min_value::numeric)
                / NULLIF(max_value::numeric - min_value::numeric, 0)
    END ASC NULLS LAST,
    schemaname, sequencename
LIMIT $2
"#;

/// Reads the bounded relation and sequence snapshot and derives findings.
pub async fn growth_snapshot(
    client: &Client,
    options: &GrowthOptions,
) -> Result<GrowthSnapshot, Diagnostic> {
    let schema = options.schema.as_deref();
    let fetch_limit = (MAX_RELATIONS + 1) as i64;
    let rows = client
        .query(RELATIONS_SQL, &[&schema, &fetch_limit])
        .await
        .map_err(|error| from_query_error(&error, 1))?;

    let mut relations = Vec::with_capacity(rows.len().min(MAX_RELATIONS));
    let mut stats_reset = None;
    let mut stats_age_hours = None;
    for row in &rows {
        if stats_reset.is_none() {
            stats_reset = get::<Option<String>>(row, "stats_reset")?;
            stats_age_hours = get::<Option<f64>>(row, "stats_age_hours")?;
        }
        relations.push(RelationGrowth {
            schema: get(row, "schema")?,
            table: get(row, "relation_name")?,
            partitioned: get(row, "partitioned")?,
            total_bytes: get::<i64>(row, "total_bytes")?.max(0) as u64,
            heap_bytes: get::<i64>(row, "heap_bytes")?.max(0) as u64,
            index_bytes: get::<i64>(row, "index_bytes")?.max(0) as u64,
            toast_bytes: get::<i64>(row, "toast_bytes")?.max(0) as u64,
            estimated_rows: get(row, "estimated_rows")?,
            live_tuples: get(row, "live_tuples")?,
            dead_tuples: get(row, "dead_tuples")?,
            inserts: get(row, "inserts")?,
            updates: get(row, "updates")?,
            deletes: get(row, "deletes")?,
            stats_age_hours,
            last_vacuum: get(row, "last_vacuum")?,
            last_analyze: get(row, "last_analyze")?,
        });
    }
    let truncated_relations = relations.len() > MAX_RELATIONS;
    relations.truncate(MAX_RELATIONS);

    let sequence_limit = (MAX_SEQUENCES + 1) as i64;
    let sequence_rows = client
        .query(SEQUENCES_SQL, &[&schema, &sequence_limit])
        .await
        .map_err(|error| from_query_error(&error, 1))?;
    let mut sequences = Vec::with_capacity(sequence_rows.len().min(MAX_SEQUENCES));
    for row in &sequence_rows {
        sequences.push(SequenceHeadroom {
            schema: get(row, "schema")?,
            name: get(row, "sequence_name")?,
            data_type: get(row, "data_type")?,
            last_value: get(row, "last_value")?,
            min_value: get(row, "min_value")?,
            max_value: get(row, "max_value")?,
            increment: get(row, "increment_by")?,
            cyclic: get(row, "cycle")?,
        });
    }
    let truncated_sequences = sequences.len() > MAX_SEQUENCES;
    sequences.truncate(MAX_SEQUENCES);

    Ok(GrowthSnapshot {
        relations,
        sequences,
        findings: Vec::new(),
        stats_reset,
        stats_age_hours,
        truncated_relations,
        truncated_sequences,
    }
    .evaluate(options))
}

fn get<'a, T>(row: &'a tokio_postgres::Row, column: &str) -> Result<T, Diagnostic>
where
    T: tokio_postgres::types::FromSql<'a>,
{
    row.try_get(column).map_err(|error| {
        Diagnostic::new(
            DiagnosticKind::Internal,
            format!("growth audit column '{column}' had an unexpected type: {error}"),
            "reading the growth audit",
        )
        .next_action("report this with the server version and the audit output")
    })
}
