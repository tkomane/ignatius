//! Pure canonical PostgreSQL schema snapshot and drift-guard diff engine for
//! Feature `057-schema-snapshots-and-drift-guard`.
//!
//! This module contains zero database, filesystem, network, process, or
//! terminal I/O. Catalog rows are extracted by [`crate::postgres::snapshot`],
//! canonicalized and hashed into a deterministic lowercase SHA-256 fingerprint
//! by [`SchemaSnapshot::finalize`], and compared by [`diff_snapshots`] into a
//! bounded [`SnapshotDiffReport`] with strictly plain-ASCII table, plain-text,
//! and JSON renderers.

#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::diagnostics::{Diagnostic, DiagnosticKind};
use crate::query::value::sanitize_for_display;

/// Current serialized schema snapshot format version.
pub const SNAPSHOT_SCHEMA_VERSION: u32 = 1;

/// Maximum number of tables retained in a single schema snapshot.
pub const MAX_SNAPSHOT_TABLES: usize = 500;

/// Maximum number of columns retained across all tables in a snapshot.
pub const MAX_SNAPSHOT_COLUMNS: usize = 5000;

/// Maximum number of indexes retained across all tables in a snapshot.
pub const MAX_SNAPSHOT_INDEXES: usize = 2000;

/// Maximum number of Row-Level Security policies retained in a snapshot.
pub const MAX_SNAPSHOT_POLICIES: usize = 1000;

/// Maximum number of table constraints retained across all tables in a snapshot.
pub const MAX_SNAPSHOT_CONSTRAINTS: usize = 5000;

/// Maximum serialized snapshot JSON payload size accepted by [`SchemaSnapshot::from_json`].
pub const MAX_SNAPSHOT_JSON_BYTES: usize = 16 * 1024 * 1024;

const MAX_JSON_NESTING: usize = 32;

/// Canonical snapshot of one table column from `pg_attribute` / `pg_attrdef`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColumnSnapshot {
    /// Column identifier (`attname`).
    pub name: String,
    /// Formatted PostgreSQL data type (`format_type(atttypid, atttypmod)`).
    pub data_type: String,
    /// Whether `NULL` values are permitted (`NOT attnotnull`).
    pub nullable: bool,
    /// Rendered default or generated expression, when present.
    pub default_expr: Option<String>,
    /// One-based attribute number (`attnum`).
    pub ordinal: i16,
}

/// Canonical snapshot of one table constraint from `pg_constraint`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConstraintSnapshot {
    /// Constraint name (`conname`).
    pub name: String,
    /// Constraint category (`PRIMARY KEY`, `FOREIGN KEY`, `UNIQUE`, `CHECK`, `EXCLUSION`).
    pub kind: String,
    /// Server-rendered constraint definition (`pg_get_constraintdef`).
    pub definition: String,
}

/// Canonical snapshot of one table index from `pg_index`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexSnapshot {
    /// Index relation name.
    pub name: String,
    /// Whether the index enforces uniqueness (`indisunique`).
    pub is_unique: bool,
    /// Whether the index backs the primary key (`indisprimary`).
    pub is_primary: bool,
    /// Full `CREATE [UNIQUE] INDEX` statement (`pg_get_indexdef`).
    pub definition: String,
}

/// Canonical snapshot of one Row-Level Security policy from `pg_policy`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicySnapshot {
    /// Policy name (`polname`).
    pub name: String,
    /// Command scope (`ALL`, `SELECT`, `INSERT`, `UPDATE`, `DELETE`).
    pub cmd: String,
    /// Whether the policy is `PERMISSIVE` (`true`) or `RESTRICTIVE` (`false`).
    pub permissive: bool,
    /// Sorted list of target role names (`PUBLIC` or role identifiers).
    pub roles: Vec<String>,
    /// Optional `USING (...)` expression (`pg_get_expr(polqual, polrelid)`).
    pub qual: Option<String>,
    /// Optional `WITH CHECK (...)` expression (`pg_get_expr(polwithcheck, polrelid)`).
    pub with_check: Option<String>,
}

/// Canonical snapshot of one ordinary or partitioned PostgreSQL table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableSnapshot {
    /// Schema identifier (`nspname`).
    pub schema: String,
    /// Table identifier (`relname`).
    pub name: String,
    /// Columns sorted by `(ordinal, name)`.
    pub columns: Vec<ColumnSnapshot>,
    /// Constraints sorted by `(name, kind, definition)`.
    pub constraints: Vec<ConstraintSnapshot>,
    /// Indexes sorted by `(name, definition)`.
    pub indexes: Vec<IndexSnapshot>,
    /// Whether Row-Level Security is enabled (`relrowsecurity`).
    pub rls_enabled: bool,
    /// Whether Row-Level Security is forced for the owner (`relforcerowsecurity`).
    pub rls_forced: bool,
    /// Attached RLS policies sorted by `(name, cmd)`.
    pub policies: Vec<PolicySnapshot>,
}

impl TableSnapshot {
    /// Returns the `schema.name` identifier for display and diff findings.
    #[must_use]
    pub fn qualified_name(&self) -> String {
        format!("{}.{}", self.schema, self.name)
    }
}

/// Canonical snapshot of one view or materialized view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewSnapshot {
    /// Schema identifier (`nspname`).
    pub schema: String,
    /// View or materialized view identifier (`relname`).
    pub name: String,
    /// Whether the relation is a materialized view (`relkind = 'm'`).
    pub materialized: bool,
    /// Lowercase SHA-256 hex digest of the server-rendered view definition.
    pub definition_sha256: String,
}

impl ViewSnapshot {
    /// Returns the `schema.name` identifier for display and diff findings.
    #[must_use]
    pub fn qualified_name(&self) -> String {
        format!("{}.{}", self.schema, self.name)
    }
}

