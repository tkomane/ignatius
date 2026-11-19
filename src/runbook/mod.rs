//! Pure Markdown and SQL buffer notebook parser, write-guard planner, and
//! plain-ASCII report renderer for Feature `058-interactive-sql-notebook-runner`.
//!
//! This module contains zero filesystem, network, process, database, or
//! terminal I/O. Advisory write classification uses [`crate::query::classify_all`];
//! consistent with Constitution IV, heuristic SQL classification is an advisory
//! safety guardrail and never a security boundary.

#[cfg(test)]
mod tests;

use std::collections::HashSet;
use std::fmt;
use std::fmt::Write as _;

use serde::{Deserialize, Serialize};

use crate::diagnostics::redaction::redact_text;
use crate::diagnostics::{Diagnostic, DiagnosticKind};
use crate::query::highlight::{TokenKind, tokens};
use crate::query::statements;
use crate::query::value::sanitize_for_display;
use crate::query::{Impact, classify_all};

/// Maximum number of SQL cells retained from a single Markdown or buffer runbook.
pub const MAX_RUNBOOK_CELLS: usize = 200;

/// Maximum input byte size accepted for a Markdown or buffer runbook (1 MiB).
pub const MAX_RUNBOOK_BYTES: usize = 1024 * 1024;

const MAX_TABLE_NAME_WIDTH: usize = 28;
const MAX_TABLE_HEADING_WIDTH: usize = 24;
const MAX_TABLE_MESSAGE_WIDTH: usize = 120;

/// One parsed, classified SQL cell inside a [`SqlRunbook`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunbookCell {
    /// Zero-based position of this cell within [`SqlRunbook::cells`].
    pub index: usize,
    /// Stable cell identifier (from `-- @name:`, `-- @cell`, or `cell-<N>`).
    pub name: String,
    /// Active Markdown heading (`#`, `##`, `###`, ...) when extracted from Markdown.
    pub heading: Option<String>,
    /// One-based line number in the source input where [`Self::sql`] begins.
    pub start_line: usize,
    /// Raw SQL body of the cell.
    pub sql: String,
    /// Whether heuristic classification detects non-read statements (`Write`,
    /// `Structural`, `Destructive`, `DO`/`CALL`, or unrecognized commands).
    pub is_write: bool,
    /// Whether heuristic classification detects destructive statements (`DROP`,
    /// `TRUNCATE`, `DELETE`, or `WITH ... DELETE`).
    pub is_destructive: bool,
    /// Whether the cell declared `-- @readonly` (advisory hint; never bypasses [`Self::is_write`]).
    pub readonly_hint: bool,
    /// Optional row-count expectation declared via `-- @expect-rows: <N>`.
    pub expect_rows: Option<u64>,
}

/// A parsed, bounded collection of executable SQL cells.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SqlRunbook {
    /// Human-readable runbook title (redacted for secrets).
    pub title: String,
    /// Ordered SQL cells bounded to [`MAX_RUNBOOK_CELLS`].
    pub cells: Vec<RunbookCell>,
    /// Whether parsing stopped early due to [`MAX_RUNBOOK_CELLS`] or [`MAX_RUNBOOK_BYTES`].
    pub truncated: bool,
}

impl SqlRunbook {
    /// Number of cells in the runbook.
    #[must_use]
    pub fn len(&self) -> usize {
        self.cells.len()
    }

    /// Whether the runbook contains zero cells.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    /// Finds the first cell matching `name`.
    #[must_use]
    pub fn find_cell(&self, name: &str) -> Option<&RunbookCell> {
        self.cells.iter().find(|cell| cell.name == name)
    }
}

/// Execution or dry-run planning status of one runbook cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CellStatus {
    /// Cell is eligible for execution during dry-run planning.
    Planned,
    /// Cell executed and met all checks.
    Passed,
    /// Cell failed during execution or failed an expectation check.
    Failed,
    /// Cell was skipped (for example when another cell was selected).
    Skipped,
    /// Cell was blocked because write SQL requires explicit write authorization.
    BlockedByWriteGuard,
}

impl CellStatus {
    /// Plain-ASCII kebab-case status label used in tables, logs, and JSON.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Planned => "planned",
            Self::Passed => "passed",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
            Self::BlockedByWriteGuard => "blocked-by-write-guard",
        }
    }
}

