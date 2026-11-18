//! Pure keyset-paginated data browser and batch CRUD SQL synthesis domain model.
//!
//! This module implements the domain logic for Feature
//! `053-live-data-browser-and-batch-crud` (`Milestone 13: Live Data CRUD, Plan Diff & Analytics`).
//! It contains zero database, filesystem, network, or terminal dependencies:
//! it builds bounded keyset-paginated `SELECT` queries, synthesizes transactional
//! reviewed `INSERT`/`UPDATE`/`DELETE` scripts with mandatory primary-key guards
//! and literal/identifier quoting, extracts keyset cursors from rows, and
//! renders deterministic plain-ASCII table, plain-text, and JSON page summaries.

#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fmt::Write as _;

use serde::{Deserialize, Serialize};

use crate::diagnostics::{Diagnostic, DiagnosticKind};
use crate::query::identifiers::quote_identifier;
use crate::query::value::{Cell, NULL_MARKER, sanitize_for_display};

/// Default number of rows requested per keyset page.
pub const DEFAULT_PAGE_SIZE: usize = 50;

/// Maximum number of rows permitted per keyset page.
pub const MAX_PAGE_SIZE: usize = 500;

/// Maximum number of row mutations permitted in a single batch CRUD plan.
pub const MAX_BATCH_MUTATIONS: usize = 200;

const TABLE_COL_MAX_WIDTH: usize = 32;

/// Sort direction for keyset-paginated table browsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SortOrder {
    /// Ascending order (`ASC`).
    #[default]
    Asc,
    /// Descending order (`DESC`).
    Desc,
}

impl SortOrder {
    /// Returns the opposite sort direction, used when paginating backward (`Prev`).
    #[must_use]
    pub const fn flipped(self) -> Self {
        match self {
            Self::Asc => Self::Desc,
            Self::Desc => Self::Asc,
        }
    }

    /// Uppercase SQL `ORDER BY` keyword.
    #[must_use]
    pub const fn sql_keyword(self) -> &'static str {
        match self {
            Self::Asc => "ASC",
            Self::Desc => "DESC",
        }
    }

    /// Stable lowercase label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Asc => "asc",
            Self::Desc => "desc",
        }
    }
}

impl fmt::Display for SortOrder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// Pagination step direction relative to a keyset cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum PageDirection {
    /// Advance forward from the cursor (or fetch the initial page).
    #[default]
    Next,
    /// Step backward before the cursor, requiring an order flip and row reversal.
    Prev,
}

impl PageDirection {
    /// Stable lowercase label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Next => "next",
            Self::Prev => "prev",
        }
    }
}

impl fmt::Display for PageDirection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// Primary-key position anchor used for keyset pagination.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeysetCursor {
    /// Primary-key column names in key order.
    pub pk_columns: Vec<String>,
    /// Primary-key values serialized in PostgreSQL text format (`None` for NULL).
    pub pk_values: Vec<Option<String>>,
}

/// Input parameters for a bounded, keyset-paginated table browse query.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowseRequest {
    /// Target schema name.
    pub schema: String,
    /// Target table or view name.
    pub table: String,
    /// Projected column names (empty selects `*` in pure SQL synthesis or triggers metadata lookup).
    pub columns: Vec<String>,
    /// Primary-key column names in key order (empty falls back to `ctid` ordering).
    pub pk_columns: Vec<String>,
    /// Optional primary sort column placed ahead of the primary-key tie-breaker.
    pub sort_column: Option<String>,
    /// Requested display sort order.
    pub sort_order: SortOrder,
    /// Desired number of rows per page (clamped to `1..=MAX_PAGE_SIZE`).
    pub page_size: usize,
    /// Optional keyset cursor for `Next` / `Prev` navigation.
    pub cursor: Option<KeysetCursor>,
    /// Direction of travel relative to `cursor`.
    pub direction: PageDirection,
    /// Optional single-expression SQL `WHERE` predicate (validated against injection tokens).
    pub filter_predicate: Option<String>,
}

impl BrowseRequest {
    /// Creates a default browse request for the given schema and table.
    #[must_use]
    pub fn new(schema: impl Into<String>, table: impl Into<String>) -> Self {
        Self {
            schema: schema.into(),
            table: table.into(),
            columns: Vec::new(),
            pk_columns: Vec::new(),
            sort_column: None,
            sort_order: SortOrder::Asc,
            page_size: DEFAULT_PAGE_SIZE,
            cursor: None,
            direction: PageDirection::Next,
            filter_predicate: None,
        }
    }

