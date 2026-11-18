//! Zero-query result analytics, Field Industrial HTML reports, and streaming
//! pure-Rust Apache Parquet binary export for Feature
//! `055-result-analytics-and-rich-exports`.
//!
//! This module is pure domain logic: it performs zero SQL queries, zero network
//! I/O, zero filesystem opens, and zero unsafe code. Callers pass an in-memory
//! [`ResultSet`] and any `std::io::Write` sink (such as [`crate::query::export::Export`]).

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::fmt::Write as _;
use std::io;

use serde::{Deserialize, Serialize};

use crate::branding::{PRODUCT_NAME, VERSION};
use crate::query::result::{RETAINED_DATA_UNAVAILABLE, ResultSet};
use crate::query::value::{Cell, NULL_MARKER, sanitize_for_display};

/// Number of histogram buckets computed for every column profile.
pub const HISTOGRAM_BUCKETS: usize = 8;

/// Maximum number of distinct non-null values tracked per column before capping.
pub const MAX_DISTINCT_TRACKED: usize = 10_000;

/// Maximum number of top frequent non-null values retained per column.
pub const MAX_TOP_VALUES: usize = 5;

const ASCII_SPARKLINE_RAMP: [char; 9] = [' ', '.', ':', '-', '=', '+', '*', '#', '%'];
const UNICODE_SPARKLINE_RAMP: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

const PARQUET_MAGIC: &[u8; 4] = b"PAR1";
const PARQUET_VERSION: i32 = 1;

/// Zero-query statistical profile of one column across retained result rows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ColumnProfile {
    /// Column name from `ResultSet::columns`.
    pub name: String,
    /// Described PostgreSQL type name, or `"unknown"` when undescribed.
    pub type_name: String,
    /// Total retained rows evaluated for this column.
    pub total_rows: usize,
    /// Count of `Cell::Null` entries.
    pub null_count: usize,
    /// Count of non-null `Cell::Text("")` entries (distinct from `Cell::Null`).
    pub empty_count: usize,
    /// Count of non-null entries (`total_rows - null_count`).
    pub non_null_count: usize,
    /// Percentage of rows that are `Cell::Null` (`0.0`..=`100.0`).
    pub null_pct: f64,
    /// Number of distinct non-null values observed (up to [`MAX_DISTINCT_TRACKED`]).
    pub distinct_count: usize,
    /// Whether distinct value tracking hit [`MAX_DISTINCT_TRACKED`].
    pub distinct_capped: bool,
    /// Minimum non-null value observed (numeric order when purely numeric, otherwise lexicographical).
    pub min_value: Option<String>,
    /// Maximum non-null value observed (numeric order when purely numeric, otherwise lexicographical).
    pub max_value: Option<String>,
    /// Count of non-null values parsed as finite numbers.
    pub numeric_count: usize,
    /// Arithmetic mean of finite numeric values, when `numeric_count > 0`.
    pub mean: Option<f64>,
    /// Population standard deviation of finite numeric values, when `numeric_count > 0`.
    pub stddev: Option<f64>,
    /// Median (`50th` percentile, linear interpolation), when `numeric_count > 0`.
    pub p50: Option<f64>,
    /// `95th` percentile (linear interpolation), when `numeric_count > 0`.
    pub p95: Option<f64>,
    /// Fixed [`HISTOGRAM_BUCKETS`]-bucket distribution over numeric values or string lengths.
    pub histogram_counts: Vec<usize>,
    /// Strictly plain-ASCII 8-character sparkline built from `histogram_counts`.
    pub sparkline_ascii: String,
    /// Unicode block-character 8-character sparkline for terminals with `!glyphs.is_ascii()`.
    pub sparkline_unicode: String,
    /// Up to [`MAX_TOP_VALUES`] most frequent non-null `(value, count)` pairs.
    pub top_values: Vec<(String, usize)>,
}

/// Complete zero-query analytics summary over a retained [`ResultSet`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResultAnalytics {
    /// Retained row count actually profiled.
    pub row_count: usize,
    /// Total rows reported by the server (`ResultSet::rows_seen`), including truncated tail rows.
    pub rows_seen: usize,
    /// Whether the underlying `ResultSet` was truncated or unavailable.
    pub truncated: bool,
    /// Number of columns in the result (`source_column_count()` when unavailable).
    pub column_count: usize,
    /// Per-column profiles in result column order.
    pub columns: Vec<ColumnProfile>,
}

impl ResultAnalytics {
    /// Total `Cell::Null` entries across all profiled columns.
    #[must_use]
    pub fn total_nulls(&self) -> usize {
        self.columns.iter().map(|c| c.null_count).sum()
    }

    /// Renders a strictly plain-ASCII tabular summary of all column profiles.
    #[must_use]
    pub fn render_table(&self) -> String {
        self.render_table_with_glyphs(false)
    }

    /// Renders a tabular summary using ASCII or Unicode sparklines depending on `unicode`.
    #[must_use]
    pub fn render_table_with_glyphs(&self, unicode: bool) -> String {
        let mut out = String::new();
        out.push_str("Result Analytics Profile\n");
        out.push_str("========================\n\n");
        let _ = writeln!(
            out,
            "Summary: rows={} | rows_seen={} | columns={} | total_nulls={} | truncated={}",
            self.row_count,
            self.rows_seen,
            self.column_count,
            self.total_nulls(),
            self.truncated
        );
        if self.truncated {
            let _ = writeln!(
                out,
                "Notice: statistics reflect only the {} retained row(s) of {} seen.",
                self.row_count, self.rows_seen
            );
        }
        out.push('\n');

        if self.columns.is_empty() {
            out.push_str("(no columns available to profile)\n");
            return out;
        }

        let headers = [
            "Column", "Type", "Non-Null", "Nulls", "Null%", "Distinct", "Min", "Max", "Mean",
            "p50", "p95", "Dist",
        ];
        let mut rows: Vec<[String; 12]> = Vec::with_capacity(self.columns.len());
        for col in &self.columns {
            let distinct = if col.distinct_capped {
                format!("{}+", col.distinct_count)
            } else {
                col.distinct_count.to_string()
            };
            let spark = if unicode {
                col.sparkline_unicode.clone()
            } else {
                col.sparkline_ascii.clone()
            };
            rows.push([
                sanitize_for_display(&col.name),
                sanitize_for_display(&col.type_name),
                col.non_null_count.to_string(),
                col.null_count.to_string(),
                format!("{:.1}%", col.null_pct),
                distinct,
                format_opt_cell(col.min_value.as_deref()),
                format_opt_cell(col.max_value.as_deref()),
                format_opt_f64(col.mean),
                format_opt_f64(col.p50),
                format_opt_f64(col.p95),
                spark,
            ]);
        }

        let mut widths = [0usize; 12];
        for (i, h) in headers.iter().enumerate() {
            widths[i] = h.len();
        }
        for row in &rows {
            for (i, cell) in row.iter().enumerate() {
                widths[i] = widths[i].max(cell.chars().count());
            }
        }

        write_padded_row(&mut out, &headers, &widths);
        let sep: Vec<String> = widths.iter().map(|w| "-".repeat(*w)).collect();
        let sep_refs: [&str; 12] = [
            &sep[0], &sep[1], &sep[2], &sep[3], &sep[4], &sep[5], &sep[6], &sep[7], &sep[8],
            &sep[9], &sep[10], &sep[11],
        ];
        write_padded_row(&mut out, &sep_refs, &widths);
        for row in &rows {
            let refs: [&str; 12] = [
                &row[0], &row[1], &row[2], &row[3], &row[4], &row[5], &row[6], &row[7], &row[8],
                &row[9], &row[10], &row[11],
            ];
            write_padded_row(&mut out, &refs, &widths);
        }

        out
    }