/// Canonical snapshot of one PostgreSQL sequence from `pg_sequence`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SequenceSnapshot {
    /// Schema identifier (`nspname`).
    pub schema: String,
    /// Sequence identifier (`relname`).
    pub name: String,
    /// Underlying integer data type (`smallint`, `integer`, `bigint`).
    pub data_type: String,
    /// Configured initial value (`seqstart`).
    pub start_value: i64,
    /// Configured increment step (`seqincrement`).
    pub increment_by: i64,
    /// Whether the sequence wraps around upon reaching its bound (`seqcycle`).
    pub cycle: bool,
}

impl SequenceSnapshot {
    /// Returns the `schema.name` identifier for display and diff findings.
    #[must_use]
    pub fn qualified_name(&self) -> String {
        format!("{}.{}", self.schema, self.name)
    }
}

/// Canonical snapshot of one PostgreSQL `ENUM` type from `pg_type` / `pg_enum`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnumSnapshot {
    /// Schema identifier (`nspname`).
    pub schema: String,
    /// Type identifier (`typname`).
    pub name: String,
    /// Enum labels in catalog `enumsortorder` order.
    pub labels: Vec<String>,
}

impl EnumSnapshot {
    /// Returns the `schema.name` identifier for display and diff findings.
    #[must_use]
    pub fn qualified_name(&self) -> String {
        format!("{}.{}", self.schema, self.name)
    }
}

/// Complete, canonical, bounded schema snapshot with deterministic SHA-256 fingerprint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchemaSnapshot {
    /// Snapshot model version ([`SNAPSHOT_SCHEMA_VERSION`]).
    pub schema_version: u32,
    /// Optional single-schema filter applied during capture.
    pub schema_filter: Option<String>,
    /// Canonical lowercase 64-character SHA-256 hex digest over the finalized model.
    pub fingerprint_sha256: String,
    /// Tables sorted by `(schema, name)`.
    pub tables: Vec<TableSnapshot>,
    /// Views and materialized views sorted by `(schema, name, materialized)`.
    pub views: Vec<ViewSnapshot>,
    /// Sequences sorted by `(schema, name)`.
    pub sequences: Vec<SequenceSnapshot>,
    /// Enum types sorted by `(schema, name)`.
    pub enums: Vec<EnumSnapshot>,
    /// True when any object bound capped the snapshot contents.
    pub truncated: bool,
}

impl Default for SchemaSnapshot {
    fn default() -> Self {
        Self::new(None)
    }
}

impl SchemaSnapshot {
    /// Creates an empty, unfinalized snapshot for an optional schema filter.
    #[must_use]
    pub fn new(schema_filter: Option<String>) -> Self {
        Self {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            schema_filter,
            fingerprint_sha256: String::new(),
            tables: Vec::new(),
            views: Vec::new(),
            sequences: Vec::new(),
            enums: Vec::new(),
            truncated: false,
        }
    }

    /// Deterministically sorts all collections, enforces bounds, computes the
    /// canonical SHA-256 fingerprint over the JSON representation with
    /// `fingerprint_sha256 = ""`, and stores the lowercase hex digest.
    #[must_use]
    pub fn finalize(mut self) -> Self {
        self.sort_and_bound();
        self.fingerprint_sha256.clear();
        let canonical_json = serde_json::to_vec(&self).unwrap_or_default();
        self.fingerprint_sha256 = sha256_hex(&canonical_json);
        self
    }

    /// Recomputes the canonical SHA-256 fingerprint without mutating `self`.
    #[must_use]
    pub fn compute_fingerprint(&self) -> String {
        let mut copy = self.clone();
        copy.sort_and_bound();
        copy.fingerprint_sha256.clear();
        let canonical_json = serde_json::to_vec(&copy).unwrap_or_default();
        sha256_hex(&canonical_json)
    }

    /// Returns `true` when `fingerprint_sha256` is non-empty and matches the
    /// recomputed canonical digest of this snapshot.
    #[must_use]
    pub fn validate_fingerprint(&self) -> bool {
        !self.fingerprint_sha256.is_empty() && self.compute_fingerprint() == self.fingerprint_sha256
    }

    /// Total number of columns across all tables in the snapshot.
    #[must_use]
    pub fn total_columns(&self) -> usize {
        self.tables.iter().map(|t| t.columns.len()).sum()
    }

    /// Total number of indexes across all tables in the snapshot.
    #[must_use]
    pub fn total_indexes(&self) -> usize {
        self.tables.iter().map(|t| t.indexes.len()).sum()
    }

    /// Total number of RLS policies across all tables in the snapshot.
    #[must_use]
    pub fn total_policies(&self) -> usize {
        self.tables.iter().map(|t| t.policies.len()).sum()
    }

    /// Parses and validates a serialized [`SchemaSnapshot`] JSON payload.
    pub fn from_json(json: &str) -> Result<Self, Diagnostic> {
        if json.len() > MAX_SNAPSHOT_JSON_BYTES {
            return Err(Diagnostic::new(
                DiagnosticKind::Usage,
                format!(
                    "schema snapshot JSON exceeds the {MAX_SNAPSHOT_JSON_BYTES}-byte safety bound"
                ),
                "parsing schema snapshot JSON",
            )
            .likely_cause("the input file is too large to be a bounded schema snapshot")
            .next_action("provide a snapshot generated by `ignatius snapshot`"));
        }
        if !json_nesting_within_bound(json) {
            return Err(Diagnostic::new(
                DiagnosticKind::Usage,
                "schema snapshot JSON is malformed or exceeds nesting depth limits",
                "parsing schema snapshot JSON",
            )
            .next_action("check that the snapshot file is valid JSON"));
        }

        let mut deserializer = serde_json::Deserializer::from_str(json);
        deserializer.disable_recursion_limit();
        let parsed = SchemaSnapshot::deserialize(&mut deserializer).map_err(|err| {
            Diagnostic::new(
                DiagnosticKind::Usage,
                format!("could not parse schema snapshot JSON: {err}"),
                "parsing schema snapshot JSON",
            )
            .next_action("verify that the file is a valid Ignatius schema snapshot")
        })?;

        if parsed.schema_version != SNAPSHOT_SCHEMA_VERSION {
            return Err(Diagnostic::new(
                DiagnosticKind::Usage,
                format!(
                    "unsupported schema snapshot version {} (expected {})",
                    parsed.schema_version, SNAPSHOT_SCHEMA_VERSION
                ),
                "validating schema snapshot version",
            )
            .next_action("regenerate the baseline snapshot with this version of Ignatius"));
        }

        Ok(parsed)
    }

