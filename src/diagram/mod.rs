//! Pure schema and entity-relationship (ER) diagram model and deterministic renderers.
//!
//! This module implements the domain logic for Feature `026-schema-er-diagrams`
//! (`M6: Full pgAdmin Replacement Parity`). It depends on no terminal, filesystem,
//! or network types: [`SchemaGraph`] is populated either from a live `pg_catalog`
//! snapshot ([`crate::postgres::metadata::schema_graph`]) or from an in-memory
//! [`crate::query::completion::CompletionCatalog`], filtered by schema or
//! foreign-key neighbourhood (`1-hop` / `2-hop`), bounded at [`MAX_DIAGRAM_TABLES`],
//! and rendered deterministically into Unicode box-drawing, ASCII, Mermaid
//! `erDiagram`, Graphviz `DOT`, or standalone vector `SVG`.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt::Write as _;

use crate::query::completion::CompletionCatalog;

/// Maximum number of tables rendered in a single diagram before requiring a
/// schema or neighbourhood filter.
pub const MAX_DIAGRAM_TABLES: usize = 200;

/// Output representation for a schema ER diagram.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiagramFormat {
    /// Terminal cards using Unicode box-drawing (`┌─┐`) and relationship arrows (`──>`).
    #[default]
    Unicode,
    /// Terminal cards using 7-bit ASCII (`+---+`) and relationship arrows (`-->`).
    Ascii,
    /// Mermaid `erDiagram` markdown syntax (`||--o{`, `||--|{`, `||--||`).
    Mermaid,
    /// Graphviz `digraph` with HTML-like table record nodes and column ports.
    Dot,
    /// Standalone dark/light vector SVG with table cards and orthogonal connectors.
    Svg,
}

impl DiagramFormat {
    /// Short human-readable label for status lines and diagnostics.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Unicode => "Unicode",
            Self::Ascii => "ASCII",
            Self::Mermaid => "Mermaid",
            Self::Dot => "Graphviz DOT",
            Self::Svg => "SVG",
        }
    }
}

/// Neighbourhood traversal depth around a selected anchor table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiagramHopDepth {
    /// Include all tables in the selected schema(s) up to [`MAX_DIAGRAM_TABLES`].
    #[default]
    AllSchema,
    /// Include only the anchor table and tables within 1 foreign-key hop.
    OneHop,
    /// Include the anchor table and tables within 2 foreign-key hops.
    TwoHops,
}

impl DiagramHopDepth {
    /// Short human-readable label for overlay chrome.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::AllSchema => "all tables",
            Self::OneHop => "1-hop neighbourhood",
            Self::TwoHops => "2-hop neighbourhood",
        }
    }

    /// Numeric hop limit when filtering around an anchor table.
    #[must_use]
    pub const fn max_hops(self) -> Option<u8> {
        match self {
            Self::AllSchema => None,
            Self::OneHop => Some(1),
            Self::TwoHops => Some(2),
        }
    }
}

/// Relationship cardinality derived from uniqueness on the foreign-key columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cardinality {
    /// Foreign-key columns form a primary key or unique constraint on the child table.
    OneToOne,
    /// Standard child-to-parent foreign key where multiple child rows may reference one parent row.
    OneToMany,
}

impl Cardinality {
    /// Short human-readable notation.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::OneToOne => "1:1",
            Self::OneToMany => "1:N",
        }
    }
}

/// One column on a [`DiagramTable`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagramColumn {
    /// Column identifier.
    pub name: String,
    /// Formatted PostgreSQL data type (`integer`, `numeric(10,2)`, `timestamptz`, etc.).
    pub data_type: String,
    /// Whether this column participates in the table's `PRIMARY KEY`.
    pub is_primary_key: bool,
    /// Whether this column participates in an outgoing `FOREIGN KEY`.
    pub is_foreign_key: bool,
    /// Whether this column participates in a single-column or composite `UNIQUE` constraint.
    pub is_unique: bool,
    /// Whether the column allows `NULL` values.
    pub nullable: bool,
}

impl DiagramColumn {
    /// Compact key badge (`PK`, `FK`, `PK,FK`, `UQ`, or empty).
    #[must_use]
    pub fn key_badge(&self) -> &'static str {
        match (self.is_primary_key, self.is_foreign_key, self.is_unique) {
            (true, true, _) => "PK,FK",
            (true, false, _) => "PK",
            (false, true, _) => "FK",
            (false, false, true) => "UQ",
            (false, false, false) => "",
        }
    }
}

