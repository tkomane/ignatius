//! Pure query performance forensics and stat statements domain model.
//!
//! This module implements the domain logic for Feature
//! `034-query-performance-and-stat-statements` (`Milestone 7: Enterprise Observability & Query Performance Forensics`).
//! It depends on no terminal, filesystem, or network types: [`QueryPerformanceSnapshot`]
//! is populated from live `pg_stat_statements` queries ([`crate::postgres::metadata::query_performance`]),
//! bounded at [`MAX_PERFORMANCE_QUERIES`], and rendered deterministically into an
//! aligned tabular overview, a machine-readable JSON document, or plain text.

use serde::{Deserialize, Serialize};
use std::fmt::Write as _;

/// Maximum number of statements retained in a single performance snapshot.
pub const MAX_PERFORMANCE_QUERIES: usize = 200;

/// Default limit of queries returned when not explicitly overridden.
pub const DEFAULT_PERFORMANCE_LIMIT: usize = 20;

/// Sort order for query performance profiling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PerformanceSortOrder {
    /// Sort by mean execution time descending (highest average latency first).
    #[default]
    MeanTime,
    /// Sort by total execution time descending (highest cumulative workload first).
    TotalTime,
    /// Sort by call count descending (most frequently invoked queries first).
    Calls,
    /// Sort by buffer cache hit ratio ascending (queries with the worst cache locality first).
    HitRatio,
    /// Sort by shared blocks read descending (queries causing the highest disk/OS cache reads).
    IoRead,
    /// Sort by total rows processed/returned descending.
    Rows,
}

impl PerformanceSortOrder {
    /// Short human-readable label for overlay chrome.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::MeanTime => "Mean Time",
            Self::TotalTime => "Total Time",
            Self::Calls => "Call Count",
            Self::HitRatio => "Cache Hit %",
            Self::IoRead => "Disk I/O Read",
            Self::Rows => "Rows Processed",
        }
    }

    /// Display name alias.
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        self.label()
    }

    /// Accepted strings for CLI parsing.
    pub const ACCEPTED: &'static [&'static str] =
        &["mean", "total", "calls", "hit-ratio", "io", "rows"];

    /// Resolves from user input string.
    #[must_use]
    pub fn from_str_loose(s: &str) -> Option<Self> {
        let trimmed = s.trim().to_ascii_lowercase();
        match trimmed.as_str() {
            "mean" | "mean-time" | "avg" | "latency" => Some(Self::MeanTime),
            "total" | "total-time" | "cum" | "cumulative" => Some(Self::TotalTime),
            "calls" | "call" | "count" | "freq" | "frequency" => Some(Self::Calls),
            "hit" | "hit-ratio" | "cache" | "cache-hit" => Some(Self::HitRatio),
            "io" | "io-read" | "read" | "disk" | "blocks" => Some(Self::IoRead),
            "rows" | "row" => Some(Self::Rows),
            _ => None,
        }
    }
}

/// Output format for performance reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PerformanceOutputFormat {
    /// Formatted terminal table.
    #[default]
    Table,
    /// Structured JSON snapshot.
    Json,
    /// Unadorned plain text.
    Plain,
}

/// One query entry from `pg_stat_statements`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatStatementEntry {
    /// Internal PostgreSQL query identifier hash.
    pub query_id: i64,
    /// Normalized query string with parameters masked.
    pub query: String,
    /// Number of times the statement has been executed.
    pub calls: u64,
    /// Cumulative execution time in milliseconds.
    pub total_exec_time_ms: f64,
    /// Minimum recorded execution time in milliseconds.
    pub min_exec_time_ms: f64,
    /// Maximum recorded execution time in milliseconds.
    pub max_exec_time_ms: f64,
    /// Mean execution time in milliseconds.
    pub mean_exec_time_ms: f64,
    /// Standard deviation of execution time in milliseconds.
    pub stddev_exec_time_ms: f64,
    /// Total number of rows retrieved or affected.
    pub rows: u64,
    /// Shared buffer blocks found in PostgreSQL shared memory cache.
    pub shared_blks_hit: u64,
    /// Shared buffer blocks read from disk or kernel page cache.
    pub shared_blks_read: u64,
    /// Shared buffer blocks dirtied during execution.
    pub shared_blks_dirtied: u64,
    /// Shared buffer blocks written to disk.
    pub shared_blks_written: u64,
}

impl StatStatementEntry {
    /// Calculates the buffer cache hit ratio as a percentage from 0.0 to 100.0.
    ///
    /// If no buffer blocks were accessed (both hit and read are 0), returns 100.0%.
    #[must_use]
    pub fn hit_ratio(&self) -> f64 {
        let total = self.shared_blks_hit + self.shared_blks_read;
        if total == 0 {
            100.0
        } else {
            (self.shared_blks_hit as f64 / total as f64) * 100.0
        }
    }

