//! Pure DDL script synthesis and bundle formatting (`027-deep-catalog-and-ddl-generator`).
//!
//! This module implements the pure domain types for `M6: Full pgAdmin Replacement Parity`
//! Feature `027-deep-catalog-and-ddl-generator`. It depends on no network, terminal, or
//! filesystem I/O: [`DdlBundle`] and [`DdlObject`] represent structured `CREATE`,
//! `ALTER`, `DROP`, and `COMMENT` statements extracted from `pg_catalog` and render them
//! into deterministic, executable PostgreSQL scripts.

use std::fmt::Write as _;

use crate::postgres::metadata::{ObjectKind, quote_identifier};

/// Maximum number of objects emitted in a single schema-wide DDL bundle before truncation.
pub const MAX_DDL_OBJECTS: usize = 500;

/// Kind of catalog object represented in a [`DdlObject`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DdlCategory {
    /// Custom `ENUM`, `DOMAIN`, or composite `TYPE`.
    Type,
    /// `SEQUENCE` object.
    Sequence,
    /// Regular, partitioned, or foreign `TABLE`.
    Table,
    /// Standard `VIEW` or `MATERIALIZED VIEW`.
    View,
    /// `FUNCTION` or `PROCEDURE`.
    Routine,
    /// Standalone `INDEX`.
    Index,
    /// Installed PostgreSQL `EXTENSION`.
    Extension,
}

impl DdlCategory {
    /// Human-readable section heading for multi-object DDL scripts.
    #[must_use]
    pub const fn heading(self) -> &'static str {
        match self {
            Self::Type => "Types and Enums",
            Self::Sequence => "Sequences",
            Self::Table => "Tables, Constraints, Indexes, and Triggers",
            Self::View => "Views and Materialized Views",
            Self::Routine => "Functions and Procedures",
            Self::Index => "Indexes",
            Self::Extension => "Extensions",
        }
    }
}

/// Deterministic DDL representation for one PostgreSQL catalog object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DdlObject {
    /// Topological category used when ordering a schema-wide DDL script.
    pub category: DdlCategory,
    /// Underlying catalog object kind when mapped to [`ObjectKind`].
    pub kind: ObjectKind,
    /// Schema name (`public`, `reporting`, etc.).
    pub schema: String,
    /// Object name.
    pub name: String,
    /// Optional `DROP ... IF EXISTS ... CASCADE;` statement.
    pub drop_statement: String,
    /// Primary `CREATE ...;` statement.
    pub create_statement: String,
    /// Dependent `ALTER TABLE`, `CREATE INDEX`, `CREATE TRIGGER`, or `COMMENT ON` statements.
    pub dependent_statements: Vec<String>,
}

impl DdlObject {
    /// Fully qualified `"schema"."name"` identifier.
    #[must_use]
    pub fn qualified_name(&self) -> String {
        format!(
            "{}.{}",
            quote_identifier(&self.schema),
            quote_identifier(&self.name)
        )
    }

    /// Renders this single object's complete SQL script.
    #[must_use]
    pub fn render_sql(&self, include_drop: bool) -> String {
        let mut out = String::new();
        if include_drop && !self.drop_statement.trim().is_empty() {
            let trimmed = self.drop_statement.trim_end();
            out.push_str(trimmed);
            if !trimmed.ends_with(';') {
                out.push(';');
            }
            out.push_str("\n\n");
        }
        let create = self.create_statement.trim_end();
        out.push_str(create);
        if !create.ends_with(';') {
            out.push(';');
        }
        out.push('\n');

        if !self.dependent_statements.is_empty() {
            out.push('\n');
            for stmt in &self.dependent_statements {
                let trimmed = stmt.trim_end();
                if trimmed.is_empty() {
                    continue;
                }
                out.push_str(trimmed);
                if !trimmed.ends_with(';') {
                    out.push(';');
                }
                out.push('\n');
            }
        }
        out
    }
}

/// Collection of [`DdlObject`] definitions for a single object or an entire schema.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DdlBundle {
    /// Schema inspected (`public`, etc.).
    pub schema: String,
    /// Specific object filter if requested (`Some("orders")`).
    pub target_object: Option<String>,
    /// Objects in deterministic dependency order.
    pub objects: Vec<DdlObject>,
    /// Set if the schema contained more than [`MAX_DDL_OBJECTS`] objects.
    pub truncated_from: Option<usize>,
}

impl DdlBundle {
    /// Sorts and bounds the bundle at [`MAX_DDL_OBJECTS`].
    #[must_use]
    pub fn bounded(mut self) -> Self {
        self.objects.sort_by(|a, b| {
            (a.category, &a.schema, &a.name).cmp(&(b.category, &b.schema, &b.name))
        });
        let total = self.objects.len();
        if total > MAX_DDL_OBJECTS {
            self.objects.truncate(MAX_DDL_OBJECTS);
            self.truncated_from = Some(total);
        }
        self
    }

