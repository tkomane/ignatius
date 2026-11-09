//! Pure server activity, lock blocker tree, and database telemetry model.
//!
//! This module implements the domain logic for Feature
//! `029-server-activity-and-locks-dashboard` (`M6: Full pgAdmin Replacement Parity`).
//! It depends on no terminal, filesystem, or network types: [`ServerActivitySnapshot`]
//! is populated from live `pg_stat_activity`, `pg_locks`, and `pg_stat_database`
//! queries ([`crate::postgres::metadata::server_activity`]), bounded at
//! [`MAX_ACTIVITY_BACKENDS`], and rendered deterministically into either a
//! live backend activity view, a hierarchical lock blocker tree view, or a
//! structured JSON document.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

/// Maximum number of active backend processes retained in a single snapshot.
pub const MAX_ACTIVITY_BACKENDS: usize = 200;

/// Maximum displayed character count for a backend SQL snippet in summary rows.
pub const MAX_QUERY_SNIPPET_CHARS: usize = 96;

/// Which view mode is active in the Server Activity & Locks dashboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ActivityViewMode {
    /// Tabular list of active and idle backends with database health metrics.
    #[default]
    Backends,
    /// Hierarchical lock blocker chains showing who holds root locks and who waits.
    LocksTree,
}

impl ActivityViewMode {
    /// Short human-readable label for overlay chrome.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Backends => "Active Backends",
            Self::LocksTree => "Lock Blocker Tree",
        }
    }

    /// Toggles between [`Self::Backends`] and [`Self::LocksTree`].
    #[must_use]
    pub const fn toggle(self) -> Self {
        match self {
            Self::Backends => Self::LocksTree,
            Self::LocksTree => Self::Backends,
        }
    }
}

/// Output format for the `ignatius activity` CLI subcommand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ActivityOutputFormat {
    /// Human-readable terminal table / hierarchical tree.
    #[default]
    Table,
    /// Machine-readable JSON snapshot.
    Json,
}

/// Summary statistics for the currently connected database from `pg_stat_database`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DatabaseHealthMetrics {
    /// Database name.
    pub database: String,
    /// Number of currently connected backends (`numbackends`).
    pub active_connections: i64,
    /// Total transactions committed (`xact_commit`).
    pub xact_commit: i64,
    /// Total transactions rolled back (`xact_rollback`).
    pub xact_rollback: i64,
    /// Buffer cache hit ratio in basis points (`0..=10000`, e.g. `9942` = `99.42%`).
    pub cache_hit_bps: i64,
    /// Deadlocks detected since stats reset (`deadlocks`).
    pub deadlocks: i64,
    /// Temporary bytes written to disk (`temp_bytes`).
    pub temp_bytes: i64,
}

impl DatabaseHealthMetrics {
    /// Formats `cache_hit_bps` as a percentage string such as `99.42%`.
    #[must_use]
    pub fn cache_hit_label(&self) -> String {
        let clamped = self.cache_hit_bps.clamp(0, 10_000);
        let whole = clamped / 100;
        let frac = (clamped % 100).abs();
        format!("{whole}.{frac:02}%")
    }
}

/// One backend process entry from `pg_stat_activity` + `pg_blocking_pids(pid)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendProcess {
    /// PostgreSQL backend process ID (`pid`).
    pub pid: i32,
    /// Connected database name (`datname`).
    pub database: String,
    /// Authenticated role name (`usename`).
    pub username: String,
    /// Client `application_name`.
    pub application_name: String,
    /// Client network address (`client_addr`), or `"local"` for Unix sockets.
    pub client_addr: String,
    /// Backend state (`active`, `idle`, `idle in transaction`, etc.).
    pub state: String,
    /// Wait event type (`Lock`, `LWLock`, `Client`, `IO`, etc.), if waiting.
    pub wait_event_type: Option<String>,
    /// Specific wait event name (`relation`, `transactionid`, `ClientRead`, etc.).
    pub wait_event: Option<String>,
    /// Elapsed milliseconds since the current query started (`>= 0`).
    pub query_age_ms: i64,
    /// Elapsed milliseconds since the current transaction started, if in a transaction.
    pub xact_age_ms: Option<i64>,
    /// Current or most recent SQL text for this backend.
    pub query: String,
    /// Process IDs of backends that are currently blocking this backend (`pg_blocking_pids`).
    pub blocked_by: Vec<i32>,
    /// Summary of the lock mode or target this backend is waiting on or holding.
    pub lock_summary: Option<String>,
    /// Whether this backend is the current Ignatius inspection session itself.
    pub is_self: bool,
}

