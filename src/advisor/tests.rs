//! Unit tests for index advisor, bloat forensics, and hypothetical indexing.

use super::*;

#[test]
fn redundant_index_generates_safe_concurrent_drop_sql() {
    let item = RedundantIndex {
        schema: "public".into(),
        table: "orders".into(),
        index_name: "idx_orders_user_id".into(),
        covering_index: Some("idx_orders_user_id_status".into()),
        reason: RedundantReason::PrefixCovered,
        columns: vec!["user_id".into()],
        size_bytes: 16 * 1024 * 1024,
        scans: 12,
    };

    assert_eq!(
        item.suggested_drop_sql(),
        "DROP INDEX CONCURRENTLY IF EXISTS \"public\".\"idx_orders_user_id\";"
    );
    assert_eq!(item.reason.label(), "PREFIX COVERED");
}

#[test]
fn bloated_index_generates_safe_concurrent_reindex_sql() {
    let item = IndexBloatMetric {
        schema: "ecommerce".into(),
        table: "line_items".into(),
        index_name: "line_items_pkey".into(),
        total_bytes: 100 * 1024 * 1024,
        wasted_bytes: 42 * 1024 * 1024,
        bloat_pct: 42.0,
    };

    assert_eq!(
        item.suggested_reindex_sql(),
        "REINDEX INDEX CONCURRENTLY \"ecommerce\".\"line_items_pkey\";"
    );
}

#[test]
fn unindexed_foreign_key_generates_canonical_concurrent_create_sql() {
    let fk = UnindexedForeignKey {
        constraint_name: "fk_order_customer".into(),
        schema: "public".into(),
        child_table: "orders".into(),
        columns: vec!["customer_id".into(), "tenant_id".into()],
        parent_table: "customers".into(),
    };

    assert_eq!(
        fk.suggested_index_name(),
        "idx_orders_customer_id_tenant_id"
    );
    assert_eq!(
        fk.suggested_create_sql(),
        "CREATE INDEX CONCURRENTLY IF NOT EXISTS \"idx_orders_customer_id_tenant_id\" ON \"public\".\"orders\" (\"customer_id\", \"tenant_id\");"
    );
}

#[test]
fn advisor_report_calculates_health_score_and_reclaimable_space() {
    let mut report = AdvisorReport::default();
    assert_eq!(report.health_score(), 100);
    assert_eq!(report.total_reclaimable_bytes(), 0);

    report.redundant_indexes.push(RedundantIndex {
        schema: "public".into(),
        table: "users".into(),
        index_name: "users_email_idx".into(),
        covering_index: None,
        reason: RedundantReason::ExactDuplicate,
        columns: vec!["email".into()],
        size_bytes: 10 * 1024 * 1024,
        scans: 0,
    });

    report.bloated_indexes.push(IndexBloatMetric {
        schema: "public".into(),
        table: "events".into(),
        index_name: "events_created_at_idx".into(),
        total_bytes: 50 * 1024 * 1024,
        wasted_bytes: 25 * 1024 * 1024,
        bloat_pct: 50.0,
    });

    report.unindexed_foreign_keys.push(UnindexedForeignKey {
        constraint_name: "fk_event_actor".into(),
        schema: "public".into(),
        child_table: "events".into(),
        columns: vec!["actor_id".into()],
        parent_table: "actors".into(),
    });

    // 100 - (1*3 + 1*5 + 1*4) = 100 - 12 = 88%
    assert_eq!(report.health_score(), 88);
    // 10 MB + 25 MB = 35 MB
    assert_eq!(report.total_reclaimable_bytes(), 35 * 1024 * 1024);
    assert_eq!(report.format_reclaimable(), "35.0 MB");
}

#[test]
fn advisor_report_renders_table_json_and_plain() {
    let mut report = AdvisorReport {
        hypopg_installed: true,
        ..Default::default()
    };

    report.redundant_indexes.push(RedundantIndex {
        schema: "public".into(),
        table: "users".into(),
        index_name: "users_email_idx".into(),
        covering_index: Some("users_email_unique".into()),
        reason: RedundantReason::ExactDuplicate,
        columns: vec!["email".into()],
        size_bytes: 5 * 1024 * 1024,
        scans: 0,
    });

    report.hypo_tests.push(HypoIndexTestResult {
        proposed_index_sql: "CREATE INDEX ON users(status);".into(),
        query_text: "SELECT * FROM users WHERE status = 'active'".into(),
        hypo_available: true,
        original_cost: 1420.5,
        hypo_cost: 12.3,
        cost_reduction_pct: 99.1,
        used_in_plan: true,
        notice: None,
    });

    let table = report.render_table(AdvisorSection::All);
    assert!(table.contains("Ignatius Index Advisor & Bloat Forensics"));
    assert!(table.contains("users_email_idx"));
    assert!(table.contains("EXACT DUPLICATE"));
    assert!(table.contains("HypoPG Status: ENABLED"));

    let json = report.render_json();
    assert!(json.contains("\"users_email_idx\""));
    assert!(json.contains("\"hypopg_installed\": true"));

    let plain = report.render_plain(AdvisorSection::Redundant);
    assert!(plain.contains("Redundant & Unused Indexes"));
}

#[test]
fn format_bytes_handles_various_scales() {
    assert_eq!(format_bytes(512), "512 B");
    assert_eq!(format_bytes(2048), "2.0 KB");
    assert_eq!(format_bytes(10 * 1024 * 1024), "10.0 MB");
    assert_eq!(format_bytes(2 * 1024 * 1024 * 1024), "2.0 GB");
}

#[test]
fn advisor_section_parsing() {
    assert_eq!(
        AdvisorSection::from_str_loose("all"),
        Some(AdvisorSection::All)
    );
    assert_eq!(
        AdvisorSection::from_str_loose("redundant"),
        Some(AdvisorSection::Redundant)
    );
    assert_eq!(
        AdvisorSection::from_str_loose("duplicates"),
        Some(AdvisorSection::Redundant)
    );
    assert_eq!(
        AdvisorSection::from_str_loose("bloat"),
        Some(AdvisorSection::Bloat)
    );
    assert_eq!(
        AdvisorSection::from_str_loose("unindexed"),
        Some(AdvisorSection::Unindexed)
    );
    assert_eq!(
        AdvisorSection::from_str_loose("hypo"),
        Some(AdvisorSection::Hypo)
    );
    assert_eq!(AdvisorSection::from_str_loose("unknown"), None);
}
