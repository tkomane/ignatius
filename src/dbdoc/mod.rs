//! Deterministic schema documentation generated from catalog metadata.
//!
//! The model is pure. Catalog IO lives in the session layer and fills it; the
//! renderers never contact a server, a file or a terminal.

use serde::{Deserialize, Serialize};

/// Current document schema version.
pub const DOCUMENT_VERSION: u32 = 1;

/// What kind of relation the catalog reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RelationKind {
    /// An ordinary table.
    Table,
    /// A partitioned parent table.
    PartitionedTable,
    /// A view.
    View,
    /// A materialized view.
    MaterializedView,
    /// A foreign table.
    ForeignTable,
}

impl RelationKind {
    /// Stable lowercase label used in documents.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Table => "table",
            Self::PartitionedTable => "partitioned table",
            Self::View => "view",
            Self::MaterializedView => "materialized view",
            Self::ForeignTable => "foreign table",
        }
    }
}

/// One column in a relation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColumnDocument {
    /// One-based ordinal position.
    pub ordinal: i32,
    /// Column name.
    pub name: String,
    /// Formatted PostgreSQL type.
    pub data_type: String,
    /// Whether the column accepts NULL.
    pub nullable: bool,
    /// Rendered default expression, when one exists.
    pub default: Option<String>,
    /// Whether the column is part of the primary key.
    pub primary_key: bool,
    /// Referenced `schema.table.column`, when a single-column foreign key.
    pub references: Option<String>,
    /// Column comment, when one exists.
    pub comment: Option<String>,
}

/// One index on a relation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexDocument {
    /// Index name.
    pub name: String,
    /// Server-rendered index definition.
    pub definition: String,
    /// Whether the index is unique.
    pub unique: bool,
    /// Whether the index backs the primary key.
    pub primary: bool,
}

/// One constraint on a relation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConstraintDocument {
    /// Constraint name.
    pub name: String,
    /// `p`, `f`, `u`, `c`, `x` or another catalog kind.
    pub kind: String,
    /// Server-rendered definition.
    pub definition: String,
}

/// One row-level security policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyDocument {
    /// Policy name.
    pub name: String,
    /// Command the policy applies to.
    pub command: String,
    /// Roles named by the policy.
    pub roles: Vec<String>,
    /// `USING` expression, when present.
    pub using: Option<String>,
    /// `WITH CHECK` expression, when present.
    pub with_check: Option<String>,
}

/// One documented relation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelationDocument {
    /// Relation name.
    pub name: String,
    /// Relation kind.
    pub kind: RelationKind,
    /// Relation comment.
    pub comment: Option<String>,
    /// Planner row estimate, when the catalog provides one.
    pub estimated_rows: Option<i64>,
    /// Whether row-level security is enabled.
    pub rls_enabled: bool,
    /// Whether row-level security is forced for the owner.
    pub rls_forced: bool,
    /// Columns in ordinal order.
    pub columns: Vec<ColumnDocument>,
    /// Indexes in name order.
    pub indexes: Vec<IndexDocument>,
    /// Constraints in name order.
    pub constraints: Vec<ConstraintDocument>,
    /// Policies in name order.
    pub policies: Vec<PolicyDocument>,
}

impl RelationDocument {
    /// Sorts every nested collection into its documented order.
    pub fn sort(&mut self) {
        self.columns.sort_by_key(|column| column.ordinal);
        self.indexes.sort_by(|a, b| a.name.cmp(&b.name));
        self.constraints.sort_by(|a, b| a.name.cmp(&b.name));
        self.policies.sort_by(|a, b| a.name.cmp(&b.name));
    }
}

/// A complete data dictionary for one schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchemaDocument {
    /// Document schema version.
    pub version: u32,
    /// Database name, when known.
    pub database: String,
    /// Schema name.
    pub schema: String,
    /// RFC3339 generation timestamp.
    pub generated_at: String,
    /// Whether a relation or column bound cut the document short.
    pub truncated: bool,
    /// Relations in name order.
    pub relations: Vec<RelationDocument>,
}