    /// Returns `page_size` clamped to `1..=MAX_PAGE_SIZE`.
    #[must_use]
    pub fn effective_page_size(&self) -> usize {
        self.page_size.clamp(1, MAX_PAGE_SIZE)
    }

    /// Returns `true` when fetched rows must be reversed after trimming to `page_size`.
    ///
    /// Backward keyset pagination (`PageDirection::Prev` with a cursor) flips the
    /// SQL sort order so `LIMIT (page_size + 1)` captures the rows closest to the
    /// cursor; reversing the trimmed rows restores the caller's display order.
    #[must_use]
    pub const fn needs_row_reversal(&self) -> bool {
        self.cursor.is_some() && matches!(self.direction, PageDirection::Prev)
    }
}

/// Builds a bounded keyset-paginated `SELECT` statement for [`BrowseRequest`].
///
/// Requests `LIMIT (clamped_page_size + 1)` rows so callers can determine
/// `has_more` without running a separate `COUNT(*)`.
pub fn build_keyset_browse_sql(req: &BrowseRequest) -> Result<String, Diagnostic> {
    validate_identifier(&req.schema, "schema name")?;
    validate_identifier(&req.table, "table name")?;

    for col in &req.columns {
        validate_identifier(col, "projected column name")?;
    }
    for pk in &req.pk_columns {
        validate_identifier(pk, "primary key column name")?;
    }
    if let Some(sort_col) = &req.sort_column {
        validate_identifier(sort_col, "sort column name")?;
    }

    let filter = match &req.filter_predicate {
        Some(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                None
            } else {
                validate_filter_predicate(trimmed)?;
                Some(trimmed)
            }
        }
        None => None,
    };

    let reverse_rows = req.needs_row_reversal();
    let effective_order = if reverse_rows {
        req.sort_order.flipped()
    } else {
        req.sort_order
    };
    let order_kw = effective_order.sql_keyword();
    let cmp_op = match effective_order {
        SortOrder::Asc => ">",
        SortOrder::Desc => "<",
    };

    let mut where_clauses = Vec::new();
    if let Some(predicate) = filter {
        where_clauses.push(format!("({predicate})"));
    }

    if let Some(cursor) = &req.cursor {
        if req.pk_columns.is_empty() {
            return Err(Diagnostic::new(
                DiagnosticKind::Usage,
                "keyset cursor navigation requires primary key columns",
                "building keyset browse query",
            )
            .next_action("provide primary key columns or browse without a cursor"));
        }
        if cursor.pk_columns != req.pk_columns {
            return Err(Diagnostic::new(
                DiagnosticKind::Usage,
                "keyset cursor columns do not match table primary key columns",
                "building keyset browse query",
            )
            .next_action("refresh the table page to obtain a matching cursor"));
        }
        if cursor.pk_values.len() != cursor.pk_columns.len() {
            return Err(Diagnostic::new(
                DiagnosticKind::Usage,
                "keyset cursor value count does not match primary key column count",
                "building keyset browse query",
            )
            .next_action("provide one cursor value for each primary key column"));
        }

        let mut quoted_cols = Vec::with_capacity(cursor.pk_columns.len());
        let mut quoted_vals = Vec::with_capacity(cursor.pk_values.len());
        for (col, opt_val) in cursor.pk_columns.iter().zip(&cursor.pk_values) {
            validate_identifier(col, "cursor primary key column")?;
            let Some(val) = opt_val else {
                return Err(Diagnostic::new(
                    DiagnosticKind::Usage,
                    format!("keyset cursor value for primary key column '{col}' cannot be NULL"),
                    "building keyset browse query",
                )
                .next_action("supply non-null primary key values in the cursor"));
            };
            quoted_cols.push(quote_identifier(col));
            quoted_vals.push(quote_literal(val)?);
        }

        let keyset_expr = if quoted_cols.len() == 1 {
            format!("{} {cmp_op} {}", quoted_cols[0], quoted_vals[0])
        } else {
            format!(
                "({}) {cmp_op} ({})",
                quoted_cols.join(", "),
                quoted_vals.join(", ")
            )
        };
        where_clauses.push(keyset_expr);
    }

    let select_list = if req.columns.is_empty() {
        "*".to_owned()
    } else {
        req.columns
            .iter()
            .map(|c| quote_identifier(c))
            .collect::<Vec<_>>()
            .join(", ")
    };

    let mut order_terms = Vec::new();
    if let Some(sort_col) = &req.sort_column {
        order_terms.push(format!("{} {order_kw}", quote_identifier(sort_col)));
    }
    for pk in &req.pk_columns {
        if req.sort_column.as_deref() != Some(pk.as_str()) {
            order_terms.push(format!("{} {order_kw}", quote_identifier(pk)));
        }
    }
    if req.pk_columns.is_empty() {
        order_terms.push(format!("ctid {order_kw}"));
    }

    let where_sql = if where_clauses.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", where_clauses.join(" AND "))
    };

    let limit = req.effective_page_size() + 1;
    let qualified_table = format!(
        "{}.{}",
        quote_identifier(&req.schema),
        quote_identifier(&req.table)
    );
    let mut sql = format!(
        "SELECT {select_list} FROM {qualified_table}{where_sql} ORDER BY {} LIMIT {limit};",
        order_terms.join(", ")
    );
    if reverse_rows {
        sql.push_str(" -- prev-page: order flipped, reverse rows after trim");
    }
    Ok(sql)
}