    /// Renders this snapshot as strictly plain-ASCII pretty JSON.
    #[must_use]
    pub fn render_json(&self) -> String {
        let raw = serde_json::to_string_pretty(self).unwrap_or_else(|_| {
            "{\"error\":\"schema snapshot could not be serialized\"}".to_owned()
        });
        escape_json_to_ascii(&raw)
    }

    /// Renders a concise, strictly plain-ASCII summary of the captured snapshot.
    #[must_use]
    pub fn render_summary(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "-- Schema Snapshot Summary --");
        let _ = writeln!(out, "Schema Version:   {}", self.schema_version);
        let _ = writeln!(
            out,
            "Schema Filter:    {}",
            sanitize_ascii(
                self.schema_filter
                    .as_deref()
                    .unwrap_or("<all user schemas>")
            )
        );
        let _ = writeln!(
            out,
            "Fingerprint:      {}",
            if self.fingerprint_sha256.is_empty() {
                "<unfinalized>".to_owned()
            } else {
                sanitize_ascii(&self.fingerprint_sha256)
            }
        );
        let _ = writeln!(out, "Tables:           {}", self.tables.len());
        let _ = writeln!(out, "Columns:          {}", self.total_columns());
        let _ = writeln!(out, "Indexes:          {}", self.total_indexes());
        let _ = writeln!(out, "RLS Policies:     {}", self.total_policies());
        let _ = writeln!(out, "Views:            {}", self.views.len());
        let _ = writeln!(out, "Sequences:        {}", self.sequences.len());
        let _ = writeln!(out, "Enums:            {}", self.enums.len());
        let _ = writeln!(
            out,
            "Truncated:        {}",
            if self.truncated { "YES" } else { "NO" }
        );
        out
    }

    fn sort_and_bound(&mut self) {
        self.tables
            .sort_by(|a, b| (&a.schema, &a.name).cmp(&(&b.schema, &b.name)));
        for table in &mut self.tables {
            table
                .columns
                .sort_by(|a, b| a.ordinal.cmp(&b.ordinal).then_with(|| a.name.cmp(&b.name)));
            table.constraints.sort_by(|a, b| {
                a.name
                    .cmp(&b.name)
                    .then_with(|| a.kind.cmp(&b.kind))
                    .then_with(|| a.definition.cmp(&b.definition))
            });
            table.indexes.sort_by(|a, b| {
                a.name
                    .cmp(&b.name)
                    .then_with(|| a.definition.cmp(&b.definition))
            });
            for policy in &mut table.policies {
                policy.roles.sort();
            }
            table
                .policies
                .sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.cmd.cmp(&b.cmd)));
        }

        self.views.sort_by(|a, b| {
            (&a.schema, &a.name, a.materialized).cmp(&(&b.schema, &b.name, b.materialized))
        });
        self.sequences
            .sort_by(|a, b| (&a.schema, &a.name).cmp(&(&b.schema, &b.name)));
        self.enums
            .sort_by(|a, b| (&a.schema, &a.name).cmp(&(&b.schema, &b.name)));

        if self.tables.len() > MAX_SNAPSHOT_TABLES {
            self.tables.truncate(MAX_SNAPSHOT_TABLES);
            self.truncated = true;
        }
        if self.views.len() > MAX_SNAPSHOT_TABLES {
            self.views.truncate(MAX_SNAPSHOT_TABLES);
            self.truncated = true;
        }
        if self.sequences.len() > MAX_SNAPSHOT_TABLES {
            self.sequences.truncate(MAX_SNAPSHOT_TABLES);
            self.truncated = true;
        }
        if self.enums.len() > MAX_SNAPSHOT_TABLES {
            self.enums.truncate(MAX_SNAPSHOT_TABLES);
            self.truncated = true;
        }

        let mut remaining_columns = MAX_SNAPSHOT_COLUMNS;
        let mut remaining_constraints = MAX_SNAPSHOT_CONSTRAINTS;
        let mut remaining_indexes = MAX_SNAPSHOT_INDEXES;
        let mut remaining_policies = MAX_SNAPSHOT_POLICIES;

        for table in &mut self.tables {
            if table.columns.len() > remaining_columns {
                table.columns.truncate(remaining_columns);
                self.truncated = true;
            }
            remaining_columns = remaining_columns.saturating_sub(table.columns.len());

            if table.constraints.len() > remaining_constraints {
                table.constraints.truncate(remaining_constraints);
                self.truncated = true;
            }
            remaining_constraints = remaining_constraints.saturating_sub(table.constraints.len());

            if table.indexes.len() > remaining_indexes {
                table.indexes.truncate(remaining_indexes);
                self.truncated = true;
            }
            remaining_indexes = remaining_indexes.saturating_sub(table.indexes.len());

            if table.policies.len() > remaining_policies {
                table.policies.truncate(remaining_policies);
                self.truncated = true;
            }
            remaining_policies = remaining_policies.saturating_sub(table.policies.len());
        }
    }
}

/// Severity assigned to a schema drift finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DriftSeverity {
    /// Destructive, compatibility-breaking, or security-weakening schema change.
    Breaking,
    /// Potentially risky or semantic schema change warranting operator review.
    Warning,
    /// Additive or non-breaking informational schema change.
    Info,
}

impl DriftSeverity {
    /// Lowercase label used in plain text and JSON diagnostics.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Breaking => "breaking",
            Self::Warning => "warning",
            Self::Info => "info",
        }
    }

    /// Uppercase fixed-width label used in tables and plain badges.
    #[must_use]
    pub const fn badge(self) -> &'static str {
        match self {
            Self::Breaking => "BREAKING",
            Self::Warning => "WARNING",
            Self::Info => "INFO",
        }
    }

    /// Numeric severity weight (`Breaking = 2`, `Warning = 1`, `Info = 0`).
    #[must_use]
    pub const fn rank(self) -> u8 {
        match self {
            Self::Breaking => 2,
            Self::Warning => 1,
            Self::Info => 0,
        }
    }
}

