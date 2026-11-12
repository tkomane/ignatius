//! Pure domain models and topological dependency ordering for cross-database data sync pipelines.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt::Write as _;

/// A unique identifier for a database relation (schema + table).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TableKey {
    /// Schema name.
    pub schema: String,
    /// Table or relation name.
    pub table: String,
}

impl TableKey {
    /// Creates a new table identifier.
    #[must_use]
    pub fn new(schema: impl Into<String>, table: impl Into<String>) -> Self {
        Self {
            schema: schema.into(),
            table: table.into(),
        }
    }

    /// Formats as schema-qualified identifier: `"schema"."table"`.
    #[must_use]
    pub fn qualified_sql(&self) -> String {
        format!(
            "\"{}\".\"{}\"",
            self.schema.replace('"', "\"\""),
            self.table.replace('"', "\"\"")
        )
    }

    /// Formats as human-readable string: `schema.table`.
    #[must_use]
    pub fn display(&self) -> String {
        format!("{}.{}", self.schema, self.table)
    }
}

/// A directed foreign key relationship between tables.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForeignKeyEdge {
    /// Child table containing the foreign key column(s).
    pub from: TableKey,
    /// Parent table referenced by the foreign key.
    pub to: TableKey,
    /// Foreign key constraint name.
    pub constraint_name: String,
}

/// A node in the table dependency graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencyNode {
    /// Table identifier.
    pub key: TableKey,
    /// Parent tables that this table directly references.
    pub parents: Vec<TableKey>,
}

/// Graph of tables and their foreign key dependencies.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DependencyGraph {
    /// All nodes in the graph.
    pub nodes: Vec<DependencyNode>,
}

/// Execution plan for synchronizing tables.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncPlan {
    /// Order in which tables must be populated (parents first).
    pub insert_order: Vec<TableKey>,
    /// Order in which tables must be truncated (children first).
    pub truncate_order: Vec<TableKey>,
    /// Whether any cyclic foreign key dependencies were detected.
    pub has_cycles: bool,
    /// Tables involved in cyclic dependency loops, if any.
    pub cyclic_tables: Vec<TableKey>,
}

impl DependencyGraph {
    /// Creates a new dependency graph from a list of tables and foreign key edges.
    #[must_use]
    pub fn build(tables: &[TableKey], edges: &[ForeignKeyEdge]) -> Self {
        let mut parent_map: HashMap<TableKey, Vec<TableKey>> = HashMap::new();
        for t in tables {
            parent_map.entry(t.clone()).or_default();
        }
        for edge in edges {
            // Ignore self-referential foreign keys in DAG topological ordering
            if edge.from != edge.to {
                parent_map
                    .entry(edge.from.clone())
                    .or_default()
                    .push(edge.to.clone());
            }
        }

        let mut nodes = Vec::with_capacity(tables.len());
        for (key, mut parents) in parent_map {
            parents.sort_by(|a, b| (&a.schema, &a.table).cmp(&(&b.schema, &b.table)));
            parents.dedup();
            nodes.push(DependencyNode { key, parents });
        }
        nodes.sort_by(|a, b| (&a.key.schema, &a.key.table).cmp(&(&b.key.schema, &b.key.table)));

        Self { nodes }
    }

    /// Computes topological dependency order for insertion and truncation.
    #[must_use]
    pub fn compute_sync_plan(&self) -> SyncPlan {
        let all_keys: HashSet<TableKey> = self.nodes.iter().map(|n| n.key.clone()).collect();

        // In-degree for insertion: number of parents inside this synchronization set
        let mut in_degree: HashMap<TableKey, usize> = HashMap::new();
        let mut children_map: HashMap<TableKey, Vec<TableKey>> = HashMap::new();

        for node in &self.nodes {
            let mut internal_parents = 0;
            for parent in &node.parents {
                if all_keys.contains(parent) && parent != &node.key {
                    internal_parents += 1;
                    children_map
                        .entry(parent.clone())
                        .or_default()
                        .push(node.key.clone());
                }
            }
            in_degree.insert(node.key.clone(), internal_parents);
        }

        // Kahn's algorithm: queue tables with 0 internal dependencies (roots)
        let mut queue: VecDeque<TableKey> = VecDeque::new();
        let mut sorted_keys: Vec<TableKey> = self.nodes.iter().map(|n| n.key.clone()).collect();
        sorted_keys.sort_by(|a, b| (&a.schema, &a.table).cmp(&(&b.schema, &b.table)));

        for k in &sorted_keys {
            if in_degree.get(k).copied().unwrap_or(0) == 0 {
                queue.push_back(k.clone());
            }
        }

        let mut insert_order = Vec::with_capacity(self.nodes.len());
        while let Some(current) = queue.pop_front() {
            insert_order.push(current.clone());

            if let Some(children) = children_map.get(&current) {
                let mut sorted_children = children.clone();
                sorted_children.sort_by(|a, b| (&a.schema, &a.table).cmp(&(&b.schema, &b.table)));
                for child in sorted_children {
                    if let Some(deg) = in_degree.get_mut(&child) {
                        *deg = deg.saturating_sub(1);
                        if *deg == 0 {
                            queue.push_back(child);
                        }
                    }
                }
            }
        }

        let has_cycles = insert_order.len() < self.nodes.len();
        let mut cyclic_tables = Vec::new();

        if has_cycles {
            // Append remaining cycle tables in deterministic order
            for node in &self.nodes {
                if !insert_order.contains(&node.key) {
                    cyclic_tables.push(node.key.clone());
                    insert_order.push(node.key.clone());
                }
            }
        }

        let mut truncate_order = insert_order.clone();
        truncate_order.reverse();

        SyncPlan {
            insert_order,
            truncate_order,
            has_cycles,
            cyclic_tables,
        }
    }
}

