//! Pure retention and growth audit model for Feature `049-retention-and-growth-audit`.
//!
//! The model is populated from catalog and statistics views by
//! [`crate::postgres::growth`]. It reads no table rows, changes no statistics,
//! and depends on no driver, terminal, filesystem or process types. Every
//! number carries the window or estimate it came from, so the report never
//! presents a planner estimate as a measurement.

#[cfg(test)]
mod tests;

use std::fmt::Write as _;

use serde::Serialize;

/// Maximum number of relations retained in one audit.
pub const MAX_RELATIONS: usize = 200;

/// Maximum number of sequences retained in one audit.
pub const MAX_SEQUENCES: usize = 200;

/// Default number of relations shown in the size ranking.
pub const DEFAULT_TOP: usize = 20;

/// Default size at which an unpartitioned relation is a partitioning candidate.
pub const DEFAULT_LARGE_TABLE_BYTES: u64 = 1024 * 1024 * 1024;

/// Severity of a retention or growth finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum GrowthSeverity {
    /// Context that is useful but not actionable on its own.
    Info,
    /// A risk that should be reviewed before it becomes an incident.
    Warning,
    /// A risk that is already close to causing failed writes or lost space.
    Critical,
}

impl GrowthSeverity {
    /// Short uppercase label used in tables and findings.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Info => "INFO",
            Self::Warning => "WARNING",
            Self::Critical => "CRITICAL",
        }
    }

    /// Numeric rank, higher is more severe.
    #[must_use]
    pub const fn rank(self) -> u8 {
        match self {
            Self::Info => 0,
            Self::Warning => 1,
            Self::Critical => 2,
        }
    }
}

/// Stable finding code for automation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrowthFindingCode {
    /// A non-cycling sequence is close to its maximum or minimum value.
    SequenceExhaustion,
    /// A relation carries a large dead tuple count because vacuum is behind.
    VacuumDebt,
    /// An unpartitioned relation is above the configured size threshold.
    PartitioningCandidate,
}

impl GrowthFindingCode {
    /// Every finding code, in stable order, for documentation parity checks.
    pub const ALL: [Self; 3] = [
        Self::SequenceExhaustion,
        Self::VacuumDebt,
        Self::PartitioningCandidate,
    ];

    /// Stable kebab-case code.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SequenceExhaustion => "sequence-exhaustion",
            Self::VacuumDebt => "vacuum-debt",
            Self::PartitioningCandidate => "partitioning-candidate",
        }
    }

    /// Fixed severity for the code.
    #[must_use]
    pub const fn severity(self) -> GrowthSeverity {
        match self {
            Self::SequenceExhaustion => GrowthSeverity::Critical,
            Self::VacuumDebt | Self::PartitioningCandidate => GrowthSeverity::Warning,
        }
    }
}

/// How the caller wants findings to affect the process exit code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GrowthFailOn {
    /// Always exit 0 after a successful audit.
    #[default]
    Never,
    /// Exit 1 when any warning or critical finding is present.
    Warning,
    /// Exit 1 only when a critical finding is present.
    Critical,
}

impl GrowthFailOn {
    /// Lowest severity that fails the run, or `None` for [`Self::Never`].
    #[must_use]
    pub const fn threshold(self) -> Option<GrowthSeverity> {
        match self {
            Self::Never => None,
            Self::Warning => Some(GrowthSeverity::Warning),
            Self::Critical => Some(GrowthSeverity::Critical),
        }
    }
}

/// Options controlling selection and finding policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrowthOptions {
    /// Restrict every section to one user schema.
    pub schema: Option<String>,
    /// Number of relations shown in the size ranking.
    pub top: usize,
    /// Size at which an unpartitioned relation becomes a partitioning candidate.
    pub large_table_bytes: u64,
    /// Finding threshold that fails the run.
    pub fail_on: GrowthFailOn,
}

impl Default for GrowthOptions {
    fn default() -> Self {
        Self {
            schema: None,
            top: DEFAULT_TOP,
            large_table_bytes: DEFAULT_LARGE_TABLE_BYTES,
            fail_on: GrowthFailOn::Never,
        }
    }
}