/// Drift-guard failure threshold used by CLI and CI gates.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum DriftFailThreshold {
    /// Fail only when at least one `Breaking` finding is detected.
    #[default]
    Breaking,
    /// Fail when any `Breaking` or `Warning` finding is detected.
    Warning,
    /// Fail when any drift finding (`Breaking`, `Warning`, or `Info`) is detected.
    Any,
    /// Never fail based on drift findings (informational reporting only).
    Never,
}

impl DriftFailThreshold {
    /// Canonical CLI flag values accepted for `--fail-on`.
    pub const ACCEPTED: &'static [&'static str] = &["breaking", "warning", "any", "never"];

    /// Lowercase threshold label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Breaking => "breaking",
            Self::Warning => "warning",
            Self::Any => "any",
            Self::Never => "never",
        }
    }

    /// Parses a user or CLI threshold flag case-insensitively with friendly aliases.
    #[must_use]
    pub fn parse_loose(input: &str) -> Option<Self> {
        match input.trim().to_ascii_lowercase().as_str() {
            "breaking" | "break" | "error" | "errors" | "critical" => Some(Self::Breaking),
            "warning" | "warn" | "warnings" => Some(Self::Warning),
            "any" | "all" | "info" => Some(Self::Any),
            "never" | "none" | "off" | "false" => Some(Self::Never),
            _ => None,
        }
    }
}

/// One structured schema drift finding between an expected and actual snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DriftFinding {
    /// Finding severity (`Breaking`, `Warning`, `Info`).
    pub severity: DriftSeverity,
    /// Stable machine-readable finding kind (for example `column_dropped`).
    pub kind: String,
    /// Qualified affected object identifier (`schema.table`, `schema.table.column`, etc.).
    pub object: String,
    /// Human-readable plain-ASCII description of the detected drift.
    pub detail: String,
}

/// Complete comparison report produced by [`diff_snapshots`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotDiffReport {
    /// SHA-256 fingerprint of the expected baseline snapshot.
    pub expected_fingerprint: String,
    /// SHA-256 fingerprint of the actual target snapshot.
    pub actual_fingerprint: String,
    /// True when `expected_fingerprint == actual_fingerprint`.
    pub fingerprints_match: bool,
    /// Deterministically ordered list of detected schema drift findings.
    pub findings: Vec<DriftFinding>,
    /// Number of `Breaking` findings.
    pub breaking_count: usize,
    /// Number of `Warning` findings.
    pub warning_count: usize,
    /// Number of `Info` findings.
    pub info_count: usize,
}

impl SnapshotDiffReport {
    /// Returns `true` when the report violates the configured failure threshold.
    #[must_use]
    pub fn exceeds_threshold(&self, threshold: DriftFailThreshold) -> bool {
        match threshold {
            DriftFailThreshold::Breaking => self.breaking_count > 0,
            DriftFailThreshold::Warning => self.breaking_count > 0 || self.warning_count > 0,
            DriftFailThreshold::Any => !self.findings.is_empty(),
            DriftFailThreshold::Never => false,
        }
    }

    /// Returns `true` when any finding was emitted or fingerprints differ.
    #[must_use]
    pub fn has_drift(&self) -> bool {
        !self.fingerprints_match || !self.findings.is_empty()
    }

    /// Renders a strictly plain-ASCII tabular drift report for terminal and CLI display.
    #[must_use]
    pub fn render_table(&self) -> String {
        let mut out = String::new();
        let status = if self.has_drift() {
            "DRIFT DETECTED"
        } else {
            "NO DRIFT"
        };
        let _ = writeln!(out, "-- Schema Drift Report --");
        let _ = writeln!(out, "Status:              {status}");
        let _ = writeln!(
            out,
            "Fingerprints Match:  {}",
            if self.fingerprints_match { "YES" } else { "NO" }
        );
        let _ = writeln!(
            out,
            "Expected SHA-256:    {}",
            sanitize_ascii(&self.expected_fingerprint)
        );
        let _ = writeln!(
            out,
            "Actual SHA-256:      {}",
            sanitize_ascii(&self.actual_fingerprint)
        );
        let _ = writeln!(
            out,
            "Summary:             breaking={} warning={} info={} total={}",
            self.breaking_count,
            self.warning_count,
            self.info_count,
            self.findings.len()
        );
        out.push('\n');
        let _ = writeln!(out, "-- Findings ({}) --", self.findings.len());
        if self.findings.is_empty() {
            out.push_str("No schema drift findings.\n");
            return out;
        }

        let kind_width = self
            .findings
            .iter()
            .map(|f| sanitize_ascii(&f.kind).len())
            .max()
            .unwrap_or(4)
            .clamp(4, 32);
        let object_width = self
            .findings
            .iter()
            .map(|f| sanitize_ascii(&f.object).len())
            .max()
            .unwrap_or(6)
            .clamp(6, 56);

        let _ = writeln!(
            out,
            "{:<9}  {:<kind_width$}  {:<object_width$}  DETAIL",
            "SEVERITY", "KIND", "OBJECT",
        );
        let rule_len = 9 + 2 + kind_width + 2 + object_width + 2 + 24;
        let _ = writeln!(out, "{}", "-".repeat(rule_len));
        for finding in &self.findings {
            let _ = writeln!(
                out,
                "{:<9}  {:<kind_width$}  {:<object_width$}  {}",
                finding.severity.badge(),
                truncate_ascii(&sanitize_ascii(&finding.kind), kind_width),
                truncate_ascii(&sanitize_ascii(&finding.object), object_width),
                sanitize_ascii(&finding.detail),
            );
        }
        out
    }