impl fmt::Display for CellStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// Per-cell outcome recorded during dry-run planning or live execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CellExecutionRecord {
    /// Zero-based cell index matching [`RunbookCell::index`].
    pub index: usize,
    /// Cell name matching [`RunbookCell::name`].
    pub name: String,
    /// Active Markdown heading when present.
    pub heading: Option<String>,
    /// Whether the cell was classified as a write/non-read operation.
    pub is_write: bool,
    /// Resulting cell status.
    pub status: CellStatus,
    /// Number of rows returned by the cell query, when applicable.
    pub rows_returned: Option<u64>,
    /// Number of rows modified by DML, when applicable.
    pub rows_affected: Option<u64>,
    /// Elapsed wall-clock execution duration in milliseconds (`0` for dry-run).
    pub duration_ms: u64,
    /// Optional human-readable diagnostic or expectation note.
    pub message: Option<String>,
}

/// Aggregated dry-run or execution report across all cells in a runbook.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunbookReport {
    /// Runbook title (redacted for secrets).
    pub title: String,
    /// Whether this report came from dry-run planning without database execution.
    pub dry_run: bool,
    /// Whether non-read cells were authorized to run.
    pub allow_writes: bool,
    /// Ordered per-cell records.
    pub cells: Vec<CellExecutionRecord>,
    /// Count of [`CellStatus::Passed`] cells.
    pub passed_count: usize,
    /// Count of [`CellStatus::Failed`] cells.
    pub failed_count: usize,
    /// Count of [`CellStatus::BlockedByWriteGuard`] cells.
    pub blocked_count: usize,
    /// Count of [`CellStatus::Skipped`] cells.
    pub skipped_count: usize,
    /// Saturated sum of per-cell durations in milliseconds.
    pub total_duration_ms: u64,
}

impl RunbookReport {
    /// Returns `true` if any cell failed or was blocked by the write guard.
    #[must_use]
    pub const fn has_failures(&self) -> bool {
        self.failed_count > 0 || self.blocked_count > 0
    }

    /// Returns `true` if at least one cell recorded [`CellStatus::Failed`].
    #[must_use]
    pub const fn has_execution_failures(&self) -> bool {
        self.failed_count > 0
    }

    /// Returns `true` if at least one cell recorded [`CellStatus::BlockedByWriteGuard`].
    #[must_use]
    pub const fn has_blocked_writes(&self) -> bool {
        self.blocked_count > 0
    }

    /// Count of [`CellStatus::Planned`] cells in the report.
    #[must_use]
    pub fn planned_count(&self) -> usize {
        self.cells
            .iter()
            .filter(|cell| cell.status == CellStatus::Planned)
            .count()
    }

    /// Renders an aligned, strictly plain-ASCII tabular summary of the runbook report.
    #[must_use]
    pub fn render_table(&self) -> String {
        let mut out = String::new();
        let mode_label = if self.dry_run { "dry-run" } else { "execute" };
        let writes_label = if self.allow_writes {
            "allowed"
        } else {
            "guarded"
        };
        let planned_count = self.planned_count();

        let _ = writeln!(out, "Runbook: {}", sanitize_ascii(&self.title));
        let _ = writeln!(out, "Mode: {mode_label} | Writes: {writes_label}");
        let _ = writeln!(
            out,
            "Summary: planned={} passed={} failed={} blocked={} skipped={} total_ms={}",
            planned_count,
            self.passed_count,
            self.failed_count,
            self.blocked_count,
            self.skipped_count,
            self.total_duration_ms
        );

        if self.cells.is_empty() {
            out.push_str("\nNo cells in runbook.\n");
            return out;
        }

        out.push('\n');
        let headers = [
            "#", "NAME", "HEADING", "ACCESS", "STATUS", "ROWS", "DURATION", "MESSAGE",
        ];
        let mut rows = Vec::with_capacity(self.cells.len());
        for cell in &self.cells {
            rows.push([
                cell.index.saturating_add(1).to_string(),
                truncate_ascii(&sanitize_ascii(&cell.name), MAX_TABLE_NAME_WIDTH),
                truncate_ascii(
                    &sanitize_ascii(cell.heading.as_deref().unwrap_or("-")),
                    MAX_TABLE_HEADING_WIDTH,
                ),
                if cell.is_write {
                    "write".to_owned()
                } else {
                    "read".to_owned()
                },
                cell.status.label().to_owned(),
                format_rows_summary(cell.rows_returned, cell.rows_affected),
                format!("{}ms", cell.duration_ms),
                truncate_ascii(
                    &sanitize_ascii(cell.message.as_deref().unwrap_or("-")),
                    MAX_TABLE_MESSAGE_WIDTH,
                ),
            ]);
        }

        let mut widths = [0usize; 7];
        for (col, header) in headers.iter().take(7).enumerate() {
            widths[col] = header.len();
        }
        for row in &rows {
            for col in 0..7 {
                widths[col] = widths[col].max(row[col].len());
            }
        }

        write_table_row(&mut out, &headers, &widths);
        let seps: Vec<String> = widths.iter().map(|w| "-".repeat(*w)).collect();
        let sep_refs = [
            seps[0].as_str(),
            seps[1].as_str(),
            seps[2].as_str(),
            seps[3].as_str(),
            seps[4].as_str(),
            seps[5].as_str(),
            seps[6].as_str(),
            "-------",
        ];
        write_table_row(&mut out, &sep_refs, &widths);

        for row in &rows {
            let refs = [
                row[0].as_str(),
                row[1].as_str(),
                row[2].as_str(),
                row[3].as_str(),
                row[4].as_str(),
                row[5].as_str(),
                row[6].as_str(),
                row[7].as_str(),
            ];
            write_table_row(&mut out, &refs, &widths);
        }

        out
    }