impl BackendProcess {
    /// Compact human-readable age label (`12ms`, `4.2s`, `3m 12s`).
    #[must_use]
    pub fn age_label(&self) -> String {
        format_duration_ms(self.query_age_ms)
    }

    /// Wait badge combining `wait_event_type` and `wait_event` (`Lock:transactionid` or `-`).
    #[must_use]
    pub fn wait_label(&self) -> String {
        match (&self.wait_event_type, &self.wait_event) {
            (Some(kind), Some(ev)) if !kind.is_empty() && !ev.is_empty() => format!("{kind}:{ev}"),
            (Some(kind), _) if !kind.is_empty() => kind.clone(),
            (_, Some(ev)) if !ev.is_empty() => ev.clone(),
            _ => "-".to_owned(),
        }
    }

    /// Single-line sanitized SQL snippet bounded to [`MAX_QUERY_SNIPPET_CHARS`].
    #[must_use]
    pub fn query_snippet(&self) -> String {
        collapse_whitespace(&self.query, MAX_QUERY_SNIPPET_CHARS)
    }
}

/// One node in a hierarchical lock blocker chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockTreeNode {
    /// The backend process at this position in the chain.
    pub backend: BackendProcess,
    /// Nesting depth (`0` = root blocker holding a lock without being blocked).
    pub depth: usize,
    /// Direct waiter PIDs blocked by this process.
    pub blocking_pids: Vec<i32>,
}

/// Complete bounded server activity and lock contention snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ServerActivitySnapshot {
    /// Database-wide health counters.
    pub metrics: DatabaseHealthMetrics,
    /// Active and idle backends ordered deterministically (blocked/active first, then by age/PID).
    pub backends: Vec<BackendProcess>,
    /// Set when total server backends exceeded [`MAX_ACTIVITY_BACKENDS`].
    pub truncated_from: Option<usize>,
}

impl ServerActivitySnapshot {
    /// Sorts backends deterministically and enforces [`MAX_ACTIVITY_BACKENDS`].
    #[must_use]
    pub fn bounded(mut self) -> Self {
        self.backends.sort_by(|a, b| {
            let a_blocked = !a.blocked_by.is_empty();
            let b_blocked = !b.blocked_by.is_empty();
            let a_active = a.state == "active" || a.state.starts_with("idle in transaction");
            let b_active = b.state == "active" || b.state.starts_with("idle in transaction");
            b_blocked
                .cmp(&a_blocked)
                .then_with(|| b_active.cmp(&a_active))
                .then_with(|| b.query_age_ms.cmp(&a.query_age_ms))
                .then_with(|| a.pid.cmp(&b.pid))
        });
        let total = self.backends.len();
        if total > MAX_ACTIVITY_BACKENDS {
            self.backends.truncate(MAX_ACTIVITY_BACKENDS);
            self.truncated_from = Some(total);
        }
        self
    }