    /// Renders a strictly plain-ASCII line-oriented report suitable for scripts and pipes.
    #[must_use]
    pub fn render_plain(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "status={}",
            if self.has_drift() { "drift" } else { "clean" }
        );
        let _ = writeln!(out, "fingerprints_match={}", self.fingerprints_match);
        let _ = writeln!(
            out,
            "expected_fingerprint={}",
            sanitize_ascii(&self.expected_fingerprint)
        );
        let _ = writeln!(
            out,
            "actual_fingerprint={}",
            sanitize_ascii(&self.actual_fingerprint)
        );
        let _ = writeln!(out, "breaking_count={}", self.breaking_count);
        let _ = writeln!(out, "warning_count={}", self.warning_count);
        let _ = writeln!(out, "info_count={}", self.info_count);
        for finding in &self.findings {
            let _ = writeln!(
                out,
                "finding: [{}] kind={} object={} detail={}",
                finding.severity.badge(),
                sanitize_ascii(&finding.kind),
                sanitize_ascii(&finding.object),
                sanitize_ascii(&finding.detail),
            );
        }
        out
    }

    /// Renders a strictly plain-ASCII pretty JSON representation of the report.
    #[must_use]
    pub fn render_json(&self) -> String {
        let raw = serde_json::to_string_pretty(self).unwrap_or_else(|_| {
            "{\"error\":\"snapshot diff report could not be serialized\"}".to_owned()
        });
        escape_json_to_ascii(&raw)
    }
}

/// Computes deterministic schema drift findings between an `expected` baseline
/// snapshot and an `actual` target snapshot.
#[must_use]
pub fn diff_snapshots(expected: &SchemaSnapshot, actual: &SchemaSnapshot) -> SnapshotDiffReport {
    let mut expected_canon = expected.clone();
    expected_canon.sort_and_bound();
    let mut actual_canon = actual.clone();
    actual_canon.sort_and_bound();

    let expected_fingerprint = if expected.fingerprint_sha256.is_empty() {
        expected_canon.compute_fingerprint()
    } else {
        expected.fingerprint_sha256.clone()
    };
    let actual_fingerprint = if actual.fingerprint_sha256.is_empty() {
        actual_canon.compute_fingerprint()
    } else {
        actual.fingerprint_sha256.clone()
    };
    let fingerprints_match = expected_fingerprint == actual_fingerprint;

    let mut findings = Vec::new();
    diff_tables(&expected_canon.tables, &actual_canon.tables, &mut findings);
    diff_views(&expected_canon.views, &actual_canon.views, &mut findings);
    diff_sequences(
        &expected_canon.sequences,
        &actual_canon.sequences,
        &mut findings,
    );
    diff_enums(&expected_canon.enums, &actual_canon.enums, &mut findings);

    findings.sort_by(|a, b| {
        b.severity
            .rank()
            .cmp(&a.severity.rank())
            .then_with(|| a.object.cmp(&b.object))
            .then_with(|| a.kind.cmp(&b.kind))
            .then_with(|| a.detail.cmp(&b.detail))
    });

    let breaking_count = findings
        .iter()
        .filter(|f| f.severity == DriftSeverity::Breaking)
        .count();
    let warning_count = findings
        .iter()
        .filter(|f| f.severity == DriftSeverity::Warning)
        .count();
    let info_count = findings
        .iter()
        .filter(|f| f.severity == DriftSeverity::Info)
        .count();

    SnapshotDiffReport {
        expected_fingerprint,
        actual_fingerprint,
        fingerprints_match,
        findings,
        breaking_count,
        warning_count,
        info_count,
    }
}

/// Computes the lowercase 64-character hex SHA-256 digest of raw bytes.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for byte in digest {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

fn diff_tables(
    expected: &[TableSnapshot],
    actual: &[TableSnapshot],
    findings: &mut Vec<DriftFinding>,
) {
    let exp_map: BTreeMap<(&str, &str), &TableSnapshot> = expected
        .iter()
        .map(|t| ((t.schema.as_str(), t.name.as_str()), t))
        .collect();
    let act_map: BTreeMap<(&str, &str), &TableSnapshot> = actual
        .iter()
        .map(|t| ((t.schema.as_str(), t.name.as_str()), t))
        .collect();

    for (key, exp_tbl) in &exp_map {
        let qualified = exp_tbl.qualified_name();
        match act_map.get(key) {
            None => {
                findings.push(DriftFinding {
                    severity: DriftSeverity::Breaking,
                    kind: "table_dropped".to_owned(),
                    object: qualified.clone(),
                    detail: format!("Table {qualified} was dropped"),
                });
            }
            Some(act_tbl) => {
                diff_table_columns(&qualified, &exp_tbl.columns, &act_tbl.columns, findings);
                diff_table_constraints(
                    &qualified,
                    &exp_tbl.constraints,
                    &act_tbl.constraints,
                    findings,
                );
                diff_table_indexes(&qualified, &exp_tbl.indexes, &act_tbl.indexes, findings);
                diff_table_rls(&qualified, exp_tbl, act_tbl, findings);
                diff_table_policies(&qualified, &exp_tbl.policies, &act_tbl.policies, findings);
            }
        }
    }

    for (key, act_tbl) in &act_map {
        if !exp_map.contains_key(key) {
            let qualified = act_tbl.qualified_name();
            findings.push(DriftFinding {
                severity: DriftSeverity::Info,
                kind: "table_added".to_owned(),
                object: qualified.clone(),
                detail: format!(
                    "Table {qualified} was added ({} column(s))",
                    act_tbl.columns.len()
                ),
            });
        }
    }
}