    /// Renders a line-oriented, strictly plain-ASCII report suitable for scripts and logs.
    #[must_use]
    pub fn render_plain(&self) -> String {
        let mut out = String::new();
        let mode_label = if self.dry_run { "dry-run" } else { "execute" };
        let _ = writeln!(out, "title={}", sanitize_ascii(&self.title));
        let _ = writeln!(out, "mode={mode_label}");
        let _ = writeln!(out, "allow_writes={}", self.allow_writes);
        let _ = writeln!(
            out,
            "summary: total={} planned={} passed={} failed={} blocked={} skipped={} total_duration_ms={}",
            self.cells.len(),
            self.planned_count(),
            self.passed_count,
            self.failed_count,
            self.blocked_count,
            self.skipped_count,
            self.total_duration_ms
        );

        for cell in &self.cells {
            let heading = cell
                .heading
                .as_deref()
                .map_or_else(|| "-".to_owned(), sanitize_ascii);
            let rows_ret = cell
                .rows_returned
                .map_or_else(|| "-".to_owned(), |n| n.to_string());
            let rows_aff = cell
                .rows_affected
                .map_or_else(|| "-".to_owned(), |n| n.to_string());
            let msg = cell
                .message
                .as_deref()
                .map_or_else(|| "-".to_owned(), sanitize_ascii);
            let _ = writeln!(
                out,
                "cell index={} name={} heading={} write={} status={} rows_returned={} rows_affected={} duration_ms={} message={}",
                cell.index,
                sanitize_ascii(&cell.name),
                heading,
                cell.is_write,
                cell.status.label(),
                rows_ret,
                rows_aff,
                cell.duration_ms,
                msg
            );
        }

        out
    }

    /// Renders a deterministic, strictly plain-ASCII JSON document for the report.
    #[must_use]
    pub fn render_json(&self) -> String {
        let raw = serde_json::to_string_pretty(self).unwrap_or_else(|_| {
            "{\"error\":\"runbook report could not be serialized\"}".to_owned()
        });
        escape_json_to_ascii(&raw)
    }

    /// Renders a strictly plain-ASCII Markdown report with an execution summary and cell table.
    #[must_use]
    pub fn render_markdown(&self) -> String {
        let mut out = String::new();
        let mode_label = if self.dry_run { "dry-run" } else { "execute" };
        let writes_label = if self.allow_writes { "yes" } else { "no" };

        let _ = writeln!(
            out,
            "# Runbook Report: {}\n",
            markdown_cell_ascii(&self.title)
        );
        let _ = writeln!(out, "- Mode: {mode_label}");
        let _ = writeln!(out, "- Allow writes: {writes_label}");
        let _ = writeln!(
            out,
            "- Summary: planned={}, passed={}, failed={}, blocked={}, skipped={}, total_duration_ms={}\n",
            self.planned_count(),
            self.passed_count,
            self.failed_count,
            self.blocked_count,
            self.skipped_count,
            self.total_duration_ms
        );

        if self.cells.is_empty() {
            out.push_str("_No cells in runbook._\n");
            return out;
        }

        out.push_str("| # | Name | Heading | Access | Status | Rows | Duration (ms) | Message |\n");
        out.push_str("| --- | --- | --- | --- | --- | --- | --- | --- |\n");
        for cell in &self.cells {
            let heading = cell
                .heading
                .as_deref()
                .map_or_else(|| "-".to_owned(), markdown_cell_ascii);
            let access = if cell.is_write { "write" } else { "read" };
            let rows = format_rows_summary(cell.rows_returned, cell.rows_affected);
            let message = cell
                .message
                .as_deref()
                .map_or_else(|| "-".to_owned(), markdown_cell_ascii);

            let _ = writeln!(
                out,
                "| {} | {} | {} | {} | {} | {} | {} | {} |",
                cell.index.saturating_add(1),
                markdown_cell_ascii(&cell.name),
                heading,
                access,
                cell.status.label(),
                rows,
                cell.duration_ms,
                message
            );
        }

        out
    }
}