/// Extracts a [`KeysetCursor`] from a text-encoded row when all `pk_columns` are present and non-NULL.
#[must_use]
pub fn extract_cursor_from_row(
    columns: &[String],
    pk_columns: &[String],
    row: &[Option<String>],
) -> Option<KeysetCursor> {
    if pk_columns.is_empty() {
        return None;
    }
    let mut pk_values = Vec::with_capacity(pk_columns.len());
    for pk in pk_columns {
        let idx = columns.iter().position(|c| c == pk)?;
        let val = row.get(idx)?.as_ref()?;
        pk_values.push(Some(val.clone()));
    }
    Some(KeysetCursor {
        pk_columns: pk_columns.to_vec(),
        pk_values,
    })
}

/// Extracts a [`KeysetCursor`] from a [`Cell`] row when all `pk_columns` are present and non-NULL.
#[must_use]
pub fn extract_cursor_from_cells(
    columns: &[String],
    pk_columns: &[String],
    row: &[Cell],
) -> Option<KeysetCursor> {
    if pk_columns.is_empty() {
        return None;
    }
    let mut pk_values = Vec::with_capacity(pk_columns.len());
    for pk in pk_columns {
        let idx = columns.iter().position(|c| c == pk)?;
        let val = row.get(idx)?.raw()?;
        pk_values.push(Some(val.to_owned()));
    }
    Some(KeysetCursor {
        pk_columns: pk_columns.to_vec(),
        pk_values,
    })
}

/// One row-level mutation queued in a [`BatchCrudPlan`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RowMutation {
    /// Insert one row with `(column_name, optional_text_value)` pairs.
    Insert {
        /// Column/value pairs to insert (`None` renders SQL `NULL`).
        values: Vec<(String, Option<String>)>,
    },
    /// Update one primary-key-identified row.
    Update {
        /// Primary-key `(column_name, text_value)` equality pairs.
        pk_match: Vec<(String, String)>,
        /// Target `(column_name, optional_text_value)` pairs (`None` renders SQL `NULL`).
        changes: Vec<(String, Option<String>)>,
    },
    /// Delete one primary-key-identified row.
    Delete {
        /// Primary-key `(column_name, text_value)` equality pairs.
        pk_match: Vec<(String, String)>,
    },
}

/// Bounded transactional batch CRUD plan that synthesizes reviewed SQL without executing it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchCrudPlan {
    /// Target schema name.
    pub schema: String,
    /// Target table name.
    pub table: String,
    /// Primary-key column names required for all `Update` and `Delete` mutations.
    pub pk_columns: Vec<String>,
    /// Ordered list of row mutations (bounded by `MAX_BATCH_MUTATIONS`).
    pub mutations: Vec<RowMutation>,
    /// When `true`, closes the transaction with `ROLLBACK; -- dry-run`; otherwise `COMMIT;`.
    pub dry_run_rollback: bool,
}

