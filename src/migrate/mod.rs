//! Forward-only migration discovery, checksums, planning and rendering.
//!
//! The domain is pure. It reads files into bounded snapshots and compares them
//! with ledger rows supplied by the session layer; it never applies SQL.

use crate::diagnostics::{Diagnostic, DiagnosticKind};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Ledger table created on first apply.
pub const LEDGER_TABLE: &str = "ignatius_schema_migrations";

/// Advisory lock key that serializes concurrent runners.
pub const ADVISORY_LOCK_KEY: i64 = 0x4947_4E41_5449_5553;

/// Largest number of migration files accepted.
pub const MAX_MIGRATIONS: usize = 1000;

/// Largest migration file accepted.
pub const MAX_MIGRATION_BYTES: usize = 4 * 1024 * 1024;

/// One migration file on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationFile {
    /// Numeric version used for ordering.
    pub version: u64,
    /// Version exactly as written in the file name (`001`).
    pub raw_version: String,
    /// Name after the version prefix.
    pub name: String,
    /// Path on disk.
    pub path: PathBuf,
    /// Raw SQL text.
    pub sql: String,
    /// Lowercase SHA-256 of the raw bytes.
    pub checksum: String,
}

/// One applied ledger row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MigrationRecord {
    /// Version text as stored.
    pub version: String,
    /// Name as stored.
    pub name: String,
    /// SHA-256 recorded at apply time.
    pub checksum: String,
    /// Server timestamp rendered as text.
    pub applied_at: String,
    /// Duration in milliseconds.
    pub execution_ms: i64,
}

/// State of one migration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum MigrationState {
    /// File and ledger agree.
    Applied,
    /// File exists, no ledger row.
    Pending,
    /// Ledger row exists, file checksum differs.
    Drifted,
    /// Ledger row exists, file is gone.
    Missing,
}

impl MigrationState {
    /// Stable lowercase label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Applied => "applied",
            Self::Pending => "pending",
            Self::Drifted => "drifted",
            Self::Missing => "missing",
        }
    }
}

/// One migration applied in this run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AppliedMigration {
    /// Version text.
    pub version: String,
    /// Migration name.
    pub name: String,
    /// SHA-256 recorded with the ledger row.
    pub checksum: String,
    /// Server execution duration in milliseconds.
    pub execution_ms: i64,
}

/// Result of applying a plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MigrationApplyReport {
    /// Migrations applied in order.
    pub applied: Vec<AppliedMigration>,
    /// Pending migrations that were already ahead of `--to`.
    pub skipped: usize,
}

/// One row of a plan, combining a file, a ledger row or both.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationEntry {
    /// Version used for ordering.
    pub version: u64,
    /// File, when present.
    pub file: Option<MigrationFile>,
    /// Ledger row, when present.
    pub record: Option<MigrationRecord>,
    /// Derived state.
    pub state: MigrationState,
}

impl MigrationEntry {
    /// Display name, preferring the file and falling back to the ledger.
    #[must_use]
    pub fn name(&self) -> &str {
        self.file.as_ref().map_or_else(
            || self.record.as_ref().map_or("", |r| r.name.as_str()),
            |f| f.name.as_str(),
        )
    }

    /// Version text as written on disk or in the ledger.
    #[must_use]
    pub fn raw_version(&self) -> &str {
        self.file.as_ref().map_or_else(
            || self.record.as_ref().map_or("", |r| r.version.as_str()),
            |f| f.raw_version.as_str(),
        )
    }
}

/// A complete migration plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationPlan {
    /// Entries in version order.
    pub entries: Vec<MigrationEntry>,
}

