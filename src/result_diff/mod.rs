//! Bounded in-memory and offline A/B result-set diffing (`067` Slice 2).
//!
//! Snapshots and diffs are strictly in-memory or loaded from bounded local
//! JSON/NDJSON exports. Nothing in this module ever touches disk on its own or
//! opens a database or network connection.

use crate::diagnostics::{Diagnostic, DiagnosticKind};
use crate::query::result::{ResultRetention, ResultSet};
use crate::query::value::{Cell, sanitize_for_display, truncate_to_width};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write as _;
use std::io::Read as _;
use std::path::Path;

/// Schema version emitted by [`ResultDiff::render_json`].
pub const RESULT_DIFF_SCHEMA_VERSION: u32 = 1;

/// Maximum input file size accepted by `ignatius result-diff` (16 MiB).
pub const MAX_DIFF_INPUT_BYTES: usize = 16 * 1024 * 1024;

/// Maximum rows retained in a [`PinnedResult`] or classified in [`ResultDiff::entries`].
pub const MAX_DIFF_ROWS: usize = 10_000;

/// Maximum columns admitted from an offline JSON/NDJSON file.
pub const MAX_DIFF_COLUMNS: usize = 512;

/// Maximum cell text bytes retained per cell in a diff snapshot or entry.
pub const MAX_DIFF_CELL_BYTES: usize = 64 * 1024;

/// Maximum differing cells recorded on a single [`DiffRowKind::Changed`] row.
pub const MAX_CELL_DIFFS_PER_ROW: usize = 256;

/// Maximum UTF-8 bytes kept in [`PinnedResult::source_sql_summary`].
pub const MAX_SOURCE_SQL_SUMMARY_BYTES: usize = 160;

/// Maximum JSON nesting depth accepted while parsing offline diff inputs.
const MAX_JSON_NESTING_DEPTH: usize = 64;

/// A bounded, strictly in-memory snapshot of a retained [`ResultSet`].
///
/// This type intentionally does not implement `Serialize` or `Deserialize` and
/// is never written to disk or crash-recovery journals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinnedResult {
    /// Bounded copy of the captured result set.
    pub result_set: ResultSet,
    /// Single-line sanitized summary of the SQL that produced the snapshot.
    pub source_sql_summary: String,
    /// Number of retained rows held in this snapshot.
    pub retained_rows: usize,
    /// Total rows reported by PostgreSQL when the snapshot was captured.
    pub rows_seen: u64,
    /// Whether the captured snapshot is a truncated prefix of the full result.
    pub truncated: bool,
    /// Optional preferred key column for A/B row alignment.
    pub key_column: Option<String>,
    /// Connection target label at capture time, used to clear on target switches.
    pub connection_target: Option<String>,
}

impl PinnedResult {
    /// Captures a bounded in-memory baseline snapshot from a retained [`ResultSet`].
    pub fn capture(
        set: &ResultSet,
        source_sql: Option<&str>,
        key_column: Option<String>,
        connection_target: Option<String>,
    ) -> Result<Self, String> {
        if !set.is_available() {
            return Err("Result data unavailable; cannot pin an unavailable result.".to_owned());
        }

        let bounded_columns: Vec<String> =
            set.columns.iter().take(MAX_DIFF_COLUMNS).cloned().collect();
        let col_count = bounded_columns.len();
        let bounded_types: Vec<Option<String>> =
            set.column_types.iter().take(col_count).cloned().collect();

        let mut captured =
            ResultSet::with_column_types(bounded_columns, bounded_types, MAX_DIFF_ROWS);
        let row_limit = set.rows.len().min(MAX_DIFF_ROWS);
        for row in set.rows.iter().take(row_limit) {
            let bounded_row: Vec<Cell> = row.iter().take(col_count).map(bound_cell).collect();
            captured.rows.push(bounded_row);
        }
        captured.rows_seen = set.rows_seen;
        captured.retention = set.retention;

        let truncated = set.is_truncated()
            || set.rows.len() > MAX_DIFF_ROWS
            || set.columns.len() > MAX_DIFF_COLUMNS;
        let retained_rows = captured.rows.len();

        Ok(Self {
            result_set: captured,
            source_sql_summary: summarize_sql(source_sql),
            retained_rows,
            rows_seen: set.rows_seen,
            truncated,
            key_column,
            connection_target,
        })
    }
}

/// Column additions, removals, and shared order between baseline and current.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct SchemaDelta {
    /// Columns present in baseline only, in baseline order.
    pub removed_columns: Vec<String>,
    /// Columns present in current only, in current order.
    pub added_columns: Vec<String>,
    /// Columns present in both baseline and current, in baseline order.
    pub shared_columns: Vec<String>,
}

impl SchemaDelta {
    /// Whether the baseline and current schemas differ.
    #[must_use]
    pub fn has_changes(&self) -> bool {
        !self.added_columns.is_empty() || !self.removed_columns.is_empty()
    }
}

/// How baseline and current rows were matched.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum AlignmentMode {
    /// Rows aligned by a unique, non-null key column present in both sets.
    Keyed {
        /// Key column name.
        column: String,
    },
    /// Rows aligned by zero-based row index, with an explicit fallback reason when a key was rejected.
    Positional {
        /// Reason key alignment fell back to positional, if a key was requested.
        reason: Option<String>,
    },
}

impl AlignmentMode {
    /// Short human-readable summary for TUI headers and reports.
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::Keyed { column } => format!("keyed ({})", sanitize_for_display(column)),
            Self::Positional { reason: None } => "positional".to_owned(),
            Self::Positional { reason: Some(why) } => {
                format!("positional ({})", sanitize_for_display(why))
            }
        }
    }
}

/// One cell-level difference on a changed row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellDiff {
    /// Shared column name.
    pub column: String,
    /// Baseline cell value.
    pub baseline: Cell,
    /// Current cell value.
    pub current: Cell,
}