impl BatchCrudPlan {
    /// Synthesizes a reviewed, transactional SQL script for all mutations in the batch.
    ///
    /// Refuses empty batches, batches exceeding [`MAX_BATCH_MUTATIONS`], empty `Update`
    /// change sets, keyless `Update`/`Delete` mutations, partial/mismatched `pk_match`
    /// predicates, and NUL bytes with explicit [`DiagnosticKind::Usage`] diagnostics.
    pub fn build_reviewed_sql(&self) -> Result<String, Diagnostic> {
        validate_identifier(&self.schema, "schema name")?;
        validate_identifier(&self.table, "table name")?;

        if self.mutations.is_empty() {
            return Err(Diagnostic::new(
                DiagnosticKind::Usage,
                "batch CRUD plan must contain at least one mutation",
                "synthesizing batch CRUD SQL",
            )
            .next_action("stage one or more Insert, Update, or Delete mutations first"));
        }
        if self.mutations.len() > MAX_BATCH_MUTATIONS {
            return Err(Diagnostic::new(
                DiagnosticKind::Usage,
                format!(
                    "batch CRUD plan has {} mutations, exceeding the limit of {MAX_BATCH_MUTATIONS}",
                    self.mutations.len()
                ),
                "synthesizing batch CRUD SQL",
            )
            .next_action("split the changes into smaller batches and try again"));
        }

        let mut seen_pks = HashSet::with_capacity(self.pk_columns.len());
        for pk in &self.pk_columns {
            validate_identifier(pk, "primary key column name")?;
            if !seen_pks.insert(pk.as_str()) {
                return Err(Diagnostic::new(
                    DiagnosticKind::Usage,
                    format!("duplicate primary key column '{pk}' in batch CRUD plan"),
                    "synthesizing batch CRUD SQL",
                )
                .next_action("deduplicate primary key columns and try again"));
            }
        }

        let qualified_table = format!(
            "{}.{}",
            quote_identifier(&self.schema),
            quote_identifier(&self.table)
        );
        let mode_label = if self.dry_run_rollback {
            "dry-run rollback"
        } else {
            "commit"
        };

        let mut out = String::new();
        let _ = writeln!(
            out,
            "-- Reviewed batch CRUD plan for {qualified_table} ({} mutations, {mode_label})",
            self.mutations.len()
        );
        out.push_str("BEGIN;\n");

        for mutation in &self.mutations {
            match mutation {
                RowMutation::Insert { values } => {
                    if values.is_empty() {
                        return Err(Diagnostic::new(
                            DiagnosticKind::Usage,
                            "Insert mutation must specify at least one column/value pair",
                            "synthesizing batch INSERT",
                        )
                        .next_action("provide at least one column and value for the new row"));
                    }
                    let mut seen_cols = HashSet::with_capacity(values.len());
                    let mut cols = Vec::with_capacity(values.len());
                    let mut vals = Vec::with_capacity(values.len());
                    for (col, val) in values {
                        validate_identifier(col, "insert column name")?;
                        if !seen_cols.insert(col.as_str()) {
                            return Err(Diagnostic::new(
                                DiagnosticKind::Usage,
                                format!("duplicate insert column '{col}'"),
                                "synthesizing batch INSERT",
                            )
                            .next_action("specify each target column at most once"));
                        }
                        cols.push(quote_identifier(col));
                        vals.push(format_optional_literal(val.as_deref())?);
                    }
                    let _ = writeln!(
                        out,
                        "INSERT INTO {qualified_table} ({}) VALUES ({});",
                        cols.join(", "),
                        vals.join(", ")
                    );
                }
                RowMutation::Update { pk_match, changes } => {
                    let where_clause = self.build_pk_where_clause(pk_match, "UPDATE")?;
                    if changes.is_empty() {
                        return Err(Diagnostic::new(
                            DiagnosticKind::Usage,
                            "Update mutation must specify at least one column change",
                            "synthesizing batch UPDATE",
                        )
                        .next_action("provide one or more changed columns"));
                    }
                    let mut seen_cols = HashSet::with_capacity(changes.len());
                    let mut assignments = Vec::with_capacity(changes.len());
                    for (col, val) in changes {
                        validate_identifier(col, "update column name")?;
                        if !seen_cols.insert(col.as_str()) {
                            return Err(Diagnostic::new(
                                DiagnosticKind::Usage,
                                format!("duplicate update column '{col}'"),
                                "synthesizing batch UPDATE",
                            )
                            .next_action("specify each updated column at most once"));
                        }
                        let lit = format_optional_literal(val.as_deref())?;
                        assignments.push(format!("{} = {lit}", quote_identifier(col)));
                    }
                    let _ = writeln!(
                        out,
                        "UPDATE {qualified_table} SET {} WHERE {where_clause};",
                        assignments.join(", ")
                    );
                }
                RowMutation::Delete { pk_match } => {
                    let where_clause = self.build_pk_where_clause(pk_match, "DELETE")?;
                    let _ = writeln!(out, "DELETE FROM {qualified_table} WHERE {where_clause};");
                }
            }
        }

        if self.dry_run_rollback {
            out.push_str("ROLLBACK; -- dry-run\n");
        } else {
            out.push_str("COMMIT;\n");
        }
        Ok(out)
    }