fn diff_table_columns(
    table_qualified: &str,
    expected: &[ColumnSnapshot],
    actual: &[ColumnSnapshot],
    findings: &mut Vec<DriftFinding>,
) {
    let exp_cols: BTreeMap<&str, &ColumnSnapshot> =
        expected.iter().map(|c| (c.name.as_str(), c)).collect();
    let act_cols: BTreeMap<&str, &ColumnSnapshot> =
        actual.iter().map(|c| (c.name.as_str(), c)).collect();

    for (col_name, exp_col) in &exp_cols {
        let object = format!("{table_qualified}.{col_name}");
        match act_cols.get(col_name) {
            None => {
                findings.push(DriftFinding {
                    severity: DriftSeverity::Breaking,
                    kind: "column_dropped".to_owned(),
                    object,
                    detail: format!(
                        "Column {} ({}) was dropped from {table_qualified}",
                        exp_col.name, exp_col.data_type
                    ),
                });
            }
            Some(act_col) => {
                if exp_col.data_type != act_col.data_type {
                    findings.push(DriftFinding {
                        severity: DriftSeverity::Breaking,
                        kind: "column_type_changed".to_owned(),
                        object: object.clone(),
                        detail: format!(
                            "Column {} type changed from {} to {}",
                            exp_col.name, exp_col.data_type, act_col.data_type
                        ),
                    });
                }
                if exp_col.nullable != act_col.nullable {
                    findings.push(DriftFinding {
                        severity: DriftSeverity::Breaking,
                        kind: "column_nullability_changed".to_owned(),
                        object: object.clone(),
                        detail: format!(
                            "Column {} nullability changed from {} to {}",
                            exp_col.name,
                            nullability_label(exp_col.nullable),
                            nullability_label(act_col.nullable)
                        ),
                    });
                }
                if exp_col.default_expr != act_col.default_expr {
                    findings.push(DriftFinding {
                        severity: DriftSeverity::Warning,
                        kind: "column_default_changed".to_owned(),
                        object: object.clone(),
                        detail: format!(
                            "Column {} default changed from {} to {}",
                            exp_col.name,
                            exp_col.default_expr.as_deref().unwrap_or("<none>"),
                            act_col.default_expr.as_deref().unwrap_or("<none>"),
                        ),
                    });
                }
                if exp_col.ordinal != act_col.ordinal {
                    findings.push(DriftFinding {
                        severity: DriftSeverity::Info,
                        kind: "column_reordered".to_owned(),
                        object,
                        detail: format!(
                            "Column {} ordinal moved from {} to {}",
                            exp_col.name, exp_col.ordinal, act_col.ordinal
                        ),
                    });
                }
            }
        }
    }

    for (col_name, act_col) in &act_cols {
        if !exp_cols.contains_key(col_name) {
            findings.push(DriftFinding {
                severity: DriftSeverity::Info,
                kind: "column_added".to_owned(),
                object: format!("{table_qualified}.{col_name}"),
                detail: format!(
                    "Column {} ({}, {}) was added to {table_qualified}",
                    act_col.name,
                    act_col.data_type,
                    nullability_label(act_col.nullable)
                ),
            });
        }
    }
}

fn diff_table_constraints(
    table_qualified: &str,
    expected: &[ConstraintSnapshot],
    actual: &[ConstraintSnapshot],
    findings: &mut Vec<DriftFinding>,
) {
    let exp_map: BTreeMap<&str, &ConstraintSnapshot> =
        expected.iter().map(|c| (c.name.as_str(), c)).collect();
    let act_map: BTreeMap<&str, &ConstraintSnapshot> =
        actual.iter().map(|c| (c.name.as_str(), c)).collect();

    for (name, exp_con) in &exp_map {
        let object = format!("{table_qualified}.{name}");
        match act_map.get(name) {
            None => {
                findings.push(DriftFinding {
                    severity: DriftSeverity::Warning,
                    kind: "constraint_dropped".to_owned(),
                    object,
                    detail: format!(
                        "Constraint {} ({}) was dropped from {table_qualified}",
                        exp_con.name, exp_con.kind
                    ),
                });
            }
            Some(act_con) => {
                if exp_con.kind != act_con.kind || exp_con.definition != act_con.definition {
                    findings.push(DriftFinding {
                        severity: DriftSeverity::Warning,
                        kind: "constraint_changed".to_owned(),
                        object,
                        detail: format!(
                            "Constraint {} on {table_qualified} changed ({}: {} -> {}: {})",
                            exp_con.name,
                            exp_con.kind,
                            exp_con.definition,
                            act_con.kind,
                            act_con.definition
                        ),
                    });
                }
            }
        }
    }

    for (name, act_con) in &act_map {
        if !exp_map.contains_key(name) {
            findings.push(DriftFinding {
                severity: DriftSeverity::Info,
                kind: "constraint_added".to_owned(),
                object: format!("{table_qualified}.{name}"),
                detail: format!(
                    "Constraint {} ({}) was added to {table_qualified}",
                    act_con.name, act_con.kind
                ),
            });
        }
    }
}

fn diff_table_indexes(
    table_qualified: &str,
    expected: &[IndexSnapshot],
    actual: &[IndexSnapshot],
    findings: &mut Vec<DriftFinding>,
) {
    let exp_map: BTreeMap<&str, &IndexSnapshot> =
        expected.iter().map(|i| (i.name.as_str(), i)).collect();
    let act_map: BTreeMap<&str, &IndexSnapshot> =
        actual.iter().map(|i| (i.name.as_str(), i)).collect();

    for (name, exp_idx) in &exp_map {
        let object = format!("{table_qualified}.{name}");
        match act_map.get(name) {
            None => {
                findings.push(DriftFinding {
                    severity: DriftSeverity::Warning,
                    kind: "index_dropped".to_owned(),
                    object,
                    detail: format!("Index {} was dropped from {table_qualified}", exp_idx.name),
                });
            }
            Some(act_idx) => {
                if exp_idx.is_unique != act_idx.is_unique
                    || exp_idx.is_primary != act_idx.is_primary
                    || exp_idx.definition != act_idx.definition
                {
                    findings.push(DriftFinding {
                        severity: DriftSeverity::Warning,
                        kind: "index_changed".to_owned(),
                        object,
                        detail: format!(
                            "Index {} on {table_qualified} changed definition or uniqueness",
                            exp_idx.name
                        ),
                    });
                }
            }
        }
    }

    for (name, act_idx) in &act_map {
        if !exp_map.contains_key(name) {
            findings.push(DriftFinding {
                severity: DriftSeverity::Info,
                kind: "index_added".to_owned(),
                object: format!("{table_qualified}.{name}"),
                detail: format!("Index {} was added to {table_qualified}", act_idx.name),
            });
        }
    }
}