    /// Renders a plain-ASCII line-oriented summary suitable for scripts and logs.
    #[must_use]
    pub fn render_plain(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "rows={} rows_seen={} columns={} total_nulls={} truncated={}",
            self.row_count,
            self.rows_seen,
            self.column_count,
            self.total_nulls(),
            self.truncated
        );
        for col in &self.columns {
            let _ = writeln!(
                out,
                "column={} type={} total={} non_null={} null={} empty={} null_pct={:.2} distinct={}{} min={} max={} mean={} stddev={} p50={} p95={} sparkline={}",
                sanitize_for_display(&col.name),
                sanitize_for_display(&col.type_name),
                col.total_rows,
                col.non_null_count,
                col.null_count,
                col.empty_count,
                col.null_pct,
                col.distinct_count,
                if col.distinct_capped { "+" } else { "" },
                format_opt_cell(col.min_value.as_deref()),
                format_opt_cell(col.max_value.as_deref()),
                format_opt_f64(col.mean),
                format_opt_f64(col.stddev),
                format_opt_f64(col.p50),
                format_opt_f64(col.p95),
                col.sparkline_ascii,
            );
        }
        out
    }

    /// Renders a deterministic pretty-printed JSON representation of the analytics profile.
    #[must_use]
    pub fn render_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_owned())
    }
}

/// Computes a zero-query statistical profile over all columns of a retained [`ResultSet`].
#[must_use]
pub fn profile_result_set(set: &ResultSet) -> ResultAnalytics {
    let rows_seen = usize::try_from(set.rows_seen).unwrap_or(usize::MAX);
    if !set.is_available() {
        return ResultAnalytics {
            row_count: 0,
            rows_seen,
            truncated: true,
            column_count: set.source_column_count(),
            columns: Vec::new(),
        };
    }

    let row_count = set.rows.len();
    let column_count = set.columns.len();
    let mut columns = Vec::with_capacity(column_count);

    for (col_idx, col_name) in set.columns.iter().enumerate() {
        let described_type = set.column_type(col_idx);
        let type_name = described_type.unwrap_or("unknown").to_owned();
        let allow_numeric_parse = described_type.is_none_or(|t| !is_non_numeric_pg_type(t));

        let mut null_count = 0usize;
        let mut empty_count = 0usize;
        let mut distinct_capped = false;
        let mut freq: HashMap<String, usize> = HashMap::new();

        let mut min_lex: Option<String> = None;
        let mut max_lex: Option<String> = None;
        let mut min_num: Option<(f64, String)> = None;
        let mut max_num: Option<(f64, String)> = None;

        let mut numeric_values: Vec<f64> = Vec::new();
        let mut string_lengths: Vec<f64> = Vec::with_capacity(row_count);
        let mut welford_count = 0usize;
        let mut welford_mean = 0.0f64;
        let mut welford_m2 = 0.0f64;

        for row in &set.rows {
            let cell = row.get(col_idx).unwrap_or(&Cell::Null);
            match cell {
                Cell::Null => {
                    null_count += 1;
                }
                Cell::Text(text) => {
                    if text.is_empty() {
                        empty_count += 1;
                    }
                    string_lengths.push(text.chars().count() as f64);

                    if let Some(count) = freq.get_mut(text) {
                        *count += 1;
                    } else if freq.len() < MAX_DISTINCT_TRACKED {
                        freq.insert(text.clone(), 1);
                    } else {
                        distinct_capped = true;
                    }

                    match &min_lex {
                        Some(cur) if text >= cur => {}
                        _ => min_lex = Some(text.clone()),
                    }
                    match &max_lex {
                        Some(cur) if text <= cur => {}
                        _ => max_lex = Some(text.clone()),
                    }

                    if allow_numeric_parse && let Some(val) = parse_finite_f64(text) {
                        welford_count += 1;
                        let delta = val - welford_mean;
                        welford_mean += delta / (welford_count as f64);
                        let delta2 = val - welford_mean;
                        welford_m2 += delta * delta2;
                        numeric_values.push(val);

                        match &min_num {
                            Some((cur, _)) if val.total_cmp(cur).is_ge() => {}
                            _ => min_num = Some((val, text.clone())),
                        }
                        match &max_num {
                            Some((cur, _)) if val.total_cmp(cur).is_le() => {}
                            _ => max_num = Some((val, text.clone())),
                        }
                    }
                }
            }
        }

        let non_null_count = row_count.saturating_sub(null_count);
        let null_pct = if row_count == 0 {
            0.0
        } else {
            (null_count as f64) * 100.0 / (row_count as f64)
        };

        let numeric_count = numeric_values.len();
        let (mean, stddev, p50, p95) = if numeric_count > 0 {
            numeric_values.sort_by(f64::total_cmp);
            let mean_val = normalize_zero(welford_mean);
            let variance = (welford_m2 / (welford_count as f64)).max(0.0);
            let stddev_val = normalize_zero(variance.sqrt());
            let p50_val = normalize_zero(percentile_linear(&numeric_values, 0.50));
            let p95_val = normalize_zero(percentile_linear(&numeric_values, 0.95));
            (
                Some(mean_val),
                Some(stddev_val),
                Some(p50_val),
                Some(p95_val),
            )
        } else {
            (None, None, None, None)
        };

        let (min_value, max_value) = if numeric_count > 0 && numeric_count == non_null_count {
            (min_num.map(|(_, raw)| raw), max_num.map(|(_, raw)| raw))
        } else {
            (min_lex, max_lex)
        };

        let histogram_counts = if numeric_count > 0
            && (numeric_count == non_null_count || described_type.is_some_and(is_numeric_pg_type))
        {
            build_histogram(&numeric_values)
        } else {
            build_histogram(&string_lengths)
        };

        let sparkline_ascii = build_sparkline(&histogram_counts, false);
        let sparkline_unicode = build_sparkline(&histogram_counts, true);

        let distinct_count = freq.len();
        let mut top_values: Vec<(String, usize)> = freq.into_iter().collect();
        top_values.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        top_values.truncate(MAX_TOP_VALUES);

        columns.push(ColumnProfile {
            name: col_name.clone(),
            type_name,
            total_rows: row_count,
            null_count,
            empty_count,
            non_null_count,
            null_pct,
            distinct_count,
            distinct_capped,
            min_value,
            max_value,
            numeric_count,
            mean,
            stddev,
            p50,
            p95,
            histogram_counts,
            sparkline_ascii,
            sparkline_unicode,
            top_values,
        });
    }

    ResultAnalytics {
        row_count,
        rows_seen,
        truncated: set.is_truncated(),
        column_count,
        columns,
    }
}

