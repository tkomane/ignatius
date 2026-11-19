//! Pure server configuration, HBA, and infrastructure catalog model for
//! Feature `052-server-config-hba-and-infra-catalog`.
//!
//! This module contains zero network, filesystem, process, or terminal I/O.
//! [`ClusterReport`] is populated from bounded read-only catalog queries in
//! [`crate::postgres::cluster`], provides deterministic filtering, HBA risk
//! classification, ASCII/JSON rendering, secret-masked option formatting, and
//! reviewed SQL generation helpers that are never auto-executed.

#[cfg(test)]
mod tests;

use std::fmt::Write as _;

use serde::{Deserialize, Serialize};

use crate::diagnostics::redaction::{REDACTED, redact_text};
use crate::query::identifiers::quote_identifier;

/// Maximum number of `pg_settings` entries retained in one report.
pub const MAX_SETTINGS: usize = 500;

/// Maximum number of `pg_hba_file_rules` entries retained in one report.
pub const MAX_HBA_RULES: usize = 500;

/// Maximum number of `pg_available_extensions` entries retained in one report.
pub const MAX_EXTENSIONS: usize = 500;

/// Maximum number of `pg_tablespace` entries retained in one report.
pub const MAX_TABLESPACES: usize = 100;

/// Maximum number of `pg_foreign_data_wrapper` entries retained in one report.
pub const MAX_FDWS: usize = 100;

/// Maximum number of `pg_foreign_server` entries retained across all wrappers.
pub const MAX_FOREIGN_SERVERS: usize = 500;

/// Section filter for server configuration, HBA, and infrastructure reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ClusterSection {
    /// Include all sections: settings, HBA rules, extensions, tablespaces, and FDWs.
    #[default]
    All,
    /// Server configuration parameters (`pg_settings`).
    Settings,
    /// Host-based authentication rules (`pg_hba_file_rules`).
    Hba,
    /// Installed and available extensions (`pg_available_extensions`).
    Extensions,
    /// Cluster tablespaces (`pg_tablespace`).
    Tablespaces,
    /// Foreign data wrappers and foreign servers.
    Fdw,
}

impl ClusterSection {
    /// Accepted CLI section strings.
    pub const ACCEPTED: &'static [&'static str] =
        &["all", "settings", "hba", "extensions", "tablespaces", "fdw"];

    /// Short lowercase section label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Settings => "settings",
            Self::Hba => "hba",
            Self::Extensions => "extensions",
            Self::Tablespaces => "tablespaces",
            Self::Fdw => "fdw",
        }
    }

    /// Resolves a section filter from user or CLI input.
    #[must_use]
    pub fn from_str_loose(input: &str) -> Option<Self> {
        match input.trim().to_ascii_lowercase().as_str() {
            "all" => Some(Self::All),
            "settings" | "setting" | "config" | "guc" | "gucs" => Some(Self::Settings),
            "hba" | "auth" | "pg_hba" => Some(Self::Hba),
            "extensions" | "extension" | "ext" => Some(Self::Extensions),
            "tablespaces" | "tablespace" | "ts" => Some(Self::Tablespaces),
            "fdw" | "fdws" | "foreign" | "servers" => Some(Self::Fdw),
            _ => None,
        }
    }
}

/// One server configuration parameter from `pg_catalog.pg_settings`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingEntry {
    /// Parameter name (for example `work_mem` or `pg_stat_statements.max`).
    pub name: String,
    /// Current effective setting value.
    pub setting: String,
    /// Unit reported by PostgreSQL (`kB`, `MB`, `ms`, `s`, etc.), when present.
    pub unit: Option<String>,
    /// Configuration category from `pg_settings.category`.
    pub category: String,
    /// Context required to apply changes (`postmaster`, `sighup`, `superuser`, `user`, etc.).
    pub context: String,
    /// Origin of the current value (`default`, `configuration file`, `session`, `user`, etc.).
    pub source: String,
    /// Initial value at server boot, when defined.
    pub boot_val: Option<String>,
    /// Value restored on `RESET`, when defined.
    pub reset_val: Option<String>,
    /// True when a server restart is required for a pending change to take effect.
    pub pending_restart: bool,
    /// True when the active value comes from a non-default source.
    pub is_non_default: bool,
}

impl SettingEntry {
    /// Formats the effective setting alongside its unit when non-empty.
    #[must_use]
    pub fn display_value(&self) -> String {
        match self.unit.as_deref() {
            Some(unit) if !unit.is_empty() => format!("{} {unit}", self.setting),
            _ => self.setting.clone(),
        }
    }

    /// Returns whether this entry matches a case-insensitive substring filter.
    #[must_use]
    pub fn matches_filter(&self, filter_lower: &str) -> bool {
        contains_ignore_ascii_case(&self.name, filter_lower)
            || contains_ignore_ascii_case(&self.setting, filter_lower)
            || contains_ignore_ascii_case(&self.category, filter_lower)
            || contains_ignore_ascii_case(&self.context, filter_lower)
            || contains_ignore_ascii_case(&self.source, filter_lower)
    }

    /// Short human-readable state flags for table rendering.
    #[must_use]
    pub fn state_label(&self) -> &'static str {
        match (self.pending_restart, self.is_non_default) {
            (true, true) => "RESTART,MODIFIED",
            (true, false) => "RESTART",
            (false, true) => "MODIFIED",
            (false, false) => "default",
        }
    }
}

/// Security risk classification for a `pg_hba.conf` rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum HbaRisk {
    /// Rule uses an authenticated method without weak-auth or parse issues.
    Ok,
    /// Rule uses local `trust`, cleartext `password`, or legacy `md5` authentication.
    Warning,
    /// Rule has a parse error or allows unauthenticated network `trust` access.
    Critical,
}

impl HbaRisk {
    /// Short uppercase label used in tables, JSON, and plain text output.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Ok => "OK",
            Self::Warning => "WARNING",
            Self::Critical => "CRITICAL",
        }
    }
}