impl GrowthOptions {
    /// Clamps the display count and the large-table threshold into their bounds.
    #[must_use]
    pub fn bounded(mut self) -> Self {
        self.top = self.top.clamp(1, MAX_RELATIONS);
        self.large_table_bytes = self.large_table_bytes.max(1);
        self
    }
}

/// One relation's size, statistics and growth window.
#[derive(Debug, Clone, PartialEq)]
pub struct RelationGrowth {
    /// Schema name.
    pub schema: String,
    /// Relation name.
    pub table: String,
    /// Whether the relation is a partitioned parent.
    pub partitioned: bool,
    /// Total allocated bytes including indexes and TOAST.
    pub total_bytes: u64,
    /// Heap bytes.
    pub heap_bytes: u64,
    /// Index bytes.
    pub index_bytes: u64,
    /// TOAST table and TOAST index bytes, read from the TOAST relation.
    pub toast_bytes: u64,
    /// Planner row estimate, or `None` when the relation has never been analyzed.
    pub estimated_rows: Option<i64>,
    /// Live tuple estimate from `pg_stat_user_tables`.
    pub live_tuples: i64,
    /// Dead tuple estimate from `pg_stat_user_tables`.
    pub dead_tuples: i64,
    /// Rows inserted since the statistics reset.
    pub inserts: i64,
    /// Rows updated since the statistics reset.
    pub updates: i64,
    /// Rows deleted since the statistics reset.
    pub deletes: i64,
    /// Age of the statistics window in hours, when the reset is known.
    pub stats_age_hours: Option<f64>,
    /// Last manual vacuum timestamp, formatted by the server.
    pub last_vacuum: Option<String>,
    /// Last manual analyze timestamp, formatted by the server.
    pub last_analyze: Option<String>,
}

impl RelationGrowth {
    /// Qualified name used in findings and ordering.
    #[must_use]
    pub fn qualified_name(&self) -> String {
        format!("{}.{}", self.schema, self.table)
    }

    /// Fraction of estimated tuples that are dead, or `None` when no tuples are known.
    #[must_use]
    pub fn dead_ratio(&self) -> Option<f64> {
        let live = self.live_tuples.max(0) as f64;
        let dead = self.dead_tuples.max(0) as f64;
        let total = live + dead;
        if total <= 0.0 {
            None
        } else {
            Some(dead / total)
        }
    }

    /// Net rows added per day since the statistics reset, or `None` when the
    /// window is too short to be meaningful.
    #[must_use]
    pub fn rows_per_day(&self) -> Option<f64> {
        let age = self.stats_age_hours?;
        if !(age.is_finite() && age >= 1.0) {
            return None;
        }
        let net = self.inserts.saturating_sub(self.deletes);
        Some(net as f64 / (age / 24.0))
    }

    /// Most recent vacuum or analyze timestamp, for a compact table cell.
    #[must_use]
    pub fn last_maintenance(&self) -> Option<&str> {
        match (&self.last_vacuum, &self.last_analyze) {
            (Some(vacuum), Some(analyze)) => Some(if vacuum >= analyze { vacuum } else { analyze }),
            (Some(vacuum), None) => Some(vacuum),
            (None, Some(analyze)) => Some(analyze),
            (None, None) => None,
        }
    }
}

/// One sequence's remaining range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SequenceHeadroom {
    /// Schema name.
    pub schema: String,
    /// Sequence name.
    pub name: String,
    /// Data type reported by `pg_sequences`.
    pub data_type: String,
    /// Last value, or `None` when the caller cannot read it.
    pub last_value: Option<i64>,
    /// Minimum value.
    pub min_value: i64,
    /// Maximum value.
    pub max_value: i64,
    /// Increment; negative for descending sequences.
    pub increment: i64,
    /// Whether the sequence wraps instead of failing.
    pub cyclic: bool,
}

impl SequenceHeadroom {
    /// Qualified name used in findings and ordering.
    #[must_use]
    pub fn qualified_name(&self) -> String {
        format!("{}.{}", self.schema, self.name)
    }

    /// Whether the current value could be read.
    #[must_use]
    pub const fn available(&self) -> bool {
        self.last_value.is_some()
    }