impl MigrationPlan {
    /// Builds a plan by matching files with records.
    ///
    /// # Errors
    ///
    /// Refuses duplicate versions and ledger versions that are not numeric.
    pub fn new(
        files: Vec<MigrationFile>,
        records: Vec<MigrationRecord>,
    ) -> Result<Self, Diagnostic> {
        let mut by_version: std::collections::BTreeMap<u64, MigrationFile> =
            std::collections::BTreeMap::new();
        for file in files {
            if by_version.insert(file.version, file.clone()).is_some() {
                return Err(Diagnostic::new(
                    DiagnosticKind::Usage,
                    format!(
                        "migration version {} appears more than once",
                        file.raw_version
                    ),
                    "planning migrations",
                )
                .next_action("give every migration a unique numeric prefix"));
            }
        }

        let mut ledger: std::collections::BTreeMap<u64, MigrationRecord> =
            std::collections::BTreeMap::new();
        for record in records {
            let version: u64 = record.version.parse().map_err(|_| {
                Diagnostic::new(
                    DiagnosticKind::Usage,
                    format!("ledger version {:?} is not numeric", record.version),
                    "planning migrations",
                )
                .likely_cause("the ledger was edited outside this tool")
            })?;
            if ledger.insert(version, record).is_some() {
                return Err(Diagnostic::new(
                    DiagnosticKind::Usage,
                    "the migration ledger contains a duplicate version",
                    "planning migrations",
                ));
            }
        }

        let mut entries = Vec::new();
        for (version, file) in &by_version {
            match ledger.remove(version) {
                Some(record) if record.checksum == file.checksum => {
                    entries.push(MigrationEntry {
                        version: *version,
                        file: Some(file.clone()),
                        record: Some(record),
                        state: MigrationState::Applied,
                    });
                }
                Some(record) => {
                    entries.push(MigrationEntry {
                        version: *version,
                        file: Some(file.clone()),
                        record: Some(record),
                        state: MigrationState::Drifted,
                    });
                }
                None => entries.push(MigrationEntry {
                    version: *version,
                    file: Some(file.clone()),
                    record: None,
                    state: MigrationState::Pending,
                }),
            }
        }
        for (version, record) in ledger {
            entries.push(MigrationEntry {
                version,
                file: None,
                record: Some(record),
                state: MigrationState::Missing,
            });
        }
        entries.sort_by_key(|entry| entry.version);
        Ok(Self { entries })
    }

    /// Pending entries in order, optionally capped at a version.
    #[must_use]
    pub fn pending(&self, up_to: Option<u64>) -> Vec<&MigrationEntry> {
        self.entries
            .iter()
            .filter(|entry| entry.state == MigrationState::Pending)
            .filter(|entry| up_to.is_none_or(|version| entry.version <= version))
            .collect()
    }

    /// Drifted entries.
    #[must_use]
    pub fn drifted(&self) -> Vec<&MigrationEntry> {
        self.entries
            .iter()
            .filter(|entry| entry.state == MigrationState::Drifted)
            .collect()
    }

    /// Missing entries.
    #[must_use]
    pub fn missing(&self) -> Vec<&MigrationEntry> {
        self.entries
            .iter()
            .filter(|entry| entry.state == MigrationState::Missing)
            .collect()
    }

    /// Applied entries.
    #[must_use]
    pub fn applied(&self) -> usize {
        self.entries
            .iter()
            .filter(|entry| entry.state == MigrationState::Applied)
            .count()
    }

    /// Whether the plan can be applied without repair.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.drifted().is_empty() && self.missing().is_empty()
    }
}