/// Parses a Markdown SQL runbook into bounded, classified [`RunbookCell`] entries.
///
/// Enforces [`MAX_RUNBOOK_BYTES`] with an explicit [`DiagnosticKind::Usage`] refusal
/// and caps retained cells at [`MAX_RUNBOOK_CELLS`] (setting
/// [`SqlRunbook::truncated`] to `true` when additional executable SQL cells exist).
/// Refuses documents with zero executable SQL cells or unclosed SQL fences with
/// [`DiagnosticKind::Usage`].
///
/// Tracks `#`/`##`/`###` (up to `######`) ATX headings outside code fences,
/// extracts fenced ```` ```sql ````, ```` ```postgresql ````, and ```` ~~~sql ````
/// blocks while ignoring prose code spans and unclosed non-SQL fences safely,
/// parses optional `-- @name: <slug>`, `-- @readonly`, and `-- @expect-rows: <N>`
/// directives, and classifies every cell via [`classify_all`].
pub fn parse_markdown_runbook(markdown: &str, title: &str) -> Result<SqlRunbook, Diagnostic> {
    if markdown.len() > MAX_RUNBOOK_BYTES {
        return Err(Diagnostic::new(
            DiagnosticKind::Usage,
            "the markdown runbook exceeded the maximum supported size",
            "parsing markdown SQL runbook",
        )
        .likely_cause(format!(
            "the input is {} bytes, exceeding the {MAX_RUNBOOK_BYTES}-byte limit",
            markdown.len()
        ))
        .next_action("split the runbook into smaller files under 1 MiB"));
    }

    let source = markdown.strip_prefix('\u{FEFF}').unwrap_or(markdown);
    if source.trim().is_empty() {
        return Err(Diagnostic::new(
            DiagnosticKind::Usage,
            "the markdown runbook is empty",
            "parsing markdown SQL runbook",
        )
        .likely_cause("the document contains only whitespace")
        .next_action("add at least one fenced ```sql code block"));
    }

    let mut cells = Vec::new();
    let mut used_names = HashSet::new();
    let mut truncated = false;
    let mut current_heading: Option<String> = None;
    let mut active_fence: Option<OpenFence> = None;

    for (line_idx, raw_line) in source.split('\n').enumerate() {
        let line_no = line_idx + 1;
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);

        if let Some(fence) = active_fence.as_mut() {
            if is_fence_close(line, fence.fence_char, fence.fence_len) {
                let finished = active_fence.take().expect("active fence is present");
                if finished.is_sql {
                    let block_sql = finished.lines.join("\n");
                    if cells.len() >= MAX_RUNBOOK_CELLS {
                        if !statements::split(&block_sql).is_empty() {
                            truncated = true;
                        }
                    } else if let Some(cell) = build_cell(
                        cells.len(),
                        None,
                        finished.heading,
                        finished.body_start_line,
                        &block_sql,
                        &mut used_names,
                    ) {
                        cells.push(cell);
                    }
                }
            } else if fence.is_sql && !truncated {
                fence
                    .lines
                    .push(strip_up_to_indent(line, fence.indent).to_owned());
            }
            continue;
        }

        if let Some(open) = parse_fence_open(line, line_no, current_heading.clone()) {
            active_fence = Some(open);
            continue;
        }

        if let Some(heading) = parse_atx_heading(line) {
            current_heading = Some(heading);
        }
    }

    if let Some(unclosed) = active_fence
        && unclosed.is_sql
    {
        return Err(Diagnostic::new(
            DiagnosticKind::Usage,
            "the markdown runbook contains an unclosed fenced SQL block",
            "parsing markdown SQL runbook",
        )
        .likely_cause(format!(
            "the fenced SQL block opened at line {} was never closed",
            unclosed.open_line
        ))
        .next_action("close the fenced SQL block with matching backticks or tildes"));
    }

    if cells.is_empty() {
        return Err(Diagnostic::new(
            DiagnosticKind::Usage,
            "the markdown runbook contained no executable SQL cells",
            "parsing markdown SQL runbook",
        )
        .likely_cause("no non-empty fenced ```sql, ```postgresql, or ~~~sql blocks were found")
        .next_action("add at least one fenced SQL block with an executable statement"));
    }

    Ok(SqlRunbook {
        title: normalize_title(title),
        cells,
        truncated,
    })
}