    /// Values left before the sequence stops, or `None` for cyclic or
    /// unreadable sequences.
    #[must_use]
    pub fn remaining(&self) -> Option<u64> {
        let last = i128::from(self.last_value?);
        if self.cyclic || self.increment == 0 {
            return None;
        }
        let step = i128::from(self.increment).abs();
        let span = if self.increment > 0 {
            i128::from(self.max_value) - last
        } else {
            last - i128::from(self.min_value)
        };
        let remaining = span.max(0) / step;
        Some(u64::try_from(remaining).unwrap_or(u64::MAX))
    }

    /// Fraction of the sequence range consumed, clamped to `0..=1`.
    #[must_use]
    pub fn used_fraction(&self) -> Option<f64> {
        let last = i128::from(self.last_value?);
        let min = i128::from(self.min_value);
        let max = i128::from(self.max_value);
        let span = max - min;
        if span <= 0 {
            return None;
        }
        let used = if self.increment >= 0 {
            last - min
        } else {
            max - last
        };
        Some((used as f64 / span as f64).clamp(0.0, 1.0))
    }

    /// Fraction of the sequence range still available.
    #[must_use]
    pub fn remaining_fraction(&self) -> Option<f64> {
        self.used_fraction()
            .map(|used| (1.0 - used).clamp(0.0, 1.0))
    }

    /// Whether the sequence is close enough to exhaustion to be critical.
    #[must_use]
    pub fn is_critical(&self) -> bool {
        if self.cyclic {
            return false;
        }
        let Some(remaining) = self.remaining() else {
            return false;
        };
        remaining <= 1000 || self.remaining_fraction().is_some_and(|f| f <= 0.05)
    }

    /// Whether the sequence warrants a warning but is not critical.
    #[must_use]
    pub fn is_warning(&self) -> bool {
        !self.cyclic
            && self
                .remaining_fraction()
                .is_some_and(|fraction| fraction <= 0.20)
    }

    /// Short state label for the table.
    #[must_use]
    pub fn state_label(&self) -> &'static str {
        if self.cyclic {
            "CYCLIC"
        } else if !self.available() {
            "UNAVAILABLE"
        } else if self.is_critical() {
            "CRITICAL"
        } else if self.is_warning() {
            "WARNING"
        } else {
            "OK"
        }
    }
}

/// One actionable finding derived from the bounded snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GrowthFinding {
    /// Stable code.
    pub code: String,
    /// Severity label.
    pub severity: String,
    /// Qualified object name.
    pub object: String,
    /// Bounded one-line explanation.
    pub message: String,
}

/// A bounded, evaluated growth audit.
#[derive(Debug, Clone, PartialEq)]
pub struct GrowthSnapshot {
    /// Relations retained, ordered by total bytes descending.
    pub relations: Vec<RelationGrowth>,
    /// Sequences retained, ordered by remaining fraction ascending.
    pub sequences: Vec<SequenceHeadroom>,
    /// Findings derived from the retained entries.
    pub findings: Vec<GrowthFinding>,
    /// Database statistics reset timestamp, formatted by the server.
    pub stats_reset: Option<String>,
    /// Age of the statistics window in hours.
    pub stats_age_hours: Option<f64>,
    /// Whether more relations existed than the retention bound.
    pub truncated_relations: bool,
    /// Whether more sequences existed than the retention bound.
    pub truncated_sequences: bool,
}

