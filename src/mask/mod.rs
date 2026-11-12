//! Pure data masking and anonymization rules domain model.
//!
//! This module implements the domain logic for Feature
//! `038-data-masking-and-anonymization-rules` (`Milestone 8: Cross-Environment Data Synchronization, Masking & Seeding`).
//! It contains zero terminal, filesystem, or network dependencies: [`MaskReport`]
//! is populated from catalog inspection in [`crate::postgres::metadata`],
//! evaluated with PII heuristics, and formatted deterministically into aligned
//! tabular overviews, structured JSON, or executable masking SQL views.

use serde::{Deserialize, Serialize};
use std::fmt::Write as _;

#[cfg(test)]
mod tests;

/// Category of sensitive personally identifiable information (PII).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PiiType {
    /// Email addresses (`email`, `user_email`).
    Email,
    /// Telephone numbers (`phone`, `mobile`, `cell`).
    Phone,
    /// Personal names (`first_name`, `last_name`, `full_name`).
    PersonName,
    /// Financial attributes (`credit_card`, `iban`, `account_number`).
    Financial,
    /// Government or national identification numbers (`ssn`, `tax_id`, `passport`).
    NationalId,
    /// Physical location or postal addresses (`address`, `street`, `zipcode`).
    Address,
    /// Credentials, tokens, or encryption keys (`password`, `token`, `secret`).
    Secret,
    /// General sensitive or confidential information.
    GenericSensitive,
}

impl PiiType {
    /// Inferred label displayed in reports and overlays.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Email => "EMAIL",
            Self::Phone => "PHONE",
            Self::PersonName => "NAME",
            Self::Financial => "FINANCIAL",
            Self::NationalId => "NATIONAL_ID",
            Self::Address => "ADDRESS",
            Self::Secret => "SECRET",
            Self::GenericSensitive => "SENSITIVE",
        }
    }

    /// Infers PII type from a column name using naming heuristics.
    #[must_use]
    pub fn infer_from_name(column_name: &str) -> Option<Self> {
        let name = column_name.trim().to_ascii_lowercase();
        let parts: Vec<&str> = name.split(['_', '-']).collect();

        if parts.contains(&"email") || name.ends_with("email") {
            return Some(Self::Email);
        }
        if parts.contains(&"phone")
            || parts.contains(&"mobile")
            || parts.contains(&"telephone")
            || parts.contains(&"cell")
        {
            return Some(Self::Phone);
        }
        if parts.contains(&"ssn")
            || name.contains("ssn")
            || name.contains("tax_id")
            || name.contains("taxid")
            || name.contains("national_id")
            || name.contains("nationalid")
            || name.contains("passport")
        {
            return Some(Self::NationalId);
        }
        if name.contains("first_name")
            || name.contains("firstname")
            || name.contains("last_name")
            || name.contains("lastname")
            || name.contains("surname")
            || name.contains("full_name")
            || name.contains("fullname")
            || name == "fname"
            || name == "lname"
        {
            return Some(Self::PersonName);
        }
        if name.contains("card")
            || name.contains("credit")
            || name.contains("iban")
            || name.contains("account_no")
            || name.contains("account_num")
            || name.contains("account_number")
            || name.contains("cvv")
            || name.contains("cvc")
        {
            return Some(Self::Financial);
        }
        if parts.contains(&"address")
            || name.contains("address")
            || name.contains("street")
            || name.contains("zipcode")
            || name.contains("zip_code")
            || name.contains("postcode")
            || name.contains("post_code")
            || name.contains("postalcode")
            || name.contains("postal_code")
            || parts.contains(&"city")
        {
            return Some(Self::Address);
        }
        if name.contains("password")
            || name.contains("secret")
            || name.contains("token")
            || name.contains("api_key")
            || name.contains("apikey")
            || name.contains("auth")
        {
            return Some(Self::Secret);
        }

        None
    }

    /// Default recommended masking rule for this PII category.
    #[must_use]
    pub fn default_rule(self) -> MaskRule {
        match self {
            Self::Email => MaskRule::Email,
            Self::Phone => MaskRule::Phone,
            Self::PersonName => MaskRule::FullName,
            Self::Financial => MaskRule::Redact {
                text: "****-****-****-XXXX".to_string(),
            },
            Self::NationalId => MaskRule::Redact {
                text: "***-**-XXXX".to_string(),
            },
            Self::Address => MaskRule::Redact {
                text: "[ANONYMIZED ADDRESS]".to_string(),
            },
            Self::Secret => MaskRule::Nullify,
            Self::GenericSensitive => MaskRule::HashHex {
                salt: "ignatius_salt".to_string(),
            },
        }
    }
}

