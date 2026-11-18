//! Pure restore-drill planning and verification for Feature `047-restore-drills`.
//!
//! A drill proves that an archive can be read, restored and queried without
//! touching the source database. This module owns the parts that can be decided
//! without a server: archive format, scratch naming and validation, table of
//! contents parsing, plan rendering and the report. Process execution and SQL
//! live in [`crate::cli`] and [`crate::postgres::drill`].

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use serde::Serialize;

/// Maximum number of table-of-contents lines accepted from `pg_restore --list`.
pub const MAX_TOC_LINES: usize = 20_000;

/// Maximum characters retained from a child-process failure.
pub const MAX_FAILURE_CHARS: usize = 2_000;

/// Middle segment of the default scratch database name.
pub const DRILL_SUFFIX: &str = "_ignatius_drill_";

/// System databases a drill may never create over or drop.
pub const SYSTEM_DATABASES: [&str; 3] = ["postgres", "template0", "template1"];

/// Archive formats a drill accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveFormat {
    /// `pg_dump` custom format.
    Custom,
    /// `pg_dump` directory format.
    Directory,
    /// `pg_dump` tar format.
    Tar,
}

impl ArchiveFormat {
    /// Every accepted format, in stable order, for documentation parity checks.
    pub const ALL: [Self; 3] = [Self::Custom, Self::Directory, Self::Tar];

    /// Short lowercase label used in reports and plans.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Custom => "custom",
            Self::Directory => "directory",
            Self::Tar => "tar",
        }
    }

    /// Infers the format from the archive path.
    ///
    /// # Errors
    ///
    /// Returns [`DrillError::PlainSql`] for a `.sql` file, which has no table of
    /// contents and cannot be inspected before loading, and
    /// [`DrillError::ArchiveMissing`] when the path does not exist.
    pub fn detect(path: &Path) -> Result<Self, DrillError> {
        if !path.exists() {
            return Err(DrillError::ArchiveMissing(path.display().to_string()));
        }
        if path.is_dir() {
            return Ok(Self::Directory);
        }
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .map(str::to_ascii_lowercase);
        match extension.as_deref() {
            Some("sql") => Err(DrillError::PlainSql),
            Some("tar") => Ok(Self::Tar),
            _ => Ok(Self::Custom),
        }
    }
}

/// Why a drill was refused or a plan could not be produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DrillError {
    /// The archive path does not exist.
    ArchiveMissing(String),
    /// A plain SQL dump was supplied.
    PlainSql,
    /// The table of contents contained no entries.
    EmptyArchive,
    /// The table of contents could not be parsed.
    CorruptToc(String),
    /// The table of contents exceeded the line bound.
    TocTooLarge(usize),
    /// The scratch database name is empty or too long.
    InvalidScratchName(String),
    /// The scratch database would be the source database.
    ScratchIsSource,
    /// The scratch database would be a system database.
    ScratchIsSystem(String),
}

impl std::fmt::Display for DrillError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ArchiveMissing(path) => {
                write!(formatter, "backup archive does not exist: {path}")
            }
            Self::PlainSql => formatter.write_str(
                "plain SQL dumps are not supported; they have no table of contents to inspect",
            ),
            Self::EmptyArchive => {
                formatter.write_str("the archive table of contents is empty or unreadable")
            }
            Self::CorruptToc(line) => {
                write!(formatter, "unexpected table of contents line: {line}")
            }
            Self::TocTooLarge(lines) => write!(
                formatter,
                "the archive table of contents has more than {MAX_TOC_LINES} lines ({lines})"
            ),
            Self::InvalidScratchName(name) => {
                write!(
                    formatter,
                    "scratch database name is empty or longer than 63 bytes: {name}"
                )
            }
            Self::ScratchIsSource => {
                formatter.write_str("the scratch database cannot be the source database")
            }
            Self::ScratchIsSystem(name) => {
                write!(
                    formatter,
                    "the scratch database cannot be a system database: {name}"
                )
            }
        }
    }
}

impl std::error::Error for DrillError {}

/// One object type counted in the archive table of contents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ObjectTypeCount {
    /// Uppercase object type, for example `TABLE` or `TABLE DATA`.
    pub kind: String,
    /// Number of entries of this type.
    pub count: usize,
}

/// Bounded summary of a `pg_restore --list` listing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TocSummary {
    /// Number of object entries in the archive.
    pub entries: usize,
    /// Object types in deterministic order.
    pub object_types: Vec<ObjectTypeCount>,
}

impl TocSummary {
    /// Human-readable one-line summary, for example `214 entries (TABLE 38, INDEX 51)`.
    #[must_use]
    pub fn summary(&self) -> String {
        let kinds = self
            .object_types
            .iter()
            .map(|entry| format!("{} {}", entry.kind, entry.count))
            .collect::<Vec<_>>()
            .join(", ");
        if kinds.is_empty() {
            format!("{} entries", self.entries)
        } else {
            format!("{} entries ({kinds})", self.entries)
        }
    }
}