impl GrowthSnapshot {
    /// Empty snapshot with no findings.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            relations: Vec::new(),
            sequences: Vec::new(),
            findings: Vec::new(),
            stats_reset: None,
            stats_age_hours: None,
            truncated_relations: false,
            truncated_sequences: false,
        }
    }

    /// Derives findings from the retained entries and sorts everything
    /// deterministically. Call after extraction.
    #[must_use]
    pub fn evaluate(mut self, options: &GrowthOptions) -> Self {
        self.relations.sort_by(|a, b| {
            b.total_bytes
                .cmp(&a.total_bytes)
                .then_with(|| a.schema.cmp(&b.schema))
                .then_with(|| a.table.cmp(&b.table))
        });
        self.relations.truncate(MAX_RELATIONS);
        self.sequences.sort_by(|a, b| {
            sequence_sort_key(a)
                .partial_cmp(&sequence_sort_key(b))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.schema.cmp(&b.schema))
                .then_with(|| a.name.cmp(&b.name))
        });
        self.sequences.truncate(MAX_SEQUENCES);

        let mut findings = Vec::new();
        for relation in &self.relations {
            if relation.dead_ratio().is_some_and(|ratio| ratio >= 0.20)
                && relation.dead_tuples >= 10_000
            {
                findings.push(GrowthFinding {
                    code: GrowthFindingCode::VacuumDebt.as_str().to_owned(),
                    severity: GrowthFindingCode::VacuumDebt.severity().label().to_owned(),
                    object: relation.qualified_name(),
                    message: format!(
                        "{} dead tuples ({:.1}% of estimated tuples) and last maintenance {}",
                        relation.dead_tuples,
                        relation.dead_ratio().unwrap_or(0.0) * 100.0,
                        relation.last_maintenance().unwrap_or("never recorded")
                    ),
                });
            }
            if !relation.partitioned
                && relation.total_bytes >= options.large_table_bytes
                && relation.total_bytes > 0
            {
                findings.push(GrowthFinding {
                    code: GrowthFindingCode::PartitioningCandidate.as_str().to_owned(),
                    severity: GrowthFindingCode::PartitioningCandidate
                        .severity()
                        .label()
                        .to_owned(),
                    object: relation.qualified_name(),
                    message: format!(
                        "unpartitioned relation is {} (threshold {}) with {} rows/day net growth",
                        format_bytes(relation.total_bytes),
                        format_bytes(options.large_table_bytes),
                        relation
                            .rows_per_day()
                            .map(|rate| format!("{rate:.0}"))
                            .unwrap_or_else(|| "unavailable".to_owned())
                    ),
                });
            }
        }
        for sequence in &self.sequences {
            let Some(code) = sequence_finding(sequence) else {
                continue;
            };
            let severity = code.severity();
            let remaining = sequence.remaining().unwrap_or(0);
            let fraction = sequence
                .remaining_fraction()
                .map(|value| value * 100.0)
                .unwrap_or(0.0);
            findings.push(GrowthFinding {
                code: code.as_str().to_owned(),
                severity: severity.label().to_owned(),
                object: sequence.qualified_name(),
                message: format!(
                    "{} values remain ({:.1}% of the range) on a {} sequence",
                    remaining,
                    fraction,
                    if sequence.increment < 0 {
                        "descending"
                    } else {
                        "ascending"
                    }
                ),
            });
        }
        findings.sort_by(|a, b| {
            let a_rank = severity_rank(&a.severity);
            let b_rank = severity_rank(&b.severity);
            b_rank
                .cmp(&a_rank)
                .then_with(|| a.code.cmp(&b.code))
                .then_with(|| a.object.cmp(&b.object))
        });
        self.findings = findings;
        self
    }

    /// Whether any finding is at or above the requested threshold.
    #[must_use]
    pub fn has_at_or_above(&self, threshold: GrowthSeverity) -> bool {
        self.findings
            .iter()
            .any(|finding| severity_rank(&finding.severity) >= threshold.rank())
    }

    /// Number of findings at a given severity.
    #[must_use]
    pub fn count_at(&self, severity: GrowthSeverity) -> usize {
        self.findings
            .iter()
            .filter(|finding| severity_rank(&finding.severity) == severity.rank())
            .count()
    }

    /// Renders the human-readable report. `top` controls the relation table
    /// only; findings always cover the whole retained snapshot.
    #[must_use]
    pub fn render_table(&self, options: &GrowthOptions) -> String {
        let top = options.top.clamp(1, MAX_RELATIONS);
        let mut out = String::new();
        out.push_str("Growth and retention audit\n");
        out.push_str("==========================\n\n");
        match (&self.stats_reset, self.stats_age_hours) {
            (Some(reset), Some(age)) => {
                let _ = writeln!(
                    out,
                    "Statistics window: since {reset} ({:.1} hours)",
                    age.max(0.0)
                );
            }
            (Some(reset), None) => {
                let _ = writeln!(out, "Statistics window: since {reset} (age unavailable)");
            }
            _ => out.push_str("Statistics window: unavailable\n"),
        }
        let _ = writeln!(
            out,
            "Retained: {} relation(s){}, {} sequence(s){}\n",
            self.relations.len(),
            if self.truncated_relations {
                " (truncated)"
            } else {
                ""
            },
            self.sequences.len(),
            if self.truncated_sequences {
                " (truncated)"
            } else {
                ""
            }
        );

        let _ = writeln!(
            out,
            "Largest relations (top {} of {})",
            top.min(self.relations.len().max(1)),
            self.relations.len()
        );
        if self.relations.is_empty() {
            out.push_str("No user relations found.\n\n");
        } else {
            out.push_str(
                "SCHEMA.TABLE                         TOTAL      HEAP     INDEX     TOAST   EST ROWS     DEAD  DEAD%  LAST MAINTENANCE\n",
            );
            out.push_str(
                "------------------------------------  --------  --------  --------  --------  ---------  -------  -----  -------------------------\n",
            );
            for relation in self.relations.iter().take(top) {
                let _ = writeln!(
                    out,
                    "{:<36}  {:>8}  {:>8}  {:>8}  {:>8}  {:>9}  {:>7}  {:>5}  {}",
                    truncate_cell(&relation.qualified_name(), 36),
                    format_bytes(relation.total_bytes),
                    format_bytes(relation.heap_bytes),
                    format_bytes(relation.index_bytes),
                    format_bytes(relation.toast_bytes),
                    relation
                        .estimated_rows
                        .map(|rows| rows.to_string())
                        .unwrap_or_else(|| "?".to_owned()),
                    relation.dead_tuples,
                    relation
                        .dead_ratio()
                        .map(|ratio| format!("{:.1}%", ratio * 100.0))
                        .unwrap_or_else(|| "-".to_owned()),
                    relation.last_maintenance().unwrap_or("never recorded"),
                );
            }
            out.push('\n');
        }

        out.push_str("Sequence headroom\n");
        if self.sequences.is_empty() {
            out.push_str("No user sequences found.\n\n");
        } else {
            out.push_str(
                "SEQUENCE                              TYPE        LAST VALUE    MAX VALUE   REMAINING  REMAIN%  STATE\n",
            );
            out.push_str(
                "------------------------------------  ----------  ------------  -----------  ---------  -------  -----------\n",
            );
            for sequence in &self.sequences {
                let _ = writeln!(
                    out,
                    "{:<36}  {:<10}  {:>12}  {:>11}  {:>9}  {:>7}  {}",
                    truncate_cell(&sequence.qualified_name(), 36),
                    truncate_cell(&sequence.data_type, 10),
                    sequence
                        .last_value
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "-".to_owned()),
                    sequence.max_value,
                    sequence
                        .remaining()
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "-".to_owned()),
                    sequence
                        .remaining_fraction()
                        .map(|fraction| format!("{:.1}%", fraction * 100.0))
                        .unwrap_or_else(|| "-".to_owned()),
                    sequence.state_label(),
                );
            }
            out.push('\n');
        }

        let _ = writeln!(
            out,
            "Findings ({} critical, {} warning)",
            self.count_at(GrowthSeverity::Critical),
            self.count_at(GrowthSeverity::Warning)
        );
        if self.findings.is_empty() {
            out.push_str("No retention or growth findings.\n");
        } else {
            for finding in &self.findings {
                let _ = writeln!(
                    out,
                    "[{}] {}  {}",
                    finding.severity, finding.code, finding.object
                );
                let _ = writeln!(out, "  {}", finding.message);
            }
        }
        out
    }

    /// Renders the machine-readable report.
    #[must_use]
    pub fn render_json(&self, options: &GrowthOptions) -> String {
        let view = JsonReport {
            stats_reset: self.stats_reset.as_deref(),
            stats_age_hours: self.stats_age_hours,
            truncated_relations: self.truncated_relations,
            truncated_sequences: self.truncated_sequences,
            top: options.top,
            large_table_bytes: options.large_table_bytes,
            relations: self
                .relations
                .iter()
                .take(options.top.clamp(1, MAX_RELATIONS))
                .map(JsonRelation::from)
                .collect(),
            sequences: self.sequences.iter().map(JsonSequence::from).collect(),
            findings: self.findings.clone(),
        };
        serde_json::to_string_pretty(&view)
            .unwrap_or_else(|_| "{\"error\":\"growth report could not be serialized\"}".to_owned())
    }
}

