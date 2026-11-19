//! Pure logical replication and replication slot safety domain model.
//!
//! This module implements the domain logic for Feature
//! `051-logical-replication-and-slot-manager` (`Milestone 12: Complete psql & Server/Replication Parity`).
//! It contains zero database, filesystem, network, or terminal dependencies:
//! [`ReplicationReport`] is populated from bounded catalog extractors in
//! [`crate::postgres::replication`], classifies slot WAL retention risk,
//! redacts subscription connection strings through [`crate::diagnostics::redaction`],
//! renders deterministic table, plain, and JSON output, and builds reviewed SQL
//! statements that are never executed automatically.

#[cfg(test)]
mod tests;

use std::fmt;
use std::fmt::Write as _;

use serde::{Deserialize, Serialize};

use crate::diagnostics::redaction::redact_connection_string;
use crate::query::identifiers::quote_identifier;

/// Maximum number of publications retained in one report.
pub const MAX_PUBLICATIONS: usize = 200;

/// Maximum number of tables retained per publication in one report.
pub const MAX_PUBLICATION_TABLES: usize = 200;

/// Maximum number of subscriptions retained in one report.
pub const MAX_SUBSCRIPTIONS: usize = 200;

/// Maximum number of replication slots retained in one report.
pub const MAX_REPLICATION_SLOTS: usize = 500;

/// Retained WAL bytes on an inactive slot that triggers a warning (100 MiB).
pub const INACTIVE_SLOT_WARNING_BYTES: i64 = 100 * 1024 * 1024;

/// Retained WAL bytes on an inactive slot that triggers a critical finding (1 GiB).
pub const INACTIVE_SLOT_CRITICAL_BYTES: i64 = 1024 * 1024 * 1024;

/// Section filter for replication inspection reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReplicationSection {
    /// Include publications, subscriptions, and replication slots.
    #[default]
    All,
    /// Include publications only.
    Publications,
    /// Include subscriptions only.
    Subscriptions,
    /// Include replication slots only.
    Slots,
}

impl ReplicationSection {
    /// Accepted CLI section names.
    pub const ACCEPTED: &'static [&'static str] = &[
        "all",
        "publications",
        "pubs",
        "subscriptions",
        "subs",
        "slots",
    ];

    /// Stable lowercase section label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Publications => "publications",
            Self::Subscriptions => "subscriptions",
            Self::Slots => "slots",
        }
    }

    /// Resolves a section filter loosely from user or CLI input.
    #[must_use]
    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "all" => Some(Self::All),
            "publications" | "publication" | "pubs" | "pub" => Some(Self::Publications),
            "subscriptions" | "subscription" | "subs" | "sub" => Some(Self::Subscriptions),
            "slots" | "slot" => Some(Self::Slots),
            _ => None,
        }
    }
}

impl fmt::Display for ReplicationSection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// Risk severity classification for a PostgreSQL replication slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SlotRiskSeverity {
    /// Slot is active and WAL status is healthy.
    #[default]
    Ok,
    /// Slot is inactive, unreserved, or approaching retained WAL warning thresholds.
    Warning,
    /// Slot has lost required WAL or is inactive above the critical WAL retention threshold.
    Critical,
}

impl SlotRiskSeverity {
    /// Stable lowercase severity label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Warning => "warning",
            Self::Critical => "critical",
        }
    }
}

impl fmt::Display for SlotRiskSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// Summary of one logical publication from `pg_publication` and `pg_publication_tables`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PublicationSummary {
    /// Publication name.
    pub name: String,
    /// Owning role name.
    pub owner: String,
    /// Whether the publication covers all tables in the database (`puballtables`).
    pub all_tables: bool,
    /// Whether `INSERT` changes are replicated (`pubinsert`).
    pub insert: bool,
    /// Whether `UPDATE` changes are replicated (`pubupdate`).
    pub update: bool,
    /// Whether `DELETE` changes are replicated (`pubdelete`).
    pub delete: bool,
    /// Whether `TRUNCATE` operations are replicated (`pubtruncate`).
    pub truncate: bool,
    /// Qualified table names (`schema.table`) retained up to [`MAX_PUBLICATION_TABLES`].
    pub tables: Vec<String>,
    /// Whether the retained table list was truncated at the bound.
    pub tables_truncated: bool,
}