/// One table or relation node in a [`SchemaGraph`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagramTable {
    /// Schema name (`public`, `reporting`, etc.).
    pub schema: String,
    /// Relation name.
    pub name: String,
    /// Ordered columns belonging to this relation.
    pub columns: Vec<DiagramColumn>,
}

impl DiagramTable {
    /// Fully qualified `schema.name` identifier.
    #[must_use]
    pub fn qualified_name(&self) -> String {
        format!("{}.{}", self.schema, self.name)
    }
}

/// A directed foreign-key relationship from a child (`source`) table to a referenced (`target`) table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForeignKeyEdge {
    /// Name of the `FOREIGN KEY` constraint in `pg_constraint`.
    pub constraint_name: String,
    /// Referencing (child) schema.
    pub source_schema: String,
    /// Referencing (child) table.
    pub source_table: String,
    /// Referencing column names in ordinal order.
    pub source_columns: Vec<String>,
    /// Referenced (parent) schema.
    pub target_schema: String,
    /// Referenced (parent) table.
    pub target_table: String,
    /// Referenced column names in ordinal order.
    pub target_columns: Vec<String>,
    /// Derived cardinality (`1:1` vs `1:N`).
    pub cardinality: Cardinality,
    /// Whether any referencing column is nullable.
    pub nullable: bool,
}

impl ForeignKeyEdge {
    /// Qualified `schema.table` of the referencing child table.
    #[must_use]
    pub fn source_qualified(&self) -> String {
        format!("{}.{}", self.source_schema, self.source_table)
    }

    /// Qualified `schema.table` of the referenced parent table.
    #[must_use]
    pub fn target_qualified(&self) -> String {
        format!("{}.{}", self.target_schema, self.target_table)
    }
}

/// Complete bounded entity-relationship graph for one or more schemas.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SchemaGraph {
    /// Tables sorted deterministically by `(schema, name)`.
    pub tables: Vec<DiagramTable>,
    /// Foreign-key edges sorted deterministically by `(source_schema, source_table, constraint_name)`.
    pub edges: Vec<ForeignKeyEdge>,
    /// Set when the source catalog exceeded [`MAX_DIAGRAM_TABLES`] and was truncated.
    pub truncated_from: Option<usize>,
}

impl SchemaGraph {
    /// Constructs a [`SchemaGraph`] from the in-memory [`CompletionCatalog`] when
    /// live constraint metadata has not yet arrived.
    #[must_use]
    pub fn from_completion_catalog(catalog: &CompletionCatalog) -> Self {
        let mut tables: Vec<DiagramTable> = catalog
            .relations
            .iter()
            .map(|rel| DiagramTable {
                schema: rel.schema.clone(),
                name: rel.name.clone(),
                columns: rel
                    .columns
                    .iter()
                    .map(|col| DiagramColumn {
                        name: col.name.clone(),
                        data_type: col.data_type.clone(),
                        is_primary_key: false,
                        is_foreign_key: false,
                        is_unique: false,
                        nullable: true,
                    })
                    .collect(),
            })
            .collect();
        tables.sort_by(|a, b| (&a.schema, &a.name).cmp(&(&b.schema, &b.name)));
        let total = tables.len();
        let truncated_from = if total > MAX_DIAGRAM_TABLES {
            tables.truncate(MAX_DIAGRAM_TABLES);
            Some(total)
        } else {
            None
        };
        Self {
            tables,
            edges: Vec::new(),
            truncated_from,
        }
    }

    /// Enforces the [`MAX_DIAGRAM_TABLES`] bound on an arbitrary graph.
    #[must_use]
    pub fn bounded(mut self) -> Self {
        self.tables
            .sort_by(|a, b| (&a.schema, &a.name).cmp(&(&b.schema, &b.name)));
        let total = self.tables.len();
        if total > MAX_DIAGRAM_TABLES {
            self.tables.truncate(MAX_DIAGRAM_TABLES);
            self.truncated_from = Some(total);
            let kept: BTreeSet<String> = self
                .tables
                .iter()
                .map(DiagramTable::qualified_name)
                .collect();
            self.edges.retain(|edge| {
                kept.contains(&edge.source_qualified()) && kept.contains(&edge.target_qualified())
            });
        }
        self
    }