#[derive(Serialize)]
struct JsonReport<'a> {
    stats_reset: Option<&'a str>,
    stats_age_hours: Option<f64>,
    truncated_relations: bool,
    truncated_sequences: bool,
    top: usize,
    large_table_bytes: u64,
    relations: Vec<JsonRelation<'a>>,
    sequences: Vec<JsonSequence<'a>>,
    findings: Vec<GrowthFinding>,
}

#[derive(Serialize)]
struct JsonRelation<'a> {
    schema: &'a str,
    table: &'a str,
    partitioned: bool,
    total_bytes: u64,
    heap_bytes: u64,
    index_bytes: u64,
    toast_bytes: u64,
    estimated_rows: Option<i64>,
    live_tuples: i64,
    dead_tuples: i64,
    dead_ratio: Option<f64>,
    inserts: i64,
    updates: i64,
    deletes: i64,
    rows_per_day: Option<f64>,
    stats_age_hours: Option<f64>,
    last_vacuum: Option<&'a str>,
    last_analyze: Option<&'a str>,
}

impl<'a> From<&'a RelationGrowth> for JsonRelation<'a> {
    fn from(relation: &'a RelationGrowth) -> Self {
        Self {
            schema: &relation.schema,
            table: &relation.table,
            partitioned: relation.partitioned,
            total_bytes: relation.total_bytes,
            heap_bytes: relation.heap_bytes,
            index_bytes: relation.index_bytes,
            toast_bytes: relation.toast_bytes,
            estimated_rows: relation.estimated_rows,
            live_tuples: relation.live_tuples,
            dead_tuples: relation.dead_tuples,
            dead_ratio: relation.dead_ratio(),
            inserts: relation.inserts,
            updates: relation.updates,
            deletes: relation.deletes,
            rows_per_day: relation.rows_per_day(),
            stats_age_hours: relation.stats_age_hours,
            last_vacuum: relation.last_vacuum.as_deref(),
            last_analyze: relation.last_analyze.as_deref(),
        }
    }
}