/// One host-based authentication rule from `pg_catalog.pg_hba_file_rules`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HbaRule {
    /// Line number in `pg_hba.conf`, when available.
    pub line_number: Option<i32>,
    /// Connection rule type (`local`, `host`, `hostssl`, `hostnossl`, `hostgssenc`, `hostnogssenc`).
    pub rule_type: String,
    /// Target database selectors (`all`, `sameuser`, `samerole`, `replication`, or names).
    pub databases: Vec<String>,
    /// Target role selectors (`all`, `+group`, or role names).
    pub users: Vec<String>,
    /// Client address and optional netmask, or `None` for `local` rules.
    pub address: Option<String>,
    /// Authentication method (`scram-sha-256`, `md5`, `password`, `trust`, `peer`, `cert`, etc.).
    pub auth_method: String,
    /// Server-reported parse error when a rule line could not be loaded cleanly.
    pub error: Option<String>,
    /// Evaluated security risk level.
    pub risk: HbaRisk,
    /// Human-readable explanation when `risk` is not [`HbaRisk::Ok`].
    pub risk_reason: Option<String>,
}

impl HbaRule {
    /// Constructs an [`HbaRule`] and classifies its security risk automatically.
    #[must_use]
    pub fn new(
        line_number: Option<i32>,
        rule_type: String,
        databases: Vec<String>,
        users: Vec<String>,
        address: Option<String>,
        auth_method: String,
        error: Option<String>,
    ) -> Self {
        let (risk, risk_reason) = classify_hba_risk(&rule_type, &auth_method, error.as_deref());
        Self {
            line_number,
            rule_type,
            databases,
            users,
            address,
            auth_method,
            error,
            risk,
            risk_reason,
        }
    }

    /// Recomputes `risk` and `risk_reason` from the rule's fields.
    #[must_use]
    pub fn with_classified_risk(mut self) -> Self {
        let (risk, risk_reason) =
            classify_hba_risk(&self.rule_type, &self.auth_method, self.error.as_deref());
        self.risk = risk;
        self.risk_reason = risk_reason;
        self
    }

    /// Returns whether this rule matches a case-insensitive substring filter.
    #[must_use]
    pub fn matches_filter(&self, filter_lower: &str) -> bool {
        contains_ignore_ascii_case(&self.rule_type, filter_lower)
            || contains_ignore_ascii_case(&self.auth_method, filter_lower)
            || contains_ignore_ascii_case(self.risk.label(), filter_lower)
            || self
                .address
                .as_deref()
                .is_some_and(|addr| contains_ignore_ascii_case(addr, filter_lower))
            || self
                .error
                .as_deref()
                .is_some_and(|err| contains_ignore_ascii_case(err, filter_lower))
            || self
                .risk_reason
                .as_deref()
                .is_some_and(|reason| contains_ignore_ascii_case(reason, filter_lower))
            || self
                .databases
                .iter()
                .any(|db| contains_ignore_ascii_case(db, filter_lower))
            || self
                .users
                .iter()
                .any(|user| contains_ignore_ascii_case(user, filter_lower))
    }
}

/// Classifies a `pg_hba_file_rules` entry into [`HbaRisk`] and an optional reason.
///
/// Rules:
/// - `Critical`: `error` is `Some`, or `auth_method == "trust"` with `rule_type != "local"`.
/// - `Warning`: local `trust`, cleartext `password`, or legacy `md5`.
/// - `Ok`: all other authenticated or explicit `reject` rules.
#[must_use]
pub fn classify_hba_risk(
    rule_type: &str,
    auth_method: &str,
    error: Option<&str>,
) -> (HbaRisk, Option<String>) {
    if let Some(err) = error {
        let trimmed = err.trim();
        let reason = if trimmed.is_empty() {
            "HBA rule reported a parse error".to_owned()
        } else {
            format!("HBA rule error: {trimmed}")
        };
        return (HbaRisk::Critical, Some(reason));
    }

    let method = auth_method.trim().to_ascii_lowercase();
    let rtype = rule_type.trim().to_ascii_lowercase();

    if method == "trust" && rtype != "local" {
        return (
            HbaRisk::Critical,
            Some(format!(
                "unauthenticated network trust rule ({rtype}) allows passwordless access"
            )),
        );
    }

    match method.as_str() {
        "trust" => (
            HbaRisk::Warning,
            Some("local trust authentication skips password verification".to_owned()),
        ),
        "password" => (
            HbaRisk::Warning,
            Some(
                "cleartext password authentication sends credentials unhashed (prefer scram-sha-256)"
                    .to_owned(),
            ),
        ),
        "md5" => (
            HbaRisk::Warning,
            Some(
                "legacy md5 authentication uses a weak hash (prefer scram-sha-256)".to_owned(),
            ),
        ),
        _ => (HbaRisk::Ok, None),
    }
}

/// One extension entry from `pg_catalog.pg_available_extensions`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AvailableExtension {
    /// Extension name.
    pub name: String,
    /// Default version shipped on the server.
    pub default_version: Option<String>,
    /// Currently installed version in the current database, or `None` if not installed.
    pub installed_version: Option<String>,
    /// Extension summary comment from the control file.
    pub comment: Option<String>,
    /// True when installed and `default_version` differs from `installed_version`.
    pub upgrade_available: bool,
}

impl AvailableExtension {
    /// Returns whether this extension is currently installed in the database.
    #[must_use]
    pub const fn is_installed(&self) -> bool {
        self.installed_version.is_some()
    }

    /// Computes whether an installed extension has a different default version available.
    #[must_use]
    pub fn has_upgrade(default_version: Option<&str>, installed_version: Option<&str>) -> bool {
        matches!(
            (default_version, installed_version),
            (Some(default), Some(installed)) if default != installed
        )
    }