impl PublicationSummary {
    /// Compact comma-separated label of published DML operations.
    #[must_use]
    pub fn operations_label(&self) -> String {
        let mut ops = Vec::with_capacity(4);
        if self.insert {
            ops.push("INSERT");
        }
        if self.update {
            ops.push("UPDATE");
        }
        if self.delete {
            ops.push("DELETE");
        }
        if self.truncate {
            ops.push("TRUNCATE");
        }
        if ops.is_empty() {
            "none".to_owned()
        } else {
            ops.join(", ")
        }
    }
}

/// Summary of one logical subscription from `pg_subscription` and `pg_stat_subscription`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SubscriptionSummary {
    /// Subscription name.
    pub name: String,
    /// Owning role name.
    pub owner: String,
    /// Whether the subscription is currently enabled (`subenabled`).
    pub enabled: bool,
    /// Publisher connection string with credentials stripped via [`redact_conninfo`].
    pub conninfo_redacted: String,
    /// Replication slot name on the publisher, or `None` when `slot_name = NONE`.
    pub slot_name: Option<String>,
    /// Derived worker and relation sync state (`streaming`, `syncing`, `initializing`, `disconnected`, `disabled`).
    pub sync_state: String,
    /// Latest WAL location received from the publisher (`pg_stat_subscription.received_lsn`).
    pub received_lsn: Option<String>,
    /// Latest WAL location reported back to the publisher (`pg_stat_subscription.latest_end_lsn`).
    pub latest_end_lsn: Option<String>,
    /// Byte difference between `received_lsn` and `latest_end_lsn`, when both are available.
    pub lag_bytes: Option<i64>,
}

/// Redacts passwords from a subscription connection string (libpq keyword/value or URI)
/// through the central [`crate::diagnostics::redaction`] implementation.
#[must_use]
pub fn redact_conninfo(raw: &str) -> String {
    redact_connection_string(raw)
}

/// Summary and safety classification for one replication slot from `pg_replication_slots`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ReplicationSlotSummary {
    /// Replication slot name.
    pub slot_name: String,
    /// Output plugin for logical slots (e.g., `pgoutput`), or `None` for physical slots.
    pub plugin: Option<String>,
    /// Slot type (`logical` or `physical`).
    pub slot_type: String,
    /// Database name for logical slots, or `None` for physical slots.
    pub database: Option<String>,
    /// Whether the slot is temporary.
    pub temporary: bool,
    /// Whether a consumer is currently attached to the slot.
    pub active: bool,
    /// Backend PID of the active consumer, if connected.
    pub active_pid: Option<i32>,
    /// PostgreSQL WAL availability state (`reserved`, `extended`, `unreserved`, `lost`, or `unknown`).
    pub wal_status: String,
    /// Bytes retained between `restart_lsn` and `pg_current_wal_lsn()`, or `None` in recovery or when unset.
    pub retained_bytes: Option<i64>,
    /// Remaining WAL bytes before slot invalidation (`pg_replication_slots.safe_wal_size`), if bounded.
    pub safe_wal_size: Option<i64>,
    /// Computed operational risk severity.
    pub risk: SlotRiskSeverity,
    /// Human-readable explanation when [`Self::risk`] is `Warning` or `Critical`.
    pub finding: Option<String>,
}