/// Classification of one aligned row entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffRowKind {
    /// Present only in current.
    Added,
    /// Present only in baseline.
    Removed,
    /// Present in both with at least one differing shared cell.
    Changed {
        /// Differing shared cells, bounded by [`MAX_CELL_DIFFS_PER_ROW`].
        cell_diffs: Vec<CellDiff>,
        /// Whether more cells differed than [`MAX_CELL_DIFFS_PER_ROW`].
        cells_truncated: bool,
    },
    /// Present in both with identical values across all shared columns.
    Unchanged,
}

impl DiffRowKind {
    /// Accessible text badge that never relies on colour alone.
    #[must_use]
    pub const fn badge(&self) -> &'static str {
        match self {
            Self::Added => "[+]",
            Self::Removed => "[-]",
            Self::Changed { .. } => "[~]",
            Self::Unchanged => "[=]",
        }
    }

    /// Machine-readable classification slug.
    #[must_use]
    pub const fn slug(&self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Removed => "removed",
            Self::Changed { .. } => "changed",
            Self::Unchanged => "unchanged",
        }
    }
}

/// One aligned row in a [`ResultDiff`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffRow {
    /// Zero-based index in the baseline retained rows, when present.
    pub baseline_index: Option<usize>,
    /// Zero-based index in the current retained rows, when present.
    pub current_index: Option<usize>,
    /// Key value when aligned in [`AlignmentMode::Keyed`].
    pub key_value: Option<String>,
    /// Row classification.
    pub kind: DiffRowKind,
    /// Baseline cell values aligned to [`ResultDiff::display_columns`].
    pub baseline_cells: Vec<Option<Cell>>,
    /// Current cell values aligned to [`ResultDiff::display_columns`].
    pub current_cells: Vec<Option<Cell>>,
}

/// Complete bounded A/B diff between a pinned baseline and a current [`ResultSet`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultDiff {
    /// Baseline SQL summary.
    pub baseline_sql_summary: String,
    /// Schema delta between baseline and current.
    pub schema_delta: SchemaDelta,
    /// Display column order (`shared_columns`, then `added_columns`, then `removed_columns`).
    pub display_columns: Vec<String>,
    /// Shared columns that are unique and non-null in both baseline and current.
    pub candidate_key_columns: Vec<String>,
    /// Alignment mode chosen for this diff.
    pub alignment: AlignmentMode,
    /// Classified row entries (bounded by [`MAX_DIFF_ROWS`]).
    pub entries: Vec<DiffRow>,
    /// Count of added rows.
    pub added_count: usize,
    /// Count of removed rows.
    pub removed_count: usize,
    /// Count of changed rows.
    pub changed_count: usize,
    /// Count of unchanged rows.
    pub unchanged_count: usize,
    /// Retained rows examined in baseline.
    pub baseline_rows: usize,
    /// Retained rows examined in current.
    pub current_rows: usize,
    /// Whether the baseline snapshot was truncated.
    pub baseline_truncated: bool,
    /// Whether the current result set was truncated.
    pub current_truncated: bool,
    /// Whether the diff entry list itself reached [`MAX_DIFF_ROWS`].
    pub entries_truncated: bool,
}

impl ResultDiff {
    /// Whether any schema or row difference exists.
    #[must_use]
    pub fn has_differences(&self) -> bool {
        self.schema_delta.has_changes()
            || self.added_count > 0
            || self.removed_count > 0
            || self.changed_count > 0
    }

    /// Whether either input or the diff output was truncated.
    #[must_use]
    pub const fn is_truncated(&self) -> bool {
        self.baseline_truncated || self.current_truncated || self.entries_truncated
    }