/// Escapes `&`, `<`, `>`, `"`, and `'` for safe HTML5 text and attribute output.
#[must_use]
pub fn escape_html(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#x27;"),
            c => out.push(c),
        }
    }
    out
}

/// Renders a standalone, offline Field Industrial HTML5 report with strict entity escaping.
#[must_use]
pub fn render_html_report(set: &ResultSet, title: &str, analytics: &ResultAnalytics) -> String {
    let safe_title = escape_html(title);
    let safe_brand = escape_html(PRODUCT_NAME);
    let safe_version = escape_html(VERSION);
    let safe_window = escape_html(&set.window_label());

    let mut out = String::with_capacity(8192);
    out.push_str("<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n");
    out.push_str("<meta charset=\"utf-8\">\n");
    out.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    let _ = writeln!(out, "<title>{safe_title} - {safe_brand}</title>");
    out.push_str("<style>\n");
    out.push_str(HTML_REPORT_CSS);
    out.push_str("</style>\n</head>\n<body>\n<main class=\"page\">\n");

    out.push_str("<header class=\"report-header\">\n");
    out.push_str("  <div class=\"brand-row\">\n");
    let _ = writeln!(
        out,
        "    <span class=\"brand-pill\">{safe_brand} v{safe_version}</span>"
    );
    let _ = writeln!(out, "    <span class=\"window-label\">{safe_window}</span>");
    out.push_str("  </div>\n");
    let _ = writeln!(out, "  <h1>{safe_title}</h1>\n");

    if set.is_truncated() || analytics.truncated {
        out.push_str("  <div class=\"truncation-banner\" role=\"status\">\n");
        let _ = writeln!(
            out,
            "    <strong>TRUNCATED RESULT:</strong> {safe_window}. Analytics and rows below cover retained data only."
        );
        out.push_str("  </div>\n");
    }

    out.push_str("  <section class=\"kpi-grid\" aria-label=\"Summary metrics\">\n");
    write_kpi_card(
        &mut out,
        "Retained Rows",
        &set.retained().to_string(),
        "kpi-sage",
    );
    write_kpi_card(
        &mut out,
        "Rows Seen",
        &set.rows_seen.to_string(),
        "kpi-bone",
    );
    write_kpi_card(
        &mut out,
        "Columns",
        &analytics.column_count.to_string(),
        "kpi-bone",
    );
    write_kpi_card(
        &mut out,
        "Total NULLs",
        &analytics.total_nulls().to_string(),
        "kpi-amber",
    );
    out.push_str("  </section>\n");
    out.push_str("</header>\n");

    if !set.is_available() {
        out.push_str("<section class=\"card\">\n");
        out.push_str("  <h2>Result Data Unavailable</h2>\n");
        let _ = writeln!(
            out,
            "  <p class=\"muted\">{}</p>",
            escape_html(RETAINED_DATA_UNAVAILABLE)
        );
        out.push_str("</section>\n");
        out.push_str("</main>\n</body>\n</html>\n");
        return out;
    }

    out.push_str("<section class=\"card\">\n");
    out.push_str("  <h2>Column Profiles</h2>\n");
    out.push_str("  <div class=\"table-scroll\">\n");
    out.push_str("    <table>\n      <thead>\n        <tr>\n");
    for heading in [
        "Column",
        "Type",
        "Non-Null",
        "Nulls",
        "Empty",
        "Distinct",
        "Min",
        "Max",
        "Mean",
        "StdDev",
        "p50",
        "p95",
        "Histogram",
        "Top Values",
    ] {
        let _ = writeln!(out, "          <th>{heading}</th>");
    }
    out.push_str("        </tr>\n      </thead>\n      <tbody>\n");

    for col in &analytics.columns {
        out.push_str("        <tr>\n");
        let _ = writeln!(
            out,
            "          <td class=\"col-name\">{}</td>",
            escape_html(&col.name)
        );
        let _ = writeln!(
            out,
            "          <td><span class=\"type-pill\">{}</span></td>",
            escape_html(&col.type_name)
        );
        let _ = writeln!(
            out,
            "          <td class=\"num\">{}</td>",
            col.non_null_count
        );
        let _ = writeln!(
            out,
            "          <td class=\"num\">{} ({:.1}%)</td>",
            col.null_count, col.null_pct
        );
        let _ = writeln!(out, "          <td class=\"num\">{}</td>", col.empty_count);
        let distinct_str = if col.distinct_capped {
            format!("{}+", col.distinct_count)
        } else {
            col.distinct_count.to_string()
        };
        let _ = writeln!(out, "          <td class=\"num\">{distinct_str}</td>");
        let _ = writeln!(
            out,
            "          <td>{}</td>",
            render_html_opt_cell(col.min_value.as_deref())
        );
        let _ = writeln!(
            out,
            "          <td>{}</td>",
            render_html_opt_cell(col.max_value.as_deref())
        );
        let _ = writeln!(
            out,
            "          <td class=\"num stat\">{}</td>",
            format_opt_f64(col.mean)
        );
        let _ = writeln!(
            out,
            "          <td class=\"num stat\">{}</td>",
            format_opt_f64(col.stddev)
        );
        let _ = writeln!(
            out,
            "          <td class=\"num stat\">{}</td>",
            format_opt_f64(col.p50)
        );
        let _ = writeln!(
            out,
            "          <td class=\"num stat\">{}</td>",
            format_opt_f64(col.p95)
        );
        out.push_str("          <td>");
        write_html_sparkline_bars(&mut out, &col.histogram_counts, &col.sparkline_ascii);
        out.push_str("</td>\n");
        out.push_str("          <td class=\"muted\">");
        write_html_top_values(&mut out, &col.top_values);
        out.push_str("</td>\n");
        out.push_str("        </tr>\n");
    }
    out.push_str("      </tbody>\n    </table>\n  </div>\n</section>\n");

    out.push_str("<section class=\"card\">\n");
    let _ = writeln!(out, "  <h2>Result Rows ({})</h2>", set.rows.len());
    out.push_str("  <div class=\"table-scroll data-grid\">\n");
    out.push_str("    <table>\n      <thead>\n        <tr>\n          <th>#</th>\n");
    for col in &set.columns {
        let _ = writeln!(out, "          <th>{}</th>", escape_html(col));
    }
    out.push_str("        </tr>\n      </thead>\n      <tbody>\n");

    if set.rows.is_empty() {
        let span = set.columns.len() + 1;
        let _ = writeln!(
            out,
            "        <tr><td colspan=\"{span}\" class=\"muted\">(0 rows)</td></tr>"
        );
    } else {
        for (row_idx, row) in set.rows.iter().enumerate() {
            out.push_str("        <tr>\n");
            let _ = writeln!(
                out,
                "          <td class=\"num muted\">{}</td>",
                row_idx + 1
            );
            for col_idx in 0..set.columns.len() {
                let cell = row.get(col_idx).unwrap_or(&Cell::Null);
                match cell {
                    Cell::Null => {
                        out.push_str(
                            "          <td class=\"cell-null\"><span class=\"badge badge-null\">[null]</span></td>\n",
                        );
                    }
                    Cell::Text(text) if text.is_empty() => {
                        out.push_str(
                            "          <td class=\"cell-empty\"><span class=\"badge badge-empty\">[empty]</span></td>\n",
                        );
                    }
                    Cell::Text(text) if text == "NULL" => {
                        out.push_str(
                            "          <td class=\"cell-text\"><span class=\"badge badge-literal\">NULL</span></td>\n",
                        );
                    }
                    Cell::Text(text) => {
                        let _ = writeln!(
                            out,
                            "          <td class=\"cell-text\">{}</td>",
                            escape_html(text)
                        );
                    }
                }
            }
            out.push_str("        </tr>\n");
        }
    }

    out.push_str("      </tbody>\n    </table>\n  </div>\n</section>\n");
    out.push_str("</main>\n</body>\n</html>\n");
    out
}