/// Parses an editor or script SQL buffer into a bounded [`SqlRunbook`].
///
/// If top-level `-- @cell <name>` markers are present (outside strings, dollar
/// quotes, and block comments), cells are split at those markers. Otherwise,
/// individual statements from [`crate::query::statements::split`] become cells.
/// Records exact 1-based `start_line` offsets and enforces [`MAX_RUNBOOK_BYTES`]
/// and [`MAX_RUNBOOK_CELLS`] by truncating deterministically with `truncated = true`.
#[must_use]
pub fn parse_buffer_runbook(buffer_sql: &str, title: &str) -> SqlRunbook {
    let source = buffer_sql.strip_prefix('\u{FEFF}').unwrap_or(buffer_sql);
    let (bounded_sql, mut truncated) = if source.len() > MAX_RUNBOOK_BYTES {
        let mut end = MAX_RUNBOOK_BYTES;
        while end > 0 && !source.is_char_boundary(end) {
            end -= 1;
        }
        (&source[..end], true)
    } else {
        (source, false)
    };

    let markers = find_cell_markers(bounded_sql);
    let mut cells = Vec::new();
    let mut used_names = HashSet::new();

    if markers.is_empty() {
        for stmt in statements::split(bounded_sql) {
            if cells.len() >= MAX_RUNBOOK_CELLS {
                truncated = true;
                break;
            }
            let start_line = line_number_at(bounded_sql, stmt.start);
            let sql = if stmt.terminated {
                format!("{};", stmt.text)
            } else {
                stmt.text.clone()
            };
            if let Some(cell) =
                build_cell(cells.len(), None, None, start_line, &sql, &mut used_names)
            {
                cells.push(cell);
            }
        }
    } else {
        let preamble = &bounded_sql[..markers[0].marker_line_start];
        if !preamble.trim().is_empty()
            && let Some(cell) = build_cell(cells.len(), None, None, 1, preamble, &mut used_names)
        {
            cells.push(cell);
        }

        for (idx, marker) in markers.iter().enumerate() {
            let slice_start = marker.body_start;
            let slice_end = markers
                .get(idx + 1)
                .map_or(bounded_sql.len(), |next| next.marker_line_start);
            if slice_start >= slice_end {
                continue;
            }
            let body = &bounded_sql[slice_start..slice_end];
            if cells.len() >= MAX_RUNBOOK_CELLS {
                if !statements::split(body).is_empty() {
                    truncated = true;
                    break;
                }
                continue;
            }
            let start_line = line_number_at(bounded_sql, slice_start);
            if let Some(cell) = build_cell(
                cells.len(),
                marker.name.clone(),
                None,
                start_line,
                body,
                &mut used_names,
            ) {
                cells.push(cell);
            }
        }
    }

    SqlRunbook {
        title: normalize_title(title),
        cells,
        truncated,
    }
}

/// Builds a deterministic dry-run plan without executing any SQL against PostgreSQL.
///
/// Write/structural/destructive cells are marked [`CellStatus::BlockedByWriteGuard`]
/// when `!allow_writes`, and runnable cells are marked [`CellStatus::Planned`].
/// When `selected_cell` is `Some(name)`, only cells with matching `cell.name` are
/// planned or guarded; all others are marked [`CellStatus::Skipped`].
#[must_use]
pub fn plan_dry_run(
    runbook: &SqlRunbook,
    allow_writes: bool,
    selected_cell: Option<&str>,
) -> RunbookReport {
    let mut records = Vec::with_capacity(runbook.cells.len());

    for cell in &runbook.cells {
        if let Some(target) = selected_cell
            && cell.name != target
        {
            records.push(CellExecutionRecord {
                index: cell.index,
                name: cell.name.clone(),
                heading: cell.heading.clone(),
                is_write: cell.is_write,
                status: CellStatus::Skipped,
                rows_returned: None,
                rows_affected: None,
                duration_ms: 0,
                message: Some(format!(
                    "skipped (selected cell: {})",
                    sanitize_ascii(target)
                )),
            });
            continue;
        }

        let (status, message) = if cell.is_write && !allow_writes {
            let reason = if cell.is_destructive {
                "blocked by write guard: destructive SQL requires --allow-writes"
            } else {
                "blocked by write guard: write or structural SQL requires --allow-writes"
            };
            let msg = if cell.readonly_hint {
                format!("{reason} (cell also declares @readonly)")
            } else {
                reason.to_owned()
            };
            (CellStatus::BlockedByWriteGuard, Some(msg))
        } else if cell.readonly_hint && cell.is_write {
            (
                CellStatus::Planned,
                Some("warning: cell marked @readonly contains write SQL".to_owned()),
            )
        } else {
            (CellStatus::Planned, None)
        };

        records.push(CellExecutionRecord {
            index: cell.index,
            name: cell.name.clone(),
            heading: cell.heading.clone(),
            is_write: cell.is_write,
            status,
            rows_returned: None,
            rows_affected: None,
            duration_ms: 0,
            message,
        });
    }

    summarize_report(runbook.title.clone(), true, allow_writes, records)
}