    /// Renders a deterministic plain-text diff report with `[+]`/`[-]`/`[~]`/`[=]` badges.
    #[must_use]
    pub fn render_text(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "Result Diff Summary");
        let _ = writeln!(
            out,
            "Baseline: {} ({} retained row(s){})",
            self.baseline_sql_summary,
            self.baseline_rows,
            if self.baseline_truncated {
                ", truncated"
            } else {
                ""
            }
        );
        let _ = writeln!(
            out,
            "Current: {} retained row(s){}",
            self.current_rows,
            if self.current_truncated {
                " (truncated)"
            } else {
                ""
            }
        );
        let _ = writeln!(out, "Alignment: {}", self.alignment.label());
        let _ = writeln!(
            out,
            "Counts: [+] {} added  [-] {} removed  [~] {} changed  [=] {} unchanged",
            self.added_count, self.removed_count, self.changed_count, self.unchanged_count
        );
        if self.schema_delta.has_changes() {
            let added = if self.schema_delta.added_columns.is_empty() {
                "none".to_owned()
            } else {
                self.schema_delta
                    .added_columns
                    .iter()
                    .map(|c| sanitize_for_display(c))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            let removed = if self.schema_delta.removed_columns.is_empty() {
                "none".to_owned()
            } else {
                self.schema_delta
                    .removed_columns
                    .iter()
                    .map(|c| sanitize_for_display(c))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            let _ = writeln!(out, "Schema delta: +{added} | -{removed}");
        } else {
            let _ = writeln!(
                out,
                "Schema delta: none ({} shared column(s))",
                self.schema_delta.shared_columns.len()
            );
        }
        if self.entries_truncated {
            let _ = writeln!(
                out,
                "Notice: diff output reached the {MAX_DIFF_ROWS}-row display bound."
            );
        }

        if !self.entries.is_empty() {
            let _ = writeln!(out);
            for entry in &self.entries {
                let id_label = row_identity_label(entry);
                match &entry.kind {
                    DiffRowKind::Added => {
                        let vals = format_row_cells(
                            &self.display_columns,
                            &entry.current_cells,
                            CellSide::Current,
                        );
                        let _ = writeln!(out, "[+] {id_label}: {vals}");
                    }
                    DiffRowKind::Removed => {
                        let vals = format_row_cells(
                            &self.display_columns,
                            &entry.baseline_cells,
                            CellSide::Baseline,
                        );
                        let _ = writeln!(out, "[-] {id_label}: {vals}");
                    }
                    DiffRowKind::Changed {
                        cell_diffs,
                        cells_truncated,
                    } => {
                        let changes: Vec<String> = cell_diffs
                            .iter()
                            .map(|diff| {
                                format!(
                                    "{}: {} -> {}",
                                    sanitize_for_display(&diff.column),
                                    format_cell_badge(&diff.baseline),
                                    format_cell_badge(&diff.current)
                                )
                            })
                            .collect();
                        let suffix = if *cells_truncated { " (+more)" } else { "" };
                        let _ = writeln!(out, "[~] {id_label}: {}{suffix}", changes.join("; "));
                    }
                    DiffRowKind::Unchanged => {
                        let vals = format_row_cells(
                            &self.display_columns,
                            &entry.current_cells,
                            CellSide::Current,
                        );
                        let _ = writeln!(out, "[=] {id_label}: {vals}");
                    }
                }
            }
        }
        out
    }

    /// Renders a deterministic Markdown diff report with `[+]`/`[-]`/`[~]`/`[=]` badges.
    #[must_use]
    pub fn render_markdown(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "# Result Diff");
        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "- **Baseline**: `{}` ({} row(s){})",
            escape_md(&self.baseline_sql_summary),
            self.baseline_rows,
            if self.baseline_truncated {
                ", truncated"
            } else {
                ""
            }
        );
        let _ = writeln!(
            out,
            "- **Current**: {} row(s){}",
            self.current_rows,
            if self.current_truncated {
                " (truncated)"
            } else {
                ""
            }
        );
        let _ = writeln!(
            out,
            "- **Alignment**: {}",
            escape_md(&self.alignment.label())
        );
        let _ = writeln!(
            out,
            "- **Counts**: `[+]` {} added, `[-]` {} removed, `[~]` {} changed, `[=]` {} unchanged",
            self.added_count, self.removed_count, self.changed_count, self.unchanged_count
        );
        if self.schema_delta.has_changes() {
            let added = if self.schema_delta.added_columns.is_empty() {
                "none".to_owned()
            } else {
                self.schema_delta
                    .added_columns
                    .iter()
                    .map(|c| escape_md(c))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            let removed = if self.schema_delta.removed_columns.is_empty() {
                "none".to_owned()
            } else {
                self.schema_delta
                    .removed_columns
                    .iter()
                    .map(|c| escape_md(c))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            let _ = writeln!(out, "- **Schema delta**: +{added} | -{removed}");
        } else {
            let _ = writeln!(
                out,
                "- **Schema delta**: none ({} shared column(s))",
                self.schema_delta.shared_columns.len()
            );
        }
        if self.entries_truncated {
            let _ = writeln!(
                out,
                "- **Notice**: diff output reached the {MAX_DIFF_ROWS}-row display bound."
            );
        }
        let _ = writeln!(out);
        let _ = writeln!(out, "| Status | Row | Detail |");
        let _ = writeln!(out, "| --- | --- | --- |");
        for entry in &self.entries {
            let badge = entry.kind.badge();
            let id_label = escape_md(&row_identity_label(entry));
            let detail = match &entry.kind {
                DiffRowKind::Added => escape_md(&format_row_cells(
                    &self.display_columns,
                    &entry.current_cells,
                    CellSide::Current,
                )),
                DiffRowKind::Removed => escape_md(&format_row_cells(
                    &self.display_columns,
                    &entry.baseline_cells,
                    CellSide::Baseline,
                )),
                DiffRowKind::Changed {
                    cell_diffs,
                    cells_truncated,
                } => {
                    let changes: Vec<String> = cell_diffs
                        .iter()
                        .map(|diff| {
                            format!(
                                "{}: {} -> {}",
                                sanitize_for_display(&diff.column),
                                format_cell_badge(&diff.baseline),
                                format_cell_badge(&diff.current)
                            )
                        })
                        .collect();
                    let suffix = if *cells_truncated { " (+more)" } else { "" };
                    escape_md(&format!("{}{suffix}", changes.join("; ")))
                }
                DiffRowKind::Unchanged => escape_md(&format_row_cells(
                    &self.display_columns,
                    &entry.current_cells,
                    CellSide::Current,
                )),
            };
            let _ = writeln!(out, "| `{badge}` | {id_label} | {detail} |");
        }
        out
    }

    /// Renders deterministic JSON output (`schema_version: 1`).
    #[must_use]
    pub fn render_json(&self) -> String {
        #[derive(Serialize)]
        struct JsonCounts {
            added: usize,
            removed: usize,
            changed: usize,
            unchanged: usize,
        }

        #[derive(Serialize)]
        struct JsonCellDiff<'a> {
            column: &'a str,
            baseline: serde_json::Value,
            current: serde_json::Value,
        }

        #[derive(Serialize)]
        struct JsonEntry<'a> {
            badge: &'static str,
            status: &'static str,
            baseline_row: Option<usize>,
            current_row: Option<usize>,
            key: Option<&'a str>,
            cell_diffs: Vec<JsonCellDiff<'a>>,
            baseline: BTreeMap<&'a str, serde_json::Value>,
            current: BTreeMap<&'a str, serde_json::Value>,
        }

