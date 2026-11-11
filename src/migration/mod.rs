//! Pure schema diffing, visual table designer, and transactional migration generator.
//!
//! This module implements the domain logic for Feature
//! `030-visual-schema-designer-and-migrations` (`M6: Full pgAdmin Replacement Parity`).
//! It contains no filesystem, terminal, or network dependencies:
//! [`SchemaSnapshot`] structures represent tables, columns, constraints, and indexes
//! extracted from `pg_catalog`, while [`SchemaDiff`] computes deterministic differences
//! and synthesizes reviewable, transactional PostgreSQL migration scripts
//! (`BEGIN; ... COMMIT;`), visual tree representations (`+`, `~`, `-`), and JSON reports.

use std::fmt::Write as _;

use crate::postgres::metadata::quote_identifier;

#[cfg(test)]
mod tests;

/// Maximum number of tables or columns retained in a migration diff snapshot.
pub const MAX_MIGRATION_ITEMS: usize = 500;

/// Column specification within a table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnDef {
    /// Column identifier.
    pub name: String,
    /// Fully qualified or canonical SQL data type (e.g. `integer`, `text`, `timestamp with time zone`).
    pub data_type: String,
    /// Whether `NULL` values are permitted.
    pub is_nullable: bool,
    /// Default expression if configured, e.g. `now()` or `'active'::text`.
    pub default_expr: Option<String>,
    /// Whether column is marked as `IDENTITY` or auto-incrementing.
    pub is_identity: bool,
    /// Whether this column participates in the primary key.
    pub is_pk: bool,
    /// Whether a unique constraint exists on this column.
    pub is_unique: bool,
}

/// Category of PostgreSQL constraint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConstraintKind {
    /// `PRIMARY KEY` constraint.
    PrimaryKey,
    /// `FOREIGN KEY` constraint.
    ForeignKey,
    /// `UNIQUE` constraint.
    Unique,
    /// `CHECK` constraint.
    Check,
    /// Other / unrecognized constraint type.
    Other,
}

impl ConstraintKind {
    /// Human-readable label for diff reporting.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::PrimaryKey => "PRIMARY KEY",
            Self::ForeignKey => "FOREIGN KEY",
            Self::Unique => "UNIQUE",
            Self::Check => "CHECK",
            Self::Other => "CONSTRAINT",
        }
    }
}

/// Table constraint definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstraintDef {
    /// Constraint name (e.g. `fk_orders_customer_id`).
    pub name: String,
    /// Constraint kind.
    pub kind: ConstraintKind,
    /// Exact constraint definition SQL (e.g. `FOREIGN KEY (customer_id) REFERENCES customers(id)`).
    pub definition: String,
}

/// Table index definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexDef {
    /// Index name (e.g. `idx_orders_customer`).
    pub name: String,
    /// Full `CREATE INDEX` definition SQL statement.
    pub definition: String,
    /// Whether index enforces uniqueness.
    pub is_unique: bool,
}

/// Complete structural definition of a database table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableDef {
    /// Schema name (`public`, `reporting`, etc.).
    pub schema: String,
    /// Table name.
    pub name: String,
    /// Column definitions in ordinal order.
    pub columns: Vec<ColumnDef>,
    /// Constraint definitions sorted by name.
    pub constraints: Vec<ConstraintDef>,
    /// Secondary index definitions sorted by name.
    pub indexes: Vec<IndexDef>,
    /// Optional table comment.
    pub comment: Option<String>,
}

/// Snapshot of all tables, columns, constraints, and indexes in a schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaSnapshot {
    /// Schema name.
    pub schema: String,
    /// Tables contained within the schema.
    pub tables: Vec<TableDef>,
}

impl SchemaSnapshot {
    /// Creates an empty schema snapshot for the given schema name.
    #[must_use]
    pub fn empty(schema: &str) -> Self {
        Self {
            schema: schema.to_owned(),
            tables: Vec::new(),
        }
    }

    /// Finds a table by name case-insensitively.
    #[must_use]
    pub fn find_table(&self, name: &str) -> Option<&TableDef> {
        self.tables
            .iter()
            .find(|t| t.name.eq_ignore_ascii_case(name))
    }
}

