//! Persistent scratchpads and SQL snippet library.

use crate::diagnostics::{Diagnostic, DiagnosticKind};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Category classification for SQL snippets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SnippetCategory {
    /// All categories included.
    #[default]
    All,
    /// Database administration, locks, and maintenance.
    Admin,
    /// Performance diagnostics, cache hit ratios, and bloat.
    Performance,
    /// Common SQL design patterns, pagination, and templates.
    Patterns,
    /// Custom user-defined snippets and scratchpads.
    User,
}

impl SnippetCategory {
    /// Human-readable label for capsules and tabs.
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::All => "ALL",
            Self::Admin => "ADMIN",
            Self::Performance => "PERFORMANCE",
            Self::Patterns => "PATTERNS",
            Self::User => "USER",
        }
    }

    /// Cycles to the next category in order.
    #[must_use]
    pub const fn next(&self) -> Self {
        match self {
            Self::All => Self::Admin,
            Self::Admin => Self::Performance,
            Self::Performance => Self::Patterns,
            Self::Patterns => Self::User,
            Self::User => Self::All,
        }
    }
}

/// An individual SQL snippet or scratchpad.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snippet {
    /// Unique identifier / filename slug.
    pub id: String,
    /// Short name for display.
    pub name: String,
    /// Human-readable title.
    pub title: String,
    /// Detailed description of the snippet's purpose and usage.
    pub description: String,
    /// Categorical domain.
    pub category: SnippetCategory,
    /// The executable SQL query or template text.
    pub sql: String,
    /// Whether this is a shipped built-in snippet.
    pub is_builtin: bool,
    /// Optional underlying filesystem path if persisted.
    pub path: Option<PathBuf>,
}

/// Library managing built-in and user-saved SQL snippets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnippetLibrary {
    /// All loaded snippets.
    pub snippets: Vec<Snippet>,
    /// Selected index within the filtered view.
    pub selected_idx: usize,
    /// Active category filter.
    pub category_filter: SnippetCategory,
    /// Active search filter query.
    pub filter_query: String,
}

impl Default for SnippetLibrary {
    fn default() -> Self {
        Self::new()
    }
}

impl SnippetLibrary {
    /// Creates a new library pre-populated with curated built-in snippets.
    #[must_use]
    pub fn new() -> Self {
        let mut lib = Self {
            snippets: Vec::new(),
            selected_idx: 0,
            category_filter: SnippetCategory::All,
            filter_query: String::new(),
        };
        lib.load_builtins();
        lib
    }