    /// Short status label for table rendering.
    #[must_use]
    pub const fn status_label(&self) -> &'static str {
        if self.upgrade_available {
            "UPGRADE"
        } else if self.installed_version.is_some() {
            "installed"
        } else {
            "available"
        }
    }

    /// Returns whether this extension matches a case-insensitive substring filter.
    #[must_use]
    pub fn matches_filter(&self, filter_lower: &str) -> bool {
        contains_ignore_ascii_case(&self.name, filter_lower)
            || self
                .default_version
                .as_deref()
                .is_some_and(|v| contains_ignore_ascii_case(v, filter_lower))
            || self
                .installed_version
                .as_deref()
                .is_some_and(|v| contains_ignore_ascii_case(v, filter_lower))
            || self
                .comment
                .as_deref()
                .is_some_and(|c| contains_ignore_ascii_case(c, filter_lower))
            || contains_ignore_ascii_case(self.status_label(), filter_lower)
    }
}

/// One tablespace entry from `pg_catalog.pg_tablespace`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TablespaceEntry {
    /// Tablespace name (`pg_default`, `pg_global`, or user-defined).
    pub name: String,
    /// Owning role name.
    pub owner: String,
    /// Filesystem directory path, or `None` for built-in cluster tablespaces.
    pub location: Option<String>,
    /// Allocated size in bytes when readable by the current role.
    pub size_bytes: Option<i64>,
    /// Tablespace storage options (`key=value`), with sensitive values redacted.
    pub options: Vec<String>,
}

impl TablespaceEntry {
    /// Returns a clone with all sensitive option values masked.
    #[must_use]
    pub fn sanitized(&self) -> Self {
        Self {
            name: self.name.clone(),
            owner: self.owner.clone(),
            location: self.location.clone(),
            size_bytes: self.size_bytes,
            options: mask_options(&self.options),
        }
    }

    /// Returns whether this tablespace matches a case-insensitive substring filter.
    #[must_use]
    pub fn matches_filter(&self, filter_lower: &str) -> bool {
        contains_ignore_ascii_case(&self.name, filter_lower)
            || contains_ignore_ascii_case(&self.owner, filter_lower)
            || self
                .location
                .as_deref()
                .is_some_and(|loc| contains_ignore_ascii_case(loc, filter_lower))
            || self
                .options
                .iter()
                .any(|opt| contains_ignore_ascii_case(&mask_option(opt), filter_lower))
    }
}

/// One foreign server entry from `pg_catalog.pg_foreign_server`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForeignServerEntry {
    /// Foreign server name.
    pub name: String,
    /// Owning role name.
    pub owner: String,
    /// Optional server type (`SERVER ... TYPE '...'`).
    pub server_type: Option<String>,
    /// Optional server version (`SERVER ... VERSION '...'`).
    pub server_version: Option<String>,
    /// Foreign server options (`key=value`), with sensitive keys masked.
    pub options: Vec<String>,
}

impl ForeignServerEntry {
    /// Returns a clone with all sensitive option values masked.
    #[must_use]
    pub fn sanitized(&self) -> Self {
        Self {
            name: self.name.clone(),
            owner: self.owner.clone(),
            server_type: self.server_type.clone(),
            server_version: self.server_version.clone(),
            options: mask_options(&self.options),
        }
    }

    /// Returns whether this server matches a case-insensitive substring filter.
    #[must_use]
    pub fn matches_filter(&self, filter_lower: &str) -> bool {
        contains_ignore_ascii_case(&self.name, filter_lower)
            || contains_ignore_ascii_case(&self.owner, filter_lower)
            || self
                .server_type
                .as_deref()
                .is_some_and(|t| contains_ignore_ascii_case(t, filter_lower))
            || self
                .server_version
                .as_deref()
                .is_some_and(|v| contains_ignore_ascii_case(v, filter_lower))
            || self
                .options
                .iter()
                .any(|opt| contains_ignore_ascii_case(&mask_option(opt), filter_lower))
    }
}

/// One foreign data wrapper from `pg_catalog.pg_foreign_data_wrapper`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FdwEntry {
    /// Foreign data wrapper name.
    pub name: String,
    /// Owning role name.
    pub owner: String,
    /// Handler function name, when configured.
    pub handler: Option<String>,
    /// Validator function name, when configured.
    pub validator: Option<String>,
    /// Foreign servers referencing this wrapper.
    pub servers: Vec<ForeignServerEntry>,
}

impl FdwEntry {
    /// Returns a clone with all attached servers' sensitive options masked.
    #[must_use]
    pub fn sanitized(&self) -> Self {
        Self {
            name: self.name.clone(),
            owner: self.owner.clone(),
            handler: self.handler.clone(),
            validator: self.validator.clone(),
            servers: self
                .servers
                .iter()
                .map(ForeignServerEntry::sanitized)
                .collect(),
        }
    }

    /// Returns whether this wrapper or any of its servers matches a filter.
    #[must_use]
    pub fn matches_filter(&self, filter_lower: &str) -> bool {
        contains_ignore_ascii_case(&self.name, filter_lower)
            || contains_ignore_ascii_case(&self.owner, filter_lower)
            || self
                .handler
                .as_deref()
                .is_some_and(|h| contains_ignore_ascii_case(h, filter_lower))
            || self
                .validator
                .as_deref()
                .is_some_and(|v| contains_ignore_ascii_case(v, filter_lower))
            || self
                .servers
                .iter()
                .any(|server| server.matches_filter(filter_lower))
    }
}