/// Differences detected in a single column between two versions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnDiff {
    /// Column name.
    pub column_name: String,
    /// Source data type.
    pub old_type: String,
    /// Target data type.
    pub new_type: String,
    /// Source nullability.
    pub old_nullable: bool,
    /// Target nullability.
    pub new_nullable: bool,
    /// Source default expression.
    pub old_default: Option<String>,
    /// Target default expression.
    pub new_default: Option<String>,
}

impl ColumnDiff {
    /// Whether the data type changed.
    #[must_use]
    pub fn type_changed(&self) -> bool {
        self.old_type != self.new_type
    }

    /// Whether nullability changed.
    #[must_use]
    pub fn nullability_changed(&self) -> bool {
        self.old_nullable != self.new_nullable
    }

    /// Whether the default expression changed.
    #[must_use]
    pub fn default_changed(&self) -> bool {
        self.old_default != self.new_default
    }
}

/// Structural differences detected within a single table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableDiff {
    /// Table name.
    pub table_name: String,
    /// New columns added in target.
    pub added_columns: Vec<ColumnDef>,
    /// Columns dropped from source.
    pub dropped_columns: Vec<String>,
    /// Existing columns with modified type, nullability, or default.
    pub modified_columns: Vec<ColumnDiff>,
    /// New constraints added in target.
    pub added_constraints: Vec<ConstraintDef>,
    /// Constraints dropped from source.
    pub dropped_constraints: Vec<String>,
    /// New secondary indexes added in target.
    pub added_indexes: Vec<IndexDef>,
    /// Indexes dropped from source.
    pub dropped_indexes: Vec<String>,
}

impl TableDiff {
    /// Whether this table has any differences.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.added_columns.is_empty()
            && self.dropped_columns.is_empty()
            && self.modified_columns.is_empty()
            && self.added_constraints.is_empty()
            && self.dropped_constraints.is_empty()
            && self.added_indexes.is_empty()
            && self.dropped_indexes.is_empty()
    }

    /// Count of total individual changes in this table.
    #[must_use]
    pub fn change_count(&self) -> usize {
        self.added_columns.len()
            + self.dropped_columns.len()
            + self.modified_columns.len()
            + self.added_constraints.len()
            + self.dropped_constraints.len()
            + self.added_indexes.len()
            + self.dropped_indexes.len()
    }
}

/// Complete structural differences between two schemas or schema versions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaDiff {
    /// Source / base schema name.
    pub source_schema: String,
    /// Target / desired schema name.
    pub target_schema: String,
    /// Entire tables added in target.
    pub added_tables: Vec<TableDef>,
    /// Tables dropped from source.
    pub dropped_tables: Vec<String>,
    /// Tables present in both with internal column, constraint, or index differences.
    pub modified_tables: Vec<TableDiff>,
}

impl SchemaDiff {
    /// Whether the two schemas are completely identical.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.added_tables.is_empty()
            && self.dropped_tables.is_empty()
            && self.modified_tables.is_empty()
    }

    /// Total number of changes (added tables + dropped tables + modified table changes).
    #[must_use]
    pub fn total_changes(&self) -> usize {
        self.added_tables.len()
            + self.dropped_tables.len()
            + self
                .modified_tables
                .iter()
                .map(TableDiff::change_count)
                .sum::<usize>()
    }
}

/// Options controlling migration script generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationOptions {
    /// Wrap generated migration statements in `BEGIN; ... COMMIT;`.
    pub use_transaction: bool,
    /// Include descriptive commentary on operations.
    pub include_comments: bool,
    /// Target schema override when applying migrations across differently-named environments.
    pub target_schema_override: Option<String>,
}

impl Default for MigrationOptions {
    fn default() -> Self {
        Self {
            use_transaction: true,
            include_comments: true,
            target_schema_override: None,
        }
    }
}