fn diff_table_rls(
    table_qualified: &str,
    expected: &TableSnapshot,
    actual: &TableSnapshot,
    findings: &mut Vec<DriftFinding>,
) {
    if expected.rls_enabled && !actual.rls_enabled {
        findings.push(DriftFinding {
            severity: DriftSeverity::Breaking,
            kind: "rls_disabled".to_owned(),
            object: table_qualified.to_owned(),
            detail: format!("Row-level security was disabled on {table_qualified}"),
        });
    } else if !expected.rls_enabled && actual.rls_enabled {
        findings.push(DriftFinding {
            severity: DriftSeverity::Info,
            kind: "rls_enabled".to_owned(),
            object: table_qualified.to_owned(),
            detail: format!("Row-level security was enabled on {table_qualified}"),
        });
    }

    if expected.rls_forced && !actual.rls_forced {
        findings.push(DriftFinding {
            severity: DriftSeverity::Warning,
            kind: "rls_force_disabled".to_owned(),
            object: table_qualified.to_owned(),
            detail: format!("FORCE ROW LEVEL SECURITY was disabled on {table_qualified}"),
        });
    } else if !expected.rls_forced && actual.rls_forced {
        findings.push(DriftFinding {
            severity: DriftSeverity::Info,
            kind: "rls_force_enabled".to_owned(),
            object: table_qualified.to_owned(),
            detail: format!("FORCE ROW LEVEL SECURITY was enabled on {table_qualified}"),
        });
    }
}

fn diff_table_policies(
    table_qualified: &str,
    expected: &[PolicySnapshot],
    actual: &[PolicySnapshot],
    findings: &mut Vec<DriftFinding>,
) {
    let exp_map: BTreeMap<&str, &PolicySnapshot> =
        expected.iter().map(|p| (p.name.as_str(), p)).collect();
    let act_map: BTreeMap<&str, &PolicySnapshot> =
        actual.iter().map(|p| (p.name.as_str(), p)).collect();

    for (name, exp_pol) in &exp_map {
        let object = format!("{table_qualified}.{name}");
        match act_map.get(name) {
            None => {
                findings.push(DriftFinding {
                    severity: DriftSeverity::Breaking,
                    kind: "policy_dropped".to_owned(),
                    object,
                    detail: format!(
                        "RLS policy {} ({}) was dropped from {table_qualified}",
                        exp_pol.name, exp_pol.cmd
                    ),
                });
            }
            Some(act_pol) => {
                if exp_pol != act_pol {
                    findings.push(DriftFinding {
                        severity: DriftSeverity::Warning,
                        kind: "policy_changed".to_owned(),
                        object,
                        detail: format!(
                            "RLS policy {} on {table_qualified} changed (cmd={} permissive={})",
                            act_pol.name, act_pol.cmd, act_pol.permissive
                        ),
                    });
                }
            }
        }
    }

    for (name, act_pol) in &act_map {
        if !exp_map.contains_key(name) {
            findings.push(DriftFinding {
                severity: DriftSeverity::Info,
                kind: "policy_added".to_owned(),
                object: format!("{table_qualified}.{name}"),
                detail: format!(
                    "RLS policy {} ({}) was added to {table_qualified}",
                    act_pol.name, act_pol.cmd
                ),
            });
        }
    }
}

fn diff_views(
    expected: &[ViewSnapshot],
    actual: &[ViewSnapshot],
    findings: &mut Vec<DriftFinding>,
) {
    let exp_map: BTreeMap<(&str, &str), &ViewSnapshot> = expected
        .iter()
        .map(|v| ((v.schema.as_str(), v.name.as_str()), v))
        .collect();
    let act_map: BTreeMap<(&str, &str), &ViewSnapshot> = actual
        .iter()
        .map(|v| ((v.schema.as_str(), v.name.as_str()), v))
        .collect();

    for (key, exp_view) in &exp_map {
        let qualified = exp_view.qualified_name();
        let kind_label = view_kind_label(exp_view.materialized);
        match act_map.get(key) {
            None => {
                findings.push(DriftFinding {
                    severity: DriftSeverity::Breaking,
                    kind: "view_dropped".to_owned(),
                    object: qualified.clone(),
                    detail: format!("{kind_label} {qualified} was dropped"),
                });
            }
            Some(act_view) => {
                if exp_view.materialized != act_view.materialized {
                    findings.push(DriftFinding {
                        severity: DriftSeverity::Breaking,
                        kind: "view_kind_changed".to_owned(),
                        object: qualified.clone(),
                        detail: format!(
                            "Relation {qualified} changed from {} to {}",
                            view_kind_label(exp_view.materialized),
                            view_kind_label(act_view.materialized)
                        ),
                    });
                }
                if exp_view.definition_sha256 != act_view.definition_sha256 {
                    findings.push(DriftFinding {
                        severity: DriftSeverity::Warning,
                        kind: "view_definition_changed".to_owned(),
                        object: qualified.clone(),
                        detail: format!("{kind_label} {qualified} definition SHA-256 changed"),
                    });
                }
            }
        }
    }

    for (key, act_view) in &act_map {
        if !exp_map.contains_key(key) {
            let qualified = act_view.qualified_name();
            findings.push(DriftFinding {
                severity: DriftSeverity::Info,
                kind: "view_added".to_owned(),
                object: qualified.clone(),
                detail: format!(
                    "{} {qualified} was added",
                    view_kind_label(act_view.materialized)
                ),
            });
        }
    }
}