    /// Returns a new [`SchemaGraph`] restricted to tables in `schema` (plus edges
    /// where at least one endpoint lies in `schema` and both endpoints are included).
    #[must_use]
    pub fn filter_schema(&self, schema: &str) -> Self {
        let tables: Vec<DiagramTable> = self
            .tables
            .iter()
            .filter(|t| t.schema.eq_ignore_ascii_case(schema))
            .cloned()
            .collect();
        let kept: BTreeSet<String> = tables.iter().map(DiagramTable::qualified_name).collect();
        let edges: Vec<ForeignKeyEdge> = self
            .edges
            .iter()
            .filter(|e| {
                kept.contains(&e.source_qualified()) && kept.contains(&e.target_qualified())
            })
            .cloned()
            .collect();
        Self {
            tables,
            edges,
            truncated_from: None,
        }
        .bounded()
    }

    /// Filters the graph to the connected foreign-key neighbourhood around `anchor`
    /// (`table` or `schema.table`) up to `depth`.
    #[must_use]
    pub fn filter_neighborhood(&self, anchor: &str, depth: DiagramHopDepth) -> Self {
        let Some(max_hops) = depth.max_hops() else {
            return self.clone().bounded();
        };
        let Some(anchor_qualified) = self.resolve_anchor(anchor) else {
            return Self::default();
        };

        let mut adjacency: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for edge in &self.edges {
            let src = edge.source_qualified();
            let tgt = edge.target_qualified();
            adjacency
                .entry(src.clone())
                .or_default()
                .insert(tgt.clone());
            adjacency.entry(tgt).or_default().insert(src);
        }

        let mut visited: BTreeSet<String> = BTreeSet::new();
        let mut queue: VecDeque<(String, u8)> = VecDeque::new();
        visited.insert(anchor_qualified.clone());
        queue.push_back((anchor_qualified, 0));

        while let Some((current, dist)) = queue.pop_front() {
            if dist >= max_hops {
                continue;
            }
            if let Some(neighbors) = adjacency.get(&current) {
                for next in neighbors {
                    if visited.insert(next.clone()) {
                        queue.push_back((next.clone(), dist + 1));
                    }
                }
            }
        }

        let tables: Vec<DiagramTable> = self
            .tables
            .iter()
            .filter(|t| visited.contains(&t.qualified_name()))
            .cloned()
            .collect();
        let edges: Vec<ForeignKeyEdge> = self
            .edges
            .iter()
            .filter(|e| {
                visited.contains(&e.source_qualified()) && visited.contains(&e.target_qualified())
            })
            .cloned()
            .collect();

        Self {
            tables,
            edges,
            truncated_from: None,
        }
        .bounded()
    }

    fn resolve_anchor(&self, anchor: &str) -> Option<String> {
        let trimmed = anchor.trim();
        if trimmed.is_empty() {
            return None;
        }
        if let Some(exact) = self
            .tables
            .iter()
            .find(|t| t.qualified_name().eq_ignore_ascii_case(trimmed))
        {
            return Some(exact.qualified_name());
        }
        self.tables
            .iter()
            .find(|t| t.name.eq_ignore_ascii_case(trimmed))
            .map(DiagramTable::qualified_name)
    }

    /// Renders this graph into the requested [`DiagramFormat`].
    #[must_use]
    pub fn render(&self, format: DiagramFormat) -> String {
        match format {
            DiagramFormat::Unicode => self.render_box(true),
            DiagramFormat::Ascii => self.render_box(false),
            DiagramFormat::Mermaid => self.render_mermaid(),
            DiagramFormat::Dot => self.render_dot(),
            DiagramFormat::Svg => self.render_svg(),
        }
    }

