//! Sidecar unit tests for the pure `replication` domain module.

use super::*;

fn sample_slot(
    slot_name: &str,
    active: bool,
    wal_status: &str,
    retained_bytes: Option<i64>,
) -> ReplicationSlotSummary {
    ReplicationSlotSummary {
        slot_name: slot_name.to_owned(),
        plugin: Some("pgoutput".to_owned()),
        slot_type: "logical".to_owned(),
        database: Some("appdb".to_owned()),
        temporary: false,
        active,
        active_pid: if active { Some(4242) } else { None },
        wal_status: wal_status.to_owned(),
        retained_bytes,
        safe_wal_size: Some(512 * 1024 * 1024),
        risk: SlotRiskSeverity::Ok,
        finding: None,
    }
    .with_assessed_risk()
}

#[test]
fn redact_conninfo_strips_keyword_and_uri_passwords() {
    let kv =
        "host=primary.internal port=5432 dbname=app user=repl password=supersecret sslmode=require";
    let kv_redacted = redact_conninfo(kv);
    assert!(!kv_redacted.contains("supersecret"));
    assert!(kv_redacted.contains("password=[redacted]"));
    assert!(kv_redacted.contains("host=primary.internal"));

    let kv_quoted = "host=db.local user=repl password='has spaces\\'quote' dbname=prod";
    let kv_quoted_redacted = redact_conninfo(kv_quoted);
    assert!(!kv_quoted_redacted.contains("has spaces"));
    assert!(kv_quoted_redacted.contains("password=[redacted]"));

    let uri = "postgres://repl_user:top%40secret@db.example.com:5432/appdb?sslmode=verify-full";
    let uri_redacted = redact_conninfo(uri);
    assert!(!uri_redacted.contains("top%40secret"));
    assert_eq!(
        uri_redacted,
        "postgres://repl_user:[redacted]@db.example.com:5432/appdb?sslmode=verify-full"
    );

    let uri_pg = "postgresql://alice:pw123@10.0.0.5/analytics";
    assert_eq!(
        redact_conninfo(uri_pg),
        "postgresql://alice:[redacted]@10.0.0.5/analytics"
    );
}

#[test]
fn slot_risk_thresholds_classify_ok_warning_critical_and_lost() {
    let ok_slot = sample_slot("active_healthy", true, "reserved", Some(16 * 1024 * 1024));
    assert_eq!(ok_slot.risk, SlotRiskSeverity::Ok);
    assert_eq!(ok_slot.risk.label(), "ok");
    assert!(ok_slot.finding.is_none());

    let inactive_small = sample_slot("inactive_small", false, "reserved", Some(1024));
    assert_eq!(inactive_small.risk, SlotRiskSeverity::Warning);
    assert_eq!(inactive_small.risk.label(), "warning");
    assert!(inactive_small.finding.is_some());

    let inactive_none = sample_slot("inactive_standby", false, "reserved", None);
    assert_eq!(inactive_none.risk, SlotRiskSeverity::Warning);

    let inactive_warn = sample_slot(
        "inactive_warn",
        false,
        "reserved",
        Some(INACTIVE_SLOT_WARNING_BYTES),
    );
    assert_eq!(inactive_warn.risk, SlotRiskSeverity::Warning);
    assert!(
        inactive_warn
            .finding
            .as_deref()
            .is_some_and(|f| f.contains("100.0 MiB"))
    );

    let active_unreserved = sample_slot("active_unreserved", true, "unreserved", Some(4096));
    assert_eq!(active_unreserved.risk, SlotRiskSeverity::Warning);
    assert!(
        active_unreserved
            .finding
            .as_deref()
            .is_some_and(|f| f.contains("unreserved"))
    );

    let inactive_critical = sample_slot(
        "inactive_crit",
        false,
        "extended",
        Some(INACTIVE_SLOT_CRITICAL_BYTES),
    );
    assert_eq!(inactive_critical.risk, SlotRiskSeverity::Critical);
    assert_eq!(inactive_critical.risk.label(), "critical");
    assert!(
        inactive_critical
            .finding
            .as_deref()
            .is_some_and(|f| f.contains("1.0 GiB"))
    );

    let inactive_crit_over_unreserved = sample_slot(
        "inactive_crit_unreserved",
        false,
        "unreserved",
        Some(INACTIVE_SLOT_CRITICAL_BYTES + 1),
    );
    assert_eq!(
        inactive_crit_over_unreserved.risk,
        SlotRiskSeverity::Critical
    );

    let lost_active = sample_slot("lost_slot", true, "lost", Some(0));
    assert_eq!(lost_active.risk, SlotRiskSeverity::Critical);
    assert!(
        lost_active
            .finding
            .as_deref()
            .is_some_and(|f| f.contains("lost"))
    );
}