/// Bounded snapshot of server settings, HBA rules, extensions, tablespaces, and FDWs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClusterReport {
    /// Bounded server settings from `pg_settings`.
    pub settings: Vec<SettingEntry>,
    /// Bounded host-based authentication rules from `pg_hba_file_rules`.
    pub hba_rules: Vec<HbaRule>,
    /// Bounded extensions from `pg_available_extensions`.
    pub extensions: Vec<AvailableExtension>,
    /// Bounded tablespaces from `pg_tablespace`.
    pub tablespaces: Vec<TablespaceEntry>,
    /// Bounded foreign data wrappers and their attached foreign servers.
    pub fdws: Vec<FdwEntry>,
    /// True when more than [`MAX_SETTINGS`] settings existed.
    pub truncated_settings: bool,
    /// True when more than [`MAX_HBA_RULES`] HBA rules existed.
    pub truncated_hba_rules: bool,
    /// True when more than [`MAX_EXTENSIONS`] extensions existed.
    pub truncated_extensions: bool,
    /// True when more than [`MAX_TABLESPACES`] tablespaces existed.
    pub truncated_tablespaces: bool,
    /// True when more than [`MAX_FDWS`] foreign data wrappers existed.
    pub truncated_fdws: bool,
    /// True when the connected role lacked privilege to inspect `pg_hba_file_rules`.
    pub hba_access_restricted: bool,
}

impl ClusterReport {
    /// Constructs an empty [`ClusterReport`].
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Sorts sections deterministically, enforces retention bounds, sets
    /// truncation flags, and masks sensitive option values.
    #[must_use]
    pub fn bounded(mut self) -> Self {
        self.settings.sort_by(|a, b| {
            b.pending_restart
                .cmp(&a.pending_restart)
                .then_with(|| b.is_non_default.cmp(&a.is_non_default))
                .then_with(|| a.name.cmp(&b.name))
        });
        if self.settings.len() > MAX_SETTINGS {
            self.settings.truncate(MAX_SETTINGS);
            self.truncated_settings = true;
        }

        self.hba_rules.sort_by(|a, b| {
            a.line_number
                .unwrap_or(i32::MAX)
                .cmp(&b.line_number.unwrap_or(i32::MAX))
        });
        if self.hba_rules.len() > MAX_HBA_RULES {
            self.hba_rules.truncate(MAX_HBA_RULES);
            self.truncated_hba_rules = true;
        }

        self.extensions.sort_by(|a, b| {
            b.installed_version
                .is_some()
                .cmp(&a.installed_version.is_some())
                .then_with(|| a.name.cmp(&b.name))
        });
        if self.extensions.len() > MAX_EXTENSIONS {
            self.extensions.truncate(MAX_EXTENSIONS);
            self.truncated_extensions = true;
        }

        self.tablespaces.sort_by(|a, b| a.name.cmp(&b.name));
        if self.tablespaces.len() > MAX_TABLESPACES {
            self.tablespaces.truncate(MAX_TABLESPACES);
            self.truncated_tablespaces = true;
        }
        for ts in &mut self.tablespaces {
            ts.options = mask_options(&ts.options);
        }

        self.fdws.sort_by(|a, b| a.name.cmp(&b.name));
        if self.fdws.len() > MAX_FDWS {
            self.fdws.truncate(MAX_FDWS);
            self.truncated_fdws = true;
        }
        for fdw in &mut self.fdws {
            fdw.servers.sort_by(|a, b| a.name.cmp(&b.name));
            for server in &mut fdw.servers {
                server.options = mask_options(&server.options);
            }
        }

        self
    }

    /// Returns a clone with all sensitive option values masked.
    #[must_use]
    pub fn sanitized(&self) -> Self {
        Self {
            settings: self.settings.clone(),
            hba_rules: self.hba_rules.clone(),
            extensions: self.extensions.clone(),
            tablespaces: self
                .tablespaces
                .iter()
                .map(TablespaceEntry::sanitized)
                .collect(),
            fdws: self.fdws.iter().map(FdwEntry::sanitized).collect(),
            truncated_settings: self.truncated_settings,
            truncated_hba_rules: self.truncated_hba_rules,
            truncated_extensions: self.truncated_extensions,
            truncated_tablespaces: self.truncated_tablespaces,
            truncated_fdws: self.truncated_fdws,
            hba_access_restricted: self.hba_access_restricted,
        }
    }

    /// Number of retained settings requiring a server restart.
    #[must_use]
    pub fn pending_restart_count(&self) -> usize {
        self.settings.iter().filter(|s| s.pending_restart).count()
    }

    /// Number of retained settings with a non-default source.
    #[must_use]
    pub fn non_default_settings_count(&self) -> usize {
        self.settings.iter().filter(|s| s.is_non_default).count()
    }

    /// Number of retained HBA rules flagged as [`HbaRisk::Warning`] or [`HbaRisk::Critical`].
    #[must_use]
    pub fn hba_warnings_count(&self) -> usize {
        self.hba_rules
            .iter()
            .filter(|rule| rule.risk != HbaRisk::Ok)
            .count()
    }

    /// Number of retained HBA rules flagged specifically as [`HbaRisk::Critical`].
    #[must_use]
    pub fn hba_critical_count(&self) -> usize {
        self.hba_rules
            .iter()
            .filter(|rule| rule.risk == HbaRisk::Critical)
            .count()
    }

    /// Number of retained installed extensions with a different default version available.
    #[must_use]
    pub fn upgradable_extensions_count(&self) -> usize {
        self.extensions
            .iter()
            .filter(|ext| ext.upgrade_available)
            .count()
    }

    /// Returns settings matching `modified_only` and optional case-insensitive substring `filter`.
    #[must_use]
    pub fn filtered_settings(
        &self,
        modified_only: bool,
        filter: Option<&str>,
    ) -> Vec<&SettingEntry> {
        let needle = normalize_filter(filter);
        self.settings
            .iter()
            .filter(|s| !modified_only || s.is_non_default || s.pending_restart)
            .filter(|s| needle.as_deref().is_none_or(|f| s.matches_filter(f)))
            .collect()
    }