    fn render_box(&self, unicode: bool) -> String {
        if self.tables.is_empty() {
            return "No tables matched the diagram filter.\n".to_owned();
        }
        let mut out = String::new();
        let _ = writeln!(
            out,
            "Schema ER Diagram ({} table{}, {} relationship{})",
            self.tables.len(),
            if self.tables.len() == 1 { "" } else { "s" },
            self.edges.len(),
            if self.edges.len() == 1 { "" } else { "s" }
        );
        if let Some(total) = self.truncated_from {
            let _ = writeln!(
                out,
                "Note: truncated to {} of {} tables. Filter by schema or table for a focused view.",
                MAX_DIAGRAM_TABLES, total
            );
        }
        out.push('\n');

        let (tl, tr, bl, br, h, v, ml, mr) = if unicode {
            ('┌', '┐', '└', '┘', '─', '│', '├', '┤')
        } else {
            ('+', '+', '+', '+', '-', '|', '+', '+')
        };

        for (idx, table) in self.tables.iter().enumerate() {
            if idx > 0 {
                out.push('\n');
            }
            let title = table.qualified_name();
            let mut width = title.chars().count() + 2;
            let rows: Vec<(String, String, String)> = table
                .columns
                .iter()
                .map(|c| {
                    let badge = c.key_badge();
                    let badge_col = if badge.is_empty() {
                        "     ".to_owned()
                    } else {
                        format!("{badge:<5}")
                    };
                    let null_suffix = if c.nullable { "NULL" } else { "NOT NULL" };
                    let type_col = format!("{} {}", c.data_type, null_suffix);
                    width = width.max(
                        badge_col.chars().count()
                            + 1
                            + c.name.chars().count()
                            + 2
                            + type_col.chars().count()
                            + 2,
                    );
                    (badge_col, c.name.clone(), type_col)
                })
                .collect();
            width = width.clamp(28, 76);

            let rule: String = std::iter::repeat_n(h, width).collect();
            let _ = writeln!(out, "{tl}{rule}{tr}");
            let title_pad = width.saturating_sub(title.chars().count() + 1);
            let _ = writeln!(out, "{v} {title}{:title_pad$}{v}", "");
            let _ = writeln!(out, "{ml}{rule}{mr}");

            if rows.is_empty() {
                let empty_msg = "(no columns)";
                let pad = width.saturating_sub(empty_msg.chars().count() + 1);
                let _ = writeln!(out, "{v} {empty_msg}{:pad$}{v}", "");
            } else {
                for (badge_col, col_name, type_col) in rows {
                    let line_text = format!("{badge_col} {col_name} : {type_col}");
                    let truncated: String =
                        line_text.chars().take(width.saturating_sub(2)).collect();
                    let pad = width.saturating_sub(truncated.chars().count() + 1);
                    let _ = writeln!(out, "{v} {truncated}{:pad$}{v}", "");
                }
            }
            let _ = writeln!(out, "{bl}{rule}{br}");
        }

        out.push('\n');
        let arrow = if unicode { "──>" } else { "-->" };
        if self.edges.is_empty() {
            let _ = writeln!(out, "Relationships: none declared in pg_constraint.");
        } else {
            let _ = writeln!(out, "Relationships:");
            for edge in &self.edges {
                let src_cols = edge.source_columns.join(", ");
                let tgt_cols = edge.target_columns.join(", ");
                let nullability = if edge.nullable {
                    "optional"
                } else {
                    "mandatory"
                };
                let _ = writeln!(
                    out,
                    "  {}.{}({}) {} {}.{}({}) [{}, {}, {}]",
                    edge.source_schema,
                    edge.source_table,
                    src_cols,
                    arrow,
                    edge.target_schema,
                    edge.target_table,
                    tgt_cols,
                    edge.cardinality.label(),
                    nullability,
                    edge.constraint_name
                );
            }
        }
        out
    }

    fn render_mermaid(&self) -> String {
        let mut out = String::from("erDiagram\n");
        for edge in &self.edges {
            let connector = match (edge.cardinality, edge.nullable) {
                (Cardinality::OneToOne, true) => "||--o|",
                (Cardinality::OneToOne, false) => "||--||",
                (Cardinality::OneToMany, true) => "||--o{",
                (Cardinality::OneToMany, false) => "||--|{",
            };
            let _ = writeln!(
                out,
                "    \"{}.{}\" {} \"{}.{}\" : \"{}\"",
                edge.target_schema,
                edge.target_table,
                connector,
                edge.source_schema,
                edge.source_table,
                sanitize_mermaid_label(&edge.constraint_name)
            );
        }
        for table in &self.tables {
            let _ = writeln!(out, "    \"{}.{}\" {{", table.schema, table.name);
            for col in &table.columns {
                let mermaid_type = sanitize_mermaid_type(&col.data_type);
                let mermaid_name = sanitize_mermaid_ident(&col.name);
                let key_suffix = match (col.is_primary_key, col.is_foreign_key, col.is_unique) {
                    (true, true, _) => " PK,FK",
                    (true, false, _) => " PK",
                    (false, true, _) => " FK",
                    (false, false, true) => " UK",
                    (false, false, false) => "",
                };
                let _ = writeln!(out, "        {mermaid_type} {mermaid_name}{key_suffix}");
            }
            out.push_str("    }\n");
        }
        out
    }

