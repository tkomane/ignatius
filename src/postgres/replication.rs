//! Bounded read-only catalog extractor for logical replication and slot safety.
//!
//! Reads `pg_publication`, `pg_publication_tables`, `pg_subscription`,
//! `pg_stat_subscription`, `pg_subscription_rel`, and `pg_replication_slots`
//! using extended-protocol bound parameters and explicit SQL row limits.
//! No table rows are read, no replication state is mutated, subscription
//! connection strings are redacted immediately on read, and SQLSTATE `42501`
//! on `pg_subscription` degrades cleanly for non-superuser roles.

use std::collections::HashMap;

use tokio_postgres::Client;
use tokio_postgres::error::SqlState;

use crate::diagnostics::{Diagnostic, DiagnosticKind};
use crate::postgres::error::from_query_error;
use crate::replication::{
    MAX_PUBLICATION_TABLES, MAX_PUBLICATIONS, MAX_REPLICATION_SLOTS, MAX_SUBSCRIPTIONS,
    PublicationSummary, ReplicationReport, ReplicationSlotSummary, SubscriptionSummary,
    redact_conninfo,
};

const PUBLICATIONS_SQL: &str = r#"
SELECT
    p.pubname::text AS name,
    COALESCE(pg_catalog.pg_get_userbyid(p.pubowner), '')::text AS owner,
    p.puballtables AS all_tables,
    p.pubinsert AS insert_op,
    p.pubupdate AS update_op,
    p.pubdelete AS delete_op,
    p.pubtruncate AS truncate_op
FROM pg_catalog.pg_publication p
ORDER BY p.pubname
LIMIT $1
"#;

const PUBLICATION_TABLES_SQL: &str = r#"
WITH retained_pubs AS (
    SELECT p.pubname::text AS pubname
    FROM pg_catalog.pg_publication p
    ORDER BY p.pubname
    LIMIT $1
),
ranked_tables AS (
    SELECT
        pt.pubname::text AS pubname,
        (pt.schemaname::text || '.' || pt.tablename::text) AS qualified_table,
        ROW_NUMBER() OVER (
            PARTITION BY pt.pubname
            ORDER BY pt.schemaname, pt.tablename
        ) AS rn
    FROM pg_catalog.pg_publication_tables pt
    JOIN retained_pubs rp ON rp.pubname = pt.pubname::text
)
SELECT pubname, qualified_table
FROM ranked_tables
WHERE rn <= $2
ORDER BY pubname, qualified_table
"#;

const SUBSCRIPTIONS_SQL: &str = r#"
SELECT
    sub.subname::text AS name,
    COALESCE(pg_catalog.pg_get_userbyid(sub.subowner), '')::text AS owner,
    sub.subenabled AS enabled,
    COALESCE(sub.subconninfo, '')::text AS conninfo,
    sub.subslotname::text AS slot_name,
    (st.pid IS NOT NULL) AS worker_active,
    st.received_lsn::text AS received_lsn,
    st.latest_end_lsn::text AS latest_end_lsn,
    CASE
        WHEN st.received_lsn IS NOT NULL AND st.latest_end_lsn IS NOT NULL THEN
            GREATEST(pg_catalog.pg_wal_lsn_diff(st.received_lsn, st.latest_end_lsn)::bigint, 0)
        ELSE NULL
    END AS lag_bytes,
    COALESCE(rels.init_count, 0)::bigint AS init_count,
    COALESCE(rels.copying_count, 0)::bigint AS copying_count