/// Streams a [`ResultSet`] as a standard Apache Parquet binary file into `out`.
///
/// Uses uncompressed `DATA_PAGE` v1 pages with `OPTIONAL` definition levels
/// (`RLE`/bit-packed hybrid framing) and `PLAIN`-encoded `BOOLEAN`, `INT64`,
/// `DOUBLE`, or `BYTE_ARRAY` (`UTF8` / `STRING`) values, followed by a Thrift
/// `TCompactProtocol` `FileMetaData` footer, 4-byte little-endian footer length,
/// and trailing `PAR1` magic.
///
/// An empty (`0`-row) available `ResultSet` writes a valid 0-row Parquet file
/// preserving the column schema. An unavailable `ResultSet` is refused with
/// [`io::ErrorKind::InvalidInput`].
pub fn write_parquet<W: io::Write>(set: &ResultSet, out: &mut W) -> io::Result<()> {
    if !set.is_available() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            RETAINED_DATA_UNAVAILABLE,
        ));
    }

    out.write_all(PARQUET_MAGIC)?;
    let mut offset: u64 = PARQUET_MAGIC.len() as u64;

    let column_specs = infer_parquet_columns(set);
    let num_cols = column_specs.len();
    let has_data = !set.rows.is_empty() && num_cols > 0;
    let num_rows: i64 = if has_data {
        i64::try_from(set.rows.len()).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidInput, "row count exceeds i64::MAX")
        })?
    } else {
        0
    };

    let mut row_groups: Vec<ParquetRowGroupMeta> = Vec::new();

    if has_data {
        let num_values_i32 = i32::try_from(set.rows.len()).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidInput, "row count exceeds i32::MAX")
        })?;
        let rg_start = offset;
        let mut chunk_metas = Vec::with_capacity(num_cols);
        let mut total_rg_bytes: i64 = 0;

        for (col_idx, spec) in column_specs.iter().enumerate() {
            let chunk_start = offset;
            let page_payload = encode_column_page_payload(set, col_idx, spec.physical_type)?;
            let payload_len_i32 = i32::try_from(page_payload.len()).map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidInput, "page size exceeds i32::MAX")
            })?;

            let page_header = encode_data_page_header(num_values_i32, payload_len_i32);
            out.write_all(&page_header)?;
            out.write_all(&page_payload)?;

            let chunk_bytes = page_header.len() + page_payload.len();
            let chunk_bytes_i64 = i64::try_from(chunk_bytes).map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidInput, "chunk size exceeds i64::MAX")
            })?;
            offset += chunk_bytes as u64;
            total_rg_bytes += chunk_bytes_i64;

            chunk_metas.push(ParquetColumnChunkMeta {
                name: spec.name.clone(),
                physical_type: spec.physical_type,
                file_offset: i64::try_from(chunk_start).map_err(|_| {
                    io::Error::new(io::ErrorKind::InvalidInput, "file offset exceeds i64::MAX")
                })?,
                num_values: num_rows,
                total_size: chunk_bytes_i64,
            });
        }

        row_groups.push(ParquetRowGroupMeta {
            columns: chunk_metas,
            total_byte_size: total_rg_bytes,
            num_rows,
            file_offset: i64::try_from(rg_start).map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "row group offset exceeds i64::MAX",
                )
            })?,
        });
    }

    let footer = encode_file_metadata(&column_specs, num_rows, &row_groups)?;
    let footer_len = u32::try_from(footer.len()).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "Parquet footer exceeds u32::MAX",
        )
    })?;

    out.write_all(&footer)?;
    out.write_all(&footer_len.to_le_bytes())?;
    out.write_all(PARQUET_MAGIC)?;
    Ok(())
}

/// Convenience helper that encodes a [`ResultSet`] into an in-memory Apache Parquet byte vector.
pub fn export_parquet_bytes(set: &ResultSet) -> io::Result<Vec<u8>> {
    let mut buf = Vec::new();
    write_parquet(set, &mut buf)?;
    Ok(buf)
}