/// Aggregates per-cell records into a [`RunbookReport`] with deterministic counts.
#[must_use]
pub fn summarize_report(
    title: String,
    dry_run: bool,
    allow_writes: bool,
    records: Vec<CellExecutionRecord>,
) -> RunbookReport {
    let mut passed_count = 0usize;
    let mut failed_count = 0usize;
    let mut blocked_count = 0usize;
    let mut skipped_count = 0usize;
    let mut total_duration_ms = 0u64;

    let cells = records
        .into_iter()
        .map(|mut record| {
            match record.status {
                CellStatus::Planned => {}
                CellStatus::Passed => passed_count += 1,
                CellStatus::Failed => failed_count += 1,
                CellStatus::Skipped => skipped_count += 1,
                CellStatus::BlockedByWriteGuard => blocked_count += 1,
            }
            total_duration_ms = total_duration_ms.saturating_add(record.duration_ms);
            record.message = record.message.map(|msg| redact_text(&msg));
            record
        })
        .collect();

    RunbookReport {
        title: normalize_title(&title),
        dry_run,
        allow_writes,
        cells,
        passed_count,
        failed_count,
        blocked_count,
        skipped_count,
        total_duration_ms,
    }
}

#[derive(Debug, Clone)]
struct OpenFence {
    fence_char: u8,
    fence_len: usize,
    indent: usize,
    is_sql: bool,
    open_line: usize,
    body_start_line: usize,
    heading: Option<String>,
    lines: Vec<String>,
}

#[derive(Debug, Default)]
struct CellDirectives {
    name: Option<String>,
    readonly_hint: bool,
    expect_rows: Option<u64>,
}

#[derive(Debug, Clone)]
struct BufferCellMarker {
    marker_line_start: usize,
    body_start: usize,
    name: Option<String>,
}

fn build_cell(
    index: usize,
    explicit_name: Option<String>,
    heading: Option<String>,
    mut start_line: usize,
    raw_sql: &str,
    used_names: &mut HashSet<String>,
) -> Option<RunbookCell> {
    let (trimmed_sql, skipped_lines) = trim_leading_blank_lines(raw_sql);
    start_line = start_line.saturating_add(skipped_lines);
    let sql = trimmed_sql.trim_end();
    if sql.is_empty() {
        return None;
    }

    let stmts = statements::split(sql);
    if stmts.is_empty() {
        return None;
    }

    let directives = extract_directives(sql);
    let name = explicit_name
        .filter(|s| !s.is_empty())
        .or(directives.name)
        .unwrap_or_else(|| next_default_name(index, used_names));
    used_names.insert(name.clone());

    let impact = classify_all(&stmts);
    let is_write = impact.needs_confirmation();
    let is_destructive = matches!(impact, Impact::Destructive) || impact.needs_typed_confirmation();

    Some(RunbookCell {
        index,
        name,
        heading,
        start_line,
        sql: sql.to_owned(),
        is_write,
        is_destructive,
        readonly_hint: directives.readonly_hint,
        expect_rows: directives.expect_rows,
    })
}

fn next_default_name(index: usize, used_names: &HashSet<String>) -> String {
    let mut num = index.saturating_add(1);
    loop {
        let candidate = format!("cell-{num}");
        if !used_names.contains(&candidate) {
            return candidate;
        }
        num = num.saturating_add(1);
    }
}

fn trim_leading_blank_lines(input: &str) -> (&str, usize) {
    let mut offset = 0usize;
    let mut skipped = 0usize;

    while offset < input.len() {
        let rest = &input[offset..];
        if let Some(nl) = rest.find('\n') {
            let line = rest[..nl].strip_suffix('\r').unwrap_or(&rest[..nl]);
            if line.trim().is_empty() {
                offset += nl + 1;
                skipped += 1;
                continue;
            }
            break;
        }
        if rest.trim().is_empty() {
            return ("", skipped);
        }
        break;
    }

    (&input[offset..], skipped)
}