    /// Loads curated built-in PostgreSQL snippets into the library.
    pub fn load_builtins(&mut self) {
        self.snippets = vec![
            Snippet {
                id: "cache-hit-ratio".into(),
                name: "cache-hit-ratio".into(),
                title: "Buffer Cache Hit Ratios".into(),
                description: "Calculates buffer cache hit ratio across all user tables and indexes (>99% is optimal).".into(),
                category: SnippetCategory::Performance,
                sql: r#"SELECT
  'index hit rate' AS metric,
  round((sum(idx_blks_hit))::numeric / nullif(sum(idx_blks_hit + idx_blks_read), 0) * 100, 2) AS hit_ratio_pct
FROM pg_statio_user_indexes
UNION ALL
SELECT
  'table hit rate' AS metric,
  round((sum(heap_blks_hit))::numeric / nullif(sum(heap_blks_hit) + sum(heap_blks_read), 0) * 100, 2) AS hit_ratio_pct
FROM pg_statio_user_tables;"#.into(),
                is_builtin: true,
                path: None,
            },
            Snippet {
                id: "table-sizes".into(),
                name: "table-sizes".into(),
                title: "Top 20 Relations by Disk Size".into(),
                description: "Lists largest database tables with segregated heap and index sizes.".into(),
                category: SnippetCategory::Performance,
                sql: r#"SELECT
  schemaname || '.' || relname AS relation,
  pg_size_pretty(pg_total_relation_size(relid)) AS total_size,
  pg_size_pretty(pg_relation_size(relid)) AS table_size,
  pg_size_pretty(pg_total_relation_size(relid) - pg_relation_size(relid)) AS index_size
FROM pg_catalog.pg_statio_user_tables
ORDER BY pg_total_relation_size(relid) DESC
LIMIT 20;"#.into(),
                is_builtin: true,
                path: None,
            },
            Snippet {
                id: "active-locks".into(),
                name: "active-locks".into(),
                title: "Blocking Locks & Waiter Hierarchy".into(),
                description: "Surfaces blocked queries and their blocking processes across the cluster.".into(),
                category: SnippetCategory::Admin,
                sql: r#"SELECT
  blocked_locks.pid AS blocked_pid,
  blocked_activity.usename AS blocked_user,
  blocking_locks.pid AS blocking_pid,
  blocking_activity.usename AS blocking_user,
  blocked_activity.query AS blocked_statement,
  blocking_activity.query AS blocking_statement
FROM pg_catalog.pg_locks blocked_locks
JOIN pg_catalog.pg_stat_activity blocked_activity ON blocked_activity.pid = blocked_locks.pid
JOIN pg_catalog.pg_locks blocking_locks
  ON blocking_locks.locktype = blocked_locks.locktype
  AND blocking_locks.database IS NOT DISTINCT FROM blocked_locks.database
  AND blocking_locks.relation IS NOT DISTINCT FROM blocked_locks.relation
  AND blocking_locks.page IS NOT DISTINCT FROM blocked_locks.page
  AND blocking_locks.tuple IS NOT DISTINCT FROM blocked_locks.tuple
  AND blocking_locks.virtualxid IS NOT DISTINCT FROM blocked_locks.virtualxid
  AND blocking_locks.transactionid IS NOT DISTINCT FROM blocked_locks.transactionid
  AND blocking_locks.classid IS NOT DISTINCT FROM blocked_locks.classid
  AND blocking_locks.objid IS NOT DISTINCT FROM blocked_locks.objid
  AND blocking_locks.objsubid IS NOT DISTINCT FROM blocked_locks.objsubid
  AND blocking_locks.pid != blocked_locks.pid
JOIN pg_catalog.pg_stat_activity blocking_activity ON blocking_activity.pid = blocking_locks.pid
WHERE NOT blocked_locks.granted;"#.into(),
                is_builtin: true,
                path: None,
            },
            Snippet {
                id: "index-bloat".into(),
                name: "index-bloat".into(),
                title: "Index Scan Telemetry & Sizes".into(),
                description: "Lists largest user indexes alongside scan counts to detect unused or bloated indexes.".into(),
                category: SnippetCategory::Performance,
                sql: r#"SELECT
  schemaname || '.' || relname AS table_name,
  indexrelname AS index_name,
  pg_size_pretty(pg_relation_size(indexrelid)) AS index_size,
  idx_scan AS number_of_scans
FROM pg_stat_user_indexes
ORDER BY pg_relation_size(indexrelid) DESC
LIMIT 25;"#.into(),
                is_builtin: true,
                path: None,
            },
            Snippet {
                id: "kill-idle-tx".into(),
                name: "kill-idle-tx".into(),
                title: "Find Idle-in-Transaction Backends".into(),
                description: "Identifies connections lingering idle in transaction for over 5 minutes.".into(),
                category: SnippetCategory::Admin,
                sql: r#"SELECT
  pid,
  usename,
  now() - state_change AS idle_duration,
  query
FROM pg_stat_activity
WHERE state = 'idle in transaction'
  AND (now() - state_change) > interval '5 minutes'
ORDER BY idle_duration DESC;"#.into(),
                is_builtin: true,
                path: None,
            },
            Snippet {
                id: "keyset-pagination".into(),
                name: "keyset-pagination".into(),
                title: "Keyset (Cursor) Pagination Pattern".into(),
                description: "Constant-time pagination pattern avoiding costly OFFSET scans.".into(),
                category: SnippetCategory::Patterns,
                sql: r#"-- Keyset Pagination Pattern
SELECT id, created_at, title
FROM articles
WHERE (created_at, id) < ('2026-11-12 00:00:00', 1000)
ORDER BY created_at DESC, id DESC
LIMIT 25;"#.into(),
                is_builtin: true,
                path: None,
            },
            Snippet {
                id: "upsert-conflict".into(),
                name: "upsert-conflict".into(),
                title: "Atomic Upsert (ON CONFLICT DO UPDATE)".into(),
                description: "Idempotent insert with automated column updates upon unique constraint collision.".into(),
                category: SnippetCategory::Patterns,
                sql: r#"-- Atomic Upsert Template
INSERT INTO accounts (id, email, updated_at)
VALUES (1, 'user@example.com', now())
ON CONFLICT (id) DO UPDATE
SET email = EXCLUDED.email,
    updated_at = now()
RETURNING id, updated_at;"#.into(),
                is_builtin: true,
                path: None,
            },
            Snippet {
                id: "audit-trigger".into(),
                name: "audit-trigger".into(),
                title: "Row-Level Audit Trail Trigger".into(),
                description: "Trigger function recording table mutations, session user, and timestamp.".into(),
                category: SnippetCategory::Patterns,
                sql: r#"-- Row-Level Audit Trail Trigger
CREATE OR REPLACE FUNCTION record_audit_trail()
RETURNS TRIGGER AS $$
BEGIN
  INSERT INTO audit_log (table_name, operation, record_id, changed_at, changed_by)
  VALUES (TG_TABLE_NAME, TG_OP, OLD.id, now(), session_user);
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;"#.into(),
                is_builtin: true,
                path: None,
            },
        ];
    }

    /// Returns the slice of snippets matching the current category filter and query.
    #[must_use]
    pub fn filtered_snippets(&self) -> Vec<&Snippet> {
        let q = self.filter_query.trim().to_lowercase();
        self.snippets
            .iter()
            .filter(|s| {
                if self.category_filter != SnippetCategory::All
                    && s.category != self.category_filter
                {
                    return false;
                }
                if q.is_empty() {
                    return true;
                }
                s.name.to_lowercase().contains(&q)
                    || s.title.to_lowercase().contains(&q)
                    || s.description.to_lowercase().contains(&q)
                    || s.sql.to_lowercase().contains(&q)
            })
            .collect()
    }