    /// Returns HBA rules matching optional case-insensitive substring `filter`.
    #[must_use]
    pub fn filtered_hba_rules(&self, filter: Option<&str>) -> Vec<&HbaRule> {
        let needle = normalize_filter(filter);
        self.hba_rules
            .iter()
            .filter(|r| needle.as_deref().is_none_or(|f| r.matches_filter(f)))
            .collect()
    }

    /// Returns extensions matching optional case-insensitive substring `filter`.
    #[must_use]
    pub fn filtered_extensions(&self, filter: Option<&str>) -> Vec<&AvailableExtension> {
        let needle = normalize_filter(filter);
        self.extensions
            .iter()
            .filter(|e| needle.as_deref().is_none_or(|f| e.matches_filter(f)))
            .collect()
    }

    /// Returns tablespaces matching optional case-insensitive substring `filter`.
    #[must_use]
    pub fn filtered_tablespaces(&self, filter: Option<&str>) -> Vec<&TablespaceEntry> {
        let needle = normalize_filter(filter);
        self.tablespaces
            .iter()
            .filter(|t| needle.as_deref().is_none_or(|f| t.matches_filter(f)))
            .collect()
    }

    /// Returns FDW entries matching optional case-insensitive substring `filter`.
    #[must_use]
    pub fn filtered_fdws(&self, filter: Option<&str>) -> Vec<&FdwEntry> {
        let needle = normalize_filter(filter);
        self.fdws
            .iter()
            .filter(|fdw| needle.as_deref().is_none_or(|f| fdw.matches_filter(f)))
            .collect()
    }

    /// Renders an aligned plain-ASCII tabular report for the selected section and filters.
    #[must_use]
    pub fn render_table(
        &self,
        section: ClusterSection,
        modified_only: bool,
        filter: Option<&str>,
    ) -> String {
        let mut out = String::new();
        let server_count: usize = self.fdws.iter().map(|f| f.servers.len()).sum();

        out.push_str("Cluster Config, HBA, and Infrastructure Catalog\n");
        out.push_str("==============================================\n\n");
        let _ = writeln!(
            out,
            "Summary: settings={} (non-default={}, pending_restart={}) | hba={} (flagged={}, critical={}, restricted={}) | extensions={} (upgradable={}) | tablespaces={} | fdws={} (servers={})",
            self.settings.len(),
            self.non_default_settings_count(),
            self.pending_restart_count(),
            self.hba_rules.len(),
            self.hba_warnings_count(),
            self.hba_critical_count(),
            self.hba_access_restricted,
            self.extensions.len(),
            self.upgradable_extensions_count(),
            self.tablespaces.len(),
            self.fdws.len(),
            server_count,
        );
        if modified_only || normalize_filter(filter).is_some() {
            let _ = writeln!(
                out,
                "Filters: section={}, modified_only={}, filter={}",
                section.label(),
                modified_only,
                normalize_filter(filter).as_deref().unwrap_or("<none>")
            );
        }
        out.push('\n');

        if matches!(section, ClusterSection::All | ClusterSection::Settings) {
            self.render_settings_section(&mut out, modified_only, filter);
        }
        if matches!(section, ClusterSection::All | ClusterSection::Hba) {
            self.render_hba_section(&mut out, filter);
        }
        if matches!(section, ClusterSection::All | ClusterSection::Extensions) {
            self.render_extensions_section(&mut out, filter);
        }
        if matches!(section, ClusterSection::All | ClusterSection::Tablespaces) {
            self.render_tablespaces_section(&mut out, filter);
        }
        if matches!(section, ClusterSection::All | ClusterSection::Fdw) {
            self.render_fdw_section(&mut out, filter);
        }

        out
    }

    /// Renders plain-ASCII output suitable for scripts, pipes, and screen readers.
    #[must_use]
    pub fn render_plain(
        &self,
        section: ClusterSection,
        modified_only: bool,
        filter: Option<&str>,
    ) -> String {
        self.render_table(section, modified_only, filter)
    }

    /// Renders the machine-readable JSON representation with all sensitive options masked.
    #[must_use]
    pub fn render_json(&self) -> String {
        let sanitized = self.sanitized();
        serde_json::to_string_pretty(&sanitized)
            .unwrap_or_else(|_| "{\"error\":\"cluster report could not be serialized\"}".to_owned())
    }

    /// Generates a reviewed `ALTER SYSTEM SET` statement (never auto-executed).
    #[must_use]
    pub fn alter_system_sql(name: &str, value: &str) -> String {
        alter_system_sql(name, value)
    }

    /// Generates a reviewed configuration reload query (never auto-executed).
    #[must_use]
    pub fn reload_conf_sql() -> String {
        reload_conf_sql()
    }

    /// Generates a reviewed `CREATE EXTENSION` statement (never auto-executed).
    #[must_use]
    pub fn create_extension_sql(name: &str) -> String {
        create_extension_sql(name)
    }

    /// Generates a reviewed `ALTER EXTENSION ... UPDATE TO` statement (never auto-executed).
    #[must_use]
    pub fn alter_extension_update_sql(name: &str, version: &str) -> String {
        alter_extension_update_sql(name, version)
    }

    /// Generates a reviewed `CREATE FOREIGN DATA WRAPPER` statement (never auto-executed).
    #[must_use]
    pub fn create_fdw_sql(fdw: &FdwEntry) -> String {
        create_fdw_sql(fdw)
    }

    /// Generates a reviewed `CREATE SERVER` statement with sensitive options masked.
    #[must_use]
    pub fn create_server_sql(server: &ForeignServerEntry, fdw_name: &str) -> String {
        create_server_sql(server, fdw_name)
    }

    /// Generates a reviewed `CREATE TABLESPACE` statement with sensitive options masked.
    #[must_use]
    pub fn create_tablespace_sql(ts: &TablespaceEntry) -> String {
        create_tablespace_sql(ts)
    }

