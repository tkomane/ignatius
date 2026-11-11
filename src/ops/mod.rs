//! Domain engine for asynchronous LISTEN/NOTIFY monitoring and guarded maintenance operations (`033-listen-notify-and-maintenance-ops`).
//!
//! Provides pure models for PostgreSQL pub/sub messaging, vacuum/analyze/reindex maintenance SQL
//! synthesis, lock level risk assessment, and live progress reporting telemetry.

#[cfg(test)]
mod tests;

use serde::{Deserialize, Serialize};

/// An asynchronous pub/sub event received from PostgreSQL via `LISTEN`/`NOTIFY`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PubSubNotification {
    /// Formatted UTC timestamp when the notification arrived.
    pub timestamp: String,
    /// Channel topic name.
    pub channel: String,
    /// Process ID (PID) of the server backend that dispatched `NOTIFY`.
    pub sender_pid: u32,
    /// Notification payload string (text or JSON).
    pub payload: String,
}

/// A pub/sub channel being actively monitored or available for listening.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationChannel {
    /// Channel name.
    pub name: String,
    /// Whether the channel currently has an active `LISTEN` subscription.
    pub active: bool,
    /// Number of notifications received on this channel during the session.
    pub message_count: usize,
}

/// Reindex operation scope level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReindexScope {
    /// Reindex a single table.
    Table,
    /// Reindex a single index.
    Index,
    /// Reindex an entire schema.
    Schema,
    /// Reindex the current database.
    Database,
}

/// The specific database maintenance command to perform.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum MaintenanceKind {
    /// Table garbage collection and space reclamation (`VACUUM`).
    Vacuum {
        /// Whether to rewrite the entire relation on disk to reclaim maximum space (`FULL`).
        full: bool,
        /// Whether to freeze transaction IDs (`FREEZE`).
        freeze: bool,
        /// Whether to print progress details and dead tuple statistics (`VERBOSE`).
        verbose: bool,
        /// Whether to update optimizer table statistics during vacuum (`ANALYZE`).
        analyze: bool,
    },
    /// Optimizer query statistics collection (`ANALYZE`).
    Analyze {
        /// Whether to print progress details (`VERBOSE`).
        verbose: bool,
    },
    /// Index rebuild to eliminate btree bloat and fragmentation (`REINDEX`).
    Reindex {
        /// Whether to build the new index without locking out concurrent writes (`CONCURRENTLY`).
        concurrently: bool,
        /// Scope of the reindex operation.
        scope: ReindexScope,
    },
    /// Cluster table according to an index (`CLUSTER`).
    Cluster {
        /// Whether to print progress details (`VERBOSE`).
        verbose: bool,
    },
}

impl MaintenanceKind {
    /// Returns the primary SQL keyword command name.
    #[must_use]
    pub const fn command_name(&self) -> &'static str {
        match self {
            Self::Vacuum { .. } => "VACUUM",
            Self::Analyze { .. } => "ANALYZE",
            Self::Reindex { .. } => "REINDEX",
            Self::Cluster { .. } => "CLUSTER",
        }
    }

    /// Returns human-readable operation summary with options.
    #[must_use]
    pub fn display_name(&self) -> String {
        match self {
            Self::Vacuum {
                full,
                freeze,
                verbose,
                analyze,
            } => {
                let mut parts = Vec::new();
                if *full {
                    parts.push("FULL");
                }
                if *freeze {
                    parts.push("FREEZE");
                }
                if *verbose {
                    parts.push("VERBOSE");
                }
                if *analyze {
                    parts.push("ANALYZE");
                }
                if parts.is_empty() {
                    "VACUUM".to_owned()
                } else {
                    format!("VACUUM ({})", parts.join(", "))
                }
            }
            Self::Analyze { verbose } => {
                if *verbose {
                    "ANALYZE (VERBOSE)".to_owned()
                } else {
                    "ANALYZE".to_owned()
                }
            }
            Self::Reindex {
                concurrently,
                scope,
            } => {
                let scope_name = match scope {
                    ReindexScope::Table => "TABLE",
                    ReindexScope::Index => "INDEX",
                    ReindexScope::Schema => "SCHEMA",
                    ReindexScope::Database => "DATABASE",
                };
                if *concurrently {
                    format!("REINDEX {scope_name} CONCURRENTLY")
                } else {
                    format!("REINDEX {scope_name}")
                }
            }
            Self::Cluster { verbose } => {
                if *verbose {
                    "CLUSTER (VERBOSE)".to_owned()
                } else {
                    "CLUSTER".to_owned()
                }
            }
        }
    }
}

/// Execution options for a database maintenance task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaintenanceOptions {
    /// Kind of maintenance operation.
    pub kind: MaintenanceKind,
    /// Target relation or identifier (e.g. `"public"."orders"`), or None for entire database.
    pub target: Option<String>,
    /// Explicit confirmation when operating against production-classified environments.
    pub confirm_production: bool,
}