/// Parses `pg_restore --list` output.
///
/// Comment lines start with `;` and are skipped. Each entry line begins with a
/// numeric dump id, a semicolon, and at least two catalog numbers before the
/// object type.
///
/// # Errors
///
/// Returns [`DrillError::EmptyArchive`] when no entries were found,
/// [`DrillError::TocTooLarge`] when the listing exceeds [`MAX_TOC_LINES`], and
/// [`DrillError::CorruptToc`] when an entry line is malformed.
pub fn parse_toc(text: &str) -> Result<TocSummary, DrillError> {
    let mut entries = 0usize;
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for (index, line) in text.lines().enumerate() {
        if index >= MAX_TOC_LINES {
            return Err(DrillError::TocTooLarge(index + 1));
        }
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with(';') {
            continue;
        }
        let Some((dump_id, rest)) = trimmed.split_once(';') else {
            return Err(DrillError::CorruptToc(bounded(trimmed, 120)));
        };
        if dump_id.trim().parse::<u64>().is_err() {
            return Err(DrillError::CorruptToc(bounded(trimmed, 120)));
        }
        let tokens: Vec<&str> = rest.split_whitespace().collect();
        if tokens.len() < 3 {
            return Err(DrillError::CorruptToc(bounded(trimmed, 120)));
        }
        let kind = object_kind(&tokens[2..]);
        *counts.entry(kind).or_insert(0) += 1;
        entries += 1;
    }
    if entries == 0 {
        return Err(DrillError::EmptyArchive);
    }
    Ok(TocSummary {
        entries,
        object_types: counts
            .into_iter()
            .map(|(kind, count)| ObjectTypeCount { kind, count })
            .collect(),
    })
}

fn object_kind(tokens: &[&str]) -> String {
    match tokens {
        ["TABLE", "DATA", ..] => "TABLE DATA".to_owned(),
        ["TABLE", "ATTACH", ..] => "TABLE ATTACH".to_owned(),
        ["SEQUENCE", "SET", ..] => "SEQUENCE SET".to_owned(),
        ["MATERIALIZED", "VIEW", "DATA", ..] => "MATERIALIZED VIEW DATA".to_owned(),
        ["MATERIALIZED", "VIEW", ..] => "MATERIALIZED VIEW".to_owned(),
        ["FK", "CONSTRAINT", ..] => "FK CONSTRAINT".to_owned(),
        ["DEFAULT", "ACL", ..] => "DEFAULT ACL".to_owned(),
        [first, ..] => first.to_ascii_uppercase(),
        [] => "UNKNOWN".to_owned(),
    }
}

/// Default scratch database name for a source and a UTC timestamp.
///
/// The timestamp is supplied by the caller so naming is testable. A name that
/// would exceed PostgreSQL's 63-byte identifier limit keeps a stable hash of
/// the full name instead of silently colliding with another database.
#[must_use]
pub fn scratch_name(source: &str, timestamp: &str) -> String {
    let suffix = format!("{DRILL_SUFFIX}{timestamp}");
    let full = format!("{source}{suffix}");
    if full.len() <= 63 {
        return full;
    }
    let tag = format!("_{:08x}", fnv1a(full.as_bytes()));
    let budget = 63usize.saturating_sub(suffix.len() + tag.len());
    let mut kept = String::new();
    for character in source.chars() {
        if kept.len() + character.len_utf8() > budget {
            break;
        }
        kept.push(character);
    }
    format!("{kept}{tag}{suffix}")
}

/// Validates an explicit or generated scratch database name.
///
/// # Errors
///
/// Returns [`DrillError::InvalidScratchName`] for an empty or over-long name,
/// [`DrillError::ScratchIsSource`] when it equals the source, and
/// [`DrillError::ScratchIsSystem`] for a system database.
pub fn validate_scratch_name(source: &str, scratch: &str) -> Result<(), DrillError> {
    let trimmed = scratch.trim();
    if trimmed.is_empty() || trimmed.len() > 63 {
        return Err(DrillError::InvalidScratchName(bounded(scratch, 80)));
    }
    if trimmed == source {
        return Err(DrillError::ScratchIsSource);
    }
    if SYSTEM_DATABASES
        .iter()
        .any(|name| trimmed.eq_ignore_ascii_case(name))
    {
        return Err(DrillError::ScratchIsSystem(trimmed.to_owned()));
    }
    Ok(())
}

/// Inputs to one drill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrillRequest {
    /// Source database named by the connection target.
    pub source_database: String,
    /// Scratch database the archive is restored into.
    pub scratch_database: String,
    /// Archive path.
    pub archive_path: String,
    /// Detected archive format.
    pub format: ArchiveFormat,
    /// Keep the scratch database after a passing drill.
    pub keep: bool,
    /// Drop the scratch database even when the drill fails.
    pub drop_on_failure: bool,
}

/// One ordered step in a drill plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrillStep {
    /// Short action label.
    pub action: &'static str,
    /// Exact action, including any redacted command.
    pub detail: String,
}