#[test]
fn report_summaries_and_truncation_reporting() {
    let report = ReplicationReport {
        publications: vec![PublicationSummary {
            name: "pub_orders".to_owned(),
            owner: "postgres".to_owned(),
            all_tables: false,
            insert: true,
            update: true,
            delete: false,
            truncate: false,
            tables: vec!["public.orders".to_owned(), "public.order_items".to_owned()],
            tables_truncated: true,
        }],
        subscriptions: vec![SubscriptionSummary {
            name: "sub_reporting".to_owned(),
            owner: "postgres".to_owned(),
            enabled: true,
            conninfo_redacted: redact_conninfo("postgres://repl:secret@primary/app"),
            slot_name: Some("sub_reporting".to_owned()),
            sync_state: "streaming".to_owned(),
            received_lsn: Some("0/16B6C50".to_owned()),
            latest_end_lsn: Some("0/16B6C50".to_owned()),
            lag_bytes: Some(0),
        }],
        slots: vec![
            sample_slot("slot_ok", true, "reserved", Some(1024)),
            sample_slot("slot_warn", false, "reserved", Some(200 * 1024 * 1024)),
            sample_slot("slot_crit", false, "lost", None),
        ],
        publications_truncated: true,
        subscriptions_truncated: false,
        slots_truncated: true,
        subscription_access_restricted: false,
    };

    assert_eq!(report.inactive_slots_count(), 2);
    assert_eq!(report.max_retained_bytes(), Some(200 * 1024 * 1024));
    assert!(report.has_critical_findings());
    assert!(report.has_warning_findings());

    let empty = ReplicationReport::default();
    assert_eq!(empty.inactive_slots_count(), 0);
    assert_eq!(empty.max_retained_bytes(), None);
    assert!(!empty.has_critical_findings());
    assert!(!empty.has_warning_findings());
}

#[test]
fn report_renders_table_plain_and_json_across_sections() {
    let report = ReplicationReport {
        publications: vec![PublicationSummary {
            name: "pub_all".to_owned(),
            owner: "alice".to_owned(),
            all_tables: true,
            insert: true,
            update: true,
            delete: true,
            truncate: true,
            tables: Vec::new(),
            tables_truncated: false,
        }],
        subscriptions: Vec::new(),
        slots: vec![sample_slot(
            "very_long_replication_slot_name_exceeding_table_width",
            false,
            "unreserved",
            Some(150 * 1024 * 1024),
        )],
        publications_truncated: true,
        subscriptions_truncated: false,
        slots_truncated: false,
        subscription_access_restricted: true,
    };

    let full_table = report.render_table(ReplicationSection::All);
    assert!(full_table.is_ascii());
    assert!(full_table.contains("Publications: 1 (truncated)"));
    assert!(full_table.contains("Subscriptions: 0 (restricted)"));
    assert!(full_table.contains("SQLSTATE 42501"));
    assert!(full_table.contains("Slot Safety Findings:"));
    assert!(full_table.contains("pub_all"));

    let slots_only_table = report.render_table(ReplicationSection::Slots);
    assert!(!slots_only_table.contains("-- Publications --"));
    assert!(!slots_only_table.contains("-- Subscriptions --"));
    assert!(slots_only_table.contains("-- Replication Slots --"));

    let plain = report.render_plain(ReplicationSection::All);
    assert!(plain.is_ascii());
    assert!(plain.contains("very_long_replication_slot_name_exceeding_table_width"));
    assert!(plain.contains("subscription_access_restricted=true"));
    assert!(plain.contains("subscriptions: restricted (SQLSTATE 42501)"));

    let json = report.render_json();
    let parsed: ReplicationReport =
        serde_json::from_str(&json).expect("rendered JSON should deserialize cleanly");
    assert_eq!(parsed, report);
    assert!(json.contains("\"risk\": \"critical\"") || json.contains("\"risk\": \"warning\""));
}

#[test]
fn section_parsing_and_display() {
    assert_eq!(ReplicationSection::All.label(), "all");
    assert_eq!(ReplicationSection::Publications.to_string(), "publications");
    assert_eq!(
        ReplicationSection::from_str_loose("PUBS"),
        Some(ReplicationSection::Publications)
    );
    assert_eq!(
        ReplicationSection::from_str_loose("sub"),
        Some(ReplicationSection::Subscriptions)
    );
    assert_eq!(
        ReplicationSection::from_str_loose("slots"),
        Some(ReplicationSection::Slots)
    );
    assert_eq!(ReplicationSection::from_str_loose("unknown"), None);
}

#[test]
fn sql_generators_escape_quotes_and_include_warnings() {
    let drop_sql = drop_slot_sql("bad'slot; DROP TABLE users;--");
    assert!(drop_sql.starts_with("-- WARNING:"));
    assert!(
        drop_sql.contains(
            "SELECT pg_catalog.pg_drop_replication_slot('bad''slot; DROP TABLE users;--');"
        )
    );

    let pub_all = create_publication_sql("events\"pub", true);
    assert_eq!(
        pub_all,
        "CREATE PUBLICATION \"events\"\"pub\" FOR ALL TABLES;\n"
    );

    let pub_selected = create_publication_sql("orders", false);
    assert!(pub_selected.contains("ALTER PUBLICATION \"orders\" ADD TABLE"));
    assert!(pub_selected.contains("CREATE PUBLICATION \"orders\";\n"));

    assert_eq!(
        alter_subscription_toggle_sql("sub\"1; --", true),
        "ALTER SUBSCRIPTION \"sub\"\"1; --\" ENABLE;\n"
    );
    assert_eq!(
        alter_subscription_toggle_sql("sub_one", false),
        "ALTER SUBSCRIPTION \"sub_one\" DISABLE;\n"
    );
}
