//! Pure index advisor, bloat forensics, and hypothetical indexing domain model.
//!
//! This module implements the domain logic for Feature
//! `035-index-advisor-and-bloat-forensics` (`Milestone 7: Enterprise Observability & Query Performance Forensics`).
//! It contains zero terminal, filesystem, or network dependencies: [`AdvisorReport`]
//! is populated from catalog extractors in [`crate::postgres::metadata`], bounded and
//! rendered deterministically into an aligned tabular overview, structured JSON, or plain text.

use serde::{Deserialize, Serialize};
use std::fmt::Write as _;

#[cfg(test)]
mod tests;

/// Reason an index was flagged as redundant or sub-optimal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RedundantReason {
    /// Two or more indexes with identical column list and indexing method.
    ExactDuplicate,
    /// Single-column or shorter leading prefix covered by a wider composite index.
    PrefixCovered,
    /// Index has recorded zero scans on a table larger than the minimum threshold.
    Unused,
}

impl RedundantReason {
    /// Human-readable label.
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::ExactDuplicate => "EXACT DUPLICATE",
            Self::PrefixCovered => "PREFIX COVERED",
            Self::Unused => "UNUSED",
        }
    }
}

/// A redundant or unused index discovered in the catalog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedundantIndex {
    /// Schema name.
    pub schema: String,
    /// Table name.
    pub table: String,
    /// Redundant index name.
    pub index_name: String,
    /// Covering index name if prefix-covered or duplicate.
    pub covering_index: Option<String>,
    /// Categorized redundancy reason.
    pub reason: RedundantReason,
    /// Indexed columns.
    pub columns: Vec<String>,
    /// Index size in bytes on disk.
    pub size_bytes: u64,
    /// Total index scans recorded in `pg_stat_user_indexes`.
    pub scans: u64,
}

impl RedundantIndex {
    /// Generates safe, non-blocking cleanup SQL.
    #[must_use]
    pub fn suggested_drop_sql(&self) -> String {
        format!(
            "DROP INDEX CONCURRENTLY IF EXISTS \"{}\".\"{}\";",
            self.schema, self.index_name
        )
    }
}

/// Index page bloat estimation metric.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IndexBloatMetric {
    /// Schema name.
    pub schema: String,
    /// Table name.
    pub table: String,
    /// Index name.
    pub index_name: String,
    /// Total allocated disk size in bytes.
    pub total_bytes: u64,
    /// Estimated dead tuple / fragmented page wasted bytes.
    pub wasted_bytes: u64,
    /// Bloat percentage (0.0 to 100.0).
    pub bloat_pct: f64,
}

impl IndexBloatMetric {
    /// Generates non-blocking reindex SQL.
    #[must_use]
    pub fn suggested_reindex_sql(&self) -> String {
        format!(
            "REINDEX INDEX CONCURRENTLY \"{}\".\"{}\";",
            self.schema, self.index_name
        )
    }
}

/// A foreign key constraint that lacks a supporting index on child table columns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnindexedForeignKey {
    /// Foreign key constraint name.
    pub constraint_name: String,
    /// Schema name.
    pub schema: String,
    /// Child table containing the foreign key.
    pub child_table: String,
    /// Foreign key columns in the child table.
    pub columns: Vec<String>,
    /// Referenced parent table.
    pub parent_table: String,
}

impl UnindexedForeignKey {
    /// Generates a canonical index name for the foreign key.
    #[must_use]
    pub fn suggested_index_name(&self) -> String {
        format!("idx_{}_{}", self.child_table, self.columns.join("_"))
    }

    /// Generates non-blocking creation SQL.
    #[must_use]
    pub fn suggested_create_sql(&self) -> String {
        let cols = self
            .columns
            .iter()
            .map(|c| format!("\"{}\"", c))
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "CREATE INDEX CONCURRENTLY IF NOT EXISTS \"{}\" ON \"{}\".\"{}\" ({});",
            self.suggested_index_name(),
            self.schema,
            self.child_table,
            cols
        )
    }
}

/// Result of evaluating a hypothetical index with HypoPG.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HypoIndexTestResult {
    /// Proposed `CREATE INDEX ...` statement.
    pub proposed_index_sql: String,
    /// Evaluated query statement.
    pub query_text: String,
    /// Whether HypoPG was available on the server.
    pub hypo_available: bool,
    /// Original query planner cost without the hypothetical index.
    pub original_cost: f64,
    /// Query planner cost with the hypothetical index active.
    pub hypo_cost: f64,
    /// Cost reduction percentage (0.0 to 100.0).
    pub cost_reduction_pct: f64,
    /// Whether the query plan node chose to use the hypothetical index.
    pub used_in_plan: bool,
    /// Explanatory guidance or installation advice.
    pub notice: Option<String>,
}