    /// Computes the hierarchical lock blocker tree from `self.backends`.
    ///
    /// Root nodes (`depth = 0`) are backends that block at least one other backend
    /// and are not themselves blocked by any known backend. Waiters appear indented
    /// underneath their blocker(s), with cycle detection to guarantee termination.
    #[must_use]
    pub fn lock_blocker_tree(&self) -> Vec<LockTreeNode> {
        let by_pid: BTreeMap<i32, &BackendProcess> =
            self.backends.iter().map(|b| (b.pid, b)).collect();
        let mut dependents: BTreeMap<i32, BTreeSet<i32>> = BTreeMap::new();
        let mut blocked_pids: BTreeSet<i32> = BTreeSet::new();
        let mut blocker_pids: BTreeSet<i32> = BTreeSet::new();

        for backend in &self.backends {
            for &blocker in &backend.blocked_by {
                if blocker != backend.pid {
                    dependents.entry(blocker).or_default().insert(backend.pid);
                    blocked_pids.insert(backend.pid);
                    blocker_pids.insert(blocker);
                }
            }
        }

        if blocker_pids.is_empty() && blocked_pids.is_empty() {
            return Vec::new();
        }

        // Root blockers: processes that block others and are not themselves blocked.
        let mut roots: Vec<i32> = blocker_pids
            .iter()
            .copied()
            .filter(|pid| !blocked_pids.contains(pid))
            .collect();
        // If a deadlock cycle exists where every blocker is also blocked, include all blockers as roots.
        if roots.is_empty() {
            roots.extend(blocker_pids.iter().copied());
        }
        roots.sort_unstable();

        let mut nodes = Vec::new();
        let mut visited_path = BTreeSet::new();
        let mut emitted = BTreeSet::new();

        for root_pid in roots {
            Self::append_lock_subtree(
                root_pid,
                0,
                &by_pid,
                &dependents,
                &mut visited_path,
                &mut emitted,
                &mut nodes,
            );
        }

        // Also include any blocked backend whose blocker was not in `roots`.
        for &blocked_pid in &blocked_pids {
            if !emitted.contains(&blocked_pid) {
                Self::append_lock_subtree(
                    blocked_pid,
                    0,
                    &by_pid,
                    &dependents,
                    &mut visited_path,
                    &mut emitted,
                    &mut nodes,
                );
            }
        }

        nodes
    }

    fn append_lock_subtree(
        pid: i32,
        depth: usize,
        by_pid: &BTreeMap<i32, &BackendProcess>,
        dependents: &BTreeMap<i32, BTreeSet<i32>>,
        visited_path: &mut BTreeSet<i32>,
        emitted: &mut BTreeSet<i32>,
        out: &mut Vec<LockTreeNode>,
    ) {
        if !visited_path.insert(pid) {
            return;
        }
        emitted.insert(pid);
        let children: Vec<i32> = dependents
            .get(&pid)
            .map(|set| set.iter().copied().collect())
            .unwrap_or_default();

        let backend = by_pid.get(&pid).map_or_else(
            || BackendProcess {
                pid,
                database: "-".to_owned(),
                username: "unknown".to_owned(),
                application_name: String::new(),
                client_addr: "-".to_owned(),
                state: "unknown".to_owned(),
                wait_event_type: None,
                wait_event: None,
                query_age_ms: 0,
                xact_age_ms: None,
                query: "<backend not visible>".to_owned(),
                blocked_by: Vec::new(),
                lock_summary: None,
                is_self: false,
            },
            |b| (*b).clone(),
        );

        out.push(LockTreeNode {
            backend,
            depth,
            blocking_pids: children.clone(),
        });

        for child_pid in children {
            Self::append_lock_subtree(
                child_pid,
                depth + 1,
                by_pid,
                dependents,
                visited_path,
                emitted,
                out,
            );
        }
        visited_path.remove(&pid);
    }

    /// Generates a safe, reviewed SQL statement to cancel (`pg_cancel_backend`)
    /// or terminate (`pg_terminate_backend`) a target PID, including an
    /// explanatory comment header with the target backend's identity.
    #[must_use]
    pub fn reviewed_backend_signal_sql(backend: &BackendProcess, terminate: bool) -> String {
        let fn_name = if terminate {
            "pg_terminate_backend"
        } else {
            "pg_cancel_backend"
        };
        let action_word = if terminate {
            "TERMINATE session"
        } else {
            "CANCEL running query"
        };
        let snippet = backend.query_snippet();
        format!(
            "-- Reviewed Server Activity action: {action_word} for PID {pid}\n\
             -- Role: {user} | Database: {db} | State: {state} | Age: {age}\n\
             -- Query: {snippet}\n\
             SELECT pg_catalog.{fn_name}({pid});\n",
            pid = backend.pid,
            user = backend.username,
            db = backend.database,
            state = backend.state,
            age = backend.age_label(),
        )
    }