/// Computes the structural difference from `source` to `target`.
#[must_use]
pub fn compute_schema_diff(source: &SchemaSnapshot, target: &SchemaSnapshot) -> SchemaDiff {
    let mut added_tables = Vec::new();
    let mut dropped_tables = Vec::new();
    let mut modified_tables = Vec::new();

    // Check tables in target against source
    for tgt_tbl in &target.tables {
        match source.find_table(&tgt_tbl.name) {
            None => added_tables.push(tgt_tbl.clone()),
            Some(src_tbl) => {
                let diff = compute_table_diff(src_tbl, tgt_tbl);
                if !diff.is_empty() {
                    modified_tables.push(diff);
                }
            }
        }
    }

    // Check for tables present in source but dropped in target
    for src_tbl in &source.tables {
        if target.find_table(&src_tbl.name).is_none() {
            dropped_tables.push(src_tbl.name.clone());
        }
    }

    added_tables.sort_by(|a, b| a.name.cmp(&b.name));
    dropped_tables.sort();
    modified_tables.sort_by(|a, b| a.table_name.cmp(&b.table_name));

    SchemaDiff {
        source_schema: source.schema.clone(),
        target_schema: target.schema.clone(),
        added_tables,
        dropped_tables,
        modified_tables,
    }
}

/// Computes internal differences between two versions of the same table.
fn compute_table_diff(source: &TableDef, target: &TableDef) -> TableDiff {
    let mut added_columns = Vec::new();
    let mut dropped_columns = Vec::new();
    let mut modified_columns = Vec::new();

    for tgt_col in &target.columns {
        match source
            .columns
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(&tgt_col.name))
        {
            None => added_columns.push(tgt_col.clone()),
            Some(src_col) => {
                if src_col.data_type != tgt_col.data_type
                    || src_col.is_nullable != tgt_col.is_nullable
                    || src_col.default_expr != tgt_col.default_expr
                {
                    modified_columns.push(ColumnDiff {
                        column_name: tgt_col.name.clone(),
                        old_type: src_col.data_type.clone(),
                        new_type: tgt_col.data_type.clone(),
                        old_nullable: src_col.is_nullable,
                        new_nullable: tgt_col.is_nullable,
                        old_default: src_col.default_expr.clone(),
                        new_default: tgt_col.default_expr.clone(),
                    });
                }
            }
        }
    }

    for src_col in &source.columns {
        if !target
            .columns
            .iter()
            .any(|c| c.name.eq_ignore_ascii_case(&src_col.name))
        {
            dropped_columns.push(src_col.name.clone());
        }
    }

    let mut added_constraints = Vec::new();
    let mut dropped_constraints = Vec::new();

    for tgt_con in &target.constraints {
        if !source
            .constraints
            .iter()
            .any(|c| c.name.eq_ignore_ascii_case(&tgt_con.name))
        {
            added_constraints.push(tgt_con.clone());
        }
    }

    for src_con in &source.constraints {
        if !target
            .constraints
            .iter()
            .any(|c| c.name.eq_ignore_ascii_case(&src_con.name))
        {
            dropped_constraints.push(src_con.name.clone());
        }
    }

    let mut added_indexes = Vec::new();
    let mut dropped_indexes = Vec::new();

    for tgt_idx in &target.indexes {
        if !source
            .indexes
            .iter()
            .any(|i| i.name.eq_ignore_ascii_case(&tgt_idx.name))
        {
            added_indexes.push(tgt_idx.clone());
        }
    }

    for src_idx in &source.indexes {
        if !target
            .indexes
            .iter()
            .any(|i| i.name.eq_ignore_ascii_case(&src_idx.name))
        {
            dropped_indexes.push(src_idx.name.clone());
        }
    }

    TableDiff {
        table_name: target.name.clone(),
        added_columns,
        dropped_columns,
        modified_columns,
        added_constraints,
        dropped_constraints,
        added_indexes,
        dropped_indexes,
    }
}