    /// Formats mean execution time with unit (ms or s).
    #[must_use]
    pub fn format_mean_time(&self) -> String {
        if self.mean_exec_time_ms >= 1000.0 {
            format!("{:.2}s", self.mean_exec_time_ms / 1000.0)
        } else {
            format!("{:.2}ms", self.mean_exec_time_ms)
        }
    }

    /// Formats total execution time with unit (ms, s, min, or h).
    #[must_use]
    pub fn format_total_time(&self) -> String {
        let secs = self.total_exec_time_ms / 1000.0;
        if secs >= 3600.0 {
            format!("{:.1}h", secs / 3600.0)
        } else if secs >= 60.0 {
            format!("{:.1}m", secs / 60.0)
        } else if secs >= 1.0 {
            format!("{:.2}s", secs)
        } else {
            format!("{:.1}ms", self.total_exec_time_ms)
        }
    }

    /// Formats average rows affected per call.
    #[must_use]
    pub fn avg_rows_per_call(&self) -> f64 {
        if self.calls == 0 {
            0.0
        } else {
            self.rows as f64 / self.calls as f64
        }
    }

    /// Returns a single-line sanitized snippet of the query for tabular display.
    #[must_use]
    pub fn query_snippet(&self, max_chars: usize) -> String {
        format_query_snippet(&self.query, max_chars)
    }
}

/// Formats a millisecond duration into a clean human-readable string.
#[must_use]
pub fn format_duration_ms(ms: f64) -> String {
    let secs = ms / 1000.0;
    if secs >= 3600.0 {
        format!("{:.1}h", secs / 3600.0)
    } else if secs >= 60.0 {
        format!("{:.1}m", secs / 60.0)
    } else if secs >= 1.0 {
        format!("{:.2}s", secs)
    } else {
        format!("{:.1}ms", ms)
    }
}

/// Formats a query snippet compressing whitespace.
#[must_use]
pub fn format_query_snippet(query: &str, max_chars: usize) -> String {
    let single_line = query.split_whitespace().collect::<Vec<_>>().join(" ");
    if single_line.chars().count() > max_chars {
        let truncated: String = single_line
            .chars()
            .take(max_chars.saturating_sub(3))
            .collect();
        format!("{truncated}...")
    } else {
        single_line
    }
}

/// A complete query performance forensics snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QueryPerformanceSnapshot {
    /// Whether the `pg_stat_statements` extension is installed and available.
    pub extension_available: bool,
    /// Detailed reason or installation hint if `extension_available` is false.
    pub missing_reason: Option<String>,
    /// Tracked statement entries, sorted by [`Self::sort_order`].
    pub queries: Vec<StatStatementEntry>,
    /// Total calls recorded across all tracked queries in this snapshot.
    pub total_calls: u64,
    /// Total cumulative runtime across all tracked queries in milliseconds.
    pub total_time_ms: f64,
    /// Global cache hit ratio across all tracked queries (0.0 to 100.0).
    pub global_hit_ratio: f64,
    /// Active sort order used to produce this snapshot.
    pub sort_order: PerformanceSortOrder,
}

impl QueryPerformanceSnapshot {
    /// Constructs a snapshot with calculated aggregate telemetry.
    #[must_use]
    pub fn new(
        mut queries: Vec<StatStatementEntry>,
        extension_available: bool,
        missing_reason: Option<String>,
        sort_order: PerformanceSortOrder,
    ) -> Self {
        queries.truncate(MAX_PERFORMANCE_QUERIES);
        sort_entries(&mut queries, sort_order);

        let mut total_calls: u64 = 0;
        let mut total_time_ms: f64 = 0.0;
        let mut total_hits: u64 = 0;
        let mut total_reads: u64 = 0;

        for q in &queries {
            total_calls = total_calls.saturating_add(q.calls);
            total_time_ms += q.total_exec_time_ms;
            total_hits = total_hits.saturating_add(q.shared_blks_hit);
            total_reads = total_reads.saturating_add(q.shared_blks_read);
        }

        let total_blocks = total_hits.saturating_add(total_reads);
        let global_hit_ratio = if total_blocks == 0 {
            100.0
        } else {
            (total_hits as f64 / total_blocks as f64) * 100.0
        };

        Self {
            extension_available,
            missing_reason,
            queries,
            total_calls,
            total_time_ms,
            global_hit_ratio,
            sort_order,
        }
    }