fn extract_directives(sql: &str) -> CellDirectives {
    let mut directives = CellDirectives::default();

    for tok in tokens(sql) {
        if tok.kind != TokenKind::Comment {
            continue;
        }
        let comment = &sql[tok.start..tok.end];
        if !comment.starts_with("--") || !is_line_start_before(sql, tok.start) {
            continue;
        }
        let body = comment[2..].trim();
        if let Some(raw_name) = parse_directive_arg(body, "@name") {
            let slug = clean_slug(raw_name);
            if !slug.is_empty() {
                directives.name = Some(slug);
            }
        } else if let Some(raw_rows) = parse_directive_arg(body, "@expect-rows") {
            if let Some(count) = raw_rows
                .split_whitespace()
                .next()
                .and_then(|num| num.parse::<u64>().ok())
            {
                directives.expect_rows = Some(count);
            }
        } else if let Some(flag) = parse_readonly_directive(body) {
            directives.readonly_hint = flag;
        }
    }

    directives
}

fn find_cell_markers(sql: &str) -> Vec<BufferCellMarker> {
    let mut markers = Vec::new();
    let bytes = sql.as_bytes();

    for tok in tokens(sql) {
        if tok.kind != TokenKind::Comment {
            continue;
        }
        let comment = &sql[tok.start..tok.end];
        if !comment.starts_with("--") || !is_line_start_before(sql, tok.start) {
            continue;
        }
        let body = comment[2..].trim();
        let Some(raw_name) = parse_optional_directive_arg(body, "@cell") else {
            continue;
        };

        let marker_line_start = sql[..tok.start].rfind('\n').map_or(0, |pos| pos + 1);
        let body_start = match bytes[tok.end..].iter().position(|&b| b == b'\n') {
            Some(rel) => tok.end + rel + 1,
            None => sql.len(),
        };
        let slug = clean_slug(raw_name);
        let name = (!slug.is_empty()).then_some(slug);

        markers.push(BufferCellMarker {
            marker_line_start,
            body_start,
            name,
        });
    }

    markers
}

fn is_line_start_before(sql: &str, offset: usize) -> bool {
    let line_start = sql[..offset].rfind('\n').map_or(0, |pos| pos + 1);
    sql[line_start..offset]
        .bytes()
        .all(|b| matches!(b, b' ' | b'\t'))
}

fn parse_directive_arg<'a>(body: &'a str, keyword: &str) -> Option<&'a str> {
    let rest = strip_ascii_prefix_ci(body, keyword)?;
    if let Some(after_colon) = rest.strip_prefix(':') {
        return Some(after_colon.trim());
    }
    if rest.starts_with([' ', '\t']) {
        return Some(rest.trim());
    }
    None
}

fn parse_optional_directive_arg<'a>(body: &'a str, keyword: &str) -> Option<&'a str> {
    let rest = strip_ascii_prefix_ci(body, keyword)?;
    if rest.is_empty() {
        return Some("");
    }
    if let Some(after_colon) = rest.strip_prefix(':') {
        return Some(after_colon.trim());
    }
    if rest.starts_with([' ', '\t']) {
        return Some(rest.trim());
    }
    None
}

fn parse_readonly_directive(body: &str) -> Option<bool> {
    let rest = parse_optional_directive_arg(body, "@readonly")?;
    let val = rest.trim_start_matches([':', ' ', '\t']).trim();
    if val.is_empty() {
        return Some(true);
    }
    let first = val.split_whitespace().next().unwrap_or("");
    if first.eq_ignore_ascii_case("false") || first.eq_ignore_ascii_case("no") || first == "0" {
        Some(false)
    } else {
        Some(true)
    }
}

fn strip_ascii_prefix_ci<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let head = text.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix)
        .then_some(&text[prefix.len()..])
}

fn parse_fence_open(line: &str, line_no: usize, heading: Option<String>) -> Option<OpenFence> {
    let indent = line.bytes().take_while(|&b| b == b' ').count();
    if indent > 3 {
        return None;
    }
    let rest = &line[indent..];
    let fence_char = *rest.as_bytes().first()?;
    if fence_char != b'`' && fence_char != b'~' {
        return None;
    }
    let fence_len = rest.bytes().take_while(|&b| b == fence_char).count();
    if fence_len < 3 {
        return None;
    }
    let info = &rest[fence_len..];
    if fence_char == b'`' && info.contains('`') {
        return None;
    }
    if fence_char == b'~' && info.contains("~~~") {
        return None;
    }

    let lang = info
        .trim()
        .split(|c: char| c.is_ascii_whitespace() || matches!(c, ',' | ';' | '{'))
        .next()
        .unwrap_or("");
    let is_sql = matches!(
        lang.to_ascii_lowercase().as_str(),
        "sql" | "postgresql" | "postgres" | "pgsql"
    );

    Some(OpenFence {
        fence_char,
        fence_len,
        indent,
        is_sql,
        open_line: line_no,
        body_start_line: line_no + 1,
        heading,
        lines: Vec::new(),
    })
}