    /// Total count of all snippets.
    #[must_use]
    pub fn total_count(&self) -> usize {
        self.snippets.len()
    }

    /// Currently selected snippet, if any.
    #[must_use]
    pub fn selected_snippet(&self) -> Option<&Snippet> {
        let filtered = self.filtered_snippets();
        if filtered.is_empty() {
            None
        } else {
            let idx = self.selected_idx.min(filtered.len().saturating_sub(1));
            Some(filtered[idx])
        }
    }

    /// Moves the selection cursor by `delta`.
    pub fn move_selection(&mut self, delta: isize) {
        let count = self.filtered_snippets().len();
        if count == 0 {
            self.selected_idx = 0;
            return;
        }
        let current = self.selected_idx as isize;
        let new_idx = (current + delta).clamp(0, (count as isize).saturating_sub(1));
        self.selected_idx = new_idx as usize;
    }

    /// Cycles the active category filter.
    pub fn cycle_category(&mut self) {
        self.category_filter = self.category_filter.next();
        self.selected_idx = 0;
    }

    /// Validates a snippet slug name according to safety boundary rules.
    pub fn validate_name(name: &str) -> Result<(), Diagnostic> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(Diagnostic::new(
                DiagnosticKind::Usage,
                "snippet name cannot be empty",
                "validating snippet name",
            )
            .next_action("provide a non-empty name such as 'my-query'"));
        }
        if trimmed.contains('/') || trimmed.contains('\\') || trimmed.contains("..") {
            return Err(Diagnostic::new(
                DiagnosticKind::Usage,
                "snippet name cannot contain path separators or '..'",
                "validating snippet name",
            )
            .next_action("use only alphanumeric characters, dashes, and underscores"));
        }
        Ok(())
    }

    /// Adds a user snippet and optionally writes it to disk if `directory` is provided.
    pub fn add_user_snippet(
        &mut self,
        name: &str,
        title: &str,
        description: &str,
        category: SnippetCategory,
        sql: &str,
        directory: Option<&Path>,
    ) -> Result<&Snippet, Diagnostic> {
        Self::validate_name(name)?;

        let id = name.trim().to_lowercase().replace(' ', "-");
        let path = if let Some(dir) = directory {
            std::fs::create_dir_all(dir).map_err(|e| {
                Diagnostic::new(
                    DiagnosticKind::Usage,
                    format!("could not create snippets directory {}: {e}", dir.display()),
                    "saving snippet",
                )
            })?;
            let file_path = dir.join(format!("{id}.sql"));
            let header = format!(
                "-- Title: {}\n-- Description: {}\n-- Category: {}\n\n{}",
                title,
                description,
                category.label(),
                sql
            );
            std::fs::write(&file_path, header).map_err(|e| {
                Diagnostic::new(
                    DiagnosticKind::Usage,
                    format!("could not write snippet file {}: {e}", file_path.display()),
                    "saving snippet",
                )
            })?;
            Some(file_path)
        } else {
            None
        };

        // Remove existing snippet with same id if present
        self.snippets.retain(|s| s.id != id);

        self.snippets.push(Snippet {
            id: id.clone(),
            name: name.trim().to_string(),
            title: title.trim().to_string(),
            description: description.trim().to_string(),
            category,
            sql: sql.to_string(),
            is_builtin: false,
            path,
        });

        let idx = self.snippets.len() - 1;
        Ok(&self.snippets[idx])
    }

    /// Loads custom user snippets from a directory.
    pub fn load_user_directory(&mut self, directory: &Path) {
        if !directory.is_dir() {
            return;
        }
        let Ok(entries) = std::fs::read_dir(directory) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("sql") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let Ok(content) = std::fs::read_to_string(&path) else {
                continue;
            };

            let mut title = stem.to_string();
            let mut description = String::new();
            let mut category = SnippetCategory::User;
            let mut sql_lines = Vec::new();

            for line in content.lines() {
                if let Some(t) = line.strip_prefix("-- Title:") {
                    title = t.trim().to_string();
                } else if let Some(d) = line.strip_prefix("-- Description:") {
                    description = d.trim().to_string();
                } else if let Some(c) = line.strip_prefix("-- Category:") {
                    let cat_str = c.trim().to_lowercase();
                    category = match cat_str.as_str() {
                        "admin" => SnippetCategory::Admin,
                        "performance" => SnippetCategory::Performance,
                        "patterns" => SnippetCategory::Patterns,
                        _ => SnippetCategory::User,
                    };
                } else {
                    sql_lines.push(line);
                }
            }

            let sql = sql_lines.join("\n").trim().to_string();
            let id = stem.to_string();
            // Don't overwrite existing built-ins with same id
            self.snippets.retain(|s| s.id != id);
            self.snippets.push(Snippet {
                id,
                name: stem.to_string(),
                title,
                description,
                category,
                sql,
                is_builtin: false,
                path: Some(path),
            });
        }
    }
}

#[cfg(test)]
mod tests;
