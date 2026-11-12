//! Pure transaction, WAL, checkpointer, and replication telemetry domain model.
//!
//! This module implements the domain logic for Feature
//! `036-transaction-and-wal-observability` (`Milestone 7: Enterprise Observability & Query Performance Forensics`).
//! It contains zero terminal, filesystem, or network dependencies: [`TelemetryReport`]
//! is populated from catalog extractors in [`crate::postgres::metadata`], bounded and
//! rendered deterministically into an aligned tabular overview, structured JSON, or plain text.

use serde::{Deserialize, Serialize};
use std::fmt::Write as _;

#[cfg(test)]
mod tests;

/// Checkpointer and background writer operational metrics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct CheckpointerStats {
    /// Number of scheduled checkpoints timed by `checkpoint_timeout`.
    pub timed_checkpoints: u64,
    /// Number of requested/forced checkpoints triggered by `max_wal_size`.
    pub requested_checkpoints: u64,
    /// Total time spent writing checkpoint buffers to disk in milliseconds.
    pub write_time_ms: f64,
    /// Total time spent synchronizing/fsyncing checkpoint buffers in milliseconds.
    pub sync_time_ms: f64,
    /// Number of buffers written during checkpoints.
    pub buffers_checkpoint: u64,
    /// Number of buffers written by the background writer.
    pub buffers_clean: u64,
    /// Number of buffers written directly by backend worker processes.
    pub buffers_backend: u64,
    /// Number of times backend workers were forced to execute fsync calls.
    pub buffers_backend_fsync: u64,
    /// Total shared buffers allocated.
    pub buffers_alloc: u64,
}

impl CheckpointerStats {
    /// Ratio of timed scheduled checkpoints to total checkpoints (0.0 to 100.0).
    ///
    /// Values above 90% indicate healthy WAL sizing where checkpoints occur on schedule.
    /// Values below 75% indicate `max_wal_size` is too low for current write traffic.
    #[must_use]
    pub fn timed_ratio(&self) -> f64 {
        let total = self
            .timed_checkpoints
            .saturating_add(self.requested_checkpoints);
        if total == 0 {
            100.0
        } else {
            (self.timed_checkpoints as f64 / total as f64) * 100.0
        }
    }

    /// Ratio of backend-written buffers to total written buffers (0.0 to 100.0).
    ///
    /// Values above 5% indicate checkpointer/cleaner saturation causing transactional latency spikes.
    #[must_use]
    pub fn backend_write_ratio(&self) -> f64 {
        let total = self
            .buffers_checkpoint
            .saturating_add(self.buffers_clean)
            .saturating_add(self.buffers_backend);
        if total == 0 {
            0.0
        } else {
            (self.buffers_backend as f64 / total as f64) * 100.0
        }
    }
}

/// Write-Ahead Logging (WAL) and archiver telemetry metrics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct WalStats {
    /// Current write-ahead log sequence number (LSN).
    pub current_lsn: String,
    /// Total WAL records written since server startup or stats reset.
    pub wal_records: u64,
    /// Number of full page images (FPI) written to WAL.
    pub wal_fpi: u64,
    /// Total bytes generated in the write-ahead log.
    pub wal_bytes: u64,
    /// Number of times WAL buffers became full.
    pub wal_buffers_full: u64,
    /// Time spent writing WAL buffers to disk in milliseconds.
    pub wal_write_time_ms: f64,
    /// Time spent syncing WAL buffers to disk in milliseconds.
    pub wal_sync_time_ms: f64,
    /// Total WAL segments successfully archived.
    pub archived_count: u64,
    /// Total WAL archive failures recorded by `pg_stat_archiver`.
    pub failed_count: u64,
    /// Name of the most recently archived WAL segment file.
    pub last_archived_wal: Option<String>,
    /// Name of the most recently failed WAL segment file.
    pub last_failed_wal: Option<String>,
}

/// Cumulative transaction throughput and error metrics per database.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct DatabaseXactStats {
    /// Database name.
    pub database_name: String,
    /// Number of committed transactions.
    pub xact_commit: u64,
    /// Number of rolled back transactions.
    pub xact_rollback: u64,
    /// Deadlocks detected in this database.
    pub deadlocks: u64,
    /// Query cancellations due to recovery/standby conflicts.
    pub conflicts: u64,
    /// Number of temporary files created by queries.
    pub temp_files: u64,
    /// Total bytes written to temporary files on disk.
    pub temp_bytes: u64,
}

