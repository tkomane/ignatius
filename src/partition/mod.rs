//! Pure partition lifecycle model for Feature `048-partition-lifecycle`.
//!
//! The model reports what the catalog contains and generates reviewed DDL for
//! range partitions. It never executes SQL. Date arithmetic is calendar-based
//! and every unsupported shape is refused rather than guessed.

#[cfg(test)]
mod tests;

use std::fmt::Write as _;

use chrono::{Datelike, NaiveDate, NaiveDateTime};
use serde::Serialize;

/// Maximum number of partitioned parents retained in one status snapshot.
pub const MAX_PARENTS: usize = 500;

/// Maximum number of child partitions retained in one status snapshot.
pub const MAX_PARTITIONS: usize = 5000;

/// Maximum number of intervals one plan may generate.
pub const MAX_PLAN_COUNT: usize = 120;

/// Partition strategy reported by `pg_partitioned_table.partstrat`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PartitionStrategy {
    /// Range partitioning.
    Range,
    /// List partitioning.
    List,
    /// Hash partitioning.
    Hash,
    /// A strategy this build does not know.
    Unknown,
}

impl PartitionStrategy {
    /// Maps the one-character catalog value.
    #[must_use]
    pub fn from_catalog(value: &str) -> Self {
        match value {
            "r" => Self::Range,
            "l" => Self::List,
            "h" => Self::Hash,
            _ => Self::Unknown,
        }
    }

    /// Uppercase label used in tables and diagnostics.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Range => "RANGE",
            Self::List => "LIST",
            Self::Hash => "HASH",
            Self::Unknown => "UNKNOWN",
        }
    }
}

/// One child partition in the status report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PartitionEntry {
    /// Schema containing the child.
    pub schema: String,
    /// Child relation name.
    pub name: String,
    /// Raw bound expression from the catalog.
    pub bound: String,
    /// Total allocated bytes.
    pub total_bytes: u64,
    /// Planner row estimate, or `None` when never analyzed.
    pub estimated_rows: Option<i64>,
}

impl PartitionEntry {
    /// Whether this is the default partition.
    #[must_use]
    pub fn is_default(&self) -> bool {
        self.bound.trim().eq_ignore_ascii_case("DEFAULT")
    }

    /// Qualified child name.
    #[must_use]
    pub fn qualified_name(&self) -> String {
        format!("{}.{}", self.schema, self.name)
    }
}

/// One partitioned parent and its retained children.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PartitionParent {
    /// Schema containing the parent.
    pub schema: String,
    /// Parent relation name.
    pub table: String,
    /// Declared strategy.
    pub strategy: PartitionStrategy,
    /// `pg_get_partkeydef` text.
    pub key_def: String,
    /// Type of the first key column, when it is a plain column.
    pub key_type: Option<String>,
    /// Retained child partitions.
    pub partitions: Vec<PartitionEntry>,
}

impl PartitionParent {
    /// Qualified parent name.
    #[must_use]
    pub fn qualified_name(&self) -> String {
        format!("{}.{}", self.schema, self.table)
    }

    /// Whether a default partition exists.
    #[must_use]
    pub fn has_default(&self) -> bool {
        self.partitions.iter().any(PartitionEntry::is_default)
    }

    /// Total bytes across retained children.
    #[must_use]
    pub fn total_bytes(&self) -> u64 {
        self.partitions.iter().map(|entry| entry.total_bytes).sum()
    }
}

/// A bounded partition status snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PartitionSnapshot {
    /// Partitioned parents, ordered by qualified name.
    pub parents: Vec<PartitionParent>,
    /// Whether more parents existed than the retention bound.
    pub truncated_parents: bool,
    /// Whether more child partitions existed than the retention bound.
    pub truncated_partitions: bool,
}