/// Deterministic transformation rule for anonymizing a database column.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MaskRule {
    /// Preserves domain or prefix while masking username: `us***@example.com`.
    Email,
    /// Formats as safe dummy telephone number: `+1-555-XXXX`.
    Phone,
    /// Replaces with generic pseudonym: `User <hash>`.
    FullName,
    /// Computes deterministic hexadecimal MD5/SHA hash with salt: `MD5(col || salt)`.
    HashHex {
        /// Deterministic secret salt.
        salt: String,
    },
    /// Replaces raw value with static text literal.
    Redact {
        /// Masked replacement text.
        text: String,
    },
    /// Replaces value with `NULL`.
    Nullify,
    /// Randomizes numeric value within range: `FLOOR(min + RANDOM() * (max - min + 1))`.
    RandomNumber {
        /// Minimum bound.
        min: i64,
        /// Maximum bound.
        max: i64,
    },
    /// Custom user-specified SQL expression.
    CustomSql(String),
}

impl MaskRule {
    /// Concise human-readable name of this rule.
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::Email => "EMAIL_MASK",
            Self::Phone => "PHONE_MASK",
            Self::FullName => "PSEUDONYM",
            Self::HashHex { .. } => "HASH_HEX",
            Self::Redact { .. } => "REDACT",
            Self::Nullify => "NULLIFY",
            Self::RandomNumber { .. } => "RAND_NUM",
            Self::CustomSql(_) => "CUSTOM_SQL",
        }
    }

    /// Synthesizes safe PostgreSQL SQL expression transforming the source column.
    #[must_use]
    pub fn to_sql_expression(&self, column_name: &str) -> String {
        let col = format!("\"{}\"", column_name.replace('"', "\"\""));
        match self {
            Self::Email => {
                format!(
                    "CASE WHEN {col} IS NULL THEN NULL ELSE REGEXP_REPLACE({col}::text, '^(.{{2}}).*(@.*)$', '\\1***\\2') END"
                )
            }
            Self::Phone => {
                format!(
                    "CASE WHEN {col} IS NULL THEN NULL ELSE '+1-555-' || LPAD((ABS(('x' || SUBSTRING(MD5({col}::text) FROM 1 FOR 4))::bit(16)::int) % 10000)::text, 4, '0') END"
                )
            }
            Self::FullName => {
                format!(
                    "CASE WHEN {col} IS NULL THEN NULL ELSE 'User ' || UPPER(SUBSTRING(MD5({col}::text) FROM 1 FOR 6)) END"
                )
            }
            Self::HashHex { salt } => {
                let escaped_salt = salt.replace('\'', "''");
                format!(
                    "CASE WHEN {col} IS NULL THEN NULL ELSE MD5({col}::text || '{escaped_salt}') END"
                )
            }
            Self::Redact { text } => {
                let escaped_text = text.replace('\'', "''");
                format!("'{escaped_text}'")
            }
            Self::Nullify => "NULL".to_string(),
            Self::RandomNumber { min, max } => {
                format!(
                    "CASE WHEN {col} IS NULL THEN NULL ELSE FLOOR({min} + RANDOM() * ({max} - {min} + 1))::bigint END"
                )
            }
            Self::CustomSql(expr) => expr.clone(),
        }
    }
}

/// Discovered column candidate carrying sensitive information.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PiiCandidate {
    /// Schema name.
    pub schema: String,
    /// Table name.
    pub table: String,
    /// Column name.
    pub column: String,
    /// Inferred or catalog data type.
    pub data_type: String,
    /// Detected PII category.
    pub pii_type: PiiType,
    /// Assigned masking rule.
    pub rule: MaskRule,
    /// Heuristic confidence score (0 to 100).
    pub confidence: u8,
}

/// Masking policy configuration for a single database table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableMaskPolicy {
    /// Schema name.
    pub schema: String,
    /// Table name.
    pub table: String,
    /// All columns belonging to the table.
    #[serde(default)]
    pub all_columns: Vec<String>,
    /// Column masking rules (column_name, rule).
    pub column_rules: Vec<(String, MaskRule)>,
}