impl DatabaseXactStats {
    /// Rollback ratio percentage (0.0 to 100.0).
    ///
    /// Ratios above 5% warrant warning; ratios above 20% indicate severe application errors.
    #[must_use]
    pub fn rollback_ratio(&self) -> f64 {
        let total = self.xact_commit.saturating_add(self.xact_rollback);
        if total == 0 {
            0.0
        } else {
            (self.xact_rollback as f64 / total as f64) * 100.0
        }
    }
}

/// Transaction ID (XID) freeze and wraparound horizon telemetry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct XidFreezeMetric {
    /// Database name.
    pub database_name: String,
    /// Transaction ID age of `datfrozenxid`.
    pub datfrozenxid_age: u64,
    /// Configured `autovacuum_freeze_max_age` threshold (typically 200,000,000).
    pub autovacuum_freeze_max_age: u64,
    /// Percentage of the 2-billion emergency wraparound ceiling consumed (0.0 to 100.0).
    pub wraparound_pct: f64,
    /// Name of the table with the oldest `relfrozenxid` in this database.
    pub oldest_table_name: Option<String>,
    /// Age of the oldest table's `relfrozenxid`.
    pub oldest_table_age: u64,
}

/// Active streaming replication standby telemetry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ReplicationStandby {
    /// Process ID of the WAL sender backend.
    pub pid: u32,
    /// IP address or socket host of the connected standby.
    pub client_addr: Option<String>,
    /// Application name declared by the standby.
    pub application_name: String,
    /// Current replication state (e.g. `streaming`, `catchup`).
    pub state: String,
    /// Synchronous replication state (e.g. `sync`, `async`, `potential`).
    pub sync_state: String,
    /// Latest LSN sent to the standby.
    pub sent_lsn: Option<String>,
    /// Unwritten lag in bytes between primary and standby.
    pub write_lag_bytes: u64,
    /// Unflushed lag in bytes.
    pub flush_lag_bytes: u64,
    /// Unapplied replay lag in bytes.
    pub replay_lag_bytes: u64,
    /// Replay latency in milliseconds if measurable.
    pub replay_lag_ms: Option<f64>,
}

/// Complete transaction and WAL observability snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct TelemetryReport {
    /// Checkpointer and background writer metrics.
    pub checkpointer: CheckpointerStats,
    /// Write-ahead logging metrics.
    pub wal: WalStats,
    /// Per-database transaction telemetry.
    pub databases: Vec<DatabaseXactStats>,
    /// Transaction ID freeze metrics.
    pub xid_freeze: Vec<XidFreezeMetric>,
    /// Connected streaming replication standbys.
    pub standbys: Vec<ReplicationStandby>,
}

impl TelemetryReport {
    /// Total committed transactions across all databases.
    #[must_use]
    pub fn total_commits(&self) -> u64 {
        self.databases.iter().map(|d| d.xact_commit).sum()
    }

    /// Total rolled back transactions across all databases.
    #[must_use]
    pub fn total_rollbacks(&self) -> u64 {
        self.databases.iter().map(|d| d.xact_rollback).sum()
    }

    /// Aggregate rollback percentage across all databases.
    #[must_use]
    pub fn aggregate_rollback_ratio(&self) -> f64 {
        let commits = self.total_commits();
        let rollbacks = self.total_rollbacks();
        let total = commits.saturating_add(rollbacks);
        if total == 0 {
            0.0
        } else {
            (rollbacks as f64 / total as f64) * 100.0
        }
    }

    /// Maximum transaction ID age across all databases.
    #[must_use]
    pub fn max_xid_age(&self) -> u64 {
        self.xid_freeze
            .iter()
            .map(|f| f.datfrozenxid_age)
            .max()
            .unwrap_or(0)
    }

    /// Maximum percentage of emergency wraparound horizon consumed.
    #[must_use]
    pub fn max_wraparound_pct(&self) -> f64 {
        self.xid_freeze
            .iter()
            .map(|f| f.wraparound_pct)
            .fold(0.0, f64::max)
    }