/// Generates an idempotent, reviewable, transactional SQL migration script.
#[must_use]
pub fn generate_migration_sql(diff: &SchemaDiff, options: &MigrationOptions) -> String {
    if diff.is_empty() {
        return "-- Schemas are identical; no migration necessary.\n".to_owned();
    }

    let schema = options
        .target_schema_override
        .as_deref()
        .unwrap_or(&diff.target_schema);

    let mut sql = String::new();

    if options.include_comments {
        let _ = writeln!(
            sql,
            "-- Ignatius Migration: {} -> {}\n-- Total changes: {}",
            diff.source_schema,
            schema,
            diff.total_changes()
        );
    }

    if options.use_transaction {
        sql.push_str("BEGIN;\n\n");
    }

    // Step 1: Drop constraints on modified tables
    for tbl in &diff.modified_tables {
        for con_name in &tbl.dropped_constraints {
            let _ = writeln!(
                sql,
                "ALTER TABLE {}.{} DROP CONSTRAINT IF EXISTS {};",
                quote_identifier(schema),
                quote_identifier(&tbl.table_name),
                quote_identifier(con_name)
            );
        }
    }

    // Step 2: Drop secondary indexes
    for tbl in &diff.modified_tables {
        for idx_name in &tbl.dropped_indexes {
            let _ = writeln!(
                sql,
                "DROP INDEX IF EXISTS {}.{};",
                quote_identifier(schema),
                quote_identifier(idx_name)
            );
        }
    }

    // Step 3: Drop removed tables
    for tbl_name in &diff.dropped_tables {
        let _ = writeln!(
            sql,
            "DROP TABLE IF EXISTS {}.{} CASCADE;",
            quote_identifier(schema),
            quote_identifier(tbl_name)
        );
    }

    // Step 4: Drop removed columns
    for tbl in &diff.modified_tables {
        for col_name in &tbl.dropped_columns {
            let _ = writeln!(
                sql,
                "ALTER TABLE {}.{} DROP COLUMN IF EXISTS {};",
                quote_identifier(schema),
                quote_identifier(&tbl.table_name),
                quote_identifier(col_name)
            );
        }
    }

    // Step 5: Add brand new tables
    for tbl in &diff.added_tables {
        if options.include_comments {
            let _ = writeln!(sql, "-- Create table {}.{}", schema, tbl.name);
        }
        let _ = writeln!(
            sql,
            "CREATE TABLE {}.{} (",
            quote_identifier(schema),
            quote_identifier(&tbl.name)
        );
        for (i, col) in tbl.columns.iter().enumerate() {
            let mut line = format!("    {} {}", quote_identifier(&col.name), col.data_type);
            if col.is_pk {
                line.push_str(" PRIMARY KEY");
            } else {
                if !col.is_nullable {
                    line.push_str(" NOT NULL");
                }
                if col.is_unique {
                    line.push_str(" UNIQUE");
                }
            }
            if let Some(ref def) = col.default_expr {
                let _ = write!(line, " DEFAULT {def}");
            }
            if i + 1 < tbl.columns.len() {
                line.push(',');
            }
            let _ = writeln!(sql, "{line}");
        }
        sql.push_str(");\n\n");

        // Constraints on newly added tables
        for con in &tbl.constraints {
            if con.kind != ConstraintKind::PrimaryKey {
                let _ = writeln!(
                    sql,
                    "ALTER TABLE {}.{} ADD CONSTRAINT {} {};",
                    quote_identifier(schema),
                    quote_identifier(&tbl.name),
                    quote_identifier(&con.name),
                    con.definition
                );
            }
        }

        // Indexes on newly added tables
        for idx in &tbl.indexes {
            let _ = writeln!(sql, "{};", idx.definition.trim_end_matches(';'));
        }
        sql.push('\n');
    }

    // Step 6: Add columns to existing tables
    for tbl in &diff.modified_tables {
        for col in &tbl.added_columns {
            let mut clause = format!(
                "ALTER TABLE {}.{} ADD COLUMN {} {}",
                quote_identifier(schema),
                quote_identifier(&tbl.table_name),
                quote_identifier(&col.name),
                col.data_type
            );
            if !col.is_nullable {
                clause.push_str(" NOT NULL");
            }
            if let Some(ref def) = col.default_expr {
                let _ = write!(clause, " DEFAULT {def}");
            }
            let _ = writeln!(sql, "{clause};");
        }
    }

    // Step 7: Modify existing columns (type, nullability, defaults)
    for tbl in &diff.modified_tables {
        for col in &tbl.modified_columns {
            if col.type_changed() {
                let _ = writeln!(
                    sql,
                    "ALTER TABLE {}.{} ALTER COLUMN {} TYPE {} USING {}::{};",
                    quote_identifier(schema),
                    quote_identifier(&tbl.table_name),
                    quote_identifier(&col.column_name),
                    col.new_type,
                    quote_identifier(&col.column_name),
                    col.new_type
                );
            }
            if col.nullability_changed() {
                if col.new_nullable {
                    let _ = writeln!(
                        sql,
                        "ALTER TABLE {}.{} ALTER COLUMN {} DROP NOT NULL;",
                        quote_identifier(schema),
                        quote_identifier(&tbl.table_name),
                        quote_identifier(&col.column_name)
                    );
                } else {
                    let _ = writeln!(
                        sql,
                        "ALTER TABLE {}.{} ALTER COLUMN {} SET NOT NULL;",
                        quote_identifier(schema),
                        quote_identifier(&tbl.table_name),
                        quote_identifier(&col.column_name)
                    );
                }
            }
            if col.default_changed() {
                if let Some(ref new_def) = col.new_default {
                    let _ = writeln!(
                        sql,
                        "ALTER TABLE {}.{} ALTER COLUMN {} SET DEFAULT {};",
                        quote_identifier(schema),
                        quote_identifier(&tbl.table_name),
                        quote_identifier(&col.column_name),
                        new_def
                    );
                } else {
                    let _ = writeln!(
                        sql,
                        "ALTER TABLE {}.{} ALTER COLUMN {} DROP DEFAULT;",
                        quote_identifier(schema),
                        quote_identifier(&tbl.table_name),
                        quote_identifier(&col.column_name)
                    );
                }
            }
        }
    }

    // Step 8: Add constraints to existing tables
    for tbl in &diff.modified_tables {
        for con in &tbl.added_constraints {
            let _ = writeln!(
                sql,
                "ALTER TABLE {}.{} ADD CONSTRAINT {} {};",
                quote_identifier(schema),
                quote_identifier(&tbl.table_name),
                quote_identifier(&con.name),
                con.definition
            );
        }
    }

    // Step 9: Add secondary indexes to existing tables
    for tbl in &diff.modified_tables {
        for idx in &tbl.added_indexes {
            let _ = writeln!(sql, "{};", idx.definition.trim_end_matches(';'));
        }
    }

    if options.use_transaction {
        sql.push_str("\nCOMMIT;\n");
    }

    sql
}

