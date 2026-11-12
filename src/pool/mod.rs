//! Pure connection pool and PgBouncer telemetry domain model.
//!
//! This module implements the domain logic for Feature
//! `037-connection-pool-and-pgbouncer-telemetry` (`Milestone 7: Enterprise Observability & Query Performance Forensics`).
//! It contains zero terminal, filesystem, or network dependencies: [`PoolReport`]
//! is populated from catalog and connection pool queries in [`crate::postgres::metadata`],
//! bounded and rendered deterministically into an aligned tabular overview, structured JSON,
//! or plain text.

use serde::{Deserialize, Serialize};
use std::fmt::Write as _;

#[cfg(test)]
mod tests;

/// Per-pool connection metrics (from PgBouncer `SHOW POOLS` or database grouping).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PgBouncerPool {
    /// Database name.
    pub database: String,
    /// User name.
    pub user: String,
    /// Client connections currently linked to a server connection and executing queries.
    pub cl_active: u64,
    /// Client connections waiting for a server connection.
    pub cl_waiting: u64,
    /// Server connections currently linked to a client.
    pub sv_active: u64,
    /// Server connections idle and immediately available for new client requests.
    pub sv_idle: u64,
    /// Server connections idle for longer than `server_check_delay`.
    pub sv_used: u64,
    /// Server connections currently running `server_check_query`.
    pub sv_tested: u64,
    /// Server connections currently logging in.
    pub sv_login: u64,
    /// Age in seconds of the oldest waiting client.
    pub maxwait_seconds: f64,
    /// Average query execution duration in milliseconds.
    pub avg_query_time_ms: f64,
    /// Pooling mode (`session`, `transaction`, `statement`, or `direct`).
    pub pool_mode: String,
}

/// Server-wide connection saturation, states, and headroom metrics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ServerConnectionStats {
    /// Total established connections to the PostgreSQL instance.
    pub total_connections: u64,
    /// Connections actively executing queries.
    pub active: u64,
    /// Connections idling outside of a transaction.
    pub idle: u64,
    /// Connections idling inside an open transaction block.
    pub idle_in_transaction: u64,
    /// Connections idling inside an aborted transaction block.
    pub idle_in_transaction_aborted: u64,
    /// Fastpath function call executions.
    pub fastpath: u64,
    /// Disabled connection slots.
    pub disabled: u64,
    /// Configured `max_connections` ceiling.
    pub max_connections: u64,
    /// Configured `superuser_reserved_connections` reserved slots.
    pub superuser_reserved_connections: u64,
}

impl ServerConnectionStats {
    /// Usable connection ceiling for application workloads.
    #[must_use]
    pub fn usable_max_connections(&self) -> u64 {
        self.max_connections
            .saturating_sub(self.superuser_reserved_connections)
    }

    /// Remaining connection headroom before non-superuser connections are refused.
    #[must_use]
    pub fn headroom(&self) -> u64 {
        self.usable_max_connections()
            .saturating_sub(self.total_connections)
    }

    /// Overall connection saturation percentage (0.0 to 100.0).
    #[must_use]
    pub fn saturation_ratio(&self) -> f64 {
        if self.max_connections == 0 {
            0.0
        } else {
            (self.total_connections as f64 / self.max_connections as f64) * 100.0
        }
    }

    /// Whether server connection capacity is approaching critical exhaustion (>= 85%).
    #[must_use]
    pub fn is_critical(&self) -> bool {
        self.saturation_ratio() >= 85.0
    }
}

/// Client connection actively blocked or waiting on pool resources or locks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ClientWaitMetric {
    /// Process ID of the backend worker or client socket.
    pub pid: u32,
    /// Database name.
    pub database: String,
    /// User name.
    pub user: String,
    /// Application name declared by client.
    pub application_name: String,
    /// Host or address of client.
    pub client_addr: Option<String>,
    /// Wait event type (e.g. `Lock`, `IO`, `Client`).
    pub wait_event_type: Option<String>,
    /// Exact wait event name.
    pub wait_event: Option<String>,
    /// Current query text.
    pub query: String,
    /// Connection state (e.g. `active`, `idle in transaction`).
    pub state: String,
    /// Active duration of this query or transaction in seconds.
    pub duration_seconds: f64,
}

/// Complete connection pool and server saturation telemetry report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PoolReport {
    /// Whether connected through a PgBouncer administrative pooler console.
    pub is_pgbouncer: bool,
    /// Server-level connection exhaustion and saturation telemetry.
    pub server_stats: ServerConnectionStats,
    /// Active pools tracked.
    pub pools: Vec<PgBouncerPool>,
    /// Waiting or lock-blocked client connections.
    pub waiting_clients: Vec<ClientWaitMetric>,
}

impl PoolReport {
    /// Total client connections across all pools.
    #[must_use]
    pub fn total_client_active(&self) -> u64 {
        self.pools.iter().map(|p| p.cl_active).sum()
    }

    /// Total clients waiting in queue across all pools.
    #[must_use]
    pub fn total_client_waiting(&self) -> u64 {
        self.pools.iter().map(|p| p.cl_waiting).sum()
    }

