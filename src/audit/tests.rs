use super::{
    AuditSection, RlsPolicyEntry, RoleAuditEntry, SecurityAuditSnapshot, TableGrantEntry,
    TableRlsAudit,
};

fn sample_snapshot() -> SecurityAuditSnapshot {
    SecurityAuditSnapshot {
        roles: vec![
            RoleAuditEntry {
                name: "ignatius_test".to_owned(),
                is_superuser: true,
                can_login: true,
                can_create_db: true,
                can_create_role: true,
                inherit: true,
                bypass_rls: true,
                connection_limit: -1,
                member_of: vec!["pg_read_all_stats".to_owned()],
            },
            RoleAuditEntry {
                name: "analyst_readonly".to_owned(),
                is_superuser: false,
                can_login: true,
                can_create_db: false,
                can_create_role: false,
                inherit: true,
                bypass_rls: false,
                connection_limit: 10,
                member_of: Vec::new(),
            },
        ],
        tables: vec![
            TableRlsAudit {
                schema: "public".to_owned(),
                table: "orders".to_owned(),
                owner: "ignatius_test".to_owned(),
                rls_enabled: true,
                rls_forced: false,
                policies: vec![RlsPolicyEntry {
                    schema: "public".to_owned(),
                    table: "orders".to_owned(),
                    policy_name: "orders_tenant_isolation".to_owned(),
                    permissive: true,
                    command: "ALL".to_owned(),
                    roles: vec!["PUBLIC".to_owned()],
                    using_expr: Some("customer_id = 1".to_owned()),
                    with_check_expr: Some("customer_id = 1".to_owned()),
                }],
            },
            TableRlsAudit {
                schema: "public".to_owned(),
                table: "customers".to_owned(),
                owner: "ignatius_test".to_owned(),
                rls_enabled: false,
                rls_forced: false,
                policies: Vec::new(),
            },
        ],
        grants: vec![TableGrantEntry {
            schema: "public".to_owned(),
            table: "orders".to_owned(),
            grantee: "analyst_readonly".to_owned(),
            privileges: vec!["SELECT".to_owned()],
            is_grantable: false,
        }],
        truncated: false,
    }
    .bounded()
}

#[test]
fn security_audit_snapshot_renders_roles_rls_and_grants_deterministically() {
    let snapshot = sample_snapshot();
    assert_eq!(snapshot.roles[0].name, "analyst_readonly");
    assert_eq!(snapshot.roles[1].name, "ignatius_test");

    let roles_text = snapshot.render_text(Some(AuditSection::Roles), true);
    assert!(roles_text.contains("SUPERUSER, LOGIN, BYPASSRLS"));
    assert!(roles_text.contains("pg_read_all_stats"));

    let rls_text = snapshot.render_text(Some(AuditSection::RlsPolicies), true);
    assert!(rls_text.contains("public.customers"));
    assert!(rls_text.contains("DISABLED"));
    assert!(rls_text.contains("orders_tenant_isolation"));
    assert!(rls_text.contains("USING: customer_id = 1"));

    let grants_text = snapshot.render_text(Some(AuditSection::Grants), true);
    assert!(grants_text.contains("analyst_readonly"));
    assert!(grants_text.contains("SELECT"));

    let json = snapshot.render_json();
    assert!(json.contains("\"policy_name\": \"orders_tenant_isolation\""));
    assert!(json.contains("\"grantee\": \"analyst_readonly\""));
}

#[test]
fn reviewed_remediation_scripts_generate_valid_ddl_without_side_effects() {
    let snapshot = sample_snapshot();
    let customers = &snapshot.tables[0];
    assert_eq!(customers.table, "customers");
    assert!(customers.has_finding());

    let rls_sql = SecurityAuditSnapshot::reviewed_rls_remediation_sql(customers);
    assert!(rls_sql.contains("ALTER TABLE \"public\".\"customers\" ENABLE ROW LEVEL SECURITY;"));
    assert!(
        rls_sql
            .contains("CREATE POLICY \"customers_isolation_policy\" ON \"public\".\"customers\"")
    );

    let role_sql = SecurityAuditSnapshot::reviewed_role_sql(&snapshot.roles[0]);
    assert!(role_sql.contains("ALTER ROLE \"analyst_readonly\" WITH LOGIN NOSUPERUSER"));

    let grant_sql = SecurityAuditSnapshot::reviewed_grant_sql(&snapshot.grants[0]);
    assert!(
        grant_sql.contains("GRANT SELECT ON TABLE \"public\".\"orders\" TO \"analyst_readonly\";")
    );
}