impl ReplicationSlotSummary {
    /// Classifies slot risk and produces an accompanying finding message when elevated.
    #[must_use]
    pub fn assess_risk(
        active: bool,
        wal_status: &str,
        retained_bytes: Option<i64>,
    ) -> (SlotRiskSeverity, Option<String>) {
        let status_norm = wal_status.trim();
        if status_norm.eq_ignore_ascii_case("lost") {
            return (
                SlotRiskSeverity::Critical,
                Some(
                    "required WAL segments have been lost (wal_status=lost); slot consumer cannot catch up"
                        .to_owned(),
                ),
            );
        }

        if !active && retained_bytes >= Some(INACTIVE_SLOT_CRITICAL_BYTES) {
            let retained_label = format_optional_bytes(retained_bytes);
            return (
                SlotRiskSeverity::Critical,
                Some(format!(
                    "inactive slot retains {retained_label} of WAL (>= 1.0 GiB critical threshold)"
                )),
            );
        }

        if status_norm.eq_ignore_ascii_case("unreserved") {
            return (
                SlotRiskSeverity::Warning,
                Some(
                    "slot WAL is unreserved (wal_status=unreserved) and may be removed at next checkpoint"
                        .to_owned(),
                ),
            );
        }

        if !active && retained_bytes >= Some(INACTIVE_SLOT_WARNING_BYTES) {
            let retained_label = format_optional_bytes(retained_bytes);
            return (
                SlotRiskSeverity::Warning,
                Some(format!(
                    "inactive slot retains {retained_label} of WAL (>= 100.0 MiB warning threshold)"
                )),
            );
        }

        if !active {
            return (
                SlotRiskSeverity::Warning,
                Some(
                    "replication slot is inactive and will retain WAL until consumed or dropped"
                        .to_owned(),
                ),
            );
        }

        (SlotRiskSeverity::Ok, None)
    }

    /// Recomputes `risk` and `finding` from `active`, `wal_status`, and `retained_bytes`.
    #[must_use]
    pub fn with_assessed_risk(mut self) -> Self {
        let (risk, finding) = Self::assess_risk(self.active, &self.wal_status, self.retained_bytes);
        self.risk = risk;
        self.finding = finding;
        self
    }
}

/// Complete bounded logical replication and slot safety snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ReplicationReport {
    /// Publications visible in the connected database.
    pub publications: Vec<PublicationSummary>,
    /// Subscriptions visible in the connected database (empty when restricted).
    pub subscriptions: Vec<SubscriptionSummary>,
    /// Cluster-wide replication slots.
    pub slots: Vec<ReplicationSlotSummary>,
    /// Whether publication results hit [`MAX_PUBLICATIONS`].
    pub publications_truncated: bool,
    /// Whether subscription results hit [`MAX_SUBSCRIPTIONS`].
    pub subscriptions_truncated: bool,
    /// Whether replication slot results hit [`MAX_REPLICATION_SLOTS`].
    pub slots_truncated: bool,
    /// Whether reading `pg_subscription` failed with SQLSTATE `42501` (insufficient privilege).
    pub subscription_access_restricted: bool,
}

impl ReplicationReport {
    /// Count of inactive replication slots in the retained snapshot.
    #[must_use]
    pub fn inactive_slots_count(&self) -> usize {
        self.slots.iter().filter(|slot| !slot.active).count()
    }

    /// Maximum retained WAL bytes across slots with measurable `retained_bytes`.
    #[must_use]
    pub fn max_retained_bytes(&self) -> Option<i64> {
        self.slots
            .iter()
            .filter_map(|slot| slot.retained_bytes)
            .max()
    }

    /// Whether any replication slot is classified as [`SlotRiskSeverity::Critical`].
    #[must_use]
    pub fn has_critical_findings(&self) -> bool {
        self.slots
            .iter()
            .any(|slot| slot.risk == SlotRiskSeverity::Critical)
    }

    /// Whether any replication slot is classified as [`SlotRiskSeverity::Warning`] or higher.
    #[must_use]
    pub fn has_warning_findings(&self) -> bool {
        self.slots
            .iter()
            .any(|slot| slot.risk >= SlotRiskSeverity::Warning)
    }