/// Ordered plan shown by `--dry-run`.
#[must_use]
pub fn planned_steps(request: &DrillRequest, summary: &TocSummary) -> Vec<DrillStep> {
    let scratch = crate::query::identifiers::quote_identifier(&request.scratch_database);
    vec![
        DrillStep {
            action: "inspect",
            detail: format!(
                "pg_restore --list {} ({})",
                request.archive_path,
                summary.summary()
            ),
        },
        DrillStep {
            action: "create",
            detail: format!("CREATE DATABASE {scratch} TEMPLATE template0"),
        },
        DrillStep {
            action: "restore",
            detail: format!(
                "pg_restore --exit-on-error -d {scratch} {} (PGPASSWORD=[redacted])",
                request.archive_path
            ),
        },
        DrillStep {
            action: "verify",
            detail: "compare user relation counts between the source and the scratch database"
                .to_owned(),
        },
        DrillStep {
            action: "drop",
            detail: format!(
                "DROP DATABASE {scratch} (skipped with --keep; failed drills are kept unless --drop-on-failure)"
            ),
        },
    ]
}

/// Outcome of a drill that ran.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DrillOutcome {
    /// The archive restored and the relation counts matched.
    Passed,
    /// The archive or the restored database failed verification.
    Failed,
}

impl DrillOutcome {
    /// Uppercase label used in the table report.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Passed => "PASSED",
            Self::Failed => "FAILED",
        }
    }
}

/// A bounded drill report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DrillReport {
    /// Source database. Never the restore target.
    pub source_database: String,
    /// Scratch database used for the restore.
    pub scratch_database: String,
    /// Archive path.
    pub archive: String,
    /// Archive format label.
    pub archive_format: String,
    /// Number of archive entries inspected.
    pub toc_entries: usize,
    /// Result.
    pub outcome: DrillOutcome,
    /// Total elapsed milliseconds.
    pub duration_ms: u64,
    /// User relations counted in the source, when reached.
    pub source_relations: Option<u64>,
    /// User relations counted in the restored database, when reached.
    pub restored_relations: Option<u64>,
    /// Restored database size in bytes, when measured.
    pub restored_bytes: Option<u64>,
    /// Whether the scratch database was left in place.
    pub scratch_kept: bool,
    /// Bounded failure reason.
    pub failure: Option<String>,
}

impl DrillReport {
    /// Whether the drill passed.
    #[must_use]
    pub fn passed(&self) -> bool {
        self.outcome == DrillOutcome::Passed
    }

    /// Renders a human-readable report.
    #[must_use]
    pub fn render_table(&self) -> String {
        let mut out = String::new();
        out.push_str("Restore drill\n=============\n\n");
        let _ = writeln!(out, "Source database:  {}", self.source_database);
        let _ = writeln!(
            out,
            "Archive:          {} ({}, {} entries)",
            self.archive, self.archive_format, self.toc_entries
        );
        let _ = writeln!(out, "Scratch database: {}", self.scratch_database);
        let _ = writeln!(
            out,
            "Result:           {} in {} ms",
            self.outcome.label(),
            self.duration_ms
        );
        match (self.source_relations, self.restored_relations) {
            (Some(source), Some(restored)) => {
                let _ = writeln!(
                    out,
                    "Relations:        {source} source, {restored} restored"
                );
            }
            (Some(source), None) => {
                let _ = writeln!(
                    out,
                    "Relations:        {source} source, restored count unavailable"
                );
            }
            _ => out.push_str("Relations:        unavailable\n"),
        }
        if let Some(bytes) = self.restored_bytes {
            let _ = writeln!(out, "Restored size:    {}", format_bytes(bytes));
        }
        let _ = writeln!(
            out,
            "Scratch database: {}",
            if self.scratch_kept {
                "kept for inspection"
            } else {
                "dropped"
            }
        );
        if let Some(failure) = &self.failure {
            let _ = writeln!(out, "\nFailure: {failure}");
        }
        out
    }

    /// Renders the report as stable JSON.
    #[must_use]
    pub fn render_json(&self) -> String {
        serde_json::to_string_pretty(self)
            .unwrap_or_else(|_| "{\"error\":\"drill report could not be serialized\"}".to_owned())
    }
}

/// Bounds a failure or diagnostic string for the report.
#[must_use]
pub fn bounded(text: &str, max_chars: usize) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_owned();
    }
    let kept: String = trimmed.chars().take(max_chars.saturating_sub(3)).collect();
    format!("{kept}...")
}

/// Whether the scratch database should be dropped after a drill.
///
/// A passing drill is cleaned up unless `keep` was asked for. A failed drill is
/// kept so it can be inspected, unless `drop_on_failure` was asked for. `keep`
/// wins over `drop_on_failure`.
#[must_use]
pub const fn should_drop(outcome: DrillOutcome, keep: bool, drop_on_failure: bool) -> bool {
    if keep {
        return false;
    }
    match outcome {
        DrillOutcome::Passed => true,
        DrillOutcome::Failed => drop_on_failure,
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

fn fnv1a(bytes: &[u8]) -> u32 {
    let mut hash: u32 = 0x811c_9dc5;
    for byte in bytes {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}