#[derive(Serialize)]
struct JsonSequence<'a> {
    schema: &'a str,
    name: &'a str,
    data_type: &'a str,
    last_value: Option<i64>,
    min_value: i64,
    max_value: i64,
    increment: i64,
    cyclic: bool,
    remaining: Option<u64>,
    used_fraction: Option<f64>,
    state: &'static str,
}

impl<'a> From<&'a SequenceHeadroom> for JsonSequence<'a> {
    fn from(sequence: &'a SequenceHeadroom) -> Self {
        Self {
            schema: &sequence.schema,
            name: &sequence.name,
            data_type: &sequence.data_type,
            last_value: sequence.last_value,
            min_value: sequence.min_value,
            max_value: sequence.max_value,
            increment: sequence.increment,
            cyclic: sequence.cyclic,
            remaining: sequence.remaining(),
            used_fraction: sequence.used_fraction(),
            state: sequence.state_label(),
        }
    }
}

fn sequence_finding(sequence: &SequenceHeadroom) -> Option<GrowthFindingCode> {
    if sequence.cyclic || !sequence.available() {
        return None;
    }
    if sequence.is_critical() || sequence.is_warning() {
        Some(GrowthFindingCode::SequenceExhaustion)
    } else {
        None
    }
}

fn sequence_sort_key(sequence: &SequenceHeadroom) -> f64 {
    // Unavailable and cyclic sequences sort after measurable ones for display,
    // but deterministic name ordering still applies within each group.
    sequence.remaining_fraction().unwrap_or(f64::INFINITY)
}

fn severity_rank(label: &str) -> u8 {
    match label {
        "CRITICAL" => 2,
        "WARNING" => 1,
        _ => 0,
    }
}

fn truncate_cell(input: &str, max_chars: usize) -> String {
    let count = input.chars().count();
    if count <= max_chars {
        input.to_owned()
    } else if max_chars <= 3 {
        input.chars().take(max_chars).collect()
    } else {
        let kept: String = input.chars().take(max_chars - 3).collect();
        format!("{kept}...")
    }
}

/// Formats a byte count using binary units, one decimal place.
#[must_use]
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}