    /// Renders an aligned ASCII tabular overview for the selected section.
    #[must_use]
    pub fn render_table(&self, section: ReplicationSection) -> String {
        let mut out = String::new();
        out.push_str("=== Ignatius Logical Replication & Slot Safety ===\n");
        let pub_note = if self.publications_truncated {
            " (truncated)"
        } else {
            ""
        };
        let sub_note = if self.subscription_access_restricted {
            " (restricted)"
        } else if self.subscriptions_truncated {
            " (truncated)"
        } else {
            ""
        };
        let slot_note = if self.slots_truncated {
            " (truncated)"
        } else {
            ""
        };
        let _ = writeln!(
            out,
            "Publications: {}{} | Subscriptions: {}{} | Slots: {}{} | Inactive Slots: {} | Max Retained WAL: {}",
            self.publications.len(),
            pub_note,
            self.subscriptions.len(),
            sub_note,
            self.slots.len(),
            slot_note,
            self.inactive_slots_count(),
            format_optional_bytes(self.max_retained_bytes())
        );
        out.push('\n');

        if matches!(
            section,
            ReplicationSection::All | ReplicationSection::Publications
        ) {
            out.push_str("-- Publications --\n");
            if self.publications.is_empty() {
                out.push_str("No publications found.\n\n");
            } else {
                let _ = writeln!(
                    out,
                    "{:<24} {:<16} {:<10} {:<30} {:<24}",
                    "NAME", "OWNER", "SCOPE", "OPERATIONS", "TABLES"
                );
                let rule = "-".repeat(108);
                let _ = writeln!(out, "{rule}");
                for pub_item in &self.publications {
                    let scope = if pub_item.all_tables {
                        "ALL TABLES"
                    } else {
                        "SELECTED"
                    };
                    let tables_summary = format_publication_tables_cell(pub_item);
                    let _ = writeln!(
                        out,
                        "{:<24} {:<16} {:<10} {:<30} {:<24}",
                        truncate_str(&pub_item.name, 24),
                        truncate_str(&pub_item.owner, 16),
                        scope,
                        truncate_str(&pub_item.operations_label(), 30),
                        truncate_str(&tables_summary, 24)
                    );
                }
                if self.publications_truncated {
                    let _ = writeln!(
                        out,
                        "Note: publication list truncated at {MAX_PUBLICATIONS} entries."
                    );
                }
                out.push('\n');
            }
        }

        if matches!(
            section,
            ReplicationSection::All | ReplicationSection::Subscriptions
        ) {
            out.push_str("-- Subscriptions --\n");
            if self.subscription_access_restricted {
                out.push_str(
                    "Subscription catalog (pg_subscription) is restricted for the current role (SQLSTATE 42501).\n\n",
                );
            } else if self.subscriptions.is_empty() {
                out.push_str("No subscriptions found.\n\n");
            } else {
                let _ = writeln!(
                    out,
                    "{:<20} {:<14} {:<8} {:<13} {:<18} {:>12} {:<28}",
                    "NAME", "OWNER", "ENABLED", "STATE", "SLOT", "LAG", "CONNINFO"
                );
                let rule = "-".repeat(119);
                let _ = writeln!(out, "{rule}");
                for sub in &self.subscriptions {
                    let slot = sub.slot_name.as_deref().unwrap_or("-");
                    let lag = sub
                        .lag_bytes
                        .map(|b| format_optional_bytes(Some(b)))
                        .unwrap_or_else(|| "-".to_owned());
                    let _ = writeln!(
                        out,
                        "{:<20} {:<14} {:<8} {:<13} {:<18} {:>12} {:<28}",
                        truncate_str(&sub.name, 20),
                        truncate_str(&sub.owner, 14),
                        if sub.enabled { "yes" } else { "no" },
                        truncate_str(&sub.sync_state, 13),
                        truncate_str(slot, 18),
                        lag,
                        truncate_str(&sub.conninfo_redacted, 28)
                    );
                }
                if self.subscriptions_truncated {
                    let _ = writeln!(
                        out,
                        "Note: subscription list truncated at {MAX_SUBSCRIPTIONS} entries."
                    );
                }
                out.push('\n');
            }
        }

        if matches!(section, ReplicationSection::All | ReplicationSection::Slots) {
            out.push_str("-- Replication Slots --\n");
            if self.slots.is_empty() {
                out.push_str("No replication slots found.\n\n");
            } else {
                let _ = writeln!(
                    out,
                    "{:<24} {:<9} {:<12} {:<14} {:<7} {:<11} {:>12} {:>12} {:<8}",
                    "SLOT",
                    "TYPE",
                    "PLUGIN",
                    "DATABASE",
                    "ACTIVE",
                    "WAL STATUS",
                    "RETAINED",
                    "SAFE WAL",
                    "RISK"
                );
                let rule = "-".repeat(117);
                let _ = writeln!(out, "{rule}");
                for slot in &self.slots {
                    let _ = writeln!(
                        out,
                        "{:<24} {:<9} {:<12} {:<14} {:<7} {:<11} {:>12} {:>12} {:<8}",
                        truncate_str(&slot.slot_name, 24),
                        truncate_str(&slot.slot_type, 9),
                        truncate_str(slot.plugin.as_deref().unwrap_or("-"), 12),
                        truncate_str(slot.database.as_deref().unwrap_or("-"), 14),
                        if slot.active { "yes" } else { "no" },
                        truncate_str(&slot.wal_status, 11),
                        format_optional_bytes(slot.retained_bytes),
                        format_optional_bytes(slot.safe_wal_size),
                        slot.risk.label().to_ascii_uppercase()
                    );
                }
                if self.slots_truncated {
                    let _ = writeln!(
                        out,
                        "Note: replication slot list truncated at {MAX_REPLICATION_SLOTS} entries."
                    );
                }
                let findings: Vec<&ReplicationSlotSummary> =
                    self.slots.iter().filter(|s| s.finding.is_some()).collect();
                if !findings.is_empty() {
                    out.push_str("\nSlot Safety Findings:\n");
                    for slot in findings {
                        if let Some(finding) = &slot.finding {
                            let _ = writeln!(
                                out,
                                "  [{}] {}: {}",
                                slot.risk.label().to_ascii_uppercase(),
                                slot.slot_name,
                                finding
                            );
                        }
                    }
                }
                out.push('\n');
            }
        }

        out
    }

