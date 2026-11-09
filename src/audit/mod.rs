//! Pure roles, privilege grant matrix, and row-level security (RLS) auditor model.
//!
//! This module implements the domain logic for Feature
//! `031-roles-privileges-and-rls-auditor` (`M6: Full pgAdmin Replacement Parity`).
//! It depends on no terminal, filesystem, or network types: [`SecurityAuditSnapshot`]
//! is populated from live `pg_roles`, `pg_auth_members`, `pg_class`, `pg_policy`, and
//! `information_schema.role_table_grants` queries
//! ([`crate::postgres::metadata::security_audit`]), bounded at [`MAX_AUDIT_ITEMS`],
//! and rendered deterministically into Role Hierarchy, Table RLS Posture & Policies,
//! ACL Privilege Grants, or JSON reports.

use std::fmt::Write as _;

use crate::postgres::metadata::quote_identifier;

/// Maximum number of roles, tables, or grants retained per section in a single audit snapshot.
pub const MAX_AUDIT_ITEMS: usize = 200;

/// Which view tab is active in the Roles, Privileges, & RLS Auditor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AuditSection {
    /// Overview of roles, attributes (`SUPERUSER`, `BYPASSRLS`, `LOGIN`), and group memberships.
    #[default]
    Roles,
    /// Row-Level Security (`RLS`) posture per table (`ENABLED`, `FORCED`, `DISABLED`) and `pg_policy` definitions.
    RlsPolicies,
    /// Tabular ACL privilege grants (`SELECT`, `INSERT`, `UPDATE`, `DELETE`, etc.) across relations.
    Grants,
}

impl AuditSection {
    /// Human-readable title for overlay headers and CLI summaries.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Roles => "Roles & Memberships",
            Self::RlsPolicies => "Row-Level Security (RLS)",
            Self::Grants => "Table Privilege Grants",
        }
    }

    /// Cycles to the next audit section (`Roles` -> `RlsPolicies` -> `Grants` -> `Roles`).
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Roles => Self::RlsPolicies,
            Self::RlsPolicies => Self::Grants,
            Self::Grants => Self::Roles,
        }
    }
}

/// One PostgreSQL role and its security attributes from `pg_roles` and `pg_auth_members`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleAuditEntry {
    /// Role identifier (`rolname`).
    pub name: String,
    /// Whether the role has `SUPERUSER` authority (`rolsuper`).
    pub is_superuser: bool,
    /// Whether the role can log in (`rolcanlogin`).
    pub can_login: bool,
    /// Whether the role can create databases (`rolcreatedb`).
    pub can_create_db: bool,
    /// Whether the role can create roles (`rolcreaterole`).
    pub can_create_role: bool,
    /// Whether the role inherits privileges of roles it is a member of (`rolinherit`).
    pub inherit: bool,
    /// Whether the role bypasses every Row-Level Security policy (`rolbypassrls`).
    pub bypass_rls: bool,
    /// Connection limit (`rolconnlimit`, `-1` means unlimited).
    pub connection_limit: i32,
    /// Roles that this role is a member of (`member_of`), sorted alphabetically.
    pub member_of: Vec<String>,
}

impl RoleAuditEntry {
    /// Compact attribute badges (`SUPERUSER`, `LOGIN`, `BYPASSRLS`, `CREATEDB`, `CREATEROLE`, `NOINHERIT`).
    #[must_use]
    pub fn attribute_badges(&self) -> String {
        let mut tags = Vec::new();
        if self.is_superuser {
            tags.push("SUPERUSER");
        }
        if self.can_login {
            tags.push("LOGIN");
        } else {
            tags.push("NOLOGIN");
        }
        if self.bypass_rls {
            tags.push("BYPASSRLS");
        }
        if self.can_create_db {
            tags.push("CREATEDB");
        }
        if self.can_create_role {
            tags.push("CREATEROLE");
        }
        if !self.inherit {
            tags.push("NOINHERIT");
        }
        tags.join(", ")
    }
}

/// One Row-Level Security policy from `pg_policy`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RlsPolicyEntry {
    /// Schema name.
    pub schema: String,
    /// Table name.
    pub table: String,
    /// Policy name (`polname`).
    pub policy_name: String,
    /// Whether the policy is `PERMISSIVE` or `RESTRICTIVE`.
    pub permissive: bool,
    /// Applicable command (`ALL`, `SELECT`, `INSERT`, `UPDATE`, `DELETE`).
    pub command: String,
    /// Target roles (`PUBLIC` or role names).
    pub roles: Vec<String>,
    /// `USING (...)` read visibility expression, if present.
    pub using_expr: Option<String>,
    /// `WITH CHECK (...)` write validation expression, if present.
    pub with_check_expr: Option<String>,
}