FROM pg_catalog.pg_subscription sub
LEFT JOIN LATERAL (
    SELECT
        st.pid,
        st.received_lsn,
        st.latest_end_lsn
    FROM pg_catalog.pg_stat_subscription st
    WHERE st.subid = sub.oid
      AND st.relid IS NULL
    ORDER BY st.received_lsn DESC NULLS LAST, st.pid NULLS LAST
    LIMIT 1
) st ON true
LEFT JOIN LATERAL (
    SELECT
        COUNT(*) FILTER (WHERE sr.srsubstate = 'i') AS init_count,
        COUNT(*) FILTER (WHERE sr.srsubstate IN ('d', 'f', 's')) AS copying_count
    FROM pg_catalog.pg_subscription_rel sr
    WHERE sr.srsubid = sub.oid
) rels ON true
WHERE sub.subdbid = (
    SELECT d.oid
    FROM pg_catalog.pg_database d
    WHERE d.datname = pg_catalog.current_database()
)
ORDER BY sub.subname
LIMIT $1
"#;

const SLOTS_SQL: &str = r#"
SELECT
    s.slot_name::text AS slot_name,
    s.plugin::text AS plugin,
    COALESCE(s.slot_type, '')::text AS slot_type,
    s.database::text AS database,
    s.temporary AS temporary,
    s.active AS active,
    s.active_pid AS active_pid,
    COALESCE(s.wal_status, 'unknown')::text AS wal_status,
    CASE
        WHEN pg_catalog.pg_is_in_recovery() OR s.restart_lsn IS NULL THEN NULL
        ELSE GREATEST(
            pg_catalog.pg_wal_lsn_diff(pg_catalog.pg_current_wal_lsn(), s.restart_lsn)::bigint,
            0
        )
    END AS retained_bytes,
    s.safe_wal_size::bigint AS safe_wal_size
FROM pg_catalog.pg_replication_slots s
ORDER BY
    s.active ASC,
    retained_bytes DESC NULLS LAST,
    s.slot_name ASC
LIMIT $1
"#;