/// Discovers and parses every `.sql` file in a directory.
///
/// # Errors
///
/// Returns a usage diagnostic for an unreadable directory, a bad file name, a
/// file over [`MAX_MIGRATION_BYTES`], too many files, or invalid UTF-8.
pub fn discover(directory: &Path) -> Result<Vec<MigrationFile>, Diagnostic> {
    let entries = std::fs::read_dir(directory).map_err(|error| {
        Diagnostic::new(
            DiagnosticKind::Usage,
            format!(
                "failed to read migration directory {}: {error}",
                directory.display()
            ),
            "discovering migrations",
        )
        .likely_cause("the directory does not exist or cannot be read")
        .next_action("pass an existing directory with --dir")
    })?;

    let mut paths: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path.extension().and_then(|extension| extension.to_str()) == Some("sql")
        })
        .collect();
    paths.sort();
    if paths.len() > MAX_MIGRATIONS {
        return Err(Diagnostic::new(
            DiagnosticKind::Usage,
            format!("more than {MAX_MIGRATIONS} migration files were found"),
            "discovering migrations",
        ));
    }

    let mut files = Vec::new();
    for path in paths {
        let metadata = std::fs::symlink_metadata(&path).map_err(|error| {
            Diagnostic::new(
                DiagnosticKind::Usage,
                format!("failed to inspect {}: {error}", path.display()),
                "discovering migrations",
            )
        })?;
        if metadata.file_type().is_symlink() {
            return Err(Diagnostic::new(
                DiagnosticKind::Usage,
                format!(
                    "refusing to follow the migration symlink {}",
                    path.display()
                ),
                "discovering migrations",
            ));
        }
        if metadata.len() as usize > MAX_MIGRATION_BYTES {
            return Err(Diagnostic::new(
                DiagnosticKind::Usage,
                format!(
                    "{} is {} bytes, over the {MAX_MIGRATION_BYTES}-byte migration bound",
                    path.display(),
                    metadata.len()
                ),
                "discovering migrations",
            ));
        }
        let (version, raw_version, name) = parse_file_name(&path)?;
        let bytes = std::fs::read(&path).map_err(|error| {
            Diagnostic::new(
                DiagnosticKind::Usage,
                format!("failed to read {}: {error}", path.display()),
                "discovering migrations",
            )
        })?;
        let sql = String::from_utf8(bytes.clone()).map_err(|_| {
            Diagnostic::new(
                DiagnosticKind::Usage,
                format!("{} is not valid UTF-8", path.display()),
                "discovering migrations",
            )
        })?;
        files.push(MigrationFile {
            version,
            raw_version,
            name,
            path,
            sql,
            checksum: checksum(&bytes),
        });
    }
    Ok(files)
}

fn parse_file_name(path: &Path) -> Result<(u64, String, String), Diagnostic> {
    let file_name = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default();
    let Some((raw_version, name)) = file_name.split_once('_') else {
        return Err(bad_name(path));
    };
    if raw_version.is_empty() || !raw_version.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(bad_name(path));
    }
    let version: u64 = raw_version.parse().map_err(|_| bad_name(path))?;
    if name.trim().is_empty() {
        return Err(bad_name(path));
    }
    Ok((version, raw_version.to_owned(), name.to_owned()))
}

fn bad_name(path: &Path) -> Diagnostic {
    Diagnostic::new(
        DiagnosticKind::Usage,
        format!(
            "migration file {} does not start with a numeric version and a name",
            path.display()
        ),
        "discovering migrations",
    )
    .likely_cause("the file name is not `<version>_<name>.sql`")
    .next_action("rename it, for example `001_create_users.sql`")
}

/// Lowercase SHA-256 of raw bytes.
#[must_use]
pub fn checksum(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(64);
    for byte in digest {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}

/// Returns the byte offset of the first top-level transaction control word.
///
/// Protected literals, identifiers and comments are masked first, so a
/// `BEGIN` inside a function body is not a refusal.
#[must_use]
pub fn transaction_control(sql: &str) -> Option<usize> {
    let masked = mask(sql);
    let upper = masked.to_ascii_uppercase();
    ["BEGIN", "COMMIT", "ROLLBACK", "SAVEPOINT", "RELEASE"]
        .iter()
        .filter_map(|word| first_word(&upper, word))
        .min()
}

fn mask(sql: &str) -> String {
    let mut bytes = sql.as_bytes().to_vec();
    for token in crate::query::highlight::tokens(sql) {
        if matches!(
            token.kind,
            crate::query::highlight::TokenKind::Literal
                | crate::query::highlight::TokenKind::Comment
                | crate::query::highlight::TokenKind::Identifier
        ) {
            for byte in &mut bytes[token.start..token.end] {
                if *byte != b'\n' && *byte != b'\r' {
                    *byte = b' ';
                }
            }
        }
    }
    for byte in &mut bytes {
        if !byte.is_ascii() {
            *byte = b' ';
        }
    }
    String::from_utf8(bytes).unwrap_or_else(|_| sql.to_owned())
}

fn first_word(haystack: &str, word: &str) -> Option<usize> {
    let bytes = haystack.as_bytes();
    let mut start = 0;
    while let Some(found) = haystack[start..].find(word) {
        let position = start + found;
        let end = position + word.len();
        let before = position == 0 || !word_byte(bytes[position - 1]);
        let after = bytes.get(end).is_none_or(|byte| !word_byte(*byte));
        if before && after {
            return Some(position);
        }
        start = end;
    }
    None
}

const fn word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$'
}