/// Live progress telemetry entry reported from `pg_stat_progress_*` system views.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MaintenanceProgressEntry {
    /// Server backend process ID.
    pub pid: u32,
    /// Database name.
    pub datname: String,
    /// Relation / table name being processed.
    pub relname: Option<String>,
    /// Command being executed (`VACUUM`, `CREATE INDEX`, `CLUSTER`).
    pub command: String,
    /// Current execution phase.
    pub phase: String,
    /// Total number of heap blocks to process.
    pub heap_blks_total: Option<i64>,
    /// Heap blocks already scanned.
    pub heap_blks_scanned: Option<i64>,
    /// Heap blocks already vacuumed.
    pub heap_blks_vacuumed: Option<i64>,
    /// Number of index vacuum passes completed.
    pub index_vacuum_count: Option<i64>,
    /// Computed progress percentage if total blocks is non-zero.
    pub percent: Option<f64>,
}

/// Result summary of an executed maintenance operation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MaintenanceResult {
    /// Executed SQL command string.
    pub executed_sql: String,
    /// Whether the command succeeded.
    pub success: bool,
    /// Elapsed execution duration in milliseconds.
    pub elapsed_ms: f64,
    /// Notices and verbal diagnostics emitted by PostgreSQL.
    pub notices: Vec<String>,
    /// Optional status or error message.
    pub message: String,
}

/// Estimates the table lock severity required by the maintenance operation.
#[must_use]
pub fn estimate_maintenance_lock_level(kind: &MaintenanceKind) -> &'static str {
    match kind {
        MaintenanceKind::Vacuum { full: true, .. } => {
            "AccessExclusiveLock (Locks reads and writes)"
        }
        MaintenanceKind::Vacuum { .. } => {
            "ShareUpdateExclusiveLock (Allows concurrent reads and writes)"
        }
        MaintenanceKind::Analyze { .. } => {
            "ShareUpdateExclusiveLock (Allows concurrent reads and writes)"
        }
        MaintenanceKind::Reindex {
            concurrently: true, ..
        } => "ShareUpdateExclusiveLock (Allows concurrent reads and writes)",
        MaintenanceKind::Reindex {
            concurrently: false,
            ..
        } => "AccessExclusiveLock (Locks reads and writes)",
        MaintenanceKind::Cluster { .. } => "AccessExclusiveLock (Locks reads and writes)",
    }
}

/// Builds the SQL statement for a database maintenance task.
///
/// # Errors
/// Returns an error string if mandatory target relation is omitted for table-scoped operations.
pub fn build_maintenance_sql(opts: &MaintenanceOptions) -> Result<String, String> {
    match &opts.kind {
        MaintenanceKind::Vacuum {
            full,
            freeze,
            verbose,
            analyze,
        } => {
            let mut flags = Vec::new();
            if *full {
                flags.push("FULL");
            }
            if *freeze {
                flags.push("FREEZE");
            }
            if *verbose {
                flags.push("VERBOSE");
            }
            if *analyze {
                flags.push("ANALYZE");
            }

            let opt_clause = if flags.is_empty() {
                String::new()
            } else {
                format!(" ({})", flags.join(", "))
            };

            if let Some(target) = &opts.target {
                let quoted_target = quote_relation_target(target);
                Ok(format!("VACUUM{opt_clause} {quoted_target};"))
            } else {
                Ok(format!("VACUUM{opt_clause};"))
            }
        }
        MaintenanceKind::Analyze { verbose } => {
            let opt_clause = if *verbose { " (VERBOSE)" } else { "" };
            if let Some(target) = &opts.target {
                let quoted_target = quote_relation_target(target);
                Ok(format!("ANALYZE{opt_clause} {quoted_target};"))
            } else {
                Ok(format!("ANALYZE{opt_clause};"))
            }
        }
        MaintenanceKind::Reindex {
            concurrently,
            scope,
        } => {
            let conc_clause = if *concurrently { " CONCURRENTLY" } else { "" };
            match scope {
                ReindexScope::Table => {
                    let target = opts
                        .target
                        .as_ref()
                        .ok_or_else(|| "reindex table requires a target table name".to_string())?;
                    Ok(format!(
                        "REINDEX TABLE{conc_clause} {};",
                        quote_relation_target(target)
                    ))
                }
                ReindexScope::Index => {
                    let target = opts
                        .target
                        .as_ref()
                        .ok_or_else(|| "reindex index requires a target index name".to_string())?;
                    Ok(format!(
                        "REINDEX INDEX{conc_clause} {};",
                        quote_relation_target(target)
                    ))
                }
                ReindexScope::Schema => {
                    let target = opts.target.as_ref().ok_or_else(|| {
                        "reindex schema requires a target schema name".to_string()
                    })?;
                    Ok(format!(
                        "REINDEX SCHEMA{conc_clause} \"{}\";",
                        target.trim_matches('"')
                    ))
                }
                ReindexScope::Database => {
                    if let Some(target) = &opts.target {
                        Ok(format!(
                            "REINDEX DATABASE{conc_clause} \"{}\";",
                            target.trim_matches('"')
                        ))
                    } else {
                        Ok(format!("REINDEX DATABASE{conc_clause} current_database();"))
                    }
                }
            }
        }
        MaintenanceKind::Cluster { verbose } => {
            let opt_clause = if *verbose { " (VERBOSE)" } else { "" };
            if let Some(target) = &opts.target {
                Ok(format!(
                    "CLUSTER{opt_clause} {};",
                    quote_relation_target(target)
                ))
            } else {
                Ok(format!("CLUSTER{opt_clause};"))
            }
        }
    }
}