impl SchemaDocument {
    /// Builds an empty document and sorts its (empty) contents.
    #[must_use]
    pub fn new(
        database: impl Into<String>,
        schema: impl Into<String>,
        generated_at: impl Into<String>,
    ) -> Self {
        Self {
            version: DOCUMENT_VERSION,
            database: database.into(),
            schema: schema.into(),
            generated_at: generated_at.into(),
            truncated: false,
            relations: Vec::new(),
        }
    }

    /// Sorts relations and every nested collection into documented order.
    pub fn sort(&mut self) {
        self.relations.sort_by(|a, b| a.name.cmp(&b.name));
        for relation in &mut self.relations {
            relation.sort();
        }
    }

    /// Number of documented columns.
    #[must_use]
    pub fn column_count(&self) -> usize {
        self.relations.iter().map(|r| r.columns.len()).sum()
    }
}

/// Escapes one Markdown table cell without allowing a cell to break the table.
#[must_use]
pub fn markdown_cell(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '|' => escaped.push_str("\\|"),
            '\n' | '\r' => escaped.push_str("<br>"),
            '\t' => escaped.push(' '),
            c if c.is_control() => escaped.push(' '),
            c => escaped.push(c),
        }
    }
    escaped
}

/// Escapes a Mermaid label or relationship caption.
#[must_use]
pub fn mermaid_label(value: &str) -> String {
    let cleaned: String = value
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    cleaned.replace('"', "#quot;")
}

/// Renders the document as Markdown with a table of contents.
#[must_use]
pub fn render_markdown(document: &SchemaDocument) -> String {
    let mut output = String::new();
    output.push_str(&format!(
        "# Data dictionary: {}.{}\n\n",
        markdown_cell(&document.database),
        markdown_cell(&document.schema)
    ));
    output.push_str(&format!(
        "Generated {} from catalog metadata. No row data is included.\n\n",
        markdown_cell(&document.generated_at)
    ));
    if document.truncated {
        output
            .push_str("> This document reached a relation or column bound and is incomplete.\n\n");
    }

    output.push_str("## Contents\n\n");
    if document.relations.is_empty() {
        output.push_str("_No relations are visible in this schema._\n\n");
        return output;
    }
    for relation in &document.relations {
        output.push_str(&format!(
            "- [{}](#{})\n",
            markdown_cell(&relation.name),
            markdown_anchor(&relation.name)
        ));
    }
    output.push('\n');

    for relation in &document.relations {
        output.push_str(&format!("## {}\n\n", markdown_cell(&relation.name)));
        output.push_str(&format!("- Kind: {}\n", relation.kind.label()));
        if let Some(rows) = relation.estimated_rows {
            output.push_str(&format!("- Estimated rows: {rows}\n"));
        }
        if relation.rls_enabled {
            output.push_str("- Row-level security: enabled");
            if relation.rls_forced {
                output.push_str(", forced");
            }
            output.push('\n');
        }
        if let Some(comment) = relation.comment.as_deref() {
            output.push('\n');
            output.push_str(&format!("{}\n", markdown_cell(comment)));
        }
        output.push('\n');

        output.push_str("| # | Column | Type | Null | Default | Key | References | Comment |\n");
        output.push_str("| --- | --- | --- | --- | --- | --- | --- | --- |\n");
        for column in &relation.columns {
            output.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} | {} | {} |\n",
                column.ordinal,
                markdown_cell(&column.name),
                markdown_cell(&column.data_type),
                if column.nullable { "YES" } else { "NO" },
                column
                    .default
                    .as_deref()
                    .map_or_else(String::new, markdown_cell),
                if column.primary_key { "PK" } else { "" },
                column
                    .references
                    .as_deref()
                    .map_or_else(String::new, markdown_cell),
                column
                    .comment
                    .as_deref()
                    .map_or_else(String::new, markdown_cell),
            ));
        }
        output.push('\n');

        if !relation.indexes.is_empty() {
            output.push_str("### Indexes\n\n");
            output.push_str("| Name | Unique | Primary | Definition |\n");
            output.push_str("| --- | --- | --- | --- |\n");
            for index in &relation.indexes {
                output.push_str(&format!(
                    "| {} | {} | {} | {} |\n",
                    markdown_cell(&index.name),
                    if index.unique { "YES" } else { "NO" },
                    if index.primary { "YES" } else { "NO" },
                    markdown_cell(&index.definition),
                ));
            }
            output.push('\n');
        }

        if !relation.constraints.is_empty() {
            output.push_str("### Constraints\n\n");
            output.push_str("| Name | Kind | Definition |\n");
            output.push_str("| --- | --- | --- |\n");
            for constraint in &relation.constraints {
                output.push_str(&format!(
                    "| {} | {} | {} |\n",
                    markdown_cell(&constraint.name),
                    markdown_cell(&constraint.kind),
                    markdown_cell(&constraint.definition),
                ));
            }
            output.push('\n');
        }

        if !relation.policies.is_empty() {
            output.push_str("### Row-level security policies\n\n");
            output.push_str("| Name | Command | Roles | USING | WITH CHECK |\n");
            output.push_str("| --- | --- | --- | --- | --- |\n");
            for policy in &relation.policies {
                output.push_str(&format!(
                    "| {} | {} | {} | {} | {} |\n",
                    markdown_cell(&policy.name),
                    markdown_cell(&policy.command),
                    markdown_cell(&policy.roles.join(", ")),
                    policy
                        .using
                        .as_deref()
                        .map_or_else(String::new, markdown_cell),
                    policy
                        .with_check
                        .as_deref()
                        .map_or_else(String::new, markdown_cell),
                ));
            }
            output.push('\n');
        }
    }

    output
}