    fn render_settings_section(&self, out: &mut String, modified_only: bool, filter: Option<&str>) {
        let rows = self.filtered_settings(modified_only, filter);
        let _ = writeln!(
            out,
            "-- Server Settings ({} shown of {}{}) --",
            rows.len(),
            self.settings.len(),
            if self.truncated_settings {
                ", truncated"
            } else {
                ""
            }
        );
        if rows.is_empty() {
            out.push_str("No settings matched the current filter.\n\n");
            return;
        }
        let _ = writeln!(
            out,
            "{:<34}  {:<18}  {:<12}  {:<18}  STATE",
            "NAME", "VALUE", "CONTEXT", "SOURCE"
        );
        let _ = writeln!(out, "{}", "-".repeat(102));
        for entry in rows {
            let _ = writeln!(
                out,
                "{:<34}  {:<18}  {:<12}  {:<18}  {}",
                truncate_cell(&entry.name, 34),
                truncate_cell(&entry.display_value(), 18),
                truncate_cell(&entry.context, 12),
                truncate_cell(&entry.source, 18),
                entry.state_label()
            );
        }
        out.push('\n');
    }

    fn render_hba_section(&self, out: &mut String, filter: Option<&str>) {
        if self.hba_access_restricted {
            out.push_str("-- Host-Based Authentication (pg_hba_file_rules) --\n");
            out.push_str(
                "HBA rules restricted: connected role lacks privilege to read pg_catalog.pg_hba_file_rules (42501).\n\n",
            );
            return;
        }
        let rows = self.filtered_hba_rules(filter);
        let _ = writeln!(
            out,
            "-- Host-Based Authentication ({} shown of {}{}) --",
            rows.len(),
            self.hba_rules.len(),
            if self.truncated_hba_rules {
                ", truncated"
            } else {
                ""
            }
        );
        if rows.is_empty() {
            out.push_str("No HBA rules found.\n\n");
            return;
        }
        let _ = writeln!(
            out,
            "{:>5}  {:<8}  {:<16}  {:<16}  {:<22}  {:<14}  RISK",
            "LINE", "TYPE", "DATABASES", "USERS", "ADDRESS", "METHOD"
        );
        let _ = writeln!(out, "{}", "-".repeat(100));
        for rule in &rows {
            let line = rule
                .line_number
                .map_or_else(|| "-".to_owned(), |n| n.to_string());
            let dbs = if rule.databases.is_empty() {
                "-".to_owned()
            } else {
                rule.databases.join(",")
            };
            let users = if rule.users.is_empty() {
                "-".to_owned()
            } else {
                rule.users.join(",")
            };
            let addr = rule.address.as_deref().unwrap_or("-");
            let method = if rule.auth_method.is_empty() {
                "-"
            } else {
                rule.auth_method.as_str()
            };
            let _ = writeln!(
                out,
                "{:>5}  {:<8}  {:<16}  {:<16}  {:<22}  {:<14}  {}",
                line,
                truncate_cell(&rule.rule_type, 8),
                truncate_cell(&dbs, 16),
                truncate_cell(&users, 16),
                truncate_cell(addr, 22),
                truncate_cell(method, 14),
                rule.risk.label(),
            );
            if let Some(reason) = rule.risk_reason.as_deref() {
                let _ = writeln!(out, "       [{}] {}", rule.risk.label(), reason);
            }
        }
        out.push('\n');
    }

    fn render_extensions_section(&self, out: &mut String, filter: Option<&str>) {
        let rows = self.filtered_extensions(filter);
        let _ = writeln!(
            out,
            "-- Extensions ({} shown of {}{}) --",
            rows.len(),
            self.extensions.len(),
            if self.truncated_extensions {
                ", truncated"
            } else {
                ""
            }
        );
        if rows.is_empty() {
            out.push_str("No extensions found.\n\n");
            return;
        }
        let _ = writeln!(
            out,
            "{:<26}  {:<12}  {:<12}  {:<10}  COMMENT",
            "NAME", "INSTALLED", "DEFAULT", "STATUS"
        );
        let _ = writeln!(out, "{}", "-".repeat(100));
        for ext in rows {
            let _ = writeln!(
                out,
                "{:<26}  {:<12}  {:<12}  {:<10}  {}",
                truncate_cell(&ext.name, 26),
                truncate_cell(ext.installed_version.as_deref().unwrap_or("-"), 12),
                truncate_cell(ext.default_version.as_deref().unwrap_or("-"), 12),
                ext.status_label(),
                truncate_cell(ext.comment.as_deref().unwrap_or("-"), 34),
            );
        }
        out.push('\n');
    }

    fn render_tablespaces_section(&self, out: &mut String, filter: Option<&str>) {
        let rows = self.filtered_tablespaces(filter);
        let _ = writeln!(
            out,
            "-- Tablespaces ({} shown of {}{}) --",
            rows.len(),
            self.tablespaces.len(),
            if self.truncated_tablespaces {
                ", truncated"
            } else {
                ""
            }
        );
        if rows.is_empty() {
            out.push_str("No tablespaces found.\n\n");
            return;
        }
        let _ = writeln!(
            out,
            "{:<20}  {:<14}  {:>10}  {:<28}  OPTIONS",
            "NAME", "OWNER", "SIZE", "LOCATION"
        );
        let _ = writeln!(out, "{}", "-".repeat(100));
        for ts in rows {
            let size = ts
                .size_bytes
                .and_then(|bytes| u64::try_from(bytes).ok())
                .map_or_else(|| "-".to_owned(), format_bytes);
            let options = if ts.options.is_empty() {
                "-".to_owned()
            } else {
                mask_options(&ts.options).join(", ")
            };
            let _ = writeln!(
                out,
                "{:<20}  {:<14}  {:>10}  {:<28}  {}",
                truncate_cell(&ts.name, 20),
                truncate_cell(&ts.owner, 14),
                size,
                truncate_cell(ts.location.as_deref().unwrap_or("-"), 28),
                options,
            );
        }
        out.push('\n');
    }