    /// Renders untruncated plain text output suitable for scripts and logs.
    #[must_use]
    pub fn render_plain(&self, section: ReplicationSection) -> String {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "summary publications={} subscriptions={} slots={} inactive_slots={} max_retained_bytes={} publications_truncated={} subscriptions_truncated={} slots_truncated={} subscription_access_restricted={}",
            self.publications.len(),
            self.subscriptions.len(),
            self.slots.len(),
            self.inactive_slots_count(),
            self.max_retained_bytes()
                .map_or_else(|| "unavailable".to_owned(), |v| v.to_string()),
            self.publications_truncated,
            self.subscriptions_truncated,
            self.slots_truncated,
            self.subscription_access_restricted
        );

        if matches!(
            section,
            ReplicationSection::All | ReplicationSection::Publications
        ) {
            if self.publications.is_empty() {
                out.push_str("publications: none\n");
            } else {
                for pub_item in &self.publications {
                    let tables = if pub_item.tables.is_empty() {
                        "-".to_owned()
                    } else {
                        pub_item.tables.join(",")
                    };
                    let _ = writeln!(
                        out,
                        "publication name={} owner={} all_tables={} ops=\"{}\" tables={} tables_truncated={}",
                        pub_item.name,
                        pub_item.owner,
                        pub_item.all_tables,
                        pub_item.operations_label(),
                        tables,
                        pub_item.tables_truncated
                    );
                }
            }
        }

        if matches!(
            section,
            ReplicationSection::All | ReplicationSection::Subscriptions
        ) {
            if self.subscription_access_restricted {
                out.push_str("subscriptions: restricted (SQLSTATE 42501)\n");
            } else if self.subscriptions.is_empty() {
                out.push_str("subscriptions: none\n");
            } else {
                for sub in &self.subscriptions {
                    let _ = writeln!(
                        out,
                        "subscription name={} owner={} enabled={} sync_state={} slot={} received_lsn={} latest_end_lsn={} lag_bytes={} conninfo=\"{}\"",
                        sub.name,
                        sub.owner,
                        sub.enabled,
                        sub.sync_state,
                        sub.slot_name.as_deref().unwrap_or("-"),
                        sub.received_lsn.as_deref().unwrap_or("-"),
                        sub.latest_end_lsn.as_deref().unwrap_or("-"),
                        sub.lag_bytes
                            .map_or_else(|| "unavailable".to_owned(), |v| v.to_string()),
                        sub.conninfo_redacted
                    );
                }
            }
        }