        #[derive(Serialize)]
        struct JsonReport<'a> {
            schema_version: u32,
            baseline_sql_summary: &'a str,
            baseline_rows: usize,
            current_rows: usize,
            baseline_truncated: bool,
            current_truncated: bool,
            entries_truncated: bool,
            alignment: &'a AlignmentMode,
            schema_delta: &'a SchemaDelta,
            counts: JsonCounts,
            entries: Vec<JsonEntry<'a>>,
        }

        let entries = self
            .entries
            .iter()
            .map(|entry| {
                let cell_diffs = match &entry.kind {
                    DiffRowKind::Changed { cell_diffs, .. } => cell_diffs
                        .iter()
                        .map(|diff| JsonCellDiff {
                            column: &diff.column,
                            baseline: cell_to_json(&diff.baseline),
                            current: cell_to_json(&diff.current),
                        })
                        .collect(),
                    _ => Vec::new(),
                };
                let baseline = cells_to_json_map(&self.display_columns, &entry.baseline_cells);
                let current = cells_to_json_map(&self.display_columns, &entry.current_cells);
                JsonEntry {
                    badge: entry.kind.badge(),
                    status: entry.kind.slug(),
                    baseline_row: entry.baseline_index.map(|i| i + 1),
                    current_row: entry.current_index.map(|i| i + 1),
                    key: entry.key_value.as_deref(),
                    cell_diffs,
                    baseline,
                    current,
                }
            })
            .collect();

        let report = JsonReport {
            schema_version: RESULT_DIFF_SCHEMA_VERSION,
            baseline_sql_summary: &self.baseline_sql_summary,
            baseline_rows: self.baseline_rows,
            current_rows: self.current_rows,
            baseline_truncated: self.baseline_truncated,
            current_truncated: self.current_truncated,
            entries_truncated: self.entries_truncated,
            alignment: &self.alignment,
            schema_delta: &self.schema_delta,
            counts: JsonCounts {
                added: self.added_count,
                removed: self.removed_count,
                changed: self.changed_count,
                unchanged: self.unchanged_count,
            },
            entries,
        };

        serde_json::to_string_pretty(&report)
            .unwrap_or_else(|_| "{\"schema_version\":1,\"error\":\"serialization failed\"}".into())
    }
}

/// Computes a bounded A/B [`ResultDiff`] between a pinned baseline and a current [`ResultSet`].
#[must_use]
pub fn diff_results(
    baseline: &PinnedResult,
    current: &ResultSet,
    key_column: Option<&str>,
) -> ResultDiff {
    let base_set = &baseline.result_set;
    let schema_delta = compute_schema_delta(&base_set.columns, &current.columns);
    let display_columns = build_display_columns(&schema_delta);

    let base_row_limit = base_set.rows.len().min(MAX_DIFF_ROWS);
    let curr_row_limit = current.rows.len().min(MAX_DIFF_ROWS);
    let base_rows = &base_set.rows[..base_row_limit];
    let curr_rows = &current.rows[..curr_row_limit];

    let candidate_key_columns = find_candidate_key_columns(
        &schema_delta.shared_columns,
        base_set,
        base_rows,
        current,
        curr_rows,
    );

    let base_lookup = build_column_index(&base_set.columns);
    let curr_lookup = build_column_index(&current.columns);
    let shared_pairs: Vec<(String, usize, usize)> = schema_delta
        .shared_columns
        .iter()
        .filter_map(|name| {
            let b_idx = base_lookup.get(name.as_str()).copied()?;
            let c_idx = curr_lookup.get(name.as_str()).copied()?;
            Some((name.clone(), b_idx, c_idx))
        })
        .collect();

    let display_mapping: Vec<(Option<usize>, Option<usize>)> = display_columns
        .iter()
        .map(|name| {
            (
                base_lookup.get(name.as_str()).copied(),
                curr_lookup.get(name.as_str()).copied(),
            )
        })
        .collect();

    let alignment = resolve_alignment(
        key_column,
        &base_set.columns,
        base_rows,
        &current.columns,
        curr_rows,
    );

    let mut entries = Vec::new();
    let mut added_count = 0usize;
    let mut removed_count = 0usize;
    let mut changed_count = 0usize;
    let mut unchanged_count = 0usize;
    let mut entries_truncated = false;

    match &alignment {
        AlignmentMode::Keyed { column } => {
            let b_key_idx = base_lookup[column.as_str()];
            let c_key_idx = curr_lookup[column.as_str()];
            let mut current_by_key: HashMap<&str, usize> = HashMap::with_capacity(curr_rows.len());
            for (c_idx, row) in curr_rows.iter().enumerate() {
                if let Some(Cell::Text(key)) = row.get(c_key_idx) {
                    current_by_key.insert(key.as_str(), c_idx);
                }
            }
            let mut matched_current = vec![false; curr_rows.len()];

            for (b_idx, b_row) in base_rows.iter().enumerate() {
                let key_text = match b_row.get(b_key_idx) {
                    Some(Cell::Text(k)) => k.as_str(),
                    _ => "",
                };
                if let Some(&c_idx) = current_by_key.get(key_text) {
                    matched_current[c_idx] = true;
                    let c_row = &curr_rows[c_idx];
                    let kind = classify_paired_row(b_row, c_row, &shared_pairs);
                    match &kind {
                        DiffRowKind::Changed { .. } => changed_count += 1,
                        DiffRowKind::Unchanged => unchanged_count += 1,
                        _ => {}
                    }
                    push_bounded_entry(
                        &mut entries,
                        &mut entries_truncated,
                        DiffRow {
                            baseline_index: Some(b_idx),
                            current_index: Some(c_idx),
                            key_value: Some(key_text.to_owned()),
                            kind,
                            baseline_cells: project_row(Some(b_row), &display_mapping, true),
                            current_cells: project_row(Some(c_row), &display_mapping, false),
                        },
                    );
                } else {
                    removed_count += 1;
                    push_bounded_entry(
                        &mut entries,
                        &mut entries_truncated,
                        DiffRow {
                            baseline_index: Some(b_idx),
                            current_index: None,
                            key_value: Some(key_text.to_owned()),
                            kind: DiffRowKind::Removed,
                            baseline_cells: project_row(Some(b_row), &display_mapping, true),
                            current_cells: vec![None; display_columns.len()],
                        },
                    );
                }
            }

            for (c_idx, c_row) in curr_rows.iter().enumerate() {
                if matched_current[c_idx] {
                    continue;
                }
                added_count += 1;
                let key_text = match c_row.get(c_key_idx) {
                    Some(Cell::Text(k)) => k.clone(),
                    _ => String::new(),
                };
                push_bounded_entry(
                    &mut entries,
                    &mut entries_truncated,
                    DiffRow {
                        baseline_index: None,
                        current_index: Some(c_idx),
                        key_value: Some(key_text),
                        kind: DiffRowKind::Added,
                        baseline_cells: vec![None; display_columns.len()],
                        current_cells: project_row(Some(c_row), &display_mapping, false),
                    },
                );
            }
        }
        AlignmentMode::Positional { .. } => {
            let max_len = base_rows.len().max(curr_rows.len());
            for idx in 0..max_len {
                match (base_rows.get(idx), curr_rows.get(idx)) {
                    (Some(b_row), Some(c_row)) => {
                        let kind = classify_paired_row(b_row, c_row, &shared_pairs);
                        match &kind {
                            DiffRowKind::Changed { .. } => changed_count += 1,
                            DiffRowKind::Unchanged => unchanged_count += 1,
                            _ => {}
                        }
                        push_bounded_entry(
                            &mut entries,
                            &mut entries_truncated,
                            DiffRow {
                                baseline_index: Some(idx),
                                current_index: Some(idx),
                                key_value: None,
                                kind,
                                baseline_cells: project_row(Some(b_row), &display_mapping, true),
                                current_cells: project_row(Some(c_row), &display_mapping, false),
                            },
                        );
                    }
                    (Some(b_row), None) => {
                        removed_count += 1;
                        push_bounded_entry(
                            &mut entries,
                            &mut entries_truncated,
                            DiffRow {
                                baseline_index: Some(idx),
                                current_index: None,
                                key_value: None,
                                kind: DiffRowKind::Removed,
                                baseline_cells: project_row(Some(b_row), &display_mapping, true),
                                current_cells: vec![None; display_columns.len()],
                            },
                        );
                    }
                    (None, Some(c_row)) => {
                        added_count += 1;
                        push_bounded_entry(
                            &mut entries,
                            &mut entries_truncated,
                            DiffRow {
                                baseline_index: None,
                                current_index: Some(idx),
                                key_value: None,
                                kind: DiffRowKind::Added,
                                baseline_cells: vec![None; display_columns.len()],
                                current_cells: project_row(Some(c_row), &display_mapping, false),
                            },
                        );
                    }
                    (None, None) => {}
                }
            }
        }
    }

    ResultDiff {
        baseline_sql_summary: baseline.source_sql_summary.clone(),
        schema_delta,
        display_columns,
        candidate_key_columns,
        alignment,
        entries,
        added_count,
        removed_count,
        changed_count,
        unchanged_count,
        baseline_rows: base_rows.len(),
        current_rows: curr_rows.len(),
        baseline_truncated: baseline.truncated || base_set.rows.len() > MAX_DIFF_ROWS,
        current_truncated: current.is_truncated() || current.rows.len() > MAX_DIFF_ROWS,
        entries_truncated,
    }
}