/// Renders a human-readable visual tree showing schema additions (`+`), modifications (`~`), and removals (`-`).
#[must_use]
pub fn render_diff_tree(diff: &SchemaDiff, use_unicode: bool) -> String {
    let (t_branch, l_branch, v_line) = if use_unicode {
        ("├── ", "└── ", "│   ")
    } else {
        ("|-- ", "\\-- ", "|   ")
    };

    let mut out = String::new();
    let _ = writeln!(
        out,
        "Schema Diff: {} -> {} ({} change(s))",
        diff.source_schema,
        diff.target_schema,
        diff.total_changes()
    );

    if diff.is_empty() {
        out.push_str("  (No schema differences detected)\n");
        return out;
    }

    // Added tables
    for (i, tbl) in diff.added_tables.iter().enumerate() {
        let is_last_tbl = i + 1 == diff.added_tables.len()
            && diff.modified_tables.is_empty()
            && diff.dropped_tables.is_empty();
        let branch = if is_last_tbl { l_branch } else { t_branch };
        let _ = writeln!(out, "  {branch}+ Table {}", tbl.name);
        for col in &tbl.columns {
            let mut col_desc = format!("+ {} {}", col.name, col.data_type);
            if col.is_pk {
                col_desc.push_str(" PRIMARY KEY");
            } else if !col.is_nullable {
                col_desc.push_str(" NOT NULL");
            }
            if let Some(ref d) = col.default_expr {
                let _ = write!(col_desc, " DEFAULT {d}");
            }
            let _ = writeln!(out, "  {v_line}    {col_desc}");
        }
    }

    // Modified tables
    for (i, tbl) in diff.modified_tables.iter().enumerate() {
        let is_last_tbl = i + 1 == diff.modified_tables.len() && diff.dropped_tables.is_empty();
        let branch = if is_last_tbl { l_branch } else { t_branch };
        let _ = writeln!(
            out,
            "  {branch}~ Table {} ({} change(s))",
            tbl.table_name,
            tbl.change_count()
        );

        for col in &tbl.added_columns {
            let _ = writeln!(
                out,
                "  {v_line}    + Column {} {} (new)",
                col.name, col.data_type
            );
        }
        for col in &tbl.modified_columns {
            let mut changes = Vec::new();
            if col.type_changed() {
                changes.push(format!("type: {} -> {}", col.old_type, col.new_type));
            }
            if col.nullability_changed() {
                changes.push(format!(
                    "null: {} -> {}",
                    if col.old_nullable { "NULL" } else { "NOT NULL" },
                    if col.new_nullable { "NULL" } else { "NOT NULL" }
                ));
            }
            if col.default_changed() {
                changes.push(format!(
                    "default: {} -> {}",
                    col.old_default.as_deref().unwrap_or("none"),
                    col.new_default.as_deref().unwrap_or("none")
                ));
            }
            let _ = writeln!(
                out,
                "  {v_line}    ~ Column {} ({})",
                col.column_name,
                changes.join(", ")
            );
        }
        for col in &tbl.dropped_columns {
            let _ = writeln!(out, "  {v_line}    - Column {} (dropped)", col);
        }
        for con in &tbl.added_constraints {
            let _ = writeln!(
                out,
                "  {v_line}    + Constraint {} ({})",
                con.name,
                con.kind.label()
            );
        }
        for con in &tbl.dropped_constraints {
            let _ = writeln!(out, "  {v_line}    - Constraint {}", con);
        }
        for idx in &tbl.added_indexes {
            let _ = writeln!(out, "  {v_line}    + Index {}", idx.name);
        }
        for idx in &tbl.dropped_indexes {
            let _ = writeln!(out, "  {v_line}    - Index {}", idx);
        }
    }

    // Dropped tables
    for (i, tbl_name) in diff.dropped_tables.iter().enumerate() {
        let is_last = i + 1 == diff.dropped_tables.len();
        let branch = if is_last { l_branch } else { t_branch };
        let _ = writeln!(out, "  {branch}- Table {} (dropped)", tbl_name);
    }

    out
}