/// Renders the document as pretty JSON.
///
/// # Errors
///
/// Returns the serializer error, which for this model means a defect.
pub fn render_json(document: &SchemaDocument) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(document)
}

/// Renders a Mermaid `erDiagram` with one entity per relation and one
/// relationship per single-column foreign key.
#[must_use]
pub fn render_mermaid(document: &SchemaDocument) -> String {
    let mut output = String::from("erDiagram\n");
    for relation in &document.relations {
        output.push_str(&format!(
            "    \"{}.{}\" {{\n",
            mermaid_label(&document.schema),
            mermaid_label(&relation.name)
        ));
        for column in &relation.columns {
            let key = if column.primary_key { " PK" } else { "" };
            output.push_str(&format!(
                "        {} {}{}\n",
                mermaid_type(&column.data_type),
                mermaid_label(&column.name),
                key
            ));
        }
        output.push_str("    }\n");
        for column in &relation.columns {
            if let Some(reference) = column.references.as_deref() {
                let Some((table, referenced_column)) = split_reference(reference) else {
                    continue;
                };
                output.push_str(&format!(
                    "    \"{}.{}\" }}o--|| \"{}.{}\" : \"{}\"\n",
                    mermaid_label(&document.schema),
                    mermaid_label(table),
                    mermaid_label(&document.schema),
                    mermaid_label(&relation.name),
                    mermaid_label(&format!("{}.{}", column.name, referenced_column)),
                ));
            }
        }
    }
    output
}

fn markdown_anchor(name: &str) -> String {
    name.chars()
        .filter_map(|c| {
            if c.is_ascii_alphanumeric() {
                Some(c.to_ascii_lowercase())
            } else if c == ' ' || c == '-' || c == '_' {
                Some('-')
            } else {
                None
            }
        })
        .collect()
}

fn mermaid_type(data_type: &str) -> String {
    let cleaned: String = data_type
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    if cleaned.is_empty() {
        "text".to_owned()
    } else {
        cleaned
    }
}

fn split_reference(reference: &str) -> Option<(&str, &str)> {
    let (relation, column) = reference.rsplit_once('.')?;
    let table = relation.rsplit('.').next().unwrap_or(relation);
    Some((table, column))
}

#[cfg(test)]
mod tests;