/// Parses a bounded local JSON or NDJSON file into a [`ResultSet`] for offline CLI diffing.
///
/// Rejects symlinks, non-regular files, files exceeding [`MAX_DIFF_INPUT_BYTES`],
/// invalid UTF-8, deeply nested JSON, and malformed shapes with [`DiagnosticKind::Usage`].
pub fn parse_result_file(path: &Path) -> Result<ResultSet, Diagnostic> {
    let context = "reading result diff input file";
    let metadata = std::fs::symlink_metadata(path).map_err(|err| {
        Diagnostic::new(
            DiagnosticKind::Usage,
            format!("could not inspect {}: {err}", path.display()),
            context,
        )
        .next_action("verify that the file exists and is readable")
    })?;

    if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
        return Err(Diagnostic::new(
            DiagnosticKind::Usage,
            format!(
                "{} must be a regular file (symlinks, directories, and special files are rejected)",
                path.display()
            ),
            context,
        )
        .next_action("pass a regular local JSON or NDJSON file path"));
    }

    if metadata.len() > MAX_DIFF_INPUT_BYTES as u64 {
        return Err(Diagnostic::new(
            DiagnosticKind::Usage,
            format!(
                "{} exceeds the {MAX_DIFF_INPUT_BYTES}-byte safety bound",
                path.display()
            ),
            context,
        )
        .next_action("provide a smaller result file within the 16 MiB limit"));
    }

    let file = std::fs::File::open(path).map_err(|err| {
        Diagnostic::new(
            DiagnosticKind::Usage,
            format!("could not read {}: {err}", path.display()),
            context,
        )
        .next_action("verify that the file exists and is readable")
    })?;
    let mut raw = Vec::new();
    file.take((MAX_DIFF_INPUT_BYTES as u64).saturating_add(1))
        .read_to_end(&mut raw)
        .map_err(|err| {
            Diagnostic::new(
                DiagnosticKind::Usage,
                format!("could not read {}: {err}", path.display()),
                context,
            )
            .next_action("verify that the file is readable")
        })?;
    if raw.len() > MAX_DIFF_INPUT_BYTES {
        return Err(Diagnostic::new(
            DiagnosticKind::Usage,
            format!(
                "{} exceeds the {MAX_DIFF_INPUT_BYTES}-byte safety bound",
                path.display()
            ),
            context,
        )
        .next_action("provide a smaller result file within the 16 MiB limit"));
    }

    let text = String::from_utf8(raw).map_err(|_| {
        Diagnostic::new(
            DiagnosticKind::Usage,
            format!("{} is not valid UTF-8 text", path.display()),
            context,
        )
        .next_action("encode the result file as UTF-8")
    })?;

    parse_result_str(&text, &path.display().to_string())
}