// ---------------------------------------------------------------------------
// Profiling & formatting helpers
// ---------------------------------------------------------------------------

fn is_non_numeric_pg_type(pg_type: &str) -> bool {
    let trimmed = pg_type.trim();
    if trimmed.ends_with("[]") {
        return true;
    }
    let base = trimmed
        .split('(')
        .next()
        .unwrap_or(trimmed)
        .trim()
        .to_ascii_lowercase();
    matches!(
        base.as_str(),
        "text"
            | "varchar"
            | "character varying"
            | "char"
            | "character"
            | "bpchar"
            | "name"
            | "bool"
            | "boolean"
            | "uuid"
            | "bytea"
            | "json"
            | "jsonb"
            | "xml"
            | "date"
            | "time"
            | "timetz"
            | "time without time zone"
            | "time with time zone"
            | "timestamp"
            | "timestamptz"
            | "timestamp without time zone"
            | "timestamp with time zone"
            | "interval"
            | "inet"
            | "cidr"
            | "macaddr"
            | "macaddr8"
            | "tsvector"
            | "tsquery"
            | "bit"
            | "varbit"
    )
}

fn is_numeric_pg_type(pg_type: &str) -> bool {
    let base = pg_type
        .trim()
        .split('(')
        .next()
        .unwrap_or(pg_type)
        .trim()
        .to_ascii_lowercase();
    matches!(
        base.as_str(),
        "int2"
            | "int4"
            | "int8"
            | "smallint"
            | "integer"
            | "bigint"
            | "smallserial"
            | "serial"
            | "bigserial"
            | "float4"
            | "float8"
            | "real"
            | "double precision"
            | "numeric"
            | "decimal"
            | "oid"
    )
}

fn parse_finite_f64(raw: &str) -> Option<f64> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let val = trimmed.parse::<f64>().ok()?;
    if val.is_finite() {
        Some(normalize_zero(val))
    } else {
        None
    }
}

fn normalize_zero(v: f64) -> f64 {
    if v == 0.0 { 0.0 } else { v }
}

fn percentile_linear(sorted: &[f64], quantile: f64) -> f64 {
    debug_assert!(!sorted.is_empty());
    if sorted.len() == 1 {
        return sorted[0];
    }
    let pos = quantile.clamp(0.0, 1.0) * ((sorted.len() - 1) as f64);
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    let frac = pos - (lo as f64);
    sorted[lo] + frac * (sorted[hi] - sorted[lo])
}

fn build_histogram(values: &[f64]) -> Vec<usize> {
    let mut counts = vec![0usize; HISTOGRAM_BUCKETS];
    if values.is_empty() {
        return counts;
    }
    let mut min_v = f64::INFINITY;
    let mut max_v = f64::NEG_INFINITY;
    for &v in values {
        if v < min_v {
            min_v = v;
        }
        if v > max_v {
            max_v = v;
        }
    }
    let span = max_v - min_v;
    if !span.is_finite() || span <= f64::EPSILON {
        counts[0] = values.len();
        return counts;
    }
    for &v in values {
        let norm = ((v - min_v) / span).clamp(0.0, 1.0);
        let idx = ((norm * (HISTOGRAM_BUCKETS as f64)).floor() as usize).min(HISTOGRAM_BUCKETS - 1);
        counts[idx] += 1;
    }
    counts
}

fn build_sparkline(counts: &[usize], unicode: bool) -> String {
    let ramp = if unicode {
        &UNICODE_SPARKLINE_RAMP
    } else {
        &ASCII_SPARKLINE_RAMP
    };
    let max_count = counts.iter().copied().max().unwrap_or(0);
    let mut spark = String::with_capacity(counts.len() * if unicode { 3 } else { 1 });
    for &count in counts {
        if count == 0 || max_count == 0 {
            spark.push(ramp[0]);
        } else {
            let idx = 1 + ((count * 7) / max_count).min(7);
            spark.push(ramp[idx]);
        }
    }
    spark
}

fn format_opt_f64(value: Option<f64>) -> String {
    match value {
        Some(v) => format!("{:.2}", normalize_zero(v)),
        None => "-".to_owned(),
    }
}

fn format_opt_cell(value: Option<&str>) -> String {
    match value {
        Some("") => "\"\"".to_owned(),
        Some(v) => sanitize_for_display(v),
        None => NULL_MARKER.to_owned(),
    }
}

fn write_padded_row(out: &mut String, cols: &[&str; 12], widths: &[usize; 12]) {
    for (i, (cell, width)) in cols.iter().zip(widths.iter()).enumerate() {
        if i > 0 {
            out.push_str(" | ");
        }
        out.push_str(cell);
        let len = cell.chars().count();
        for _ in len..*width {
            out.push(' ');
        }
    }
    out.push('\n');
}

// ---------------------------------------------------------------------------
// Field Industrial HTML helpers
// ---------------------------------------------------------------------------