fn is_fence_close(line: &str, fence_char: u8, min_len: usize) -> bool {
    let indent = line.bytes().take_while(|&b| b == b' ').count();
    if indent > 3 {
        return false;
    }
    let rest = &line[indent..];
    let close_len = rest.bytes().take_while(|&b| b == fence_char).count();
    if close_len < min_len {
        return false;
    }
    rest[close_len..].bytes().all(|b| matches!(b, b' ' | b'\t'))
}

fn strip_up_to_indent(line: &str, max_spaces: usize) -> &str {
    let spaces = line
        .bytes()
        .take(max_spaces)
        .take_while(|&b| b == b' ')
        .count();
    &line[spaces..]
}

fn parse_atx_heading(line: &str) -> Option<String> {
    let indent = line.bytes().take_while(|&b| b == b' ').count();
    if indent > 3 {
        return None;
    }
    let rest = &line[indent..];
    let level = rest.bytes().take_while(|&b| b == b'#').count();
    if !(1..=6).contains(&level) {
        return None;
    }
    let after = &rest[level..];
    if !after.is_empty() && !after.starts_with([' ', '\t']) {
        return None;
    }

    let mut content = after.trim();
    if content.ends_with('#') {
        let without_hashes = content.trim_end_matches('#');
        if without_hashes.is_empty() || without_hashes.ends_with([' ', '\t']) {
            content = without_hashes.trim();
        }
    }
    if content.is_empty() {
        return None;
    }
    let cleaned = sanitize_for_display(&redact_text(content));
    let trimmed = cleaned.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

fn line_number_at(text: &str, byte_offset: usize) -> usize {
    let clamped = byte_offset.min(text.len());
    text[..clamped].bytes().filter(|&b| b == b'\n').count() + 1
}

fn normalize_title(title: &str) -> String {
    let redacted = redact_text(title.trim());
    let sanitized = sanitize_for_display(&redacted);
    let collapsed = sanitized.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        "Untitled Runbook".to_owned()
    } else {
        collapsed
    }
}

fn clean_slug(raw: &str) -> String {
    let redacted = redact_text(raw);
    let sanitized = sanitize_for_display(&redacted);
    sanitized.trim().to_owned()
}

fn format_rows_summary(rows_returned: Option<u64>, rows_affected: Option<u64>) -> String {
    match (rows_returned, rows_affected) {
        (Some(ret), Some(aff)) => format!("{ret} ret, {aff} aff"),
        (Some(ret), None) => format!("{ret} ret"),
        (None, Some(aff)) => format!("{aff} aff"),
        (None, None) => "-".to_owned(),
    }
}

fn write_table_row(out: &mut String, cols: &[&str; 8], widths: &[usize; 7]) {
    let _ = writeln!(
        out,
        "{:>w0$}  {:<w1$}  {:<w2$}  {:<w3$}  {:<w4$}  {:<w5$}  {:>w6$}  {}",
        cols[0],
        cols[1],
        cols[2],
        cols[3],
        cols[4],
        cols[5],
        cols[6],
        cols[7],
        w0 = widths[0],
        w1 = widths[1],
        w2 = widths[2],
        w3 = widths[3],
        w4 = widths[4],
        w5 = widths[5],
        w6 = widths[6],
    );
}

fn markdown_cell_ascii(value: &str) -> String {
    let safe = sanitize_ascii(value);
    let mut out = String::with_capacity(safe.len());
    for ch in safe.chars() {
        match ch {
            '|' => out.push_str("\\|"),
            c => out.push(c),
        }
    }
    out
}

fn sanitize_ascii(input: &str) -> String {
    let safe = sanitize_for_display(&redact_text(input));
    let mut out = String::with_capacity(safe.len());
    for ch in safe.chars() {
        if ch.is_ascii() && !ch.is_ascii_control() {
            out.push(ch);
        } else {
            let _ = write!(out, "\\u{{{:04X}}}", ch as u32);
        }
    }
    out
}

fn truncate_ascii(input: &str, max_len: usize) -> String {
    if input.len() <= max_len {
        return input.to_owned();
    }
    if max_len <= 3 {
        return input.chars().take(max_len).collect();
    }
    let mut out: String = input.chars().take(max_len - 3).collect();
    out.push_str("...");
    out
}

fn escape_json_to_ascii(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        if ch.is_ascii() {
            out.push(ch);
        } else {
            let code = ch as u32;
            if code <= 0xFFFF {
                let _ = write!(out, "\\u{code:04x}");
            } else {
                let adjusted = code - 0x1_0000;
                let high = 0xD800 + (adjusted >> 10);
                let low = 0xDC00 + (adjusted & 0x3FF);
                let _ = write!(out, "\\u{high:04x}\\u{low:04x}");
            }
        }
    }
    out
}
