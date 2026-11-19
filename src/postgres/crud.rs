//! Async PostgreSQL metadata and keyset-paginated data browser adapter.
//!
//! Provides read-only column and primary-key inspection against `pg_catalog`
//! (`pg_attribute` and `pg_index`) plus bounded keyset-paginated `SELECT`
//! execution returning both [`ResultSet`] and [`BrowsePage`].

use tokio_postgres::{Client, SimpleQueryMessage};

use crate::crud::{
    BrowsePage, BrowseRequest, KeysetCursor, PageDirection, build_keyset_browse_sql,
    extract_cursor_from_row,
};
use crate::diagnostics::{Diagnostic, DiagnosticKind};
use crate::postgres::error::from_query_error;
use crate::query::result::ResultSet;
use crate::query::value::{Cell, MAX_RETAINED_VALUE_BYTES};

const COLUMNS_SQL: &str = r#"
SELECT a.attname::text AS column_name
FROM pg_catalog.pg_attribute a
JOIN pg_catalog.pg_class c ON c.oid = a.attrelid
JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
WHERE n.nspname = $1
  AND c.relname = $2
  AND c.relkind IN ('r', 'p', 'v', 'm', 'f')
  AND a.attnum > 0
  AND NOT a.attisdropped
ORDER BY a.attnum
"#;

const PRIMARY_KEY_SQL: &str = r#"
SELECT a.attname::text AS column_name
FROM pg_catalog.pg_index i
JOIN pg_catalog.pg_class c ON c.oid = i.indrelid
JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
CROSS JOIN LATERAL pg_catalog.generate_subscripts(i.indkey, 1) AS k(pos)
JOIN pg_catalog.pg_attribute a
  ON a.attrelid = c.oid
 AND a.attnum = i.indkey[k.pos]
WHERE n.nspname = $1
  AND c.relname = $2
  AND i.indisprimary
  AND NOT a.attisdropped
ORDER BY k.pos
"#;

/// Combined result of a keyset-paginated browse query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowsePageResult {
    /// Structured browse page summary with plain-ASCII renderers.
    pub page: BrowsePage,
    /// Repo-standard retained result set (`Cell::Null` / `Cell::Text`).
    pub result_set: ResultSet,
    /// Exact read-only SQL query submitted to PostgreSQL.
    pub sql: String,
    /// Whether additional rows exist beyond this page in the forward direction.
    pub has_more: bool,
    /// Derived keyset cursor for advancing to the next page, when `has_more` is true.
    pub next_cursor: Option<KeysetCursor>,
    /// Derived keyset cursor for stepping backward to the previous page, when `has_prev` is true.
    pub prev_cursor: Option<KeysetCursor>,
}

/// Inspects a table's visible column names (in `attnum` order) and primary-key
/// column names (in key order).
pub async fn inspect_table_columns_and_pk(
    client: &Client,
    schema: &str,
    table: &str,
) -> Result<(Vec<String>, Vec<String>), Diagnostic> {
    if schema.trim().is_empty() || table.trim().is_empty() {
        return Err(Diagnostic::new(
            DiagnosticKind::Usage,
            "schema and table names cannot be empty",
            "inspecting table columns and primary key",
        )
        .next_action("provide non-empty schema and table names"));
    }

    let col_rows = client
        .query(COLUMNS_SQL, &[&schema, &table])
        .await
        .map_err(|err| from_query_error(&err, 1))?;

    if col_rows.is_empty() {
        return Err(Diagnostic::new(
            DiagnosticKind::Query,
            format!("table \"{schema}\".\"{table}\" was not found or has no visible columns"),
            "inspecting table columns and primary key",
        )
        .next_action("verify the schema and table names and your SELECT privileges"));
    }

    let mut columns = Vec::with_capacity(col_rows.len());
    for row in &col_rows {
        let name: String = row.try_get("column_name").map_err(|err| {
            Diagnostic::new(
                DiagnosticKind::Internal,
                format!("column metadata row had unexpected type: {err}"),
                "inspecting table columns",
            )
        })?;
        columns.push(name);
    }

    let pk_rows = client
        .query(PRIMARY_KEY_SQL, &[&schema, &table])
        .await
        .map_err(|err| from_query_error(&err, 2))?;

    let mut pk_columns = Vec::with_capacity(pk_rows.len());
    for row in &pk_rows {
        let name: String = row.try_get("column_name").map_err(|err| {
            Diagnostic::new(
                DiagnosticKind::Internal,
                format!("primary key metadata row had unexpected type: {err}"),
                "inspecting table primary key",
            )
        })?;
        pk_columns.push(name);
    }

    Ok((columns, pk_columns))
}