const HTML_REPORT_CSS: &str = r#"
:root {
  --bg: #16181E;
  --surface: #1C1F27;
  --card: #232732;
  --border: #3E465B;
  --text: #F4F3EF;
  --muted: #9EA6B8;
  --accent: #FF6A00;
  --amber: #FBBF24;
  --sage: #24C992;
}
* { box-sizing: border-box; }
body {
  margin: 0;
  background: var(--bg);
  color: var(--text);
  font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace;
  font-size: 13px;
  line-height: 1.5;
}
.page {
  max-width: 1400px;
  margin: 0 auto;
  padding: 24px;
}
.report-header {
  background: var(--surface);
  border: 1px solid var(--border);
  border-top: 3px solid var(--accent);
  border-radius: 6px;
  padding: 18px 20px;
  margin-bottom: 18px;
}
.brand-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 12px;
  margin-bottom: 8px;
}
.brand-pill {
  background: var(--accent);
  color: #16181E;
  font-weight: 700;
  padding: 2px 8px;
  border-radius: 3px;
  font-size: 11px;
  letter-spacing: 0.04em;
  text-transform: uppercase;
}
.window-label {
  color: var(--muted);
  font-size: 12px;
}
h1 {
  margin: 0 0 14px 0;
  font-size: 20px;
  color: var(--text);
}
h2 {
  margin: 0 0 12px 0;
  font-size: 14px;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  color: var(--accent);
}
.truncation-banner {
  background: rgba(251, 191, 36, 0.12);
  border: 1px solid var(--amber);
  color: var(--amber);
  padding: 8px 12px;
  border-radius: 4px;
  margin-bottom: 14px;
}
.kpi-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(160px, 1fr));
  gap: 12px;
}
.kpi-card {
  background: var(--card);
  border: 1px solid var(--border);
  border-radius: 4px;
  padding: 10px 12px;
}
.kpi-label {
  color: var(--muted);
  font-size: 11px;
  text-transform: uppercase;
}
.kpi-value {
  font-size: 20px;
  font-weight: 700;
  margin-top: 2px;
}
.kpi-sage { color: var(--sage); }
.kpi-amber { color: var(--amber); }
.kpi-bone { color: var(--text); }
.card {
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: 6px;
  padding: 16px 18px;
  margin-bottom: 18px;
}
.table-scroll {
  overflow: auto;
  border: 1px solid var(--border);
  border-radius: 4px;
  background: var(--card);
}
.data-grid {
  max-height: 70vh;
}
table {
  width: 100%;
  border-collapse: collapse;
  text-align: left;
}
thead th {
  position: sticky;
  top: 0;
  background: var(--surface);
  color: var(--text);
  border-bottom: 2px solid var(--border);
  padding: 8px 10px;
  font-size: 12px;
  white-space: nowrap;
  z-index: 1;
}
tbody td {
  border-bottom: 1px solid var(--border);
  padding: 6px 10px;
  vertical-align: middle;
  white-space: nowrap;
}
tbody tr:hover {
  background: rgba(255, 106, 0, 0.06);
}
.col-name { font-weight: 600; color: var(--text); }
.type-pill {
  background: var(--surface);
  border: 1px solid var(--border);
  color: var(--muted);
  padding: 1px 6px;
  border-radius: 3px;
  font-size: 11px;
}
.num { text-align: right; font-variant-numeric: tabular-nums; }
.stat { color: var(--sage); }
.muted { color: var(--muted); }
.badge {
  display: inline-block;
  padding: 1px 6px;
  border-radius: 3px;
  font-size: 11px;
}
.badge-null {
  background: rgba(251, 191, 36, 0.14);
  border: 1px solid rgba(251, 191, 36, 0.45);
  color: var(--amber);
}
.badge-empty {
  background: rgba(158, 166, 184, 0.12);
  border: 1px dashed var(--border);
  color: var(--muted);
}
.badge-literal {
  background: rgba(36, 201, 146, 0.12);
  border: 1px solid rgba(36, 201, 146, 0.4);
  color: var(--text);
}
.spark-bars {
  display: inline-flex;
  align-items: flex-end;
  gap: 2px;
  height: 18px;
  padding: 2px 4px;
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: 3px;
}
.spark-bar {
  width: 5px;
  background: var(--accent);
  border-radius: 1px 1px 0 0;
}
.spark-bar-zero {
  background: var(--border);
  height: 2px;
}
"#;

fn write_kpi_card(out: &mut String, label: &str, value: &str, tone_class: &str) {
    out.push_str("    <div class=\"kpi-card\">\n");
    let _ = writeln!(out, "      <div class=\"kpi-label\">{label}</div>");
    let _ = writeln!(
        out,
        "      <div class=\"kpi-value {tone_class}\">{}</div>",
        escape_html(value)
    );
    out.push_str("    </div>\n");
}

fn render_html_opt_cell(value: Option<&str>) -> String {
    match value {
        None => "<span class=\"badge badge-null\">[null]</span>".to_owned(),
        Some("") => "<span class=\"badge badge-empty\">[empty]</span>".to_owned(),
        Some(v) => escape_html(v),
    }
}

fn write_html_sparkline_bars(out: &mut String, counts: &[usize], ascii_spark: &str) {
    let max_count = counts.iter().copied().max().unwrap_or(0);
    let _ = write!(
        out,
        "<span class=\"spark-bars\" title=\"sparkline: {}\">",
        escape_html(ascii_spark)
    );
    for &count in counts {
        if count == 0 || max_count == 0 {
            out.push_str("<span class=\"spark-bar spark-bar-zero\"></span>");
        } else {
            let pct = ((count * 100) / max_count).clamp(12, 100);
            let _ = write!(
                out,
                "<span class=\"spark-bar\" style=\"height:{pct}%\"></span>"
            );
        }
    }
    out.push_str("</span>");
}

fn write_html_top_values(out: &mut String, top_values: &[(String, usize)]) {
    if top_values.is_empty() {
        out.push('-');
        return;
    }
    for (idx, (val, count)) in top_values.iter().enumerate() {
        if idx > 0 {
            out.push_str(", ");
        }
        if val.is_empty() {
            let _ = write!(
                out,
                "<span class=\"badge badge-empty\">[empty]</span> ({count})"
            );
        } else {
            let _ = write!(out, "{} ({count})", escape_html(val));
        }
    }
}

// ---------------------------------------------------------------------------
// Pure-Rust Apache Parquet Writer & Thrift TCompactProtocol Encoder
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParquetPhysicalType {
    Boolean = 0,
    Int64 = 2,
    Double = 5,
    ByteArray = 6,
}

#[derive(Debug, Clone)]
struct ParquetColumnSpec {
    name: String,
    physical_type: ParquetPhysicalType,
}

#[derive(Debug, Clone)]
struct ParquetColumnChunkMeta {
    name: String,
    physical_type: ParquetPhysicalType,
    file_offset: i64,
    num_values: i64,
    total_size: i64,
}

#[derive(Debug, Clone)]
struct ParquetRowGroupMeta {
    columns: Vec<ParquetColumnChunkMeta>,
    total_byte_size: i64,
    num_rows: i64,
    file_offset: i64,
}

fn infer_parquet_columns(set: &ResultSet) -> Vec<ParquetColumnSpec> {
    let mut specs = Vec::with_capacity(set.columns.len());
    for (col_idx, name) in set.columns.iter().enumerate() {
        let physical_type = match set
            .column_type(col_idx)
            .map(normalize_pg_type_name)
            .as_deref()
        {
            Some("bool" | "boolean") if all_non_null_match(set, col_idx, parse_pg_bool) => {
                ParquetPhysicalType::Boolean
            }
            Some(
                "int2" | "int4" | "int8" | "smallint" | "integer" | "bigint" | "smallserial"
                | "serial" | "bigserial" | "oid",
            ) if all_non_null_match(set, col_idx, |s| s.trim().parse::<i64>().ok()) => {
                ParquetPhysicalType::Int64
            }
            Some("float4" | "float8" | "real" | "double precision")
                if all_non_null_match(set, col_idx, parse_finite_f64) =>
            {
                ParquetPhysicalType::Double
            }
            _ => ParquetPhysicalType::ByteArray,
        };
        specs.push(ParquetColumnSpec {
            name: name.clone(),
            physical_type,
        });
    }
    specs
}