    fn build_pk_where_clause(
        &self,
        pk_match: &[(String, String)],
        op: &str,
    ) -> Result<String, Diagnostic> {
        if self.pk_columns.is_empty() {
            return Err(Diagnostic::new(
                DiagnosticKind::Usage,
                format!("keyless {op} refused: table has no primary key columns configured"),
                format!("synthesizing batch {op}"),
            )
            .likely_cause(
                "an UPDATE or DELETE without primary-key predicates can modify unintended rows",
            )
            .next_action("add primary key columns to the batch plan before synthesizing SQL"));
        }
        if pk_match.is_empty() {
            return Err(Diagnostic::new(
                DiagnosticKind::Usage,
                format!("{op} mutation requires non-empty primary key match values"),
                format!("synthesizing batch {op}"),
            )
            .next_action("supply values for every primary key column"));
        }

        let mut match_map: HashMap<&str, &str> = HashMap::with_capacity(pk_match.len());
        for (col, val) in pk_match {
            validate_identifier(col, "primary key match column")?;
            if match_map.insert(col.as_str(), val.as_str()).is_some() {
                return Err(Diagnostic::new(
                    DiagnosticKind::Usage,
                    format!("duplicate primary key column '{col}' in {op} match"),
                    format!("synthesizing batch {op}"),
                )
                .next_action("supply each primary key column exactly once"));
            }
        }

        if match_map.len() != self.pk_columns.len()
            || !self
                .pk_columns
                .iter()
                .all(|pk| match_map.contains_key(pk.as_str()))
        {
            return Err(Diagnostic::new(
                DiagnosticKind::Usage,
                format!(
                    "{op} primary key match does not cover all configured primary key columns ({})",
                    self.pk_columns.join(", ")
                ),
                format!("synthesizing batch {op}"),
            )
            .next_action("match every primary key column exactly"));
        }

        let mut clauses = Vec::with_capacity(self.pk_columns.len());
        for pk in &self.pk_columns {
            let val = match_map[pk.as_str()];
            let lit = quote_literal(val)?;
            clauses.push(format!("{} = {lit}", quote_identifier(pk)));
        }
        Ok(clauses.join(" AND "))
    }
}

/// Summary of one keyset-paginated browse page with deterministic renderers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowsePage {
    /// Target schema name.
    pub schema: String,
    /// Target table name.
    pub table: String,
    /// Projected column names in display order.
    pub columns: Vec<String>,
    /// Primary-key column names in key order.
    pub pk_columns: Vec<String>,
    /// Retained row values in display order (`None` represents SQL `NULL`).
    pub rows: Vec<Vec<Option<String>>>,
    /// Effective page size applied to this page.
    pub page_size: usize,
    /// Whether at least one more row exists forward (`LIMIT page_size + 1` lookahead).
    pub has_more: bool,
    /// Whether a preceding page exists behind the current page.
    pub has_prev: bool,
    /// Cursor pointing at the last row of this page when `has_more` is true.
    pub next_cursor: Option<KeysetCursor>,
    /// Cursor pointing at the first row of this page when `has_prev` is true.
    pub prev_cursor: Option<KeysetCursor>,
}