    fn render_dot(&self) -> String {
        let mut out = String::from(
            "digraph schema_er_diagram {\n\
             \x20   graph [rankdir=LR, bgcolor=\"transparent\", fontname=\"monospace\", pad=\"0.4\", nodesep=\"0.5\", ranksep=\"0.9\"];\n\
             \x20   node [shape=plain, fontname=\"monospace\", fontsize=10];\n\
             \x20   edge [color=\"#7aa2f7\", fontname=\"monospace\", fontsize=9, fontcolor=\"#a9b1d6\"];\n\n",
        );

        for table in &self.tables {
            let qualified = escape_dot_string(&table.qualified_name());
            let title_html = escape_html(&table.qualified_name());
            let _ = writeln!(
                out,
                "    \"{qualified}\" [label=<\n\
                 \x20       <table border=\"1\" cellborder=\"0\" cellspacing=\"0\" cellpadding=\"4\" color=\"#3b4261\" bgcolor=\"#1a1b26\">\n\
                 \x20           <tr><td bgcolor=\"#24283b\" colspan=\"3\" align=\"left\"><font color=\"#c0caf5\"><b>{title_html}</b></font></td></tr>"
            );
            for col in &table.columns {
                let port = escape_html(&col.name);
                let badge = escape_html(col.key_badge());
                let name = escape_html(&col.name);
                let dtype = escape_html(&col.data_type);
                let _ = writeln!(
                    out,
                    "            <tr><td align=\"left\"><font color=\"#bb9af7\">{badge}</font></td><td port=\"{port}\" align=\"left\"><font color=\"#c0caf5\">{name}</font></td><td align=\"left\"><font color=\"#565f89\">{dtype}</font></td></tr>"
                );
            }
            out.push_str("        </table>\n    >];\n");
        }

        if !self.edges.is_empty() {
            out.push('\n');
        }
        for edge in &self.edges {
            let src_node = escape_dot_string(&edge.source_qualified());
            let tgt_node = escape_dot_string(&edge.target_qualified());
            let label = escape_dot_string(&format!(
                "{} ({})",
                edge.constraint_name,
                edge.cardinality.label()
            ));
            if let (Some(src_col), Some(tgt_col)) =
                (edge.source_columns.first(), edge.target_columns.first())
            {
                let src_port = escape_dot_string(src_col);
                let tgt_port = escape_dot_string(tgt_col);
                let _ = writeln!(
                    out,
                    "    \"{src_node}\":\"{src_port}\" -> \"{tgt_node}\":\"{tgt_port}\" [label=\"{label}\"];"
                );
            } else {
                let _ = writeln!(
                    out,
                    "    \"{src_node}\" -> \"{tgt_node}\" [label=\"{label}\"];"
                );
            }
        }
        out.push_str("}\n");
        out
    }