fn normalize_pg_type_name(pg_type: &str) -> String {
    pg_type
        .trim()
        .split('(')
        .next()
        .unwrap_or(pg_type)
        .trim()
        .to_ascii_lowercase()
}

fn all_non_null_match<T, F>(set: &ResultSet, col_idx: usize, mut parser: F) -> bool
where
    F: FnMut(&str) -> Option<T>,
{
    for row in &set.rows {
        if let Some(Cell::Text(text)) = row.get(col_idx)
            && parser(text).is_none()
        {
            return false;
        }
    }
    true
}

fn parse_pg_bool(raw: &str) -> Option<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "t" | "true" | "1" | "yes" | "y" | "on" => Some(true),
        "f" | "false" | "0" | "no" | "n" | "off" => Some(false),
        _ => None,
    }
}

fn encode_column_page_payload(
    set: &ResultSet,
    col_idx: usize,
    physical_type: ParquetPhysicalType,
) -> io::Result<Vec<u8>> {
    let mut def_levels = Vec::with_capacity(set.rows.len());
    let mut values_buf = Vec::new();
    let mut bool_bits: Vec<bool> = Vec::new();

    for row in &set.rows {
        let cell = row.get(col_idx).unwrap_or(&Cell::Null);
        match cell {
            Cell::Null => def_levels.push(0u8),
            Cell::Text(text) => {
                def_levels.push(1u8);
                match physical_type {
                    ParquetPhysicalType::Boolean => {
                        let b = parse_pg_bool(text).ok_or_else(|| {
                            io::Error::new(io::ErrorKind::InvalidData, "invalid boolean cell")
                        })?;
                        bool_bits.push(b);
                    }
                    ParquetPhysicalType::Int64 => {
                        let v = text.trim().parse::<i64>().map_err(|_| {
                            io::Error::new(io::ErrorKind::InvalidData, "invalid int64 cell")
                        })?;
                        values_buf.extend_from_slice(&v.to_le_bytes());
                    }
                    ParquetPhysicalType::Double => {
                        let v = parse_finite_f64(text).ok_or_else(|| {
                            io::Error::new(io::ErrorKind::InvalidData, "invalid double cell")
                        })?;
                        values_buf.extend_from_slice(&v.to_le_bytes());
                    }
                    ParquetPhysicalType::ByteArray => {
                        let len = u32::try_from(text.len()).map_err(|_| {
                            io::Error::new(
                                io::ErrorKind::InvalidInput,
                                "cell byte length exceeds u32::MAX",
                            )
                        })?;
                        values_buf.extend_from_slice(&len.to_le_bytes());
                        values_buf.extend_from_slice(text.as_bytes());
                    }
                }
            }
        }
    }

    if physical_type == ParquetPhysicalType::Boolean && !bool_bits.is_empty() {
        let byte_len = bool_bits.len().div_ceil(8);
        values_buf.resize(byte_len, 0u8);
        for (idx, bit) in bool_bits.into_iter().enumerate() {
            if bit {
                values_buf[idx / 8] |= 1u8 << (idx % 8);
            }
        }
    }

    let encoded_defs = encode_rle_bit_packed_hybrid_width1(&def_levels);
    let defs_len = u32::try_from(encoded_defs.len()).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "definition level stream exceeds u32::MAX",
        )
    })?;

    let mut payload = Vec::with_capacity(4 + encoded_defs.len() + values_buf.len());
    payload.extend_from_slice(&defs_len.to_le_bytes());
    payload.extend_from_slice(&encoded_defs);
    payload.extend_from_slice(&values_buf);
    Ok(payload)
}

/// Encodes `bit_width = 1` levels using Parquet RLE/Bit-Packed Hybrid encoding
/// (without the 4-byte length prefix, which the caller prepends).
fn encode_rle_bit_packed_hybrid_width1(levels: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    if levels.is_empty() {
        return out;
    }

    let first = levels[0] & 1;
    if levels.iter().all(|&lvl| (lvl & 1) == first) {
        let header = (levels.len() as u64) << 1; // LSB = 0 -> RLE run
        write_uvarint(&mut out, header);
        out.push(first);
        return out;
    }

    // Mixed levels: emit LSB-first bit-packed runs in chunks of <= 1024 groups (8192 values).
    const MAX_CHUNK_VALUES: usize = 1024 * 8;
    for chunk in levels.chunks(MAX_CHUNK_VALUES) {
        let groups = chunk.len().div_ceil(8);
        let header = ((groups as u64) << 1) | 1; // LSB = 1 -> bit-packed run
        write_uvarint(&mut out, header);
        for group_slice in chunk.chunks(8) {
            let mut packed = 0u8;
            for (bit_idx, &lvl) in group_slice.iter().enumerate() {
                packed |= (lvl & 1) << bit_idx;
            }
            out.push(packed);
        }
    }
    out
}

fn encode_data_page_header(num_values: i32, payload_size: i32) -> Vec<u8> {
    const PAGE_TYPE_DATA_PAGE: i32 = 0;
    const ENCODING_PLAIN: i32 = 0;
    const ENCODING_RLE: i32 = 3;

    let mut w = CompactWriter::new();
    w.struct_begin();
    w.write_i32_field(1, PAGE_TYPE_DATA_PAGE);
    w.write_i32_field(2, payload_size); // uncompressed_page_size
    w.write_i32_field(3, payload_size); // compressed_page_size

    w.write_field_begin(5, CompactType::Struct as u8);
    w.struct_begin();
    w.write_i32_field(1, num_values);
    w.write_i32_field(2, ENCODING_PLAIN);
    w.write_i32_field(3, ENCODING_RLE); // definition_level_encoding
    w.write_i32_field(4, ENCODING_RLE); // repetition_level_encoding
    w.struct_end();

    w.struct_end();
    w.into_bytes()
}