/// RLS posture and attached policies for one user table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableRlsAudit {
    /// Schema name.
    pub schema: String,
    /// Table name.
    pub table: String,
    /// Table owner role name.
    pub owner: String,
    /// Whether `ENABLE ROW LEVEL SECURITY` is active (`relrowsecurity`).
    pub rls_enabled: bool,
    /// Whether `FORCE ROW LEVEL SECURITY` is active (`relforcerowsecurity`).
    pub rls_forced: bool,
    /// Attached policies on this table sorted by `policy_name`.
    pub policies: Vec<RlsPolicyEntry>,
}

impl TableRlsAudit {
    /// Qualified `schema.table` name.
    #[must_use]
    pub fn qualified_name(&self) -> String {
        format!("{}.{}", self.schema, self.table)
    }

    /// Plain-language RLS posture badge (`FORCED`, `ENABLED`, `ENABLED (NO POLICIES!)`, `DISABLED`).
    #[must_use]
    pub fn posture_badge(&self) -> &'static str {
        if self.rls_enabled && self.policies.is_empty() {
            "ENABLED (NO POLICIES)"
        } else if self.rls_forced {
            "FORCED"
        } else if self.rls_enabled {
            "ENABLED"
        } else {
            "DISABLED"
        }
    }

    /// Whether this table has a security posture warning (RLS disabled or enabled with zero policies).
    #[must_use]
    pub fn has_finding(&self) -> bool {
        !self.rls_enabled || self.policies.is_empty()
    }
}

/// Aggregated privilege grants on one relation for one grantee role.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableGrantEntry {
    /// Schema name.
    pub schema: String,
    /// Table or view name.
    pub table: String,
    /// Role receiving the grant (`grantee`).
    pub grantee: String,
    /// Sorted list of granted privileges (`SELECT`, `INSERT`, `UPDATE`, `DELETE`, etc.).
    pub privileges: Vec<String>,
    /// Whether any privilege was granted `WITH GRANT OPTION`.
    pub is_grantable: bool,
}

impl TableGrantEntry {
    /// Qualified `schema.table` name.
    #[must_use]
    pub fn qualified_name(&self) -> String {
        format!("{}.{}", self.schema, self.table)
    }
}

/// Complete bounded security audit snapshot across roles, RLS posture, and privilege grants.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SecurityAuditSnapshot {
    /// Roles ordered deterministically by name.
    pub roles: Vec<RoleAuditEntry>,
    /// User tables and their RLS posture/policies ordered by `(schema, table)`.
    pub tables: Vec<TableRlsAudit>,
    /// Aggregated relation privilege grants ordered by `(schema, table, grantee)`.
    pub grants: Vec<TableGrantEntry>,
    /// Set if any collection exceeded [`MAX_AUDIT_ITEMS`].
    pub truncated: bool,
}

impl SecurityAuditSnapshot {
    /// Sorts all collections deterministically and enforces [`MAX_AUDIT_ITEMS`].
    #[must_use]
    pub fn bounded(mut self) -> Self {
        self.roles.sort_by(|a, b| a.name.cmp(&b.name));
        self.tables
            .sort_by(|a, b| (&a.schema, &a.table).cmp(&(&b.schema, &b.table)));
        for tbl in &mut self.tables {
            tbl.policies
                .sort_by(|a, b| a.policy_name.cmp(&b.policy_name));
        }
        self.grants.sort_by(|a, b| {
            (&a.schema, &a.table, &a.grantee).cmp(&(&b.schema, &b.table, &b.grantee))
        });

        if self.roles.len() > MAX_AUDIT_ITEMS {
            self.roles.truncate(MAX_AUDIT_ITEMS);
            self.truncated = true;
        }
        if self.tables.len() > MAX_AUDIT_ITEMS {
            self.tables.truncate(MAX_AUDIT_ITEMS);
            self.truncated = true;
        }
        if self.grants.len() > MAX_AUDIT_ITEMS {
            self.grants.truncate(MAX_AUDIT_ITEMS);
            self.truncated = true;
        }
        self
    }