fn diff_sequences(
    expected: &[SequenceSnapshot],
    actual: &[SequenceSnapshot],
    findings: &mut Vec<DriftFinding>,
) {
    let exp_map: BTreeMap<(&str, &str), &SequenceSnapshot> = expected
        .iter()
        .map(|s| ((s.schema.as_str(), s.name.as_str()), s))
        .collect();
    let act_map: BTreeMap<(&str, &str), &SequenceSnapshot> = actual
        .iter()
        .map(|s| ((s.schema.as_str(), s.name.as_str()), s))
        .collect();

    for (key, exp_seq) in &exp_map {
        let qualified = exp_seq.qualified_name();
        match act_map.get(key) {
            None => {
                findings.push(DriftFinding {
                    severity: DriftSeverity::Breaking,
                    kind: "sequence_dropped".to_owned(),
                    object: qualified.clone(),
                    detail: format!("Sequence {qualified} was dropped"),
                });
            }
            Some(act_seq) => {
                if exp_seq.data_type != act_seq.data_type {
                    findings.push(DriftFinding {
                        severity: DriftSeverity::Breaking,
                        kind: "sequence_type_changed".to_owned(),
                        object: qualified.clone(),
                        detail: format!(
                            "Sequence {qualified} data type changed from {} to {}",
                            exp_seq.data_type, act_seq.data_type
                        ),
                    });
                }
                if exp_seq.start_value != act_seq.start_value
                    || exp_seq.increment_by != act_seq.increment_by
                    || exp_seq.cycle != act_seq.cycle
                {
                    findings.push(DriftFinding {
                        severity: DriftSeverity::Warning,
                        kind: "sequence_changed".to_owned(),
                        object: qualified.clone(),
                        detail: format!(
                            "Sequence {qualified} options changed (start={}->{}, increment={}->{}, cycle={}->{})",
                            exp_seq.start_value,
                            act_seq.start_value,
                            exp_seq.increment_by,
                            act_seq.increment_by,
                            exp_seq.cycle,
                            act_seq.cycle,
                        ),
                    });
                }
            }
        }
    }

    for (key, act_seq) in &act_map {
        if !exp_map.contains_key(key) {
            let qualified = act_seq.qualified_name();
            findings.push(DriftFinding {
                severity: DriftSeverity::Info,
                kind: "sequence_added".to_owned(),
                object: qualified.clone(),
                detail: format!("Sequence {qualified} ({}) was added", act_seq.data_type),
            });
        }
    }
}

fn diff_enums(
    expected: &[EnumSnapshot],
    actual: &[EnumSnapshot],
    findings: &mut Vec<DriftFinding>,
) {
    let exp_map: BTreeMap<(&str, &str), &EnumSnapshot> = expected
        .iter()
        .map(|e| ((e.schema.as_str(), e.name.as_str()), e))
        .collect();
    let act_map: BTreeMap<(&str, &str), &EnumSnapshot> = actual
        .iter()
        .map(|e| ((e.schema.as_str(), e.name.as_str()), e))
        .collect();

    for (key, exp_enum) in &exp_map {
        let qualified = exp_enum.qualified_name();
        match act_map.get(key) {
            None => {
                findings.push(DriftFinding {
                    severity: DriftSeverity::Breaking,
                    kind: "enum_dropped".to_owned(),
                    object: qualified.clone(),
                    detail: format!("Enum type {qualified} was dropped"),
                });
            }
            Some(act_enum) => {
                let exp_set: BTreeSet<&str> = exp_enum.labels.iter().map(String::as_str).collect();
                let act_set: BTreeSet<&str> = act_enum.labels.iter().map(String::as_str).collect();

                for label in &exp_enum.labels {
                    if !act_set.contains(label.as_str()) {
                        findings.push(DriftFinding {
                            severity: DriftSeverity::Warning,
                            kind: "enum_label_removed".to_owned(),
                            object: qualified.clone(),
                            detail: format!("Enum label '{label}' was removed from {qualified}"),
                        });
                    }
                }
                for label in &act_enum.labels {
                    if !exp_set.contains(label.as_str()) {
                        findings.push(DriftFinding {
                            severity: DriftSeverity::Info,
                            kind: "enum_label_added".to_owned(),
                            object: qualified.clone(),
                            detail: format!("Enum label '{label}' was added to {qualified}"),
                        });
                    }
                }

                let common_exp: Vec<&str> = exp_enum
                    .labels
                    .iter()
                    .map(String::as_str)
                    .filter(|lbl| act_set.contains(lbl))
                    .collect();
                let common_act: Vec<&str> = act_enum
                    .labels
                    .iter()
                    .map(String::as_str)
                    .filter(|lbl| exp_set.contains(lbl))
                    .collect();
                if common_exp != common_act {
                    findings.push(DriftFinding {
                        severity: DriftSeverity::Warning,
                        kind: "enum_labels_reordered".to_owned(),
                        object: qualified,
                        detail: "Existing enum labels changed relative sort order".to_owned(),
                    });
                }
            }
        }
    }

    for (key, act_enum) in &act_map {
        if !exp_map.contains_key(key) {
            let qualified = act_enum.qualified_name();
            findings.push(DriftFinding {
                severity: DriftSeverity::Info,
                kind: "enum_added".to_owned(),
                object: qualified.clone(),
                detail: format!(
                    "Enum type {qualified} was added ({} label(s))",
                    act_enum.labels.len()
                ),
            });
        }
    }
}

const fn nullability_label(nullable: bool) -> &'static str {
    if nullable { "NULL" } else { "NOT NULL" }
}

const fn view_kind_label(materialized: bool) -> &'static str {
    if materialized {
        "Materialized view"
    } else {
        "View"
    }
}

fn json_nesting_within_bound(json: &str) -> bool {
    let mut depth = 0usize;
    let mut quoted = false;
    let mut escaped = false;
    for ch in json.chars() {
        if quoted {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                quoted = false;
            }
            continue;
        }
        match ch {
            '"' => quoted = true,
            '{' | '[' => {
                depth += 1;
                if depth > MAX_JSON_NESTING {
                    return false;
                }
            }
            '}' | ']' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    !quoted
}

fn sanitize_ascii(input: &str) -> String {
    let safe = sanitize_for_display(input);
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