fn encode_file_metadata(
    columns: &[ParquetColumnSpec],
    num_rows: i64,
    row_groups: &[ParquetRowGroupMeta],
) -> io::Result<Vec<u8>> {
    const REPETITION_OPTIONAL: i32 = 1;
    const CONVERTED_TYPE_UTF8: i32 = 0;
    const ENCODING_PLAIN: i32 = 0;
    const ENCODING_RLE: i32 = 3;
    const CODEC_UNCOMPRESSED: i32 = 0;

    let mut w = CompactWriter::new();
    w.struct_begin();

    // 1: version
    w.write_i32_field(1, PARQUET_VERSION);

    // 2: schema (root + one leaf SchemaElement per column)
    let schema_len = 1 + columns.len();
    w.write_field_begin(2, CompactType::List as u8);
    w.write_list_begin(CompactType::Struct as u8, schema_len);

    // Root SchemaElement
    w.struct_begin();
    w.write_string_field(4, "ignatius_result");
    let num_children = i32::try_from(columns.len()).map_err(|_| {
        io::Error::new(io::ErrorKind::InvalidInput, "column count exceeds i32::MAX")
    })?;
    w.write_i32_field(5, num_children);
    w.struct_end();

    // Leaf SchemaElements
    for col in columns {
        w.struct_begin();
        w.write_i32_field(1, col.physical_type as i32);
        w.write_i32_field(3, REPETITION_OPTIONAL);
        w.write_string_field(4, &col.name);
        if col.physical_type == ParquetPhysicalType::ByteArray {
            w.write_i32_field(6, CONVERTED_TYPE_UTF8);
            // 10: LogicalType::STRING (union containing field 1: empty StringType struct)
            w.write_field_begin(10, CompactType::Struct as u8);
            w.struct_begin();
            w.write_field_begin(1, CompactType::Struct as u8);
            w.struct_begin();
            w.struct_end();
            w.struct_end();
        }
        w.struct_end();
    }

    // 3: num_rows
    w.write_i64_field(3, num_rows);

    // 4: row_groups
    w.write_field_begin(4, CompactType::List as u8);
    w.write_list_begin(CompactType::Struct as u8, row_groups.len());
    for rg in row_groups {
        w.struct_begin();

        // 1: columns (list<ColumnChunk>)
        w.write_field_begin(1, CompactType::List as u8);
        w.write_list_begin(CompactType::Struct as u8, rg.columns.len());
        for col in &rg.columns {
            w.struct_begin();
            // 2: file_offset
            w.write_i64_field(2, col.file_offset);
            // 3: meta_data (ColumnMetaData)
            w.write_field_begin(3, CompactType::Struct as u8);
            w.struct_begin();
            w.write_i32_field(1, col.physical_type as i32);
            w.write_field_begin(2, CompactType::List as u8);
            w.write_list_begin(CompactType::I32 as u8, 2);
            w.write_i32(ENCODING_PLAIN);
            w.write_i32(ENCODING_RLE);
            w.write_field_begin(3, CompactType::List as u8);
            w.write_list_begin(CompactType::Binary as u8, 1);
            w.write_binary(col.name.as_bytes());
            w.write_i32_field(4, CODEC_UNCOMPRESSED);
            w.write_i64_field(5, col.num_values);
            w.write_i64_field(6, col.total_size); // total_uncompressed_size
            w.write_i64_field(7, col.total_size); // total_compressed_size
            w.write_i64_field(9, col.file_offset); // data_page_offset
            w.struct_end();

            w.struct_end();
        }

        // 2: total_byte_size
        w.write_i64_field(2, rg.total_byte_size);
        // 3: num_rows
        w.write_i64_field(3, rg.num_rows);
        // 5: file_offset
        w.write_i64_field(5, rg.file_offset);
        // 6: total_compressed_size
        w.write_i64_field(6, rg.total_byte_size);

        w.struct_end();
    }

    // 6: created_by
    let created_by = format!("{PRODUCT_NAME} {VERSION}");
    w.write_string_field(6, &created_by);

    w.struct_end();
    Ok(w.into_bytes())
}

// ---------------------------------------------------------------------------
// Minimal Thrift TCompactProtocol encoder
// ---------------------------------------------------------------------------

#[repr(u8)]
#[derive(Debug, Clone, Copy)]
enum CompactType {
    I32 = 5,
    I64 = 6,
    Binary = 8,
    List = 9,
    Struct = 12,
}

#[derive(Debug)]
struct CompactWriter {
    buf: Vec<u8>,
    field_stack: Vec<i16>,
}

impl CompactWriter {
    fn new() -> Self {
        Self {
            buf: Vec::new(),
            field_stack: Vec::new(),
        }
    }

    fn into_bytes(self) -> Vec<u8> {
        self.buf
    }

    fn struct_begin(&mut self) {
        self.field_stack.push(0);
    }

    fn struct_end(&mut self) {
        self.buf.push(0x00); // TType::STOP
        self.field_stack.pop();
    }

    fn write_field_begin(&mut self, field_id: i16, compact_type: u8) {
        let last = *self.field_stack.last().unwrap_or(&0);
        let delta = field_id - last;
        if delta > 0 && delta <= 15 {
            self.buf.push(((delta as u8) << 4) | (compact_type & 0x0f));
        } else {
            self.buf.push(compact_type & 0x0f);
            self.write_i32(i32::from(field_id));
        }
        if let Some(top) = self.field_stack.last_mut() {
            *top = field_id;
        }
    }

    fn write_list_begin(&mut self, elem_type: u8, len: usize) {
        if len <= 14 {
            self.buf.push(((len as u8) << 4) | (elem_type & 0x0f));
        } else {
            self.buf.push(0xf0 | (elem_type & 0x0f));
            write_uvarint(&mut self.buf, len as u64);
        }
    }

    fn write_i32_field(&mut self, field_id: i16, value: i32) {
        self.write_field_begin(field_id, CompactType::I32 as u8);
        self.write_i32(value);
    }

    fn write_i64_field(&mut self, field_id: i16, value: i64) {
        self.write_field_begin(field_id, CompactType::I64 as u8);
        self.write_i64(value);
    }

    fn write_string_field(&mut self, field_id: i16, value: &str) {
        self.write_field_begin(field_id, CompactType::Binary as u8);
        self.write_binary(value.as_bytes());
    }

    fn write_i32(&mut self, value: i32) {
        let zigzag = ((value << 1) ^ (value >> 31)) as u32;
        write_uvarint(&mut self.buf, u64::from(zigzag));
    }

    fn write_i64(&mut self, value: i64) {
        let zigzag = ((value << 1) ^ (value >> 63)) as u64;
        write_uvarint(&mut self.buf, zigzag);
    }

    fn write_binary(&mut self, bytes: &[u8]) {
        write_uvarint(&mut self.buf, bytes.len() as u64);
        self.buf.extend_from_slice(bytes);
    }
}

fn write_uvarint(buf: &mut Vec<u8>, mut value: u64) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        buf.push(byte);
        if value == 0 {
            break;
        }
    }
}