    fn render_fdw_section(&self, out: &mut String, filter: Option<&str>) {
        let rows = self.filtered_fdws(filter);
        let _ = writeln!(
            out,
            "-- Foreign Data Wrappers ({} shown of {}{}) --",
            rows.len(),
            self.fdws.len(),
            if self.truncated_fdws {
                ", truncated"
            } else {
                ""
            }
        );
        if rows.is_empty() {
            out.push_str("No foreign data wrappers found.\n\n");
            return;
        }
        let _ = writeln!(
            out,
            "{:<20}  {:<14}  {:<24}  {:<24}  {:>7}",
            "FDW", "OWNER", "HANDLER", "VALIDATOR", "SERVERS"
        );
        let _ = writeln!(out, "{}", "-".repeat(97));
        let needle = normalize_filter(filter);
        let mut visible_servers: Vec<(&str, &ForeignServerEntry)> = Vec::new();
        for fdw in &rows {
            let _ = writeln!(
                out,
                "{:<20}  {:<14}  {:<24}  {:<24}  {:>7}",
                truncate_cell(&fdw.name, 20),
                truncate_cell(&fdw.owner, 14),
                truncate_cell(fdw.handler.as_deref().unwrap_or("-"), 24),
                truncate_cell(fdw.validator.as_deref().unwrap_or("-"), 24),
                fdw.servers.len(),
            );
            for server in &fdw.servers {
                let include = match needle.as_deref() {
                    None => true,
                    Some(f) => contains_ignore_ascii_case(&fdw.name, f) || server.matches_filter(f),
                };
                if include {
                    visible_servers.push((fdw.name.as_str(), server));
                }
            }
        }
        out.push('\n');

        if !visible_servers.is_empty() {
            let _ = writeln!(out, "-- Foreign Servers ({}) --", visible_servers.len());
            let _ = writeln!(
                out,
                "{:<20}  {:<16}  {:<14}  {:<12}  {:<10}  OPTIONS",
                "SERVER", "FDW", "OWNER", "TYPE", "VERSION"
            );
            let _ = writeln!(out, "{}", "-".repeat(104));
            for (fdw_name, server) in visible_servers {
                let options = if server.options.is_empty() {
                    "-".to_owned()
                } else {
                    mask_options(&server.options).join(", ")
                };
                let _ = writeln!(
                    out,
                    "{:<20}  {:<16}  {:<14}  {:<12}  {:<10}  {}",
                    truncate_cell(&server.name, 20),
                    truncate_cell(fdw_name, 16),
                    truncate_cell(&server.owner, 14),
                    truncate_cell(server.server_type.as_deref().unwrap_or("-"), 12),
                    truncate_cell(server.server_version.as_deref().unwrap_or("-"), 10),
                    options,
                );
            }
            out.push('\n');
        }
    }
}

/// Masks a single `key=value` catalog option when the key looks sensitive
/// (`password`, `secret`, `token`, `key`, `passphrase`, `credential`), and
/// passes non-sensitive option text through [`redact_text`].
#[must_use]
pub fn mask_option(option: &str) -> String {
    if let Some((raw_key, _raw_val)) = option.split_once('=')
        && is_sensitive_option_key(raw_key)
    {
        return format!("{}={REDACTED}", raw_key.trim());
    }
    redact_text(option)
}

/// Masks every option in a slice using [`mask_option`].
#[must_use]
pub fn mask_options(options: &[String]) -> Vec<String> {
    options.iter().map(|opt| mask_option(opt)).collect()
}

/// Generates a reviewed `ALTER SYSTEM SET` statement (never auto-executed).
#[must_use]
pub fn alter_system_sql(name: &str, value: &str) -> String {
    format!(
        "ALTER SYSTEM SET {} = {};",
        quote_qualified_name(name),
        quote_literal(value)
    )
}

/// Generates the reviewed PostgreSQL configuration reload statement (never auto-executed).
#[must_use]
pub fn reload_conf_sql() -> String {
    "SELECT pg_catalog.pg_reload_conf();".to_owned()
}

/// Generates a reviewed `CREATE EXTENSION` statement (never auto-executed).
#[must_use]
pub fn create_extension_sql(name: &str) -> String {
    format!("CREATE EXTENSION IF NOT EXISTS {};", quote_identifier(name))
}

/// Generates a reviewed `ALTER EXTENSION ... UPDATE TO` statement (never auto-executed).
#[must_use]
pub fn alter_extension_update_sql(name: &str, version: &str) -> String {
    format!(
        "ALTER EXTENSION {} UPDATE TO {};",
        quote_identifier(name),
        quote_literal(version)
    )
}

/// Generates a reviewed `CREATE FOREIGN DATA WRAPPER` statement (never auto-executed).
#[must_use]
pub fn create_fdw_sql(fdw: &FdwEntry) -> String {
    let mut out = format!(
        "CREATE FOREIGN DATA WRAPPER {}",
        quote_identifier(&fdw.name)
    );
    if let Some(handler) = fdw
        .handler
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let _ = write!(out, "\n    HANDLER {}", quote_qualified_name(handler));
    }
    if let Some(validator) = fdw
        .validator
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let _ = write!(out, "\n    VALIDATOR {}", quote_qualified_name(validator));
    }
    out.push(';');
    out
}

/// Generates a reviewed `CREATE SERVER` statement with sensitive options masked
/// (never auto-executed).
#[must_use]
pub fn create_server_sql(server: &ForeignServerEntry, fdw_name: &str) -> String {
    let mut out = format!(
        "CREATE SERVER IF NOT EXISTS {}",
        quote_identifier(&server.name)
    );
    if let Some(server_type) = server
        .server_type
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let _ = write!(out, "\n    TYPE {}", quote_literal(server_type));
    }
    if let Some(server_version) = server
        .server_version
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let _ = write!(out, "\n    VERSION {}", quote_literal(server_version));
    }
    let _ = write!(
        out,
        "\n    FOREIGN DATA WRAPPER {}",
        quote_identifier(fdw_name)
    );
    let formatted_options: Vec<String> = server
        .options
        .iter()
        .filter_map(|opt| format_option_pair(opt, " "))
        .collect();
    if !formatted_options.is_empty() {
        let _ = write!(out, "\n    OPTIONS ({})", formatted_options.join(", "));
    }
    out.push(';');
    out
}