    /// Constructs an empty snapshot indicating the extension is missing.
    #[must_use]
    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self {
            extension_available: false,
            missing_reason: Some(reason.into()),
            queries: Vec::new(),
            total_calls: 0,
            total_time_ms: 0.0,
            global_hit_ratio: 100.0,
            sort_order: PerformanceSortOrder::default(),
        }
    }

    /// Reranks existing queries under a new sort order in memory.
    pub fn reorder(&mut self, new_sort: PerformanceSortOrder) {
        self.sort_order = new_sort;
        sort_entries(&mut self.queries, new_sort);
    }

    /// Builds the SQL command to reset execution statistics.
    #[must_use]
    pub const fn build_reset_sql() -> &'static str {
        "SELECT pg_stat_statements_reset();"
    }

    /// Renders the performance snapshot as an aligned terminal table.
    #[must_use]
    pub fn render_table(&self) -> String {
        if !self.extension_available {
            let reason = self
                .missing_reason
                .as_deref()
                .unwrap_or("pg_stat_statements is not installed");
            return format!(
                "Query performance statistics are unavailable:\n\
                 {reason}\n\n\
                 To enable pg_stat_statements:\n\
                 1. Add 'pg_stat_statements' to shared_preload_libraries in postgresql.conf\n\
                 2. Restart PostgreSQL\n\
                 3. Run: CREATE EXTENSION pg_stat_statements;\n"
            );
        }

        if self.queries.is_empty() {
            return "No recorded queries in pg_stat_statements.\n".to_owned();
        }

        let mut out = String::new();
        let _ = writeln!(
            out,
            "STATISTICS SUMMARY: {} queries tracked | total runtime: {:.2}s | total calls: {} | global cache hit: {:.1}%\n\
             SORT ORDER: {}\n",
            self.queries.len(),
            self.total_time_ms / 1000.0,
            self.total_calls,
            self.global_hit_ratio,
            self.sort_order.label()
        );

        let _ = writeln!(
            out,
            "{:<4} {:>10} {:>8} {:>10} {:>8} {:>10} QUERY SNIPPET",
            "RANK", "MEAN", "CALLS", "TOTAL", "HIT %", "READ BLKS"
        );
        let _ = writeln!(
            out,
            "{:<4} {:>10} {:>8} {:>10} {:>8} {:>10} ----------------------------------------",
            "----", "----------", "--------", "----------", "--------", "----------"
        );

        for (idx, q) in self.queries.iter().enumerate() {
            let _ = writeln!(
                out,
                "{:<4} {:>10} {:>8} {:>10} {:>7.1}% {:>10} {}",
                idx + 1,
                q.format_mean_time(),
                q.calls,
                q.format_total_time(),
                q.hit_ratio(),
                q.shared_blks_read,
                q.query_snippet(60)
            );
        }

        out
    }

    /// Serializes snapshot as structured JSON.
    pub fn render_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Renders unadorned plain text.
    #[must_use]
    pub fn render_plain(&self) -> String {
        if !self.extension_available {
            return self
                .missing_reason
                .clone()
                .unwrap_or_else(|| "pg_stat_statements unavailable".to_owned());
        }

        let mut out = String::new();
        for (idx, q) in self.queries.iter().enumerate() {
            let _ = writeln!(
                out,
                "{}\t{}\t{}\t{}\t{:.1}%\t{}\t{}",
                idx + 1,
                q.format_mean_time(),
                q.calls,
                q.format_total_time(),
                q.hit_ratio(),
                q.shared_blks_read,
                q.query_snippet(80)
            );
        }
        out
    }
}

fn sort_entries(queries: &mut [StatStatementEntry], sort: PerformanceSortOrder) {
    queries.sort_by(|a, b| match sort {
        PerformanceSortOrder::MeanTime => b
            .mean_exec_time_ms
            .partial_cmp(&a.mean_exec_time_ms)
            .unwrap_or(std::cmp::Ordering::Equal),
        PerformanceSortOrder::TotalTime => b
            .total_exec_time_ms
            .partial_cmp(&a.total_exec_time_ms)
            .unwrap_or(std::cmp::Ordering::Equal),
        PerformanceSortOrder::Calls => b.calls.cmp(&a.calls),
        PerformanceSortOrder::HitRatio => a
            .hit_ratio()
            .partial_cmp(&b.hit_ratio())
            .unwrap_or(std::cmp::Ordering::Equal),
        PerformanceSortOrder::IoRead => b.shared_blks_read.cmp(&a.shared_blks_read),
        PerformanceSortOrder::Rows => b.rows.cmp(&a.rows),
    });
}

#[cfg(test)]
mod tests;