/// Complete index advisor report snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct AdvisorReport {
    /// Discovered duplicate, prefix-covered, or unused indexes.
    pub redundant_indexes: Vec<RedundantIndex>,
    /// Indexes with estimated B-Tree page bloat.
    pub bloated_indexes: Vec<IndexBloatMetric>,
    /// Foreign keys lacking a supporting index on child columns.
    pub unindexed_foreign_keys: Vec<UnindexedForeignKey>,
    /// Hypothetical index test results.
    pub hypo_tests: Vec<HypoIndexTestResult>,
    /// Whether HypoPG extension is installed in the target database.
    pub hypopg_installed: bool,
}

impl AdvisorReport {
    /// Calculates overall index health score from 0% (poor) to 100% (flawless).
    #[must_use]
    pub fn health_score(&self) -> u8 {
        let issues = self.redundant_indexes.len() * 3
            + self.unindexed_foreign_keys.len() * 5
            + self
                .bloated_indexes
                .iter()
                .filter(|b| b.bloat_pct >= 30.0)
                .count()
                * 4;
        100u8.saturating_sub(issues as u8).max(10)
    }

    /// Total reclaimable disk space in bytes (redundant index sizes + wasted bloat).
    #[must_use]
    pub fn total_reclaimable_bytes(&self) -> u64 {
        let redundant: u64 = self.redundant_indexes.iter().map(|r| r.size_bytes).sum();
        let bloat: u64 = self.bloated_indexes.iter().map(|b| b.wasted_bytes).sum();
        redundant.saturating_add(bloat)
    }

    /// Formats total reclaimable bytes as a human-readable string (e.g. `14.2 MB`).
    #[must_use]
    pub fn format_reclaimable(&self) -> String {
        format_bytes(self.total_reclaimable_bytes())
    }