/// Generates a reviewed `CREATE TABLESPACE` statement with sensitive options
/// masked (never auto-executed).
#[must_use]
pub fn create_tablespace_sql(ts: &TablespaceEntry) -> String {
    let Some(location) = ts
        .location
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    else {
        if ts.name == "pg_default" || ts.name == "pg_global" {
            return format!(
                "-- Built-in tablespace {} has no custom LOCATION and cannot be recreated.",
                quote_identifier(&ts.name)
            );
        }
        let mut out = format!(
            "-- Location unavailable; review and replace LOCATION before executing.\nCREATE TABLESPACE {}",
            quote_identifier(&ts.name)
        );
        if !ts.owner.trim().is_empty() {
            let _ = write!(out, "\n    OWNER {}", quote_identifier(&ts.owner));
        }
        out.push_str("\n    LOCATION '/path/to/tablespace'");
        let formatted_options: Vec<String> = ts
            .options
            .iter()
            .filter_map(|opt| format_option_pair(opt, " = "))
            .collect();
        if !formatted_options.is_empty() {
            let _ = write!(out, "\n    WITH ({})", formatted_options.join(", "));
        }
        out.push(';');
        return out;
    };

    let mut out = format!("CREATE TABLESPACE {}", quote_identifier(&ts.name));
    if !ts.owner.trim().is_empty() {
        let _ = write!(out, "\n    OWNER {}", quote_identifier(&ts.owner));
    }
    let _ = write!(out, "\n    LOCATION {}", quote_literal(location));
    let formatted_options: Vec<String> = ts
        .options
        .iter()
        .filter_map(|opt| format_option_pair(opt, " = "))
        .collect();
    if !formatted_options.is_empty() {
        let _ = write!(out, "\n    WITH ({})", formatted_options.join(", "));
    }
    out.push(';');
    out
}

/// Formats a byte count using plain binary units with one decimal place.
#[must_use]
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

fn is_sensitive_option_key(key: &str) -> bool {
    let lower = key.trim().to_ascii_lowercase();
    lower.contains("password")
        || lower.contains("passwd")
        || lower.contains("secret")
        || lower.contains("token")
        || lower.contains("passphrase")
        || lower.contains("credential")
        || lower == "key"
        || lower.ends_with("_key")
        || lower.ends_with("key")
        || lower.starts_with("key_")
        || lower.contains("_key_")
}

fn format_option_pair(raw_option: &str, separator: &str) -> Option<String> {
    let masked = mask_option(raw_option);
    let (raw_key, raw_val) = masked.split_once('=')?;
    let key = raw_key.trim();
    if key.is_empty() {
        return None;
    }
    let formatted_key = if is_simple_ascii_ident(key) {
        key.to_owned()
    } else {
        quote_identifier(key)
    };
    Some(format!(
        "{formatted_key}{separator}{}",
        quote_literal(raw_val)
    ))
}

fn is_simple_ascii_ident(ident: &str) -> bool {
    let mut chars = ident.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_lowercase() || first == '_')
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn quote_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn quote_qualified_name(name: &str) -> String {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return quote_identifier("");
    }
    split_qualified_parts(trimmed)
        .into_iter()
        .map(|part| quote_identifier(&part))
        .collect::<Vec<_>>()
        .join(".")
}

fn split_qualified_parts(input: &str) -> Vec<String> {
    let chars: Vec<char> = input.chars().collect();
    let mut parts = Vec::new();
    let mut idx = 0;
    while idx < chars.len() {
        if chars[idx] == '"' {
            let start = idx;
            idx += 1;
            let mut unquoted = String::new();
            let mut closed = false;
            while idx < chars.len() {
                if chars[idx] == '"' {
                    if idx + 1 < chars.len() && chars[idx + 1] == '"' {
                        unquoted.push('"');
                        idx += 2;
                    } else {
                        idx += 1;
                        closed = true;
                        break;
                    }
                } else {
                    unquoted.push(chars[idx]);
                    idx += 1;
                }
            }
            if closed && (idx == chars.len() || chars[idx] == '.') {
                parts.push(unquoted);
                if idx < chars.len() && chars[idx] == '.' {
                    idx += 1;
                    if idx == chars.len() {
                        parts.push(String::new());
                    }
                }
            } else {
                // Malformed quoted input: keep the remainder literal so quote_identifier
                // escapes every embedded quote safely.
                let remainder: String = chars[start..].iter().collect();
                parts.push(remainder);
                break;
            }
        } else {
            let start = idx;
            while idx < chars.len() && chars[idx] != '.' {
                if chars[idx] == '"' {
                    // Embedded quote in an unquoted segment: preserve whole remainder literally.
                    let remainder: String = chars[start..].iter().collect();
                    parts.push(remainder);
                    return parts;
                }
                idx += 1;
            }
            let seg: String = chars[start..idx].iter().collect();
            parts.push(seg);
            if idx < chars.len() && chars[idx] == '.' {
                idx += 1;
                if idx == chars.len() {
                    parts.push(String::new());
                }
            }
        }
    }
    if parts.is_empty() {
        parts.push(String::new());
    }
    parts
}

fn normalize_filter(filter: Option<&str>) -> Option<String> {
    filter
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_ascii_lowercase)
}

fn contains_ignore_ascii_case(haystack: &str, needle_lower: &str) -> bool {
    haystack.to_ascii_lowercase().contains(needle_lower)
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