        if matches!(section, ReplicationSection::All | ReplicationSection::Slots) {
            if self.slots.is_empty() {
                out.push_str("slots: none\n");
            } else {
                for slot in &self.slots {
                    let _ = writeln!(
                        out,
                        "slot name={} type={} plugin={} database={} temporary={} active={} active_pid={} wal_status={} retained_bytes={} safe_wal_size={} risk={} finding={}",
                        slot.slot_name,
                        slot.slot_type,
                        slot.plugin.as_deref().unwrap_or("-"),
                        slot.database.as_deref().unwrap_or("-"),
                        slot.temporary,
                        slot.active,
                        slot.active_pid
                            .map_or_else(|| "-".to_owned(), |pid| pid.to_string()),
                        slot.wal_status,
                        slot.retained_bytes
                            .map_or_else(|| "unavailable".to_owned(), |v| v.to_string()),
                        slot.safe_wal_size
                            .map_or_else(|| "unavailable".to_owned(), |v| v.to_string()),
                        slot.risk.label(),
                        slot.finding.as_deref().unwrap_or("-")
                    );
                }
            }
        }

        out
    }

    /// Renders the replication report as structured JSON.
    #[must_use]
    pub fn render_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_owned())
    }
}

/// Generates a reviewed SQL statement to drop a replication slot.
///
/// This helper never executes SQL; callers present the output for explicit
/// operator review before running it.
#[must_use]
pub fn drop_slot_sql(slot_name: &str) -> String {
    let escaped = slot_name.replace('\'', "''");
    format!(
        "-- WARNING: Dropping a replication slot permanently stops WAL retention for that consumer.\n\
         -- Verify active state, retained bytes, and downstream subscribers before running.\n\
         SELECT pg_catalog.pg_drop_replication_slot('{escaped}');\n"
    )
}

/// Generates a reviewed `CREATE PUBLICATION` statement with safe identifier quoting.
///
/// This helper never executes SQL.
#[must_use]
pub fn create_publication_sql(name: &str, all_tables: bool) -> String {
    let ident = quote_identifier(name);
    if all_tables {
        format!("CREATE PUBLICATION {ident} FOR ALL TABLES;\n")
    } else {
        format!(
            "-- Add tables with: ALTER PUBLICATION {ident} ADD TABLE schema_name.table_name;\n\
             CREATE PUBLICATION {ident};\n"
        )
    }
}

/// Generates a reviewed `ALTER SUBSCRIPTION ... ENABLE|DISABLE` statement with safe
/// identifier quoting.
///
/// This helper never executes SQL.
#[must_use]
pub fn alter_subscription_toggle_sql(name: &str, enable: bool) -> String {
    let ident = quote_identifier(name);
    let action = if enable { "ENABLE" } else { "DISABLE" };
    format!("ALTER SUBSCRIPTION {ident} {action};\n")
}

fn format_publication_tables_cell(pub_item: &PublicationSummary) -> String {
    if pub_item.all_tables {
        if pub_item.tables.is_empty() {
            return "ALL TABLES".to_owned();
        }
        let suffix = if pub_item.tables_truncated { "+" } else { "" };
        return format!("ALL ({}{} tables)", pub_item.tables.len(), suffix);
    }
    if pub_item.tables.is_empty() {
        "none".to_owned()
    } else if pub_item.tables.len() == 1 && !pub_item.tables_truncated {
        pub_item.tables[0].clone()
    } else {
        let suffix = if pub_item.tables_truncated { "+" } else { "" };
        format!(
            "{}{} ({})",
            pub_item.tables.len(),
            suffix,
            pub_item.tables[0]
        )
    }
}

fn format_optional_bytes(bytes: Option<i64>) -> String {
    const UNITS: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    let Some(raw) = bytes else {
        return "unavailable".to_owned();
    };
    if raw < 0 {
        return format!("{raw} B");
    }
    let abs = raw.unsigned_abs();
    if abs < 1024 {
        return format!("{abs} B");
    }
    let mut value = abs as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

fn truncate_str(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_owned()
    } else {
        let truncated: String = s.chars().take(max_len.saturating_sub(3)).collect();
        format!("{truncated}...")
    }
}