/// Operational status of an individual table synchronization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncTableStatus {
    /// Scheduled for transfer.
    Pending,
    /// Currently streaming rows across databases.
    Streaming,
    /// Completed successfully.
    Completed,
    /// Failed with an error.
    Failed,
}

impl SyncTableStatus {
    /// Human-readable status label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::Streaming => "STREAMING",
            Self::Completed => "COMPLETED",
            Self::Failed => "FAILED",
        }
    }
}

/// Live progress telemetry for a single table copy pipeline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncTableProgress {
    /// Table identifier.
    pub key: TableKey,
    /// Current transfer state.
    pub status: SyncTableStatus,
    /// Number of rows transferred.
    pub rows_copied: u64,
    /// Total bytes transferred.
    pub bytes_transferred: u64,
    /// Elapsed duration in milliseconds.
    pub duration_ms: u64,
    /// Diagnostic message or error explanation, if any.
    pub message: Option<String>,
}

/// Comprehensive report of cross-database synchronization.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SyncReport {
    /// Source database endpoint description.
    pub source_endpoint: String,
    /// Target database endpoint description.
    pub target_endpoint: String,
    /// Total rows copied across all tables.
    pub total_rows: u64,
    /// Total bytes transferred.
    pub total_bytes: u64,
    /// Total elapsed time in milliseconds.
    pub total_duration_ms: u64,
    /// Whether any foreign key cycles required deferred constraint checks.
    pub has_cycles: bool,
    /// Per-table transfer records.
    pub table_progress: Vec<SyncTableProgress>,
}

impl SyncReport {
    /// Renders an aligned tabular report of synchronization telemetry.
    #[must_use]
    pub fn render_table(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "=== Ignatius Cross-Database Sync Pipeline ===");
        let _ = writeln!(out, "Source: {}", self.source_endpoint);
        let _ = writeln!(out, "Target: {}", self.target_endpoint);
        let duration_sec = self.total_duration_ms as f64 / 1000.0;
        let rate_rows = if duration_sec > 0.0 {
            self.total_rows as f64 / duration_sec
        } else {
            0.0
        };
        let _ = writeln!(
            out,
            "Total Tables: {} | Total Rows: {} | Bytes: {} | Duration: {:.2}s ({:.1} rows/s)",
            self.table_progress.len(),
            self.total_rows,
            format_bytes(self.total_bytes),
            duration_sec,
            rate_rows
        );
        if self.has_cycles {
            let _ = writeln!(
                out,
                "Notice: Cyclic foreign keys detected; deferred constraints applied."
            );
        }
        let _ = writeln!(out);

        let _ = writeln!(
            out,
            "{:<16} {:<24} {:<12} {:>12} {:>14} {:>10}",
            "SCHEMA", "TABLE", "STATUS", "ROWS", "BYTES", "TIME(MS)"
        );
        let _ = writeln!(out, "{}", "-".repeat(92));

        for t in &self.table_progress {
            let _ = writeln!(
                out,
                "{:<16} {:<24} {:<12} {:>12} {:>14} {:>10}",
                truncate_str(&t.key.schema, 16),
                truncate_str(&t.key.table, 24),
                t.status.label(),
                t.rows_copied,
                format_bytes(t.bytes_transferred),
                t.duration_ms
            );
        }
        let _ = writeln!(out);

        out
    }

    /// Renders structured JSON format.
    #[must_use]
    pub fn render_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string())
    }

    /// Renders plain text format.
    #[must_use]
    pub fn render_plain(&self) -> String {
        self.render_table()
    }
}

/// Truncates string with ellipsis if exceeding max length.
#[must_use]
pub fn truncate_str(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_len.saturating_sub(3)).collect();
        format!("{truncated}...")
    }
}

/// Formats byte quantities into human-readable strings (B, KiB, MiB, GiB).
#[must_use]
pub fn format_bytes(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = KIB * 1024;
    const GIB: u64 = MIB * 1024;

    if bytes >= GIB {
        format!("{:.2} GiB", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{:.2} MiB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.2} KiB", bytes as f64 / KIB as f64)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests;