impl PartitionSnapshot {
    /// Empty snapshot.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            parents: Vec::new(),
            truncated_parents: false,
            truncated_partitions: false,
        }
    }

    /// Finds a parent by exact schema and table name.
    #[must_use]
    pub fn find(&self, schema: &str, table: &str) -> Option<&PartitionParent> {
        self.parents
            .iter()
            .find(|parent| parent.schema == schema && parent.table == table)
    }

    /// Sorts parents and children deterministically.
    #[must_use]
    pub fn sorted(mut self) -> Self {
        self.parents
            .sort_by(|a, b| a.schema.cmp(&b.schema).then_with(|| a.table.cmp(&b.table)));
        for parent in &mut self.parents {
            parent.partitions.sort_by(|a, b| {
                let a_key = bound_sort_key(&a.bound);
                let b_key = bound_sort_key(&b.bound);
                a_key.cmp(&b_key).then_with(|| a.name.cmp(&b.name))
            });
        }
        self
    }

    /// Renders the human-readable status report.
    #[must_use]
    pub fn render_table(&self) -> String {
        let mut out = String::new();
        if self.parents.is_empty() {
            out.push_str("No partitioned tables found.\n");
            return out;
        }
        let partition_count: usize = self.parents.iter().map(|p| p.partitions.len()).sum();
        let _ = writeln!(
            out,
            "Partitioned tables: {}{}",
            self.parents.len(),
            if self.truncated_parents {
                " (parent list truncated)"
            } else {
                ""
            }
        );
        let _ = writeln!(
            out,
            "Partitions: {}{}\n",
            partition_count,
            if self.truncated_partitions {
                " (partition list truncated)"
            } else {
                ""
            }
        );
        for parent in &self.parents {
            let _ = writeln!(
                out,
                "{}  [{}]  {}  ({} partition(s){})",
                parent.qualified_name(),
                parent.strategy.label(),
                parent.key_def,
                parent.partitions.len(),
                if parent.has_default() {
                    ", default"
                } else {
                    ""
                }
            );
            if parent.partitions.is_empty() {
                out.push_str("  (no partitions)\n\n");
                continue;
            }
            out.push_str(
                "  PARTITION                              BOUND                                                        SIZE      EST ROWS\n",
            );
            out.push_str(
                "  -------------------------------------  -----------------------------------------------------------  --------  ---------\n",
            );
            for partition in &parent.partitions {
                let _ = writeln!(
                    out,
                    "  {:<37}  {:<59}  {:>8}  {:>9}",
                    truncate_cell(&partition.qualified_name(), 37),
                    truncate_cell(&partition.bound, 59),
                    format_bytes(partition.total_bytes),
                    partition
                        .estimated_rows
                        .map(|rows| rows.to_string())
                        .unwrap_or_else(|| "?".to_owned()),
                );
            }
            out.push('\n');
        }
        out
    }

    /// Renders the machine-readable status report.
    #[must_use]
    pub fn render_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| {
            "{\"error\":\"partition status could not be serialized\"}".to_owned()
        })
    }
}

/// Calendar period for a generated plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanPeriod {
    /// One day per interval.
    Daily,
    /// One ISO week per interval, starting Monday.
    Weekly,
    /// One calendar month per interval.
    Monthly,
    /// One calendar year per interval.
    Yearly,
}

impl PlanPeriod {
    /// Every period, in stable order, for documentation parity checks.
    pub const ALL: [Self; 4] = [Self::Daily, Self::Weekly, Self::Monthly, Self::Yearly];

    /// Short name used in the plan header.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Daily => "daily",
            Self::Weekly => "weekly",
            Self::Monthly => "monthly",
            Self::Yearly => "yearly",
        }
    }

    /// Whether `start` is aligned to this period.
    #[must_use]
    pub fn is_aligned(self, start: NaiveDate) -> bool {
        match self {
            Self::Daily => true,
            Self::Weekly => start.weekday() == chrono::Weekday::Mon,
            Self::Monthly => start.day() == 1,
            Self::Yearly => start.month() == 1 && start.day() == 1,
        }
    }

    /// The first day after an interval that starts at `start`.
    #[must_use]
    pub fn next(self, start: NaiveDate) -> Option<NaiveDate> {
        match self {
            Self::Daily => start.checked_add_days(chrono::Days::new(1)),
            Self::Weekly => start.checked_add_days(chrono::Days::new(7)),
            Self::Monthly => add_months(start, 1),
            Self::Yearly => add_months(start, 12),
        }
    }

    /// Deterministic partition name suffix for an interval starting at `start`.
    #[must_use]
    pub fn name_suffix(self, start: NaiveDate) -> String {
        match self {
            Self::Daily => format!(
                "p{:04}_{:02}_{:02}",
                start.year(),
                start.month(),
                start.day()
            ),
            Self::Weekly => format!(
                "p{:04}_W{:02}",
                start.iso_week().year(),
                start.iso_week().week()
            ),
            Self::Monthly => format!("p{:04}_{:02}", start.year(), start.month()),
            Self::Yearly => format!("p{:04}", start.year()),
        }
    }
}

