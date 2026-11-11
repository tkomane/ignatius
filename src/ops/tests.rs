use super::*;

#[test]
fn build_maintenance_sql_vacuum_variants() {
    let plain_vacuum = MaintenanceOptions {
        kind: MaintenanceKind::Vacuum {
            full: false,
            freeze: false,
            verbose: false,
            analyze: false,
        },
        target: None,
        confirm_production: false,
    };
    assert_eq!(build_maintenance_sql(&plain_vacuum).unwrap(), "VACUUM;");

    let full_target = MaintenanceOptions {
        kind: MaintenanceKind::Vacuum {
            full: true,
            freeze: true,
            verbose: true,
            analyze: true,
        },
        target: Some("public.orders".to_string()),
        confirm_production: false,
    };
    assert_eq!(
        build_maintenance_sql(&full_target).unwrap(),
        "VACUUM (FULL, FREEZE, VERBOSE, ANALYZE) \"public\".\"orders\";"
    );
}

#[test]
fn build_maintenance_sql_analyze() {
    let opts = MaintenanceOptions {
        kind: MaintenanceKind::Analyze { verbose: true },
        target: Some("public.users".to_string()),
        confirm_production: false,
    };
    assert_eq!(
        build_maintenance_sql(&opts).unwrap(),
        "ANALYZE (VERBOSE) \"public\".\"users\";"
    );
}

#[test]
fn build_maintenance_sql_reindex_scopes() {
    let table_reindex = MaintenanceOptions {
        kind: MaintenanceKind::Reindex {
            concurrently: true,
            scope: ReindexScope::Table,
        },
        target: Some("public.orders".to_string()),
        confirm_production: false,
    };
    assert_eq!(
        build_maintenance_sql(&table_reindex).unwrap(),
        "REINDEX TABLE CONCURRENTLY \"public\".\"orders\";"
    );

    let index_reindex = MaintenanceOptions {
        kind: MaintenanceKind::Reindex {
            concurrently: false,
            scope: ReindexScope::Index,
        },
        target: Some("public.orders_pkey".to_string()),
        confirm_production: false,
    };
    assert_eq!(
        build_maintenance_sql(&index_reindex).unwrap(),
        "REINDEX INDEX \"public\".\"orders_pkey\";"
    );

    let schema_reindex = MaintenanceOptions {
        kind: MaintenanceKind::Reindex {
            concurrently: true,
            scope: ReindexScope::Schema,
        },
        target: Some("reporting".to_string()),
        confirm_production: false,
    };
    assert_eq!(
        build_maintenance_sql(&schema_reindex).unwrap(),
        "REINDEX SCHEMA CONCURRENTLY \"reporting\";"
    );

    let db_reindex = MaintenanceOptions {
        kind: MaintenanceKind::Reindex {
            concurrently: false,
            scope: ReindexScope::Database,
        },
        target: None,
        confirm_production: false,
    };
    assert_eq!(
        build_maintenance_sql(&db_reindex).unwrap(),
        "REINDEX DATABASE current_database();"
    );
}

#[test]
fn estimate_maintenance_lock_levels() {
    let full_vac = MaintenanceKind::Vacuum {
        full: true,
        freeze: false,
        verbose: false,
        analyze: false,
    };
    assert!(estimate_maintenance_lock_level(&full_vac).contains("AccessExclusiveLock"));

    let standard_vac = MaintenanceKind::Vacuum {
        full: false,
        freeze: false,
        verbose: true,
        analyze: true,
    };
    assert!(estimate_maintenance_lock_level(&standard_vac).contains("ShareUpdateExclusiveLock"));

    let conc_reindex = MaintenanceKind::Reindex {
        concurrently: true,
        scope: ReindexScope::Table,
    };
    assert!(estimate_maintenance_lock_level(&conc_reindex).contains("ShareUpdateExclusiveLock"));

    let lock_reindex = MaintenanceKind::Reindex {
        concurrently: false,
        scope: ReindexScope::Table,
    };
    assert!(estimate_maintenance_lock_level(&lock_reindex).contains("AccessExclusiveLock"));
}

#[test]
fn listen_unlisten_notify_sql_building() {
    assert_eq!(build_listen_sql("order_events"), "LISTEN \"order_events\";");
    assert_eq!(
        build_unlisten_sql("order_events"),
        "UNLISTEN \"order_events\";"
    );
    assert_eq!(build_unlisten_sql("*"), "UNLISTEN *;");

    assert_eq!(
        build_notify_sql("order_events", None),
        "NOTIFY \"order_events\";"
    );
    assert_eq!(
        build_notify_sql("order_events", Some("{\"id\": 42}")),
        "NOTIFY \"order_events\", '{\"id\": 42}';"
    );
    assert_eq!(
        build_notify_sql("chat", Some("It's alive")),
        "NOTIFY \"chat\", 'It''s alive';"
    );
}

#[test]
fn render_notifications_table_and_jsonl() {
    let empty: Vec<PubSubNotification> = Vec::new();
    assert_eq!(
        render_notifications_table(&empty),
        "No pub/sub notifications received."
    );

    let sample = vec![PubSubNotification {
        timestamp: "2026-11-11T12:00:00Z".to_string(),
        channel: "order_events".to_string(),
        sender_pid: 14205,
        payload: "{\"status\": \"paid\"}".to_string(),
    }];

    let table = render_notifications_table(&sample);
    assert!(table.contains("order_events"));
    assert!(table.contains("14205"));
    assert!(table.contains("{\"status\": \"paid\"}"));

    let jsonl = render_notifications_jsonl(&sample);
    assert!(jsonl.contains("\"channel\":\"order_events\""));
    assert!(jsonl.contains("\"sender_pid\":14205"));
}

#[test]
fn render_maintenance_progress_table_and_json() {
    let empty: Vec<MaintenanceProgressEntry> = Vec::new();
    assert!(render_maintenance_progress_table(&empty).contains("No active vacuum"));

    let entries = vec![MaintenanceProgressEntry {
        pid: 29481,
        datname: "ignatius_demo".to_string(),
        relname: Some("orders".to_string()),
        command: "VACUUM".to_string(),
        phase: "scanning_heap".to_string(),
        heap_blks_total: Some(1000),
        heap_blks_scanned: Some(450),
        heap_blks_vacuumed: Some(0),
        index_vacuum_count: Some(0),
        percent: Some(45.0),
    }];

    let table = render_maintenance_progress_table(&entries);
    assert!(table.contains("29481"));
    assert!(table.contains("orders"));
    assert!(table.contains("45.0%"));
    assert!(table.contains("450 / 1000"));

    let json = render_maintenance_progress_json(&entries);
    assert!(json.contains("\"command\": \"VACUUM\""));
    assert!(json.contains("\"percent\": 45.0"));
}