/// Parses bounded JSON or NDJSON text into a [`ResultSet`].
pub fn parse_result_str(text: &str, label: &str) -> Result<ResultSet, Diagnostic> {
    let context = "parsing result diff input";
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(
            Diagnostic::new(DiagnosticKind::Usage, format!("{label} is empty"), context)
                .next_action(
                    "provide a JSON export file, JSON array of row objects, or NDJSON file",
                ),
        );
    }

    if !json_nesting_within_bound(trimmed, MAX_JSON_NESTING_DEPTH) {
        return Err(Diagnostic::new(
            DiagnosticKind::Usage,
            format!("{label} exceeds JSON nesting depth limit ({MAX_JSON_NESTING_DEPTH})"),
            context,
        )
        .next_action("check that the input file is valid bounded JSON or NDJSON"));
    }

    if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
        return match value {
            serde_json::Value::Object(map) => {
                if is_ignatius_export_envelope(&map) {
                    parse_ignatius_export_object(map, label)
                } else {
                    // Single-line NDJSON object `{ "id": 1, ... }`.
                    parse_row_objects(vec![serde_json::Value::Object(map)], label)
                }
            }
            serde_json::Value::Array(items) => {
                if is_ignatius_export_array(&items) {
                    if items.len() != 1 {
                        return Err(Diagnostic::new(
                            DiagnosticKind::Usage,
                            format!(
                                "{label} contains {} result sets; result-diff compares one result set at a time",
                                items.len()
                            ),
                            context,
                        )
                        .next_action("export a single statement result set for diffing"));
                    }
                    let Some(serde_json::Value::Object(map)) = items.into_iter().next() else {
                        unreachable!();
                    };
                    parse_ignatius_export_object(map, label)
                } else {
                    parse_row_objects(items, label)
                }
            }
            _ => Err(Diagnostic::new(
                DiagnosticKind::Usage,
                format!("{label} must be a JSON result object, array of row objects, or NDJSON"),
                context,
            )
            .next_action("provide Ignatius JSON export or object-per-row JSON/NDJSON")),
        };
    }

    // Fall back to NDJSON: each non-empty line must be a JSON object.
    let mut rows = Vec::new();
    for (line_no, line) in trimmed.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let val: serde_json::Value = serde_json::from_str(line).map_err(|err| {
            Diagnostic::new(
                DiagnosticKind::Usage,
                format!(
                    "{label} line {} is malformed JSON/NDJSON: {err}",
                    line_no + 1
                ),
                context,
            )
            .next_action("verify that every line is a valid JSON object")
        })?;
        if !val.is_object() {
            return Err(Diagnostic::new(
                DiagnosticKind::Usage,
                format!(
                    "{label} line {} must be a JSON object in NDJSON input",
                    line_no + 1
                ),
                context,
            )
            .next_action("use one JSON object per line for NDJSON"));
        }
        rows.push(val);
        if rows.len() > MAX_DIFF_ROWS {
            break;
        }
    }
    parse_row_objects(rows, label)
}

fn is_ignatius_export_envelope(map: &serde_json::Map<String, serde_json::Value>) -> bool {
    let Some(cols) = map.get("columns").and_then(serde_json::Value::as_array) else {
        return false;
    };
    let Some(_rows) = map.get("rows").and_then(serde_json::Value::as_array) else {
        return false;
    };
    if !cols.iter().all(serde_json::Value::is_string) {
        return false;
    }
    map.keys().all(|k| {
        matches!(
            k.as_str(),
            "columns"
                | "rows"
                | "rows_seen"
                | "truncated"
                | "data_unavailable"
                | "source_column_count"
        )
    })
}

fn is_ignatius_export_array(items: &[serde_json::Value]) -> bool {
    !items.is_empty()
        && items
            .iter()
            .all(|v| v.as_object().is_some_and(is_ignatius_export_envelope))
}

fn parse_ignatius_export_object(
    mut map: serde_json::Map<String, serde_json::Value>,
    label: &str,
) -> Result<ResultSet, Diagnostic> {
    let context = "parsing Ignatius JSON result export";
    if map.contains_key("data_unavailable") {
        return Err(Diagnostic::new(
            DiagnosticKind::Usage,
            format!("{label} reports unavailable retained result data"),
            context,
        )
        .next_action("re-export with sufficient retained data before diffing"));
    }

    let raw_columns = map
        .remove("columns")
        .and_then(|v| match v {
            serde_json::Value::Array(arr) => Some(arr),
            _ => None,
        })
        .ok_or_else(|| {
            Diagnostic::new(
                DiagnosticKind::Usage,
                format!("{label} is missing a `columns` array"),
                context,
            )
        })?;

    if raw_columns.len() > MAX_DIFF_COLUMNS {
        return Err(Diagnostic::new(
            DiagnosticKind::Usage,
            format!("{label} exceeds the {MAX_DIFF_COLUMNS}-column safety limit"),
            context,
        )
        .next_action("project fewer columns before diffing"));
    }

    let columns: Vec<String> = raw_columns
        .into_iter()
        .map(|v| match v {
            serde_json::Value::String(s) => Ok(s),
            _ => Err(Diagnostic::new(
                DiagnosticKind::Usage,
                format!("{label} has a non-string column name"),
                context,
            )),
        })
        .collect::<Result<Vec<_>, _>>()?;

    let raw_rows = map
        .remove("rows")
        .and_then(|v| match v {
            serde_json::Value::Array(arr) => Some(arr),
            _ => None,
        })
        .ok_or_else(|| {
            Diagnostic::new(
                DiagnosticKind::Usage,
                format!("{label} is missing a `rows` array"),
                context,
            )
        })?;

    let rows_seen = map
        .get("rows_seen")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(raw_rows.len() as u64);
    let was_truncated = map
        .get("truncated")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);

    let mut set = ResultSet::new(columns.clone(), MAX_DIFF_ROWS);
    for (row_idx, item) in raw_rows.into_iter().enumerate() {
        if set.rows.len() >= MAX_DIFF_ROWS {
            set.retention = ResultRetention::RowCap;
            break;
        }
        let row = match item {
            serde_json::Value::Object(obj) => columns
                .iter()
                .map(|col| obj.get(col).map_or(Cell::Null, json_value_to_cell))
                .collect(),
            serde_json::Value::Array(arr) if arr.len() == columns.len() => {
                arr.iter().map(json_value_to_cell).collect()
            }
            _ => {
                return Err(Diagnostic::new(
                    DiagnosticKind::Usage,
                    format!(
                        "{label} row {} has an invalid shape or column count",
                        row_idx + 1
                    ),
                    context,
                )
                .next_action("ensure every row matches the declared columns"));
            }
        };
        set.rows.push(row);
    }

    set.rows_seen = rows_seen.max(set.rows.len() as u64);
    if was_truncated && matches!(set.retention, ResultRetention::Complete) {
        set.retention = ResultRetention::RowCap;
    }
    Ok(set)
}