/// Type of the range partition key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyType {
    /// `date`.
    Date,
    /// `timestamp without time zone`.
    Timestamp,
    /// `timestamp with time zone`.
    Timestamptz,
}

impl KeyType {
    /// Maps a `format_type` value to a supported planning type.
    #[must_use]
    pub fn from_catalog(value: &str) -> Option<Self> {
        let normalized = value.trim().to_ascii_lowercase();
        if normalized == "date" {
            Some(Self::Date)
        } else if normalized.starts_with("timestamp") && normalized.contains("with time zone") {
            Some(Self::Timestamptz)
        } else if normalized.starts_with("timestamp") {
            Some(Self::Timestamp)
        } else {
            None
        }
    }

    fn literal(self, start: NaiveDate) -> String {
        match self {
            Self::Date => format!("'{}'", start.format("%Y-%m-%d")),
            Self::Timestamp => format!("'{} 00:00:00'", start.format("%Y-%m-%d")),
            Self::Timestamptz => format!("'{} 00:00:00+00'", start.format("%Y-%m-%d")),
        }
    }
}

/// One endpoint of a range partition bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RangeValue {
    /// `MINVALUE`.
    Min,
    /// `MAXVALUE`.
    Max,
    /// A concrete date or timestamp, normalised to a naive server-time value.
    Point(NaiveDateTime),
}

/// A parsed `FOR VALUES FROM (...) TO (...)` bound.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RangeBound {
    /// Inclusive lower endpoint.
    pub from: RangeValue,
    /// Exclusive upper endpoint.
    pub to: RangeValue,
}

impl RangeBound {
    /// Whether this bound overlaps the half-open interval `[start, end)`.
    #[must_use]
    pub fn overlaps(&self, start: NaiveDateTime, end: NaiveDateTime) -> bool {
        let from_before_end = match self.from {
            RangeValue::Point(point) => point < end,
            RangeValue::Min => true,
            RangeValue::Max => false,
        };
        let to_after_start = match self.to {
            RangeValue::Point(point) => start < point,
            RangeValue::Min => false,
            RangeValue::Max => true,
        };
        from_before_end && to_after_start
    }

    /// Whether the bound ends on or before `cutoff`, making it safe to expire.
    #[must_use]
    pub fn ends_on_or_before(&self, cutoff: NaiveDateTime) -> bool {
        match self.to {
            RangeValue::Point(point) => point <= cutoff,
            RangeValue::Min | RangeValue::Max => false,
        }
    }
}

/// Parsed form of a child bound.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PartitionBound {
    /// A parsed range bound.
    Range(RangeBound),
    /// The default partition.
    Default,
    /// A bound this build cannot interpret.
    Unparsed,
}

/// Parses a `pg_get_expr(relpartbound)` value.
#[must_use]
pub fn parse_bound(bound: &str) -> PartitionBound {
    if bound.trim().eq_ignore_ascii_case("DEFAULT") {
        return PartitionBound::Default;
    }
    let Some(rest) = bound.trim().strip_prefix("FOR VALUES FROM (") else {
        return PartitionBound::Unparsed;
    };
    let Some((from, rest)) = rest.split_once(") TO (") else {
        return PartitionBound::Unparsed;
    };
    let Some(to) = rest.strip_suffix(')') else {
        return PartitionBound::Unparsed;
    };
    match (parse_value(from), parse_value(to)) {
        (Some(from), Some(to)) => PartitionBound::Range(RangeBound { from, to }),
        _ => PartitionBound::Unparsed,
    }
}

fn parse_value(value: &str) -> Option<RangeValue> {
    let trimmed = value.trim();
    if trimmed.eq_ignore_ascii_case("MINVALUE") {
        return Some(RangeValue::Min);
    }
    if trimmed.eq_ignore_ascii_case("MAXVALUE") {
        return Some(RangeValue::Max);
    }
    let literal = trimmed.strip_prefix('\'')?.strip_suffix('\'')?;
    parse_literal(literal).map(RangeValue::Point)
}