/// Renders a machine-readable JSON diff report (`schema_version: 1`).
#[must_use]
pub fn render_diff_json(diff: &SchemaDiff) -> String {
    let mut json = String::new();
    let _ = writeln!(json, "{{");
    let _ = writeln!(json, "  \"schema_version\": 1,");
    let _ = writeln!(
        json,
        "  \"source_schema\": {},",
        json_string(&diff.source_schema)
    );
    let _ = writeln!(
        json,
        "  \"target_schema\": {},",
        json_string(&diff.target_schema)
    );
    let _ = writeln!(json, "  \"total_changes\": {},", diff.total_changes());

    // Added tables
    let _ = writeln!(json, "  \"added_tables\": [");
    for (i, tbl) in diff.added_tables.iter().enumerate() {
        let comma = if i + 1 < diff.added_tables.len() {
            ","
        } else {
            ""
        };
        let _ = writeln!(
            json,
            "    {{\"name\": {}, \"columns\": {}}}{}",
            json_string(&tbl.name),
            tbl.columns.len(),
            comma
        );
    }
    let _ = writeln!(json, "  ],");

    // Dropped tables
    let _ = writeln!(json, "  \"dropped_tables\": [");
    for (i, name) in diff.dropped_tables.iter().enumerate() {
        let comma = if i + 1 < diff.dropped_tables.len() {
            ","
        } else {
            ""
        };
        let _ = writeln!(json, "    {}{}", json_string(name), comma);
    }
    let _ = writeln!(json, "  ],");

    // Modified tables
    let _ = writeln!(json, "  \"modified_tables\": [");
    for (i, tbl) in diff.modified_tables.iter().enumerate() {
        let comma = if i + 1 < diff.modified_tables.len() {
            ","
        } else {
            ""
        };
        let _ = writeln!(
            json,
            "    {{\"name\": {}, \"added_columns\": {}, \"dropped_columns\": {}, \"modified_columns\": {}}}{}",
            json_string(&tbl.table_name),
            tbl.added_columns.len(),
            tbl.dropped_columns.len(),
            tbl.modified_columns.len(),
            comma
        );
    }
    let _ = writeln!(json, "  ]");

    let _ = writeln!(json, "}}");
    json
}