    /// Renders the complete SQL DDL script for all objects in the bundle.
    #[must_use]
    pub fn render_sql(&self, include_drop: bool) -> String {
        if self.objects.is_empty() {
            return format!(
                "-- No matching objects found in schema {}.\n",
                quote_identifier(&self.schema)
            );
        }

        if self.objects.len() == 1 && self.target_object.is_some() {
            return self.objects[0].render_sql(include_drop);
        }

        let mut out = String::new();
        let _ = writeln!(
            out,
            "-- Ignatius DDL Script for schema {} ({} object{})",
            quote_identifier(&self.schema),
            self.objects.len(),
            if self.objects.len() == 1 { "" } else { "s" }
        );
        if let Some(total) = self.truncated_from {
            let _ = writeln!(
                out,
                "-- Note: truncated to {} of {} objects.",
                MAX_DDL_OBJECTS, total
            );
        }
        out.push('\n');

        if include_drop {
            out.push_str("-- Drop existing objects in reverse dependency order\n");
            for obj in self.objects.iter().rev() {
                let drop_stmt = obj.drop_statement.trim_end();
                if !drop_stmt.is_empty() {
                    out.push_str(drop_stmt);
                    if !drop_stmt.ends_with(';') {
                        out.push(';');
                    }
                    out.push('\n');
                }
            }
            out.push('\n');
        }

        let mut current_category: Option<DdlCategory> = None;
        for (idx, obj) in self.objects.iter().enumerate() {
            if current_category != Some(obj.category) {
                if idx > 0 {
                    out.push('\n');
                }
                let _ = writeln!(out, "-- == {} ==", obj.category.heading());
                current_category = Some(obj.category);
            } else if idx > 0 {
                out.push('\n');
            }
            out.push_str(&obj.render_sql(false));
        }
        out
    }
}

/// Synthesizes a canonical `DROP ... IF EXISTS ... CASCADE;` statement for an [`ObjectKind`].
#[must_use]
pub fn canonical_drop_statement(
    kind: ObjectKind,
    schema: &str,
    name: &str,
    identity_args: Option<&str>,
) -> String {
    let qualified = format!("{}.{}", quote_identifier(schema), quote_identifier(name));
    match kind {
        ObjectKind::Table | ObjectKind::PartitionedTable => {
            format!("DROP TABLE IF EXISTS {qualified} CASCADE;")
        }
        ObjectKind::View => format!("DROP VIEW IF EXISTS {qualified} CASCADE;"),
        ObjectKind::MaterializedView => {
            format!("DROP MATERIALIZED VIEW IF EXISTS {qualified} CASCADE;")
        }
        ObjectKind::Sequence => format!("DROP SEQUENCE IF EXISTS {qualified} CASCADE;"),
        ObjectKind::ForeignTable => format!("DROP FOREIGN TABLE IF EXISTS {qualified} CASCADE;"),
        ObjectKind::Function => {
            if let Some(args) = identity_args {
                format!("DROP FUNCTION IF EXISTS {qualified}({args}) CASCADE;")
            } else {
                format!("DROP FUNCTION IF EXISTS {qualified} CASCADE;")
            }
        }
        ObjectKind::Index => format!("DROP INDEX IF EXISTS {qualified} CASCADE;"),
        ObjectKind::Extension => {
            format!(
                "DROP EXTENSION IF EXISTS {} CASCADE;",
                quote_identifier(name)
            )
        }
        ObjectKind::Column => format!("-- Column {qualified} cannot be dropped standalone;"),
    }
}

/// Specification of `pg_sequence` attributes for [`format_sequence_ddl`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SequenceDdlSpec<'a> {
    /// Schema name.
    pub schema: &'a str,
    /// Sequence name.
    pub name: &'a str,
    /// Sequence integer data type (`smallint`, `integer`, `bigint`).
    pub data_type: &'a str,
    /// Initial value (`START WITH`).
    pub start_value: i64,
    /// Step size (`INCREMENT BY`).
    pub increment_by: i64,
    /// Minimum value (`MINVALUE`).
    pub min_value: i64,
    /// Maximum value (`MAXVALUE`).
    pub max_value: i64,
    /// Preallocated cache count (`CACHE`).
    pub cache_size: i64,
    /// Whether the sequence wraps around (`CYCLE` vs `NO CYCLE`).
    pub is_cycled: bool,
}

/// Synthesizes a `CREATE SEQUENCE` statement from `pg_sequence` attributes.
#[must_use]
pub fn format_sequence_ddl(spec: SequenceDdlSpec<'_>) -> String {
    let cycle_clause = if spec.is_cycled { "CYCLE" } else { "NO CYCLE" };
    format!(
        "CREATE SEQUENCE {}.{}\n    AS {}\n    START WITH {}\n    INCREMENT BY {}\n    MINVALUE {}\n    MAXVALUE {}\n    CACHE {}\n    {cycle_clause};",
        quote_identifier(spec.schema),
        quote_identifier(spec.name),
        spec.data_type,
        spec.start_value,
        spec.increment_by,
        spec.min_value,
        spec.max_value,
        spec.cache_size,
    )
}

/// Synthesizes a `CREATE TYPE ... AS ENUM (...)` statement from enum labels.
#[must_use]
pub fn format_enum_ddl(schema: &str, name: &str, labels: &[String]) -> String {
    let quoted_labels: Vec<String> = labels
        .iter()
        .map(|label| format!("'{}'", label.replace('\'', "''")))
        .collect();
    format!(
        "CREATE TYPE {}.{} AS ENUM ({});",
        quote_identifier(schema),
        quote_identifier(name),
        quoted_labels.join(", ")
    )
}

#[cfg(test)]
mod tests;