fn parse_row_objects(items: Vec<serde_json::Value>, label: &str) -> Result<ResultSet, Diagnostic> {
    let context = "parsing JSON/NDJSON row objects";
    let mut columns: Vec<String> = Vec::new();
    let mut seen_columns: HashSet<String> = HashSet::new();
    let mut objects = Vec::with_capacity(items.len().min(MAX_DIFF_ROWS));
    let mut truncated = false;

    for (idx, item) in items.into_iter().enumerate() {
        let serde_json::Value::Object(obj) = item else {
            return Err(Diagnostic::new(
                DiagnosticKind::Usage,
                format!("{label} row {} must be a JSON object", idx + 1),
                context,
            )
            .next_action("ensure all rows are JSON objects"));
        };
        if objects.len() >= MAX_DIFF_ROWS {
            truncated = true;
            break;
        }
        for key in obj.keys() {
            if seen_columns.insert(key.clone()) {
                if columns.len() >= MAX_DIFF_COLUMNS {
                    return Err(Diagnostic::new(
                        DiagnosticKind::Usage,
                        format!("{label} exceeds the {MAX_DIFF_COLUMNS}-column safety limit"),
                        context,
                    )
                    .next_action("project fewer columns before diffing"));
                }
                columns.push(key.clone());
            }
        }
        objects.push(obj);
    }

    let total_rows = objects.len() as u64;
    let mut set = ResultSet::new(columns.clone(), MAX_DIFF_ROWS);
    for obj in objects {
        let row = columns
            .iter()
            .map(|col| obj.get(col).map_or(Cell::Null, json_value_to_cell))
            .collect();
        set.rows.push(row);
    }
    set.rows_seen = total_rows;
    if truncated {
        set.retention = ResultRetention::RowCap;
    }
    Ok(set)
}

fn json_value_to_cell(value: &serde_json::Value) -> Cell {
    match value {
        serde_json::Value::Null => Cell::Null,
        serde_json::Value::String(s) => bound_text_cell(s),
        serde_json::Value::Bool(b) => Cell::Text(b.to_string()),
        serde_json::Value::Number(n) => Cell::Text(n.to_string()),
        other => bound_text_cell(&other.to_string()),
    }
}

fn bound_cell(cell: &Cell) -> Cell {
    match cell {
        Cell::Null => Cell::Null,
        Cell::Text(s) => bound_text_cell(s),
    }
}

fn bound_text_cell(text: &str) -> Cell {
    if text.len() <= MAX_DIFF_CELL_BYTES {
        return Cell::Text(text.to_owned());
    }
    let mut end = MAX_DIFF_CELL_BYTES;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    Cell::Text(format!("{}...", &text[..end]))
}

fn summarize_sql(source_sql: Option<&str>) -> String {
    let Some(raw) = source_sql else {
        return "(ad-hoc result)".to_owned();
    };
    let collapsed: String = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    let clean = sanitize_for_display(&collapsed);
    if clean.is_empty() {
        return "(ad-hoc result)".to_owned();
    }
    truncate_to_width(&clean, MAX_SOURCE_SQL_SUMMARY_BYTES, false)
}

fn compute_schema_delta(baseline: &[String], current: &[String]) -> SchemaDelta {
    let base_set: HashSet<&str> = baseline.iter().map(String::as_str).collect();
    let curr_set: HashSet<&str> = current.iter().map(String::as_str).collect();

    let mut shared_seen = HashSet::new();
    let mut shared_columns = Vec::new();
    let mut removed_seen = HashSet::new();
    let mut removed_columns = Vec::new();
    for col in baseline {
        if curr_set.contains(col.as_str()) {
            if shared_seen.insert(col.clone()) {
                shared_columns.push(col.clone());
            }
        } else if removed_seen.insert(col.clone()) {
            removed_columns.push(col.clone());
        }
    }

    let mut added_seen = HashSet::new();
    let mut added_columns = Vec::new();
    for col in current {
        if !base_set.contains(col.as_str()) && added_seen.insert(col.clone()) {
            added_columns.push(col.clone());
        }
    }

    SchemaDelta {
        removed_columns,
        added_columns,
        shared_columns,
    }
}

fn build_display_columns(delta: &SchemaDelta) -> Vec<String> {
    let mut cols = Vec::with_capacity(
        delta.shared_columns.len() + delta.added_columns.len() + delta.removed_columns.len(),
    );
    cols.extend(delta.shared_columns.iter().cloned());
    cols.extend(delta.added_columns.iter().cloned());
    cols.extend(delta.removed_columns.iter().cloned());
    cols
}

fn build_column_index(columns: &[String]) -> HashMap<&str, usize> {
    let mut map = HashMap::with_capacity(columns.len());
    for (idx, col) in columns.iter().enumerate() {
        map.entry(col.as_str()).or_insert(idx);
    }
    map
}

fn unique_column_index(columns: &[String], target: &str) -> Result<usize, String> {
    let mut matches = columns
        .iter()
        .enumerate()
        .filter(|(_, col)| col.as_str() == target)
        .map(|(idx, _)| idx);
    let Some(first) = matches.next() else {
        return Err(format!(
            "key column '{}' is missing",
            sanitize_for_display(target)
        ));
    };
    if matches.next().is_some() {
        return Err(format!(
            "key column '{}' is ambiguous (duplicate column name)",
            sanitize_for_display(target)
        ));
    }
    Ok(first)
}

fn validate_key_values(
    rows: &[Vec<Cell>],
    col_idx: usize,
    side: &str,
    key_name: &str,
) -> Result<(), String> {
    let mut seen: HashSet<&str> = HashSet::with_capacity(rows.len());
    for row in rows {
        match row.get(col_idx) {
            Some(Cell::Text(val)) => {
                if !seen.insert(val.as_str()) {
                    return Err(format!(
                        "key column '{}' has duplicate values in {side}",
                        sanitize_for_display(key_name)
                    ));
                }
            }
            _ => {
                return Err(format!(
                    "key column '{}' contains NULL in {side}",
                    sanitize_for_display(key_name)
                ));
            }
        }
    }
    Ok(())
}