    /// Generates a reviewed, non-destructive SQL script to enable RLS and create a starter
    /// tenant-isolation policy on `table`, or inspect/alter its existing policies.
    #[must_use]
    pub fn reviewed_rls_remediation_sql(table: &TableRlsAudit) -> String {
        let fq = format!(
            "{}.{}",
            quote_identifier(&table.schema),
            quote_identifier(&table.table)
        );
        let mut out = String::new();
        let _ = writeln!(
            out,
            "-- Reviewed RLS Remediation Script for {}.{}\n\
             -- Current posture: {} ({} attached policy/policies)",
            table.schema,
            table.table,
            table.posture_badge(),
            table.policies.len()
        );
        let _ = writeln!(out, "ALTER TABLE {fq} ENABLE ROW LEVEL SECURITY;");
        if !table.rls_forced {
            let _ = writeln!(out, "-- Optional: enforce RLS for table owner as well:");
            let _ = writeln!(out, "-- ALTER TABLE {fq} FORCE ROW LEVEL SECURITY;");
        }
        if table.policies.is_empty() {
            let policy_ident = quote_identifier(&format!("{}_isolation_policy", table.table));
            let _ = writeln!(
                out,
                "\nCREATE POLICY {policy_ident} ON {fq}\n  \
                 AS PERMISSIVE\n  \
                 FOR ALL\n  \
                 TO PUBLIC\n  \
                 USING (true)\n  \
                 WITH CHECK (true);"
            );
        } else {
            for p in &table.policies {
                let pol_ident = quote_identifier(&p.policy_name);
                let mode = if p.permissive {
                    "PERMISSIVE"
                } else {
                    "RESTRICTIVE"
                };
                let roles = if p.roles.is_empty() {
                    "PUBLIC".to_owned()
                } else {
                    p.roles.join(", ")
                };
                let using_clause = p
                    .using_expr
                    .as_deref()
                    .map_or(String::new(), |u| format!("\n  USING ({u})"));
                let check_clause = p
                    .with_check_expr
                    .as_deref()
                    .map_or(String::new(), |c| format!("\n  WITH CHECK ({c})"));
                let _ = writeln!(
                    out,
                    "\n-- Existing policy: {}\n\
                     CREATE POLICY {pol_ident} ON {fq}\n  \
                     AS {mode}\n  \
                     FOR {}\n  \
                     TO {roles}{using_clause}{check_clause};",
                    p.policy_name, p.command
                );
            }
        }
        out
    }

    /// Generates a reviewed SQL script for a role (`ALTER ROLE` attributes and `GRANT` memberships).
    #[must_use]
    pub fn reviewed_role_sql(role: &RoleAuditEntry) -> String {
        let role_ident = quote_identifier(&role.name);
        let mut out = String::new();
        let _ = writeln!(
            out,
            "-- Reviewed Role Security Script for {}\n\
             -- Current attributes: {}",
            role.name,
            role.attribute_badges()
        );
        let login = if role.can_login { "LOGIN" } else { "NOLOGIN" };
        let superuser = if role.is_superuser {
            "SUPERUSER"
        } else {
            "NOSUPERUSER"
        };
        let createdb = if role.can_create_db {
            "CREATEDB"
        } else {
            "NOCREATEDB"
        };
        let createrole = if role.can_create_role {
            "CREATEROLE"
        } else {
            "NOCREATEROLE"
        };
        let inherit = if role.inherit { "INHERIT" } else { "NOINHERIT" };
        let bypassrls = if role.bypass_rls {
            "BYPASSRLS"
        } else {
            "NOBYPASSRLS"
        };
        let _ = writeln!(
            out,
            "ALTER ROLE {role_ident} WITH {login} {superuser} {createdb} {createrole} {inherit} {bypassrls} CONNECTION LIMIT {};",
            role.connection_limit
        );
        for group in &role.member_of {
            let _ = writeln!(out, "GRANT {} TO {role_ident};", quote_identifier(group));
        }
        out
    }