/// Reads a bounded logical replication and replication slot safety report from PostgreSQL.
pub async fn replication_report(client: &Client) -> Result<ReplicationReport, Diagnostic> {
    let pub_limit = (MAX_PUBLICATIONS + 1) as i64;
    let pub_rows = client
        .query(PUBLICATIONS_SQL, &[&pub_limit])
        .await
        .map_err(|error| from_query_error(&error, 1))?;
    let publications_truncated = pub_rows.len() > MAX_PUBLICATIONS;

    let mut publications = Vec::with_capacity(pub_rows.len().min(MAX_PUBLICATIONS));
    let mut pub_index_by_name: HashMap<String, usize> = HashMap::new();
    for row in pub_rows.iter().take(MAX_PUBLICATIONS) {
        let name: String = get(row, "name")?;
        pub_index_by_name.insert(name.clone(), publications.len());
        publications.push(PublicationSummary {
            name,
            owner: get(row, "owner")?,
            all_tables: get(row, "all_tables")?,
            insert: get(row, "insert_op")?,
            update: get(row, "update_op")?,
            delete: get(row, "delete_op")?,
            truncate: get(row, "truncate_op")?,
            tables: Vec::new(),
            tables_truncated: false,
        });
    }

    if !publications.is_empty() {
        let retained_pub_limit = MAX_PUBLICATIONS as i64;
        let table_limit = (MAX_PUBLICATION_TABLES + 1) as i64;
        let table_rows = client
            .query(PUBLICATION_TABLES_SQL, &[&retained_pub_limit, &table_limit])
            .await
            .map_err(|error| from_query_error(&error, 2))?;

        for row in &table_rows {
            let pubname: String = get(row, "pubname")?;
            let Some(index) = pub_index_by_name.get(&pubname).copied() else {
                continue;
            };
            let entry = &mut publications[index];
            if entry.tables.len() < MAX_PUBLICATION_TABLES {
                entry.tables.push(get(row, "qualified_table")?);
            } else {
                entry.tables_truncated = true;
            }
        }
    }

    let sub_limit = (MAX_SUBSCRIPTIONS + 1) as i64;
    let (subscriptions, subscriptions_truncated, subscription_access_restricted) =
        match client.query(SUBSCRIPTIONS_SQL, &[&sub_limit]).await {
            Ok(rows) => {
                let truncated = rows.len() > MAX_SUBSCRIPTIONS;
                let mut items = Vec::with_capacity(rows.len().min(MAX_SUBSCRIPTIONS));
                for row in rows.iter().take(MAX_SUBSCRIPTIONS) {
                    let enabled: bool = get(row, "enabled")?;
                    let raw_conninfo: String = get(row, "conninfo")?;
                    let worker_active: bool = get(row, "worker_active")?;
                    let init_count: i64 = get(row, "init_count")?;
                    let copying_count: i64 = get(row, "copying_count")?;
                    let sync_state = derive_subscription_sync_state(
                        enabled,
                        worker_active,
                        init_count,
                        copying_count,
                    );
                    items.push(SubscriptionSummary {
                        name: get(row, "name")?,
                        owner: get(row, "owner")?,
                        enabled,
                        conninfo_redacted: redact_conninfo(&raw_conninfo),
                        slot_name: get(row, "slot_name")?,
                        sync_state,
                        received_lsn: get(row, "received_lsn")?,
                        latest_end_lsn: get(row, "latest_end_lsn")?,
                        lag_bytes: get(row, "lag_bytes")?,
                    });
                }
                (items, truncated, false)
            }
            Err(error) if is_permission_denied(&error) => (Vec::new(), false, true),
            Err(error) => return Err(from_query_error(&error, 3)),
        };

    let slot_limit = (MAX_REPLICATION_SLOTS + 1) as i64;
    let slot_rows = client
        .query(SLOTS_SQL, &[&slot_limit])
        .await
        .map_err(|error| from_query_error(&error, 4))?;
    let slots_truncated = slot_rows.len() > MAX_REPLICATION_SLOTS;

    let mut slots = Vec::with_capacity(slot_rows.len().min(MAX_REPLICATION_SLOTS));
    for row in slot_rows.iter().take(MAX_REPLICATION_SLOTS) {
        let active: bool = get(row, "active")?;
        let wal_status: String = get(row, "wal_status")?;
        let retained_bytes: Option<i64> = get(row, "retained_bytes")?;
        let (risk, finding) =
            ReplicationSlotSummary::assess_risk(active, &wal_status, retained_bytes);
        slots.push(ReplicationSlotSummary {
            slot_name: get(row, "slot_name")?,
            plugin: get(row, "plugin")?,
            slot_type: get(row, "slot_type")?,
            database: get(row, "database")?,
            temporary: get(row, "temporary")?,
            active,
            active_pid: get(row, "active_pid")?,
            wal_status,
            retained_bytes,
            safe_wal_size: get(row, "safe_wal_size")?,
            risk,
            finding,
        });
    }

    Ok(ReplicationReport {
        publications,
        subscriptions,
        slots,
        publications_truncated,
        subscriptions_truncated,
        slots_truncated,
        subscription_access_restricted,
    })
}

fn derive_subscription_sync_state(
    enabled: bool,
    worker_active: bool,
    init_count: i64,
    copying_count: i64,
) -> String {
    if !enabled {
        "disabled".to_owned()
    } else if copying_count > 0 {
        "syncing".to_owned()
    } else if init_count > 0 {
        "initializing".to_owned()
    } else if worker_active {
        "streaming".to_owned()
    } else {
        "disconnected".to_owned()
    }
}

fn is_permission_denied(error: &tokio_postgres::Error) -> bool {
    error.as_db_error().is_some_and(|db| {
        *db.code() == SqlState::INSUFFICIENT_PRIVILEGE || db.code().code() == "42501"
    })
}

fn get<'a, T>(row: &'a tokio_postgres::Row, column: &str) -> Result<T, Diagnostic>
where
    T: tokio_postgres::types::FromSql<'a>,
{
    row.try_get(column).map_err(|error| {
        Diagnostic::new(
            DiagnosticKind::Internal,
            format!("replication catalog column '{column}' had an unexpected type: {error}"),
            "reading replication catalogs",
        )
        .next_action("report this with the server version and the replication report output")
    })
}