    /// Renders the snapshot as human-readable text for the TUI overlay or CLI `--format table`.
    #[must_use]
    pub fn render_text(&self, mode: ActivityViewMode, unicode: bool) -> String {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "Database: {} | Connections: {} | Cache Hit: {} | Commits: {} | Rollbacks: {} | Deadlocks: {}",
            if self.metrics.database.is_empty() {
                "-"
            } else {
                &self.metrics.database
            },
            self.metrics.active_connections,
            self.metrics.cache_hit_label(),
            self.metrics.xact_commit,
            self.metrics.xact_rollback,
            self.metrics.deadlocks,
        );
        let rule_ch = if unicode { '─' } else { '-' };
        let _ = writeln!(out, "{}", rule_ch.to_string().repeat(100));

        match mode {
            ActivityViewMode::Backends => {
                let _ = writeln!(
                    out,
                    "{:<8} {:<14} {:<16} {:<20} {:<18} {:<8} {:<12} QUERY",
                    "PID", "USER", "APPLICATION", "STATE", "WAIT", "AGE", "BLOCKED BY"
                );
                let _ = writeln!(out, "{}", rule_ch.to_string().repeat(100));
                if self.backends.is_empty() {
                    let _ = writeln!(out, "(no active backend processes reported)");
                }
                for b in &self.backends {
                    let blocked = if b.blocked_by.is_empty() {
                        "-".to_owned()
                    } else {
                        b.blocked_by
                            .iter()
                            .map(i32::to_string)
                            .collect::<Vec<_>>()
                            .join(",")
                    };
                    let app = if b.application_name.is_empty() {
                        "-"
                    } else {
                        &b.application_name
                    };
                    let _ = writeln!(
                        out,
                        "{:<8} {:<14} {:<16} {:<20} {:<18} {:<8} {:<12} {}",
                        b.pid,
                        truncate_cell(&b.username, 14),
                        truncate_cell(app, 16),
                        truncate_cell(&b.state, 20),
                        truncate_cell(&b.wait_label(), 18),
                        b.age_label(),
                        truncate_cell(&blocked, 12),
                        b.query_snippet(),
                    );
                }
            }
            ActivityViewMode::LocksTree => {
                let tree = self.lock_blocker_tree();
                if tree.is_empty() {
                    let _ = writeln!(
                        out,
                        "No lock contention detected (0 blocking chains in pg_locks / pg_stat_activity)."
                    );
                } else {
                    let _ = writeln!(
                        out,
                        "Active Lock Blocker Chains ({} process(es) involved):",
                        tree.len()
                    );
                    let _ = writeln!(out, "{}", rule_ch.to_string().repeat(100));
                    for node in &tree {
                        let prefix = if node.depth == 0 {
                            String::new()
                        } else if unicode {
                            format!("{}└─ ", "  ".repeat(node.depth.saturating_sub(1)))
                        } else {
                            format!("{}+- ", "  ".repeat(node.depth.saturating_sub(1)))
                        };
                        let role_tag = if node.depth == 0 {
                            "BLOCKER"
                        } else {
                            "WAITING"
                        };
                        let lock_note = node.backend.lock_summary.as_deref().unwrap_or("-");
                        let _ = writeln!(
                            out,
                            "{prefix}[{role_tag}] PID {pid} ({user}) state={state} wait={wait} lock={lock_note} age={age} | {query}",
                            pid = node.backend.pid,
                            user = node.backend.username,
                            state = node.backend.state,
                            wait = node.backend.wait_label(),
                            age = node.backend.age_label(),
                            query = node.backend.query_snippet(),
                        );
                    }
                }
            }
        }