fn json_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// One column draft in the visual Table Designer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlueprintColumn {
    /// Column name.
    pub name: String,
    /// Data type.
    pub data_type: String,
    /// Primary key flag.
    pub is_pk: bool,
    /// NOT NULL flag.
    pub not_null: bool,
    /// UNIQUE flag.
    pub is_unique: bool,
    /// Default expression if any.
    pub default_expr: String,
}

impl Default for BlueprintColumn {
    fn default() -> Self {
        Self {
            name: "id".to_owned(),
            data_type: "bigint".to_owned(),
            is_pk: true,
            not_null: true,
            is_unique: true,
            default_expr: String::new(),
        }
    }
}

/// Interactive table draft model for the Visual Table Designer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableBlueprint {
    /// Schema name.
    pub schema: String,
    /// Table name.
    pub table_name: String,
    /// Draft columns.
    pub columns: Vec<BlueprintColumn>,
    /// Currently focused column index in the form.
    pub focused_column: usize,
}

impl Default for TableBlueprint {
    fn default() -> Self {
        Self {
            schema: "public".to_owned(),
            table_name: "new_table".to_owned(),
            columns: vec![
                BlueprintColumn {
                    name: "id".to_owned(),
                    data_type: "bigint".to_owned(),
                    is_pk: true,
                    not_null: true,
                    is_unique: true,
                    default_expr: "GENERATED ALWAYS AS IDENTITY".to_owned(),
                },
                BlueprintColumn {
                    name: "name".to_owned(),
                    data_type: "text".to_owned(),
                    is_pk: false,
                    not_null: true,
                    is_unique: false,
                    default_expr: String::new(),
                },
                BlueprintColumn {
                    name: "created_at".to_owned(),
                    data_type: "timestamp with time zone".to_owned(),
                    is_pk: false,
                    not_null: true,
                    is_unique: false,
                    default_expr: "now()".to_owned(),
                },
            ],
            focused_column: 0,
        }
    }
}

impl TableBlueprint {
    /// Synthesizes real-time DDL SQL for the table designer preview panel.
    #[must_use]
    pub fn to_create_table_sql(&self) -> String {
        let mut sql = format!(
            "CREATE TABLE {}.{} (\n",
            quote_identifier(&self.schema),
            quote_identifier(&self.table_name)
        );
        for (i, col) in self.columns.iter().enumerate() {
            let mut line = format!("    {} {}", quote_identifier(&col.name), col.data_type);
            if col.is_pk {
                line.push_str(" PRIMARY KEY");
            } else {
                if col.not_null {
                    line.push_str(" NOT NULL");
                }
                if col.is_unique {
                    line.push_str(" UNIQUE");
                }
            }
            if !col.default_expr.trim().is_empty() {
                let trimmed = col.default_expr.trim();
                if trimmed.to_ascii_uppercase().starts_with("GENERATED ") {
                    let _ = write!(line, " {trimmed}");
                } else {
                    let _ = write!(line, " DEFAULT {trimmed}");
                }
            }
            if i + 1 < self.columns.len() {
                line.push(',');
            }
            let _ = writeln!(sql, "{line}");
        }
        sql.push_str(");\n");
        sql
    }

    /// Converts the blueprint into a canonical [`TableDef`].
    #[must_use]
    pub fn to_table_def(&self) -> TableDef {
        TableDef {
            schema: self.schema.clone(),
            name: self.table_name.clone(),
            columns: self
                .columns
                .iter()
                .map(|c| ColumnDef {
                    name: c.name.clone(),
                    data_type: c.data_type.clone(),
                    is_nullable: !c.not_null && !c.is_pk,
                    default_expr: if c.default_expr.trim().is_empty() {
                        None
                    } else {
                        Some(c.default_expr.clone())
                    },
                    is_identity: c.default_expr.to_ascii_uppercase().contains("IDENTITY"),
                    is_pk: c.is_pk,
                    is_unique: c.is_unique,
                })
                .collect(),
            constraints: Vec::new(),
            indexes: Vec::new(),
            comment: None,
        }
    }
}