    /// Renders the advisor findings into an aligned terminal table.
    #[must_use]
    pub fn render_table(&self, section: AdvisorSection) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "=== Ignatius Index Advisor & Bloat Forensics ===");
        let _ = writeln!(
            out,
            "Health Score: {}% | Reclaimable: {} | Redundant: {} | Bloated: {} | Unindexed FKs: {}",
            self.health_score(),
            self.format_reclaimable(),
            self.redundant_indexes.len(),
            self.bloated_indexes.len(),
            self.unindexed_foreign_keys.len()
        );
        let _ = writeln!(
            out,
            "HypoPG Status: {}",
            if self.hypopg_installed {
                "ENABLED"
            } else {
                "NOT INSTALLED (run 'CREATE EXTENSION hypopg;')"
            }
        );
        let _ = writeln!(out);

        if matches!(section, AdvisorSection::All | AdvisorSection::Redundant) {
            let _ = writeln!(out, "-- Redundant & Unused Indexes --");
            if self.redundant_indexes.is_empty() {
                let _ = writeln!(out, "No redundant or duplicate indexes detected.");
            } else {
                let _ = writeln!(
                    out,
                    "{:<24} {:<24} {:<16} {:>10} {:>8} {:<24}",
                    "TABLE", "INDEX", "REASON", "SIZE", "SCANS", "COVERED BY"
                );
                let _ = writeln!(out, "{}", "-".repeat(110));
                for r in &self.redundant_indexes {
                    let table_qual = format!("{}.{}", r.schema, r.table);
                    let covered = r.covering_index.as_deref().unwrap_or("-");
                    let _ = writeln!(
                        out,
                        "{:<24} {:<24} {:<16} {:>10} {:>8} {:<24}",
                        truncate_str(&table_qual, 24),
                        truncate_str(&r.index_name, 24),
                        r.reason.label(),
                        format_bytes(r.size_bytes),
                        r.scans,
                        truncate_str(covered, 24)
                    );
                }
            }
            let _ = writeln!(out);
        }

        if matches!(section, AdvisorSection::All | AdvisorSection::Bloat) {
            let _ = writeln!(out, "-- B-Tree Index Bloat Forensics --");
            if self.bloated_indexes.is_empty() {
                let _ = writeln!(out, "No bloated indexes exceeding threshold detected.");
            } else {
                let _ = writeln!(
                    out,
                    "{:<24} {:<24} {:>10} {:>10} {:>8} {:<32}",
                    "TABLE", "INDEX", "TOTAL", "WASTED", "BLOAT %", "REMEDIATION"
                );
                let _ = writeln!(out, "{}", "-".repeat(112));
                for b in &self.bloated_indexes {
                    let table_qual = format!("{}.{}", b.schema, b.table);
                    let _ = writeln!(
                        out,
                        "{:<24} {:<24} {:>10} {:>10} {:>7.1}% {:<32}",
                        truncate_str(&table_qual, 24),
                        truncate_str(&b.index_name, 24),
                        format_bytes(b.total_bytes),
                        format_bytes(b.wasted_bytes),
                        b.bloat_pct,
                        b.suggested_reindex_sql()
                    );
                }
            }
            let _ = writeln!(out);
        }

        if matches!(section, AdvisorSection::All | AdvisorSection::Unindexed) {
            let _ = writeln!(out, "-- Unindexed Foreign Keys --");
            if self.unindexed_foreign_keys.is_empty() {
                let _ = writeln!(out, "All foreign keys have supporting child indexes.");
            } else {
                let _ = writeln!(
                    out,
                    "{:<28} {:<24} {:<20} {:<24}",
                    "CONSTRAINT", "CHILD TABLE", "COLUMNS", "REFERENCES"
                );
                let _ = writeln!(out, "{}", "-".repeat(100));
                for fk in &self.unindexed_foreign_keys {
                    let _ = writeln!(
                        out,
                        "{:<28} {:<24} {:<20} {:<24}",
                        truncate_str(&fk.constraint_name, 28),
                        truncate_str(&format!("{}.{}", fk.schema, fk.child_table), 24),
                        truncate_str(&fk.columns.join(", "), 20),
                        truncate_str(&fk.parent_table, 24)
                    );
                }
            }
            let _ = writeln!(out);
        }

        if matches!(section, AdvisorSection::All | AdvisorSection::Hypo)
            && !self.hypo_tests.is_empty()
        {
            let _ = writeln!(out, "-- Hypothetical Index Results (HypoPG) --");
            for t in &self.hypo_tests {
                let _ = writeln!(
                    out,
                    "Proposed: {}\nOriginal Cost: {:.2} -> Hypo Cost: {:.2} (Savings: {:.1}% | Used: {})\n",
                    t.proposed_index_sql,
                    t.original_cost,
                    t.hypo_cost,
                    t.cost_reduction_pct,
                    if t.used_in_plan { "YES" } else { "NO" }
                );
            }
        }

        out
    }

    /// Renders the snapshot as formatted JSON.
    #[must_use]
    pub fn render_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string())
    }

    /// Renders plain text output suitable for screen readers and simple pipes.
    #[must_use]
    pub fn render_plain(&self, section: AdvisorSection) -> String {
        self.render_table(section)
    }
}

/// Filter section for advisor reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AdvisorSection {
    /// Include all diagnostics (redundant, bloat, unindexed, hypo).
    #[default]
    All,
    /// Redundant and unused indexes.
    Redundant,
    /// B-Tree index bloat forensics.
    Bloat,
    /// Missing foreign key indexes.
    Unindexed,
    /// Hypothetical index test results.
    Hypo,
}

impl AdvisorSection {
    /// Accepted CLI strings.
    pub const ACCEPTED: &'static [&'static str] =
        &["all", "redundant", "bloat", "unindexed", "hypo"];

    /// Resolves loosely from user input.
    #[must_use]
    pub fn from_str_loose(s: &str) -> Option<Self> {
        let trimmed = s.trim().to_ascii_lowercase();
        match trimmed.as_str() {
            "all" => Some(Self::All),
            "redundant" | "duplicates" | "dup" => Some(Self::Redundant),
            "bloat" | "wasted" => Some(Self::Bloat),
            "unindexed" | "fk" | "foreign-keys" => Some(Self::Unindexed),
            "hypo" | "hypopg" => Some(Self::Hypo),
            _ => None,
        }
    }
}

/// Output format for advisor reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AdvisorFormat {
    /// Aligned table overview.
    #[default]
    Table,
    /// Machine-readable JSON.
    Json,
    /// Linear plain text.
    Plain,
}

/// Helper to format raw bytes into human readable binary units.
#[must_use]
pub fn format_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;

    let b = bytes as f64;
    if b >= GIB {
        format!("{:.1} GB", b / GIB)
    } else if b >= MIB {
        format!("{:.1} MB", b / MIB)
    } else if b >= KIB {
        format!("{:.1} KB", b / KIB)
    } else {
        format!("{} B", bytes)
    }
}

/// Truncates string with ellipsis if exceeding max length.
pub(crate) fn truncate_str(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_len.saturating_sub(3)).collect();
        format!("{}...", truncated)
    }
}