    /// Generates a reviewed `GRANT` / `REVOKE` SQL template for a [`TableGrantEntry`].
    #[must_use]
    pub fn reviewed_grant_sql(grant: &TableGrantEntry) -> String {
        let fq = format!(
            "{}.{}",
            quote_identifier(&grant.schema),
            quote_identifier(&grant.table)
        );
        let grantee = if grant.grantee.eq_ignore_ascii_case("PUBLIC") {
            "PUBLIC".to_owned()
        } else {
            quote_identifier(&grant.grantee)
        };
        let privs = if grant.privileges.is_empty() {
            "SELECT".to_owned()
        } else {
            grant.privileges.join(", ")
        };
        let grant_opt = if grant.is_grantable {
            " WITH GRANT OPTION"
        } else {
            ""
        };
        format!(
            "-- Reviewed Privilege Grant Script for {}.{}\n\
             GRANT {privs} ON TABLE {fq} TO {grantee}{grant_opt};\n\
             -- To revoke instead:\n\
             -- REVOKE {privs} ON TABLE {fq} FROM {grantee};\n",
            grant.schema, grant.table
        )
    }

    /// Renders a human-readable report for the selected `section` (or all sections when `None`).
    #[must_use]
    pub fn render_text(&self, section: Option<AuditSection>, unicode: bool) -> String {
        let mut out = String::new();
        let rule_ch = if unicode { '─' } else { '-' };
        let rls_enabled_count = self.tables.iter().filter(|t| t.rls_enabled).count();
        let rls_disabled_count = self.tables.len().saturating_sub(rls_enabled_count);
        let superuser_count = self.roles.iter().filter(|r| r.is_superuser).count();

        let _ = writeln!(
            out,
            "Roles: {} ({} superuser) | Tables: {} ({} RLS enabled, {} RLS disabled) | Grants: {}",
            self.roles.len(),
            superuser_count,
            self.tables.len(),
            rls_enabled_count,
            rls_disabled_count,
            self.grants.len(),
        );
        let _ = writeln!(out, "{}", rule_ch.to_string().repeat(100));

        let show_roles = matches!(section, None | Some(AuditSection::Roles));
        let show_rls = matches!(section, None | Some(AuditSection::RlsPolicies));
        let show_grants = matches!(section, None | Some(AuditSection::Grants));

        if show_roles {
            let _ = writeln!(
                out,
                "{:<22} {:<42} {:<10} MEMBER OF",
                "ROLE", "ATTRIBUTES", "CONN LIMIT"
            );
            let _ = writeln!(out, "{}", rule_ch.to_string().repeat(100));
            if self.roles.is_empty() {
                let _ = writeln!(out, "(no roles reported)");
            }
            for role in &self.roles {
                let conn = if role.connection_limit < 0 {
                    "unlimited".to_owned()
                } else {
                    role.connection_limit.to_string()
                };
                let groups = if role.member_of.is_empty() {
                    "-".to_owned()
                } else {
                    role.member_of.join(", ")
                };
                let _ = writeln!(
                    out,
                    "{:<22} {:<42} {:<10} {}",
                    truncate_cell(&role.name, 22),
                    truncate_cell(&role.attribute_badges(), 42),
                    conn,
                    groups
                );
            }
            if section.is_none() {
                let _ = writeln!(out);
            }
        }

        if show_rls {
            let _ = writeln!(
                out,
                "{:<28} {:<16} {:<22} POLICIES",
                "TABLE", "OWNER", "RLS POSTURE"
            );
            let _ = writeln!(out, "{}", rule_ch.to_string().repeat(100));
            if self.tables.is_empty() {
                let _ = writeln!(out, "(no user tables found)");
            }
            for tbl in &self.tables {
                let pol_summary = if tbl.policies.is_empty() {
                    "none".to_owned()
                } else {
                    tbl.policies
                        .iter()
                        .map(|p| format!("{} ({})", p.policy_name, p.command))
                        .collect::<Vec<_>>()
                        .join(", ")
                };
                let _ = writeln!(
                    out,
                    "{:<28} {:<16} {:<22} {}",
                    truncate_cell(&tbl.qualified_name(), 28),
                    truncate_cell(&tbl.owner, 16),
                    tbl.posture_badge(),
                    pol_summary
                );
                for p in &tbl.policies {
                    let branch = if unicode { "  └─ " } else { "  +- " };
                    let mode = if p.permissive {
                        "PERMISSIVE"
                    } else {
                        "RESTRICTIVE"
                    };
                    let roles = if p.roles.is_empty() {
                        "PUBLIC".to_owned()
                    } else {
                        p.roles.join(",")
                    };
                    let using_txt = p.using_expr.as_deref().unwrap_or("-");
                    let check_txt = p.with_check_expr.as_deref().unwrap_or("-");
                    let _ = writeln!(
                        out,
                        "{branch}POLICY {} [{mode} {} TO {roles}] USING: {using_txt} | CHECK: {check_txt}",
                        p.policy_name, p.command
                    );
                }
            }
            if section.is_none() {
                let _ = writeln!(out);
            }
        }

        if show_grants {
            let _ = writeln!(
                out,
                "{:<28} {:<20} {:<12} PRIVILEGES",
                "TABLE", "GRANTEE", "GRANT OPTION"
            );
            let _ = writeln!(out, "{}", rule_ch.to_string().repeat(100));
            if self.grants.is_empty() {
                let _ = writeln!(out, "(no explicit table privilege grants found)");
            }
            for g in &self.grants {
                let opt = if g.is_grantable { "YES" } else { "NO" };
                let _ = writeln!(
                    out,
                    "{:<28} {:<20} {:<12} {}",
                    truncate_cell(&g.qualified_name(), 28),
                    truncate_cell(&g.grantee, 20),
                    opt,
                    g.privileges.join(", ")
                );
            }
        }

        if self.truncated {
            let _ = writeln!(
                out,
                "-- Note: audit results truncated to first {} entries.",
                MAX_AUDIT_ITEMS
            );
        }
        out
    }