impl BrowsePage {
    /// Renders an aligned plain-ASCII table summary suitable for terminals and plain logs.
    #[must_use]
    pub fn render_table(&self) -> String {
        let mut out = String::new();
        let pk_label = if self.pk_columns.is_empty() {
            "none (ctid)".to_owned()
        } else {
            self.pk_columns.join(",")
        };
        let _ = writeln!(
            out,
            "Table: {}.{} (rows={}, page_size={}, pk=[{}], has_prev={}, has_more={})",
            quote_identifier(&self.schema),
            quote_identifier(&self.table),
            self.rows.len(),
            self.page_size,
            pk_label,
            self.has_prev,
            self.has_more
        );

        if self.columns.is_empty() {
            out.push_str("(no columns)\n");
            return out;
        }

        let mut widths: Vec<usize> = self
            .columns
            .iter()
            .map(|c| {
                sanitize_for_display(c)
                    .chars()
                    .count()
                    .clamp(4, TABLE_COL_MAX_WIDTH)
            })
            .collect();

        for row in &self.rows {
            for (idx, cell) in row.iter().enumerate() {
                if let Some(w) = widths.get_mut(idx) {
                    let rendered = format_cell_display(cell.as_deref());
                    let len = rendered.chars().count().min(TABLE_COL_MAX_WIDTH);
                    if len > *w {
                        *w = len;
                    }
                }
            }
        }

        let header_cells: Vec<String> = self
            .columns
            .iter()
            .enumerate()
            .map(|(i, col)| {
                let text = truncate_ascii(&sanitize_for_display(col), widths[i]);
                format!("{text:<width$}", width = widths[i])
            })
            .collect();
        let _ = writeln!(out, "| {} |", header_cells.join(" | "));

        let rule_cells: Vec<String> = widths.iter().map(|w| "-".repeat(*w)).collect();
        let _ = writeln!(out, "+-{}-+", rule_cells.join("-+-"));

        if self.rows.is_empty() {
            out.push_str("(0 rows)\n");
        } else {
            for row in &self.rows {
                let cells: Vec<String> = (0..self.columns.len())
                    .map(|i| {
                        let raw = row.get(i).and_then(Option::as_deref);
                        let rendered = truncate_ascii(&format_cell_display(raw), widths[i]);
                        format!("{rendered:<width$}", width = widths[i])
                    })
                    .collect();
                let _ = writeln!(out, "| {} |", cells.join(" | "));
            }
        }

        if let Some(prev) = &self.prev_cursor {
            let _ = writeln!(out, "prev_cursor: {}", format_cursor_summary(prev));
        }
        if let Some(next) = &self.next_cursor {
            let _ = writeln!(out, "next_cursor: {}", format_cursor_summary(next));
        }

        out
    }

    /// Renders untruncated plain-ASCII key/value lines for scripts and logs.
    #[must_use]
    pub fn render_plain(&self) -> String {
        let mut out = String::new();
        let pk_label = if self.pk_columns.is_empty() {
            "-".to_owned()
        } else {
            self.pk_columns.join(",")
        };
        let _ = writeln!(
            out,
            "browse schema={} table={} rows={} page_size={} pk={} has_prev={} has_more={}",
            sanitize_for_display(&self.schema),
            sanitize_for_display(&self.table),
            self.rows.len(),
            self.page_size,
            sanitize_for_display(&pk_label),
            self.has_prev,
            self.has_more
        );
        let cols_line = if self.columns.is_empty() {
            "-".to_owned()
        } else {
            self.columns
                .iter()
                .map(|c| sanitize_for_display(c))
                .collect::<Vec<_>>()
                .join(",")
        };
        let _ = writeln!(out, "columns={cols_line}");

        if let Some(prev) = &self.prev_cursor {
            let _ = writeln!(out, "prev_cursor={}", format_cursor_summary(prev));
        }
        if let Some(next) = &self.next_cursor {
            let _ = writeln!(out, "next_cursor={}", format_cursor_summary(next));
        }

        for (row_idx, row) in self.rows.iter().enumerate() {
            let mut fields = Vec::with_capacity(self.columns.len());
            for (col_idx, col) in self.columns.iter().enumerate() {
                let raw = row.get(col_idx).and_then(Option::as_deref);
                let val = match raw {
                    None => NULL_MARKER.to_owned(),
                    Some("") => "\"\"".to_owned(),
                    Some(text) => sanitize_for_display(text),
                };
                fields.push(format!("{}={}", sanitize_for_display(col), val));
            }
            let _ = writeln!(out, "row[{row_idx}] {}", fields.join(" "));
        }
        out
    }

    /// Renders the browse page as pretty-printed JSON.
    #[must_use]
    pub fn render_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_owned())
    }
}

fn format_cell_display(value: Option<&str>) -> String {
    match value {
        None => NULL_MARKER.to_owned(),
        Some(text) => sanitize_for_display(text),
    }
}