/// Executes a bounded read-only keyset browse query and returns the trimmed page,
/// [`ResultSet`], and derived navigation cursors.
pub async fn browse_table_page(
    client: &Client,
    req: &BrowseRequest,
) -> Result<BrowsePageResult, Diagnostic> {
    let mut effective_req = req.clone();
    if effective_req.columns.is_empty() {
        let (cols, pks) = inspect_table_columns_and_pk(client, &req.schema, &req.table).await?;
        effective_req.columns = cols;
        if effective_req.pk_columns.is_empty() {
            effective_req.pk_columns = pks;
        }
    }

    let sql = build_keyset_browse_sql(&effective_req)?;
    let page_size = effective_req.effective_page_size();
    let reverse_rows = effective_req.needs_row_reversal();

    let messages = client
        .simple_query(&sql)
        .await
        .map_err(|err| from_query_error(&err, 1))?;

    let mut result_columns: Option<Vec<String>> = None;
    let mut raw_text_rows: Vec<Vec<Option<String>>> = Vec::new();
    let mut raw_cell_rows: Vec<Vec<Cell>> = Vec::new();

    for message in messages {
        match message {
            SimpleQueryMessage::RowDescription(cols) => {
                result_columns = Some(cols.iter().map(|c| c.name().to_owned()).collect());
            }
            SimpleQueryMessage::Row(row) => {
                let mut text_row = Vec::with_capacity(row.len());
                let mut cell_row = Vec::with_capacity(row.len());
                for i in 0..row.len() {
                    let val = row.get(i);
                    let cell = Cell::from_option_bounded(val).map_err(|bytes| {
                        Diagnostic::new(
                            DiagnosticKind::Query,
                            format!(
                                "browsed cell value is {bytes} bytes, exceeding the {MAX_RETAINED_VALUE_BYTES}-byte limit"
                            ),
                            "retaining browsed table row",
                        )
                        .next_action("exclude oversized columns from the projection or stream to export")
                    })?;
                    text_row.push(val.map(str::to_owned));
                    cell_row.push(cell);
                }
                raw_text_rows.push(text_row);
                raw_cell_rows.push(cell_row);
            }
            SimpleQueryMessage::CommandComplete(_) => {}
            _ => {}
        }
    }

    let columns = result_columns.unwrap_or_else(|| effective_req.columns.clone());
    let fetched_extra = raw_text_rows.len() > page_size;
    raw_text_rows.truncate(page_size);
    raw_cell_rows.truncate(page_size);

    if reverse_rows {
        raw_text_rows.reverse();
        raw_cell_rows.reverse();
    }

    let (has_more, has_prev) = match effective_req.direction {
        PageDirection::Next => (fetched_extra, effective_req.cursor.is_some()),
        PageDirection::Prev => {
            if effective_req.cursor.is_some() {
                (!raw_text_rows.is_empty(), fetched_extra)
            } else {
                (fetched_extra, false)
            }
        }
    };

    let next_cursor = if has_more {
        raw_text_rows
            .last()
            .and_then(|row| extract_cursor_from_row(&columns, &effective_req.pk_columns, row))
    } else {
        None
    };

    let prev_cursor = if has_prev {
        raw_text_rows
            .first()
            .and_then(|row| extract_cursor_from_row(&columns, &effective_req.pk_columns, row))
    } else {
        None
    };

    let mut result_set = ResultSet::new(columns.clone(), page_size);
    for cell_row in raw_cell_rows {
        let _ = result_set.push(cell_row);
    }

    let page = BrowsePage {
        schema: effective_req.schema,
        table: effective_req.table,
        columns,
        pk_columns: effective_req.pk_columns,
        rows: raw_text_rows,
        page_size,
        has_more,
        has_prev,
        next_cursor: next_cursor.clone(),
        prev_cursor: prev_cursor.clone(),
    };

    Ok(BrowsePageResult {
        page,
        result_set,
        sql,
        has_more,
        next_cursor,
        prev_cursor,
    })
}