    /// Renders an aligned tabular overview for the chosen section.
    #[must_use]
    pub fn render_table(&self, section: TelemetrySection) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "=== Ignatius Transaction & WAL Observability ===");
        let _ = writeln!(
            out,
            "Commits: {} | Rollback Ratio: {:.2}% | WAL LSN: {} | Checkpoints Timed: {:.1}% | Max XID Age: {} ({:.1}%)",
            self.total_commits(),
            self.aggregate_rollback_ratio(),
            self.wal.current_lsn,
            self.checkpointer.timed_ratio(),
            self.max_xid_age(),
            self.max_wraparound_pct()
        );
        let _ = writeln!(
            out,
            "Archived WALs: {} | Archive Failures: {} | Standbys: {}",
            self.wal.archived_count,
            self.wal.failed_count,
            self.standbys.len()
        );
        let _ = writeln!(out);

        if matches!(
            section,
            TelemetrySection::All | TelemetrySection::Checkpointer
        ) {
            let _ = writeln!(out, "-- Checkpointer & Background Writer --");
            let _ = writeln!(
                out,
                "{:<20} {:<12} {:<12} {:<16} {:<16}",
                "TIMED / REQ", "WRITE (MS)", "SYNC (MS)", "CKPT BUFFERS", "BACKEND BUFFERS"
            );
            let _ = writeln!(out, "{}", "-".repeat(78));
            let check_counts = format!(
                "{} / {}",
                self.checkpointer.timed_checkpoints, self.checkpointer.requested_checkpoints
            );
            let _ = writeln!(
                out,
                "{:<20} {:<12.1} {:<12.1} {:<16} {:<16} ({:.1}%)",
                check_counts,
                self.checkpointer.write_time_ms,
                self.checkpointer.sync_time_ms,
                self.checkpointer.buffers_checkpoint,
                self.checkpointer.buffers_backend,
                self.checkpointer.backend_write_ratio()
            );
            let _ = writeln!(out);
        }

        if matches!(section, TelemetrySection::All | TelemetrySection::Wal) {
            let _ = writeln!(out, "-- Write-Ahead Log (WAL) & Archiver --");
            let _ = writeln!(out, "Current LSN:      {}", self.wal.current_lsn);
            let _ = writeln!(
                out,
                "WAL Bytes:        {} ({} records, {} FPI)",
                format_bytes(self.wal.wal_bytes),
                self.wal.wal_records,
                self.wal.wal_fpi
            );
            let _ = writeln!(
                out,
                "WAL Write/Sync:   {:.1} ms write / {:.1} ms sync",
                self.wal.wal_write_time_ms, self.wal.wal_sync_time_ms
            );
            let _ = writeln!(
                out,
                "Archiver Status:  {} archived, {} failed (Last: {})",
                self.wal.archived_count,
                self.wal.failed_count,
                self.wal.last_archived_wal.as_deref().unwrap_or("none")
            );
            let _ = writeln!(out);
        }

        if matches!(
            section,
            TelemetrySection::All | TelemetrySection::Transactions
        ) {
            let _ = writeln!(out, "-- Database Transactions & Spills --");
            let _ = writeln!(
                out,
                "{:<22} {:>14} {:>12} {:>10} {:>10} {:>14}",
                "DATABASE", "COMMITS", "ROLLBACKS", "ROLLBACK %", "DEADLOCKS", "TEMP BYTES"
            );
            let _ = writeln!(out, "{}", "-".repeat(86));
            for db in &self.databases {
                let _ = writeln!(
                    out,
                    "{:<22} {:>14} {:>12} {:>9.2}% {:>10} {:>14}",
                    truncate_str(&db.database_name, 22),
                    db.xact_commit,
                    db.xact_rollback,
                    db.rollback_ratio(),
                    db.deadlocks,
                    format_bytes(db.temp_bytes)
                );
            }
            let _ = writeln!(out);
        }

        if matches!(section, TelemetrySection::All | TelemetrySection::Freeze) {
            let _ = writeln!(out, "-- Transaction ID (XID) Wraparound Forensics --");
            let _ = writeln!(
                out,
                "{:<22} {:>16} {:>10} {:<24} {:>14}",
                "DATABASE", "FROZEN XID AGE", "% HORIZON", "OLDEST TABLE", "TABLE AGE"
            );
            let _ = writeln!(out, "{}", "-".repeat(90));
            for f in &self.xid_freeze {
                let oldest_tbl = f.oldest_table_name.as_deref().unwrap_or("-");
                let _ = writeln!(
                    out,
                    "{:<22} {:>16} {:>9.1}% {:<24} {:>14}",
                    truncate_str(&f.database_name, 22),
                    f.datfrozenxid_age,
                    f.wraparound_pct,
                    truncate_str(oldest_tbl, 24),
                    f.oldest_table_age
                );
            }
            let _ = writeln!(out);
        }

        if matches!(
            section,
            TelemetrySection::All | TelemetrySection::Replication
        ) && !self.standbys.is_empty()
        {
            let _ = writeln!(out, "-- Streaming Replication Standbys --");
            let _ = writeln!(
                out,
                "{:<8} {:<18} {:<16} {:<12} {:>14} {:>12}",
                "PID", "APPLICATION", "STATE", "SYNC", "REPLAY LAG", "LAG (MS)"
            );
            let _ = writeln!(out, "{}", "-".repeat(84));
            for s in &self.standbys {
                let lag_ms_str = s
                    .replay_lag_ms
                    .map(|m| format!("{:.1}", m))
                    .unwrap_or_else(|| "-".to_string());
                let _ = writeln!(
                    out,
                    "{:<8} {:<18} {:<16} {:<12} {:>14} {:>12}",
                    s.pid,
                    truncate_str(&s.application_name, 18),
                    s.state,
                    s.sync_state,
                    format_bytes(s.replay_lag_bytes),
                    lag_ms_str
                );
            }
            let _ = writeln!(out);
        }

        out
    }

    /// Renders the snapshot report as structured JSON.
    #[must_use]
    pub fn render_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string())
    }

    /// Renders plain text output suitable for scripts and terminal inspection.
    #[must_use]
    pub fn render_plain(&self, section: TelemetrySection) -> String {
        self.render_table(section)
    }
}