fn format_cursor_summary(cursor: &KeysetCursor) -> String {
    cursor
        .pk_columns
        .iter()
        .zip(&cursor.pk_values)
        .map(|(col, val)| {
            let v = match val {
                None => NULL_MARKER.to_owned(),
                Some(s) => sanitize_for_display(s),
            };
            format!("{}={v}", sanitize_for_display(col))
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn truncate_ascii(text: &str, max_len: usize) -> String {
    if text.chars().count() <= max_len {
        return text.to_owned();
    }
    if max_len <= 3 {
        return text.chars().take(max_len).collect();
    }
    let prefix: String = text.chars().take(max_len - 3).collect();
    format!("{prefix}...")
}

fn validate_identifier(name: &str, label: &str) -> Result<(), Diagnostic> {
    if name.trim().is_empty() {
        return Err(Diagnostic::new(
            DiagnosticKind::Usage,
            format!("{label} cannot be empty"),
            "validating SQL identifier",
        )
        .next_action(format!("provide a non-empty {label}")));
    }
    if name.contains('\0') {
        return Err(Diagnostic::new(
            DiagnosticKind::Usage,
            format!("{label} cannot contain NUL bytes"),
            "validating SQL identifier",
        )
        .next_action(format!("remove NUL bytes from the {label}")));
    }
    Ok(())
}

fn quote_literal(value: &str) -> Result<String, Diagnostic> {
    if value.contains('\0') {
        return Err(Diagnostic::new(
            DiagnosticKind::Usage,
            "SQL literal values cannot contain NUL bytes",
            "formatting SQL literal",
        )
        .next_action("remove NUL bytes from the value and try again"));
    }
    Ok(format!("'{}'", value.replace('\'', "''")))
}

fn format_optional_literal(value: Option<&str>) -> Result<String, Diagnostic> {
    match value {
        None => Ok("NULL".to_owned()),
        Some(text) => quote_literal(text),
    }
}

fn validate_filter_predicate(predicate: &str) -> Result<(), Diagnostic> {
    if predicate.contains('\0') {
        return Err(Diagnostic::new(
            DiagnosticKind::Usage,
            "filter predicate cannot contain NUL bytes",
            "validating filter predicate",
        )
        .next_action("remove NUL bytes from the filter expression"));
    }
    if predicate.contains(';') {
        return Err(Diagnostic::new(
            DiagnosticKind::Usage,
            "filter predicate cannot contain semicolons (`;`)",
            "validating filter predicate",
        )
        .likely_cause("semicolons can terminate statements and introduce multi-statement SQL")
        .next_action("enter a single boolean WHERE expression without semicolons"));
    }
    if predicate.contains("--") || predicate.contains("/*") || predicate.contains("*/") {
        return Err(Diagnostic::new(
            DiagnosticKind::Usage,
            "filter predicate cannot contain SQL comment delimiters (`--`, `/*`, `*/`)",
            "validating filter predicate",
        )
        .next_action("remove SQL comments from the filter expression"));
    }

    let mut in_single_quote = false;
    let mut paren_depth: usize = 0;
    let mut chars = predicate.chars().peekable();
    while let Some(ch) = chars.next() {
        if in_single_quote {
            if ch == '\'' {
                if chars.peek() == Some(&'\'') {
                    let _ = chars.next();
                } else {
                    in_single_quote = false;
                }
            }
            continue;
        }
        match ch {
            '\'' => in_single_quote = true,
            '(' => paren_depth += 1,
            ')' => {
                if paren_depth == 0 {
                    return Err(Diagnostic::new(
                        DiagnosticKind::Usage,
                        "filter predicate has an unmatched closing parenthesis `)`",
                        "validating filter predicate",
                    )
                    .next_action("balance all parentheses in the filter expression"));
                }
                paren_depth -= 1;
            }
            _ => {}
        }
    }

    if in_single_quote {
        return Err(Diagnostic::new(
            DiagnosticKind::Usage,
            "filter predicate has an unterminated string literal",
            "validating filter predicate",
        )
        .next_action("close all single-quoted string literals in the filter expression"));
    }
    if paren_depth != 0 {
        return Err(Diagnostic::new(
            DiagnosticKind::Usage,
            "filter predicate has an unclosed opening parenthesis `(`",
            "validating filter predicate",
        )
        .next_action("balance all parentheses in the filter expression"));
    }

    Ok(())
}