fn parse_literal(value: &str) -> Option<NaiveDateTime> {
    if let Ok(date) = NaiveDate::parse_from_str(value, "%Y-%m-%d") {
        return date.and_hms_opt(0, 0, 0);
    }
    for format in [
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S%z",
        "%Y-%m-%d %H:%M:%S%.f%z",
        "%Y-%m-%d %H:%M:%S%:z",
        "%Y-%m-%d %H:%M:%S%.f%:z",
        "%Y-%m-%d %H:%M:%S%#z",
        "%Y-%m-%d %H:%M:%S%.f%#z",
        "%Y-%m-%dT%H:%M:%S%z",
        "%Y-%m-%dT%H:%M:%S%.f%z",
        "%Y-%m-%dT%H:%M:%S%:z",
        "%Y-%m-%dT%H:%M:%S%.f%:z",
        "%Y-%m-%dT%H:%M:%S%#z",
        "%Y-%m-%dT%H:%M:%S%.f%#z",
    ] {
        if let Ok(value) = NaiveDateTime::parse_from_str(value, format) {
            return Some(value);
        }
        if let Ok(value) = chrono::DateTime::parse_from_str(value, format) {
            return Some(value.naive_utc());
        }
    }
    None
}

/// One generated partition creation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedCreate {
    /// New child name.
    pub name: String,
    /// Interval start.
    pub from: NaiveDate,
    /// Interval end.
    pub to: NaiveDate,
}

/// One child the plan proposes to expire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedExpiry {
    /// Existing child schema.
    pub schema: String,
    /// Existing child name.
    pub name: String,
    /// Raw bound, copied into the review comment.
    pub bound: String,
}

/// A reviewed plan. Nothing here has been executed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartitionPlan {
    /// Parent schema.
    pub schema: String,
    /// Parent table.
    pub table: String,
    /// Period used to generate the intervals.
    pub period: PlanPeriod,
    /// First interval start.
    pub start: NaiveDate,
    /// Number of intervals requested.
    pub count: usize,
    /// Partitions to create.
    pub creates: Vec<PlannedCreate>,
    /// Partitions to detach and drop.
    pub expires: Vec<PlannedExpiry>,
    /// Whether a default partition exists.
    pub has_default: bool,
    /// Key type, used to render unambiguous literals.
    pub key_type: KeyType,
}

impl PartitionPlan {
    /// Number of SQL statements the plan contains, counting detach and drop
    /// separately.
    #[must_use]
    pub fn statement_count(&self) -> usize {
        self.creates.len() + self.expires.len() * 2
    }

    /// Renders the plan as SQL for review.
    #[must_use]
    pub fn render_sql(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "-- Generated by ignatius partition plan for {}.{}",
            quote_ident(&self.schema),
            quote_ident(&self.table)
        );
        let _ = writeln!(
            out,
            "-- Period: {}, start: {}, count: {}",
            self.period.label(),
            self.start.format("%Y-%m-%d"),
            self.count
        );
        out.push_str("-- Review this file and apply it through the migration process.\n");
        if self.has_default {
            out.push_str(
                "-- A default partition exists. If it holds rows for these ranges, PostgreSQL\n\
                 -- will refuse the creation; move those rows first.\n",
            );
        }
        out.push('\n');
        for create in &self.creates {
            let _ = writeln!(
                out,
                "CREATE TABLE IF NOT EXISTS {}.{} PARTITION OF {}.{} FOR VALUES FROM ({}) TO ({});",
                quote_ident(&self.schema),
                quote_ident(&create.name),
                quote_ident(&self.schema),
                quote_ident(&self.table),
                self.key_type.literal(create.from),
                self.key_type.literal(create.to),
            );
        }
        if !self.expires.is_empty() {
            out.push('\n');
        }
        for expiry in &self.expires {
            let qualified = format!(
                "{}.{}",
                quote_ident(&expiry.schema),
                quote_ident(&expiry.name)
            );
            let _ = writeln!(
                out,
                "-- Expired partition {qualified} (bound {})",
                expiry.bound
            );
            let _ = writeln!(
                out,
                "ALTER TABLE {}.{} DETACH PARTITION {}.{};",
                quote_ident(&self.schema),
                quote_ident(&self.table),
                quote_ident(&expiry.schema),
                quote_ident(&expiry.name),
            );
            let _ = writeln!(
                out,
                "DROP TABLE {}.{};",
                quote_ident(&expiry.schema),
                quote_ident(&expiry.name),
            );
        }
        out
    }
}