/// Safely quotes a relation name (e.g. `public.orders` -> `"public"."orders"`).
#[must_use]
pub fn quote_relation_target(raw: &str) -> String {
    let raw = raw.trim();
    if let Some((schema, table)) = raw.split_once('.') {
        format!(
            "\"{}\".\"{}\"",
            schema.trim_matches('"'),
            table.trim_matches('"')
        )
    } else {
        format!("\"{}\"", raw.trim_matches('"'))
    }
}

/// Builds a `LISTEN <channel>;` SQL statement.
#[must_use]
pub fn build_listen_sql(channel: &str) -> String {
    format!("LISTEN \"{}\";", channel.trim_matches('"'))
}

/// Builds an `UNLISTEN <channel>;` SQL statement.
#[must_use]
pub fn build_unlisten_sql(channel: &str) -> String {
    if channel == "*" {
        "UNLISTEN *;".to_string()
    } else {
        format!("UNLISTEN \"{}\";", channel.trim_matches('"'))
    }
}

/// Builds a `NOTIFY <channel>, '<payload>';` SQL statement.
#[must_use]
pub fn build_notify_sql(channel: &str, payload: Option<&str>) -> String {
    let clean_channel = channel.trim_matches('"');
    if let Some(p) = payload {
        let escaped = p.replace('\'', "''");
        format!("NOTIFY \"{clean_channel}\", '{escaped}';")
    } else {
        format!("NOTIFY \"{clean_channel}\";")
    }
}

/// Formats received pub/sub notifications as an aligned plain-text table.
#[must_use]
pub fn render_notifications_table(events: &[PubSubNotification]) -> String {
    if events.is_empty() {
        return "No pub/sub notifications received.".to_string();
    }
    let mut out = String::new();
    out.push_str("TIME                     │ CHANNEL        │ PID    │ PAYLOAD\n");
    out.push_str(
        "─────────────────────────┼────────────────┼────────┼──────────────────────────────────\n",
    );
    for ev in events {
        out.push_str(&format!(
            "{:<24} │ {:<14} │ {:<6} │ {}\n",
            ev.timestamp, ev.channel, ev.sender_pid, ev.payload
        ));
    }
    out
}

/// Formats received pub/sub notifications as line-delimited JSON (JSONL).
#[must_use]
pub fn render_notifications_jsonl(events: &[PubSubNotification]) -> String {
    let mut out = String::new();
    for ev in events {
        if let Ok(line) = serde_json::to_string(ev) {
            out.push_str(&line);
            out.push('\n');
        }
    }
    out
}

/// Formats active maintenance progress telemetry into an aligned table.
#[must_use]
pub fn render_maintenance_progress_table(entries: &[MaintenanceProgressEntry]) -> String {
    if entries.is_empty() {
        return "No active vacuum, reindex, or maintenance operations currently running."
            .to_string();
    }
    let mut out = String::new();
    out.push_str("PID    │ COMMAND │ RELATION             │ PHASE                │ PROGRESS  │ BLOCKS SCANNED\n");
    out.push_str("───────┼─────────┼──────────────────────┼──────────────────────┼───────────┼────────────────\n");
    for e in entries {
        let rel = e.relname.as_deref().unwrap_or("-");
        let pct_str = e
            .percent
            .map(|p| format!("{p:.1}%"))
            .unwrap_or_else(|| "-".to_string());
        let blks = match (e.heap_blks_scanned, e.heap_blks_total) {
            (Some(s), Some(t)) if t > 0 => format!("{s} / {t}"),
            (Some(s), _) => format!("{s}"),
            _ => "-".to_string(),
        };
        out.push_str(&format!(
            "{:<6} │ {:<7} │ {:<20} │ {:<20} │ {:<9} │ {}\n",
            e.pid, e.command, rel, e.phase, pct_str, blks
        ));
    }
    out
}

/// Formats active maintenance progress telemetry as structured JSON.
#[must_use]
pub fn render_maintenance_progress_json(entries: &[MaintenanceProgressEntry]) -> String {
    serde_json::to_string_pretty(entries).unwrap_or_else(|_| "[]".to_string())
}