    /// Renders the snapshot as a deterministic JSON document for `ignatius audit --format json`.
    #[must_use]
    pub fn render_json(&self) -> String {
        let mut out = String::new();
        out.push_str("{\n  \"roles\": [\n");
        for (idx, r) in self.roles.iter().enumerate() {
            let comma = if idx + 1 == self.roles.len() { "" } else { "," };
            let groups = r
                .member_of
                .iter()
                .map(|m| json_string(m))
                .collect::<Vec<_>>()
                .join(", ");
            let _ = writeln!(
                out,
                "    {{\"name\": {}, \"is_superuser\": {}, \"can_login\": {}, \"can_create_db\": {}, \"can_create_role\": {}, \"inherit\": {}, \"bypass_rls\": {}, \"connection_limit\": {}, \"member_of\": [{}]}}{comma}",
                json_string(&r.name),
                r.is_superuser,
                r.can_login,
                r.can_create_db,
                r.can_create_role,
                r.inherit,
                r.bypass_rls,
                r.connection_limit,
                groups
            );
        }
        out.push_str("  ],\n  \"tables\": [\n");
        for (idx, t) in self.tables.iter().enumerate() {
            let comma = if idx + 1 == self.tables.len() {
                ""
            } else {
                ","
            };
            let policies_json = t
                .policies
                .iter()
                .map(|p| {
                    let roles = p
                        .roles
                        .iter()
                        .map(|r| json_string(r))
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!(
                        "{{\"policy_name\": {}, \"permissive\": {}, \"command\": {}, \"roles\": [{}], \"using_expr\": {}, \"with_check_expr\": {}}}",
                        json_string(&p.policy_name),
                        p.permissive,
                        json_string(&p.command),
                        roles,
                        json_opt_string(p.using_expr.as_deref()),
                        json_opt_string(p.with_check_expr.as_deref()),
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            let _ = writeln!(
                out,
                "    {{\"schema\": {}, \"table\": {}, \"owner\": {}, \"rls_enabled\": {}, \"rls_forced\": {}, \"posture\": {}, \"policies\": [{}]}}{comma}",
                json_string(&t.schema),
                json_string(&t.table),
                json_string(&t.owner),
                t.rls_enabled,
                t.rls_forced,
                json_string(t.posture_badge()),
                policies_json
            );
        }
        out.push_str("  ],\n  \"grants\": [\n");
        for (idx, g) in self.grants.iter().enumerate() {
            let comma = if idx + 1 == self.grants.len() {
                ""
            } else {
                ","
            };
            let privs = g
                .privileges
                .iter()
                .map(|p| json_string(p))
                .collect::<Vec<_>>()
                .join(", ");
            let _ = writeln!(
                out,
                "    {{\"schema\": {}, \"table\": {}, \"grantee\": {}, \"is_grantable\": {}, \"privileges\": [{}]}}{comma}",
                json_string(&g.schema),
                json_string(&g.table),
                json_string(&g.grantee),
                g.is_grantable,
                privs
            );
        }
        out.push_str("  ]\n}\n");
        out
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