/// Why a plan was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanError {
    /// The parent is not range partitioned.
    UnsupportedStrategy(PartitionStrategy),
    /// The key is not a single plain column.
    UnsupportedKey(String),
    /// The key type is not date or timestamp.
    UnsupportedKeyType(String),
    /// The start date is not aligned to the period.
    StartNotAligned {
        /// Period requested.
        period: PlanPeriod,
        /// Start supplied.
        start: NaiveDate,
    },
    /// The count is outside the bound.
    InvalidCount(usize),
    /// A generated interval overflowed the calendar.
    DateOverflow,
    /// An existing bound could not be parsed.
    UnparsableBound {
        /// Existing child name.
        partition: String,
        /// Raw bound.
        bound: String,
    },
    /// A generated interval overlaps an existing partition.
    Overlap {
        /// Interval start.
        from: NaiveDate,
        /// Interval end.
        to: NaiveDate,
        /// Existing child name.
        partition: String,
    },
}

impl std::fmt::Display for PlanError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedStrategy(strategy) => write!(
                formatter,
                "only RANGE partitioning is supported; this table uses {}",
                strategy.label()
            ),
            Self::UnsupportedKey(key) => write!(
                formatter,
                "only a single plain-column range key is supported; this table uses {key}"
            ),
            Self::UnsupportedKeyType(data_type) => write!(
                formatter,
                "only date, timestamp and timestamptz keys are supported; this key is {data_type}"
            ),
            Self::StartNotAligned { period, start } => write!(
                formatter,
                "start {} is not aligned to the {} period",
                start.format("%Y-%m-%d"),
                period.label()
            ),
            Self::InvalidCount(count) => write!(
                formatter,
                "count {count} is outside the supported range 1..={MAX_PLAN_COUNT}"
            ),
            Self::DateOverflow => {
                formatter.write_str("the requested intervals overflow the calendar")
            }
            Self::UnparsableBound { partition, bound } => write!(
                formatter,
                "existing partition {partition} has a bound this build cannot parse: {bound}"
            ),
            Self::Overlap {
                from,
                to,
                partition,
            } => write!(
                formatter,
                "interval {} to {} overlaps existing partition {partition}",
                from.format("%Y-%m-%d"),
                to.format("%Y-%m-%d")
            ),
        }
    }
}

impl std::error::Error for PlanError {}

/// Extracts a single plain column name from `pg_get_partkeydef` output.
#[must_use]
pub fn parse_single_column_key(key_def: &str) -> Option<String> {
    let open = key_def.find('(')?;
    let close = key_def.rfind(')')?;
    let inside = key_def.get(open + 1..close)?.trim();
    if inside.is_empty() || inside.contains(',') || inside.contains('(') {
        return None;
    }
    if inside.starts_with('"') && inside.ends_with('"') && inside.len() >= 2 {
        let unquoted = &inside[1..inside.len() - 1];
        if unquoted.contains('"') {
            return None;
        }
        Some(unquoted.to_owned())
    } else {
        Some(inside.to_owned())
    }
}