    /// Renders an aligned tabular overview for the chosen section.
    #[must_use]
    pub fn render_table(&self, section: PoolSection) -> String {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "=== Ignatius Connection Pool & Saturation Telemetry ==="
        );
        let _ = writeln!(
            out,
            "Connections: {}/{} ({:.1}%) | Headroom: {} | Mode: {} | Waiting Clients: {}",
            self.server_stats.total_connections,
            self.server_stats.max_connections,
            self.server_stats.saturation_ratio(),
            self.server_stats.headroom(),
            if self.is_pgbouncer {
                "PgBouncer"
            } else {
                "Direct PostgreSQL"
            },
            self.total_client_waiting()
        );
        let _ = writeln!(out);

        if matches!(section, PoolSection::All | PoolSection::Server) {
            let _ = writeln!(out, "-- Server Connection Saturation & States --");
            let _ = writeln!(
                out,
                "{:<10} {:<10} {:<16} {:<16} {:<10} {:<10}",
                "ACTIVE", "IDLE", "IDLE IN XACT", "ABORTED XACT", "RESERVED", "HEADROOM"
            );
            let _ = writeln!(out, "{}", "-".repeat(78));
            let _ = writeln!(
                out,
                "{:<10} {:<10} {:<16} {:<16} {:<10} {:<10}",
                self.server_stats.active,
                self.server_stats.idle,
                self.server_stats.idle_in_transaction,
                self.server_stats.idle_in_transaction_aborted,
                self.server_stats.superuser_reserved_connections,
                self.server_stats.headroom()
            );
            let _ = writeln!(out);
        }

        if matches!(section, PoolSection::All | PoolSection::Pools) && !self.pools.is_empty() {
            let _ = writeln!(out, "-- Pool Allocations & Client Queues --");
            let _ = writeln!(
                out,
                "{:<20} {:<16} {:>8} {:>8} {:>8} {:>8} {:>10} {:<12}",
                "DATABASE", "USER", "CL_ACT", "CL_WAIT", "SV_ACT", "SV_IDLE", "MAXWAIT(S)", "MODE"
            );
            let _ = writeln!(out, "{}", "-".repeat(96));
            for p in &self.pools {
                let _ = writeln!(
                    out,
                    "{:<20} {:<16} {:>8} {:>8} {:>8} {:>8} {:>10.2} {:<12}",
                    truncate_str(&p.database, 20),
                    truncate_str(&p.user, 16),
                    p.cl_active,
                    p.cl_waiting,
                    p.sv_active,
                    p.sv_idle,
                    p.maxwait_seconds,
                    p.pool_mode
                );
            }
            let _ = writeln!(out);
        }

        if matches!(section, PoolSection::All | PoolSection::Wait)
            && !self.waiting_clients.is_empty()
        {
            let _ = writeln!(out, "-- Blocked & Waiting Client Forensics --");
            let _ = writeln!(
                out,
                "{:<8} {:<16} {:<14} {:<18} {:<12} {:<18} {:>10}",
                "PID", "DATABASE", "USER", "APPLICATION", "WAIT TYPE", "EVENT", "DURATION(S)"
            );
            let _ = writeln!(out, "{}", "-".repeat(100));
            for w in &self.waiting_clients {
                let _ = writeln!(
                    out,
                    "{:<8} {:<16} {:<14} {:<18} {:<12} {:<18} {:>10.2}",
                    w.pid,
                    truncate_str(&w.database, 16),
                    truncate_str(&w.user, 14),
                    truncate_str(&w.application_name, 18),
                    w.wait_event_type.as_deref().unwrap_or("-"),
                    w.wait_event.as_deref().unwrap_or("-"),
                    w.duration_seconds
                );
            }
            let _ = writeln!(out);
        }

        out
    }

    /// Renders the report as structured JSON.
    #[must_use]
    pub fn render_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string())
    }

    /// Renders plain text output suitable for scripts.
    #[must_use]
    pub fn render_plain(&self, section: PoolSection) -> String {
        self.render_table(section)
    }
}

/// Section filter for connection pool reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PoolSection {
    /// Include all diagnostics (server saturation, pools, waiting clients).
    #[default]
    All,
    /// Pool allocations and queue metrics.
    Pools,
    /// Server connection capacity, state breakdown, and headroom.
    Server,
    /// Waiting or lock-blocked client forensics.
    Wait,
}

impl PoolSection {
    /// Accepted CLI strings.
    pub const ACCEPTED: &'static [&'static str] =
        &["all", "pools", "server", "connections", "wait", "queues"];

    /// Resolves loosely from user input.
    #[must_use]
    pub fn from_str_loose(s: &str) -> Option<Self> {
        let trimmed = s.trim().to_ascii_lowercase();
        match trimmed.as_str() {
            "all" => Some(Self::All),
            "pools" | "pool" => Some(Self::Pools),
            "server" | "connections" | "conn" | "saturation" => Some(Self::Server),
            "wait" | "queues" | "waiting" => Some(Self::Wait),
            _ => None,
        }
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