        if let Some(total) = self.truncated_from {
            let _ = writeln!(
                out,
                "-- Note: showing first {} of {} backend processes.",
                MAX_ACTIVITY_BACKENDS, total
            );
        }
        out
    }

    /// Renders the snapshot as a deterministic JSON document for `ignatius activity --format json`.
    #[must_use]
    pub fn render_json(&self) -> String {
        let tree = self.lock_blocker_tree();
        let mut out = String::new();
        out.push_str("{\n");
        let _ = writeln!(
            out,
            "  \"database\": {},\n  \"active_connections\": {},\n  \"cache_hit_bps\": {},\n  \"cache_hit_ratio\": {},\n  \"xact_commit\": {},\n  \"xact_rollback\": {},\n  \"deadlocks\": {},\n  \"temp_bytes\": {},",
            json_string(&self.metrics.database),
            self.metrics.active_connections,
            self.metrics.cache_hit_bps,
            json_string(&self.metrics.cache_hit_label()),
            self.metrics.xact_commit,
            self.metrics.xact_rollback,
            self.metrics.deadlocks,
            self.metrics.temp_bytes,
        );
        out.push_str("  \"backends\": [\n");
        for (idx, b) in self.backends.iter().enumerate() {
            let comma = if idx + 1 == self.backends.len() {
                ""
            } else {
                ","
            };
            let blocked_by = b
                .blocked_by
                .iter()
                .map(i32::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            let _ = writeln!(
                out,
                "    {{\"pid\": {}, \"database\": {}, \"username\": {}, \"application_name\": {}, \"client_addr\": {}, \"state\": {}, \"wait_event_type\": {}, \"wait_event\": {}, \"query_age_ms\": {}, \"blocked_by\": [{}], \"lock_summary\": {}, \"query\": {}}}{comma}",
                b.pid,
                json_string(&b.database),
                json_string(&b.username),
                json_string(&b.application_name),
                json_string(&b.client_addr),
                json_string(&b.state),
                json_opt_string(b.wait_event_type.as_deref()),
                json_opt_string(b.wait_event.as_deref()),
                b.query_age_ms,
                blocked_by,
                json_opt_string(b.lock_summary.as_deref()),
                json_string(&b.query),
            );
        }
        out.push_str("  ],\n");
        out.push_str("  \"lock_blocker_tree\": [\n");
        for (idx, node) in tree.iter().enumerate() {
            let comma = if idx + 1 == tree.len() { "" } else { "," };
            let blocking = node
                .blocking_pids
                .iter()
                .map(i32::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            let _ = writeln!(
                out,
                "    {{\"pid\": {}, \"depth\": {}, \"username\": {}, \"state\": {}, \"blocking_pids\": [{}], \"query\": {}}}{comma}",
                node.backend.pid,
                node.depth,
                json_string(&node.backend.username),
                json_string(&node.backend.state),
                blocking,
                json_string(&node.backend.query),
            );
        }
        out.push_str("  ]\n}\n");
        out
    }
}

fn format_duration_ms(ms: i64) -> String {
    let ms = ms.max(0);
    if ms < 1_000 {
        format!("{ms}ms")
    } else if ms < 60_000 {
        let secs = ms / 1_000;
        let tenths = (ms % 1_000) / 100;
        format!("{secs}.{tenths}s")
    } else {
        let mins = ms / 60_000;
        let secs = (ms % 60_000) / 1_000;
        format!("{mins}m{secs:02}s")
    }
}

fn collapse_whitespace(input: &str, max_chars: usize) -> String {
    let mut collapsed = String::new();
    for word in input.split_whitespace() {
        if !collapsed.is_empty() {
            collapsed.push(' ');
        }
        collapsed.push_str(word);
        if collapsed.chars().count() > max_chars {
            break;
        }
    }
    let count = collapsed.chars().count();
    if count <= max_chars {
        collapsed
    } else {
        let kept: String = collapsed
            .chars()
            .take(max_chars.saturating_sub(3))
            .collect();
        format!("{kept}...")
    }
}

fn truncate_cell(input: &str, max_chars: usize) -> String {
    let count = input.chars().count();
    if count <= max_chars {
        input.to_owned()
    } else if max_chars <= 3 {
        input.chars().take(max_chars).collect()
    } else {
        let kept: String = input.chars().take(max_chars - 3).collect();
        format!("{kept}...")
    }
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

fn json_opt_string(value: Option<&str>) -> String {
    value.map_or_else(|| "null".to_owned(), json_string)
}

#[cfg(test)]
mod tests;