/// Builds a reviewed plan for one parent.
///
/// # Errors
///
/// Returns [`PlanError`] when the table shape is unsupported, an existing bound
/// cannot be parsed, an interval overlaps, or the request is out of bounds.
pub fn plan_partitions(
    parent: &PartitionParent,
    period: PlanPeriod,
    start: NaiveDate,
    count: usize,
    expire_before: Option<NaiveDate>,
) -> Result<PartitionPlan, PlanError> {
    if parent.strategy != PartitionStrategy::Range {
        return Err(PlanError::UnsupportedStrategy(parent.strategy));
    }
    if count == 0 || count > MAX_PLAN_COUNT {
        return Err(PlanError::InvalidCount(count));
    }
    if !period.is_aligned(start) {
        return Err(PlanError::StartNotAligned { period, start });
    }
    let Some(column) = parse_single_column_key(&parent.key_def) else {
        return Err(PlanError::UnsupportedKey(parent.key_def.clone()));
    };
    if column.is_empty() {
        return Err(PlanError::UnsupportedKey(parent.key_def.clone()));
    }
    let key_type = parent
        .key_type
        .as_deref()
        .and_then(KeyType::from_catalog)
        .ok_or_else(|| {
            PlanError::UnsupportedKeyType(
                parent
                    .key_type
                    .clone()
                    .unwrap_or_else(|| "an expression key".to_owned()),
            )
        })?;

    let mut existing = Vec::with_capacity(parent.partitions.len());
    let mut has_default = false;
    for partition in &parent.partitions {
        match parse_bound(&partition.bound) {
            PartitionBound::Range(bound) => existing.push((partition, bound)),
            PartitionBound::Default => has_default = true,
            PartitionBound::Unparsed => {
                return Err(PlanError::UnparsableBound {
                    partition: partition.qualified_name(),
                    bound: partition.bound.clone(),
                });
            }
        }
    }

    let mut creates = Vec::with_capacity(count);
    let mut cursor = start;
    for _ in 0..count {
        let end = period.next(cursor).ok_or(PlanError::DateOverflow)?;
        let start_time = cursor.and_hms_opt(0, 0, 0).ok_or(PlanError::DateOverflow)?;
        let end_time = end.and_hms_opt(0, 0, 0).ok_or(PlanError::DateOverflow)?;
        if let Some((partition, _)) = existing
            .iter()
            .find(|(_, bound)| bound.overlaps(start_time, end_time))
        {
            return Err(PlanError::Overlap {
                from: cursor,
                to: end,
                partition: partition.qualified_name(),
            });
        }
        let name = fit_identifier(&parent.table, &period.name_suffix(cursor));
        creates.push(PlannedCreate {
            name,
            from: cursor,
            to: end,
        });
        cursor = end;
    }

    let mut expires = Vec::new();
    if let Some(cutoff) = expire_before {
        let cutoff = cutoff.and_hms_opt(0, 0, 0).ok_or(PlanError::DateOverflow)?;
        for (partition, bound) in &existing {
            if bound.ends_on_or_before(cutoff) {
                expires.push(PlannedExpiry {
                    schema: partition.schema.clone(),
                    name: partition.name.clone(),
                    bound: partition.bound.clone(),
                });
            }
        }
    }

    Ok(PartitionPlan {
        schema: parent.schema.clone(),
        table: parent.table.clone(),
        period,
        start,
        count,
        creates,
        expires,
        has_default,
        key_type,
    })
}

/// Truncates an identifier to PostgreSQL's 63-byte limit with a stable suffix.
#[must_use]
pub fn fit_identifier(parent: &str, suffix: &str) -> String {
    let full = format!("{parent}_{suffix}");
    if full.len() <= 63 {
        return full;
    }
    let hash = fnv1a(full.as_bytes());
    let tag = format!("_{hash:08x}");
    let budget = 63usize.saturating_sub(tag.len());
    let mut kept = String::new();
    for character in parent.chars() {
        if kept.len() + character.len_utf8() > budget {
            break;
        }
        kept.push(character);
    }
    format!("{kept}{tag}")
}

fn fnv1a(bytes: &[u8]) -> u32 {
    let mut hash: u32 = 0x811c_9dc5;
    for byte in bytes {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

fn add_months(date: NaiveDate, months: u32) -> Option<NaiveDate> {
    let total = date.month0() + months;
    let year = date.year().checked_add(i32::try_from(total / 12).ok()?)?;
    let month = total % 12 + 1;
    NaiveDate::from_ymd_opt(year, month, date.day())
}

fn bound_sort_key(bound: &str) -> (u8, i64) {
    match parse_bound(bound) {
        PartitionBound::Range(range) => match range.from {
            RangeValue::Point(point) => (0, point.and_utc().timestamp()),
            RangeValue::Min => (0, i64::MIN),
            RangeValue::Max => (1, 0),
        },
        PartitionBound::Default => (2, 0),
        PartitionBound::Unparsed => (3, 0),
    }
}

fn quote_ident(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
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

fn format_bytes(bytes: u64) -> String {
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