    fn render_svg(&self) -> String {
        let cols_per_row = 3usize;
        let card_width = 280usize;
        let col_gap = 60usize;
        let row_gap = 40usize;
        let pad = 28usize;

        let mut row_heights: Vec<usize> = Vec::new();
        for chunk in self.tables.chunks(cols_per_row) {
            let max_cols = chunk
                .iter()
                .map(|t| t.columns.len().max(1))
                .max()
                .unwrap_or(1);
            row_heights.push(44 + max_cols * 22 + 12);
        }

        let total_cols = self.tables.len().clamp(1, cols_per_row);
        let svg_width = pad * 2 + total_cols * card_width + total_cols.saturating_sub(1) * col_gap;
        let total_cards_height: usize =
            row_heights.iter().sum::<usize>() + row_heights.len().saturating_sub(1) * row_gap;
        let footer_height = if self.edges.is_empty() {
            40
        } else {
            44 + self.edges.len() * 20
        };
        let svg_height = (pad * 2 + total_cards_height + footer_height).max(160);

        let mut positions: BTreeMap<String, (usize, usize, usize, usize)> = BTreeMap::new();
        let mut out = String::new();
        let _ = writeln!(
            out,
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {svg_width} {svg_height}\" width=\"{svg_width}\" height=\"{svg_height}\">"
        );
        out.push_str(
            "  <defs>\n\
             \x20   <marker id=\"fk-arrow\" markerWidth=\"8\" markerHeight=\"8\" refX=\"7\" refY=\"4\" orient=\"auto\">\n\
             \x20     <path d=\"M0,0 L8,4 L0,8 Z\" fill=\"#7aa2f7\" />\n\
             \x20   </marker>\n\
             \x20 </defs>\n\
             \x20 <rect width=\"100%\" height=\"100%\" fill=\"#1a1b26\" rx=\"8\" />\n",
        );

        let mut y_cursor = pad;
        for (row_idx, chunk) in self.tables.chunks(cols_per_row).enumerate() {
            for (col_idx, table) in chunk.iter().enumerate() {
                let x = pad + col_idx * (card_width + col_gap);
                let y = y_cursor;
                let h = 44 + table.columns.len().max(1) * 22;
                positions.insert(table.qualified_name(), (x, y, card_width, h));
            }
            y_cursor += row_heights[row_idx] + row_gap;
        }

        for edge in &self.edges {
            if let (Some(&(sx, sy, sw, sh)), Some(&(tx, ty, tw, th))) = (
                positions.get(&edge.source_qualified()),
                positions.get(&edge.target_qualified()),
            ) {
                let (x1, y1) = (sx + sw / 2, sy + sh / 2);
                let (x2, y2) = (tx + tw / 2, ty + th / 2);
                let mid_x = (x1 + x2) / 2;
                let _ = writeln!(
                    out,
                    "  <path d=\"M {x1} {y1} L {mid_x} {y1} L {mid_x} {y2} L {x2} {y2}\" fill=\"none\" stroke=\"#7aa2f7\" stroke-width=\"1.5\" stroke-dasharray=\"4,3\" marker-end=\"url(#fk-arrow)\" />"
                );
            }
        }

        for table in &self.tables {
            let Some(&(x, y, w, h)) = positions.get(&table.qualified_name()) else {
                continue;
            };
            let title = escape_html(&table.qualified_name());
            let _ = writeln!(
                out,
                "  <g transform=\"translate({x},{y})\">\n\
                 \x20   <rect width=\"{w}\" height=\"{h}\" rx=\"6\" fill=\"#1f2335\" stroke=\"#3b4261\" stroke-width=\"1.5\" />\n\
                 \x20   <rect width=\"{w}\" height=\"32\" rx=\"6\" fill=\"#24283b\" />\n\
                 \x20   <text x=\"12\" y=\"21\" fill=\"#7aa2f7\" font-family=\"monospace\" font-size=\"13\" font-weight=\"bold\">{title}</text>"
            );
            for (idx, col) in table.columns.iter().enumerate() {
                let cy = 50 + idx * 22;
                let badge = escape_html(col.key_badge());
                let name = escape_html(&col.name);
                let dtype = escape_html(&col.data_type);
                let _ = writeln!(
                    out,
                    "    <text x=\"12\" y=\"{cy}\" fill=\"#bb9af7\" font-family=\"monospace\" font-size=\"11\">{badge}</text>\n\
                     \x20   <text x=\"58\" y=\"{cy}\" fill=\"#c0caf5\" font-family=\"monospace\" font-size=\"12\">{name}</text>\n\
                     \x20   <text x=\"176\" y=\"{cy}\" fill=\"#565f89\" font-family=\"monospace\" font-size=\"11\">{dtype}</text>"
                );
            }
            out.push_str("  </g>\n");
        }

        if !self.edges.is_empty() {
            let mut ey = y_cursor + 8;
            for edge in &self.edges {
                let summary = escape_html(&format!(
                    "{}.{}({}) -> {}.{}({}) [{}]",
                    edge.source_schema,
                    edge.source_table,
                    edge.source_columns.join(", "),
                    edge.target_schema,
                    edge.target_table,
                    edge.target_columns.join(", "),
                    edge.cardinality.label()
                ));
                let _ = writeln!(
                    out,
                    "  <text x=\"{pad}\" y=\"{ey}\" fill=\"#9ece6a\" font-family=\"monospace\" font-size=\"11\">{summary}</text>"
                );
                ey += 20;
            }
        }
        out.push_str("</svg>\n");
        out
    }
}

fn sanitize_mermaid_type(raw: &str) -> String {
    let mut s: String = raw
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect();
    while s.contains("__") {
        s = s.replace("__", "_");
    }
    let trimmed = s.trim_matches('_');
    if trimmed.is_empty() {
        "unknown".to_owned()
    } else {
        trimmed.to_owned()
    }
}

fn sanitize_mermaid_ident(raw: &str) -> String {
    let mut s: String = raw
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect();
    if s.is_empty() || s.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        s.insert(0, '_');
    }
    s
}

fn sanitize_mermaid_label(raw: &str) -> String {
    raw.replace('"', "'").replace(['\n', '\r'], " ")
}

fn escape_dot_string(raw: &str) -> String {
    raw.replace('\\', "\\\\").replace('"', "\\\"")
}

fn escape_html(raw: &str) -> String {
    raw.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests;