/// Filter section for telemetry reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TelemetrySection {
    /// Include all telemetry diagnostics.
    #[default]
    All,
    /// Checkpointer and background writer statistics.
    Checkpointer,
    /// Write-Ahead Logging (WAL) and archiver metrics.
    Wal,
    /// Database transaction throughput, rollbacks, and deadlocks.
    Transactions,
    /// Transaction ID (XID) freeze and wraparound horizon forensics.
    Freeze,
    /// Streaming replication standbys and replication lag.
    Replication,
}

impl TelemetrySection {
    /// Accepted CLI strings.
    pub const ACCEPTED: &'static [&'static str] = &[
        "all",
        "checkpointer",
        "ckpt",
        "wal",
        "transactions",
        "xact",
        "freeze",
        "xid",
        "replication",
        "standbys",
    ];

    /// Resolves loosely from user input.
    #[must_use]
    pub fn from_str_loose(s: &str) -> Option<Self> {
        let trimmed = s.trim().to_ascii_lowercase();
        match trimmed.as_str() {
            "all" => Some(Self::All),
            "checkpointer" | "ckpt" | "bgwriter" => Some(Self::Checkpointer),
            "wal" | "archiver" => Some(Self::Wal),
            "transactions" | "xact" | "commits" => Some(Self::Transactions),
            "freeze" | "xid" | "wraparound" => Some(Self::Freeze),
            "replication" | "standbys" | "replicas" => Some(Self::Replication),
            _ => None,
        }
    }
}

/// Helper to format raw bytes into human readable binary units.
#[must_use]
pub fn format_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;

    let b = bytes as f64;
    if b >= GIB {
        format!("{:.1} GB", b / GIB)
    } else if b >= MIB {
        format!("{:.1} MB", b / MIB)
    } else if b >= KIB {
        format!("{:.1} KB", b / KIB)
    } else {
        format!("{} B", bytes)
    }
}

/// Truncates string with ellipsis if exceeding max length.
pub(crate) fn truncate_str(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_len.saturating_sub(3)).collect();
        format!("{}...", truncated)
    }
}