impl TableMaskPolicy {
    /// Generates a `CREATE OR REPLACE VIEW` statement masking specified columns.
    #[must_use]
    pub fn generate_view_sql(&self, view_prefix: &str, all_columns: &[String]) -> String {
        let view_name = format!("\"{view_prefix}{}\"", self.table.replace('"', "\"\""));
        let source_table = format!(
            "\"{}\".\"{}\"",
            self.schema.replace('"', "\"\""),
            self.table.replace('"', "\"\"")
        );

        let mut out = String::new();
        let _ = writeln!(out, "CREATE OR REPLACE VIEW {view_name} AS");
        let _ = writeln!(out, "SELECT");

        for (i, col) in all_columns.iter().enumerate() {
            let is_last = i == all_columns.len().saturating_sub(1);
            let comma = if is_last { "" } else { "," };

            if let Some((_, rule)) = self.column_rules.iter().find(|(c, _)| c == col) {
                let expr = rule.to_sql_expression(col);
                let _ = writeln!(out, "    {expr} AS \"{}\"{comma}", col.replace('"', "\"\""));
            } else {
                let _ = writeln!(out, "    \"{}\"{comma}", col.replace('"', "\"\""));
            }
        }

        let _ = writeln!(out, "FROM {source_table};");
        out
    }
}

/// Complete report of discovered PII candidates, active rules, and generated views.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct MaskReport {
    /// Discovered sensitive column candidates.
    pub candidates: Vec<PiiCandidate>,
    /// Configured table policies.
    pub policies: Vec<TableMaskPolicy>,
}

impl MaskReport {
    /// Total count of discovered PII candidates.
    #[must_use]
    pub fn total_candidates(&self) -> usize {
        self.candidates.len()
    }

    /// Renders an aligned tabular overview of discovered PII candidates.
    #[must_use]
    pub fn render_table(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "=== Ignatius Data Masking & PII Forensics ===");
        let _ = writeln!(
            out,
            "Total Sensitive Columns: {} | Configured Policies: {}",
            self.candidates.len(),
            self.policies.len()
        );
        let _ = writeln!(out);

        if self.candidates.is_empty() {
            let _ = writeln!(out, "No sensitive PII columns detected.");
            return out;
        }

        let _ = writeln!(
            out,
            "{:<16} {:<20} {:<20} {:<14} {:<14} {:<14}",
            "SCHEMA", "TABLE", "COLUMN", "TYPE", "PII CATEGORY", "ASSIGNED RULE"
        );
        let _ = writeln!(out, "{}", "-".repeat(102));

        for c in &self.candidates {
            let _ = writeln!(
                out,
                "{:<16} {:<20} {:<20} {:<14} {:<14} {:<14}",
                truncate_str(&c.schema, 16),
                truncate_str(&c.table, 20),
                truncate_str(&c.column, 20),
                truncate_str(&c.data_type, 14),
                c.pii_type.label(),
                c.rule.name()
            );
        }
        let _ = writeln!(out);

        out
    }

    /// Renders JSON serialization of candidates and policies.
    #[must_use]
    pub fn render_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string())
    }

    /// Renders plain text output.
    #[must_use]
    pub fn render_plain(&self) -> String {
        self.render_table()
    }

    /// Generates executable SQL script defining all masked views.
    #[must_use]
    pub fn render_sql(&self, view_prefix: &str) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "-- Ignatius Data Masking View Definitions");
        let _ = writeln!(
            out,
            "-- Generated automatically with deterministic pseudonymization"
        );
        let _ = writeln!(out);

        for policy in &self.policies {
            // Use table's all_columns if populated, otherwise reconstruct candidate columns
            let cols = if policy.all_columns.is_empty() {
                self.candidates
                    .iter()
                    .filter(|c| c.schema == policy.schema && c.table == policy.table)
                    .map(|c| c.column.clone())
                    .collect::<Vec<_>>()
            } else {
                policy.all_columns.clone()
            };

            let view_sql = policy.generate_view_sql(view_prefix, &cols);
            out.push_str(&view_sql);
            let _ = writeln!(out);
        }

        out
    }
}

/// Truncates string with ellipsis if exceeding max length.
pub(crate) fn truncate_str(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_len.saturating_sub(3)).collect();
        format!("{truncated}...")
    }
}