fn resolve_alignment(
    key_column: Option<&str>,
    base_cols: &[String],
    base_rows: &[Vec<Cell>],
    curr_cols: &[String],
    curr_rows: &[Vec<Cell>],
) -> AlignmentMode {
    let Some(col) = key_column.map(str::trim).filter(|c| !c.is_empty()) else {
        return AlignmentMode::Positional { reason: None };
    };
    let b_idx = match unique_column_index(base_cols, col) {
        Ok(idx) => idx,
        Err(reason) => {
            return AlignmentMode::Positional {
                reason: Some(format!("{reason} in baseline")),
            };
        }
    };
    let c_idx = match unique_column_index(curr_cols, col) {
        Ok(idx) => idx,
        Err(reason) => {
            return AlignmentMode::Positional {
                reason: Some(format!("{reason} in current")),
            };
        }
    };
    if let Err(reason) = validate_key_values(base_rows, b_idx, "baseline", col) {
        return AlignmentMode::Positional {
            reason: Some(reason),
        };
    }
    if let Err(reason) = validate_key_values(curr_rows, c_idx, "current", col) {
        return AlignmentMode::Positional {
            reason: Some(reason),
        };
    }
    AlignmentMode::Keyed {
        column: col.to_owned(),
    }
}

fn find_candidate_key_columns(
    shared_columns: &[String],
    base_set: &ResultSet,
    base_rows: &[Vec<Cell>],
    curr_set: &ResultSet,
    curr_rows: &[Vec<Cell>],
) -> Vec<String> {
    shared_columns
        .iter()
        .filter(|col| {
            matches!(
                resolve_alignment(
                    Some(col.as_str()),
                    &base_set.columns,
                    base_rows,
                    &curr_set.columns,
                    curr_rows
                ),
                AlignmentMode::Keyed { .. }
            )
        })
        .cloned()
        .collect()
}

fn classify_paired_row(
    b_row: &[Cell],
    c_row: &[Cell],
    shared_pairs: &[(String, usize, usize)],
) -> DiffRowKind {
    let mut cell_diffs = Vec::new();
    let mut cells_truncated = false;

    for (name, b_idx, c_idx) in shared_pairs {
        let b_cell = b_row.get(*b_idx).cloned().unwrap_or(Cell::Null);
        let c_cell = c_row.get(*c_idx).cloned().unwrap_or(Cell::Null);
        if b_cell != c_cell {
            if cell_diffs.len() < MAX_CELL_DIFFS_PER_ROW {
                cell_diffs.push(CellDiff {
                    column: name.clone(),
                    baseline: b_cell,
                    current: c_cell,
                });
            } else {
                cells_truncated = true;
                break;
            }
        }
    }

    if cell_diffs.is_empty() {
        DiffRowKind::Unchanged
    } else {
        DiffRowKind::Changed {
            cell_diffs,
            cells_truncated,
        }
    }
}

fn project_row(
    row: Option<&[Cell]>,
    mapping: &[(Option<usize>, Option<usize>)],
    use_baseline: bool,
) -> Vec<Option<Cell>> {
    let Some(row) = row else {
        return vec![None; mapping.len()];
    };
    mapping
        .iter()
        .map(|(b_idx, c_idx)| {
            let idx = if use_baseline { *b_idx } else { *c_idx };
            idx.and_then(|i| row.get(i).cloned())
        })
        .collect()
}

fn push_bounded_entry(entries: &mut Vec<DiffRow>, truncated: &mut bool, row: DiffRow) {
    if entries.len() < MAX_DIFF_ROWS {
        entries.push(row);
    } else {
        *truncated = true;
    }
}

enum CellSide {
    Baseline,
    Current,
}

fn row_identity_label(entry: &DiffRow) -> String {
    if let Some(key) = &entry.key_value {
        return format!("key={}", sanitize_for_display(key));
    }
    match (entry.baseline_index, entry.current_index) {
        (Some(b), Some(c)) if b == c => format!("row {}", b + 1),
        (Some(b), Some(c)) => format!("row {} -> {}", b + 1, c + 1),
        (Some(b), None) => format!("row {} (baseline)", b + 1),
        (None, Some(c)) => format!("row {} (current)", c + 1),
        (None, None) => "row ?".to_owned(),
    }
}

fn format_cell_badge(cell: &Cell) -> String {
    match cell {
        Cell::Null => "[null]".to_owned(),
        Cell::Text(s) if s.is_empty() => "\"\"".to_owned(),
        Cell::Text(s) => sanitize_for_display(s),
    }
}

fn format_row_cells(columns: &[String], cells: &[Option<Cell>], _side: CellSide) -> String {
    columns
        .iter()
        .zip(cells)
        .filter_map(|(col, cell)| {
            cell.as_ref()
                .map(|val| format!("{}={}", sanitize_for_display(col), format_cell_badge(val)))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn escape_md(text: &str) -> String {
    sanitize_for_display(text).replace('|', "\\|")
}

fn cell_to_json(cell: &Cell) -> serde_json::Value {
    match cell {
        Cell::Null => serde_json::Value::Null,
        Cell::Text(s) => serde_json::Value::String(s.clone()),
    }
}

fn cells_to_json_map<'a>(
    columns: &'a [String],
    cells: &[Option<Cell>],
) -> BTreeMap<&'a str, serde_json::Value> {
    let mut map = BTreeMap::new();
    for (col, cell) in columns.iter().zip(cells) {
        if let Some(val) = cell {
            map.insert(col.as_str(), cell_to_json(val));
        }
    }
    map
}

fn json_nesting_within_bound(text: &str, max_depth: usize) -> bool {
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for ch in text.chars() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '{' | '[' => {
                depth += 1;
                if depth > max_depth {
                    return false;
                }
            }
            '}' | ']' => {
                depth = depth.saturating_sub(1);
            }
            _ => {}
        }
    }
    true
}

#[cfg(test)]
mod tests;
