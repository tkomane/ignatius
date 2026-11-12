//! Bounded catalog reads for partition status and planning.
//!
//! Two queries preserve the bound: one for partitioned parents and one for
//! their children. Neither reads table rows and neither changes the catalog.

use std::collections::HashMap;

use crate::diagnostics::{Diagnostic, DiagnosticKind};
use crate::partition::{
    MAX_PARENTS, MAX_PARTITIONS, PartitionEntry, PartitionParent, PartitionSnapshot,
    PartitionStrategy,
};
use crate::postgres::error::from_query_error;
use tokio_postgres::Client;

const PARENTS_SQL: &str = r#"
SELECT
    c.oid AS parent_oid,
    n.nspname::text AS schema,
    c.relname::text AS table_name,
    p.partstrat::text AS strategy,
    pg_catalog.pg_get_partkeydef(c.oid)::text AS key_def,
    pg_catalog.format_type(a.atttypid, a.atttypmod) AS key_type
FROM pg_catalog.pg_class c
JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
JOIN pg_catalog.pg_partitioned_table p ON p.partrelid = c.oid
LEFT JOIN pg_catalog.pg_attribute a
    ON a.attrelid = c.oid AND a.attnum = p.partattrs[0]
WHERE c.relkind = 'p'
  AND n.nspname !~ '^pg_' AND n.nspname <> 'information_schema'
  AND ($1::text IS NULL OR lower(n.nspname::text) = lower($1::text))
ORDER BY n.nspname, c.relname
LIMIT $2
"#;

const PARTITIONS_SQL: &str = r#"
WITH parents AS (
    SELECT c.oid, n.nspname, c.relname
    FROM pg_catalog.pg_class c
    JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
    WHERE c.relkind = 'p'
      AND n.nspname !~ '^pg_' AND n.nspname <> 'information_schema'
      AND ($1::text IS NULL OR lower(n.nspname::text) = lower($1::text))
    ORDER BY n.nspname, c.relname
    LIMIT $2
)
SELECT
    p.oid AS parent_oid,
    cn.nspname::text AS child_schema,
    c.relname::text AS child,
    pg_catalog.pg_get_expr(c.relpartbound, c.oid)::text AS bound,
    pg_catalog.pg_total_relation_size(c.oid)::bigint AS total_bytes,
    CASE WHEN c.reltuples < 0 THEN NULL ELSE c.reltuples::bigint END AS estimated_rows
FROM parents p
JOIN LATERAL pg_catalog.pg_partition_tree(p.oid) pt ON pt.level > 0
JOIN pg_catalog.pg_class c ON c.oid = pt.relid
JOIN pg_catalog.pg_namespace cn ON cn.oid = c.relnamespace
ORDER BY p.nspname, p.relname, c.relname
LIMIT $3
"#;

/// Reads a bounded partition snapshot, optionally filtered to one schema.
pub async fn partition_snapshot(
    client: &Client,
    schema: Option<&str>,
) -> Result<PartitionSnapshot, Diagnostic> {
    let parent_limit = (MAX_PARENTS + 1) as i64;
    let parent_rows = client
        .query(PARENTS_SQL, &[&schema, &parent_limit])
        .await
        .map_err(|error| from_query_error(&error, 1))?;
    let truncated_parents = parent_rows.len() > MAX_PARENTS;

    let mut parents = Vec::with_capacity(parent_rows.len().min(MAX_PARENTS));
    let mut index_by_oid: HashMap<u32, usize> = HashMap::new();
    for row in parent_rows.iter().take(MAX_PARENTS) {
        let oid: u32 = get(row, "parent_oid")?;
        index_by_oid.insert(oid, parents.len());
        parents.push(PartitionParent {
            schema: get(row, "schema")?,
            table: get(row, "table_name")?,
            strategy: PartitionStrategy::from_catalog(&get::<String>(row, "strategy")?),
            key_def: get(row, "key_def")?,
            key_type: get(row, "key_type")?,
            partitions: Vec::new(),
        });
    }

    let partition_limit = (MAX_PARTITIONS + 1) as i64;
    let partition_rows = client
        .query(
            PARTITIONS_SQL,
            &[&schema, &(MAX_PARENTS as i64), &partition_limit],
        )
        .await
        .map_err(|error| from_query_error(&error, 1))?;
    let truncated_partitions = partition_rows.len() > MAX_PARTITIONS;
    for row in partition_rows.iter().take(MAX_PARTITIONS) {
        let parent_oid: u32 = get(row, "parent_oid")?;
        let Some(index) = index_by_oid.get(&parent_oid).copied() else {
            continue;
        };
        parents[index].partitions.push(PartitionEntry {
            schema: get(row, "child_schema")?,
            name: get(row, "child")?,
            bound: get(row, "bound")?,
            total_bytes: get::<i64>(row, "total_bytes")?.max(0) as u64,
            estimated_rows: get(row, "estimated_rows")?,
        });
    }

    Ok(PartitionSnapshot {
        parents,
        truncated_parents,
        truncated_partitions,
    }
    .sorted())
}

fn get<'a, T>(row: &'a tokio_postgres::Row, column: &str) -> Result<T, Diagnostic>
where
    T: tokio_postgres::types::FromSql<'a>,
{
    row.try_get(column).map_err(|error| {
        Diagnostic::new(
            DiagnosticKind::Internal,
            format!("partition column '{column}' had an unexpected type: {error}"),
            "reading the partition catalog",
        )
        .next_action("report this with the server version and the partition status output")
    })
}