/// Renders the plan as a table.
#[must_use]
pub fn render_status_table(plan: &MigrationPlan) -> String {
    let mut output = String::new();
    output.push_str("VERSION  STATE    NAME\n");
    for entry in &plan.entries {
        output.push_str(&format!(
            "{:<8} {:<8} {}\n",
            entry.raw_version(),
            entry.state.label(),
            entry.name()
        ));
    }
    let pending = plan.pending(None).len();
    if pending == 0 && plan.is_clean() {
        output.push_str("No pending migrations.\n");
    } else if !plan.is_clean() {
        output.push_str("Drift or missing files must be repaired before apply.\n");
    }
    output
}

/// Renders the plan as JSON.
///
/// # Errors
///
/// Returns the serializer error, which for this model means a defect.
pub fn render_status_json(plan: &MigrationPlan) -> Result<String, serde_json::Error> {
    #[derive(Serialize)]
    struct Entry<'a> {
        version: &'a str,
        name: &'a str,
        state: MigrationState,
        checksum: Option<&'a str>,
        applied_checksum: Option<&'a str>,
        applied_at: Option<&'a str>,
    }
    #[derive(Serialize)]
    struct Report<'a> {
        version: u32,
        clean: bool,
        applied: usize,
        pending: usize,
        drifted: usize,
        missing: usize,
        entries: Vec<Entry<'a>>,
    }
    let entries = plan
        .entries
        .iter()
        .map(|entry| Entry {
            version: entry.raw_version(),
            name: entry.name(),
            state: entry.state,
            checksum: entry.file.as_ref().map(|file| file.checksum.as_str()),
            applied_checksum: entry.record.as_ref().map(|record| record.checksum.as_str()),
            applied_at: entry
                .record
                .as_ref()
                .map(|record| record.applied_at.as_str()),
        })
        .collect();
    serde_json::to_string_pretty(&Report {
        version: 1,
        clean: plan.is_clean(),
        applied: plan.applied(),
        pending: plan.pending(None).len(),
        drifted: plan.drifted().len(),
        missing: plan.missing().len(),
        entries,
    })
}

/// Renders the pending plan as reviewable SQL.
#[must_use]
pub fn render_plan_sql(plan: &MigrationPlan, up_to: Option<u64>) -> String {
    let pending = plan.pending(up_to);
    if pending.is_empty() {
        return "-- No pending migrations.\n".to_owned();
    }
    let mut output = String::new();
    for entry in pending {
        let Some(file) = entry.file.as_ref() else {
            continue;
        };
        output.push_str(&format!(
            "-- ---- {} {} ({}) ----\n",
            file.raw_version, file.name, file.checksum
        ));
        output.push_str(file.sql.trim_end());
        if !file.sql.trim_end().ends_with(';') {
            output.push(';');
        }
        output.push_str("\n\n");
    }
    output
}

/// Renders the pending plan as a table.
#[must_use]
pub fn render_plan_table(plan: &MigrationPlan, up_to: Option<u64>) -> String {
    let pending = plan.pending(up_to);
    if pending.is_empty() {
        return "No pending migrations.\n".to_owned();
    }
    let mut output = String::from("ORDER  VERSION  NAME\n");
    for (index, entry) in pending.iter().enumerate() {
        output.push_str(&format!(
            "{:<6} {:<8} {}\n",
            index + 1,
            entry.raw_version(),
            entry.name()
        ));
    }
    output
}

#[cfg(test)]
mod tests;
