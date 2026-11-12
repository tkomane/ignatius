use super::*;

#[test]
fn test_server_connection_stats_calculations() {
    let stats = ServerConnectionStats {
        total_connections: 85,
        active: 40,
        idle: 35,
        idle_in_transaction: 10,
        idle_in_transaction_aborted: 0,
        fastpath: 0,
        disabled: 0,
        max_connections: 100,
        superuser_reserved_connections: 3,
    };

    assert_eq!(stats.usable_max_connections(), 97);
    assert_eq!(stats.headroom(), 12);
    assert!((stats.saturation_ratio() - 85.0).abs() < f64::EPSILON);
    assert!(stats.is_critical());

    let low_stats = ServerConnectionStats {
        total_connections: 10,
        max_connections: 100,
        superuser_reserved_connections: 3,
        ..Default::default()
    };
    assert_eq!(low_stats.usable_max_connections(), 97);
    assert_eq!(low_stats.headroom(), 87);
    assert!((low_stats.saturation_ratio() - 10.0).abs() < f64::EPSILON);
    assert!(!low_stats.is_critical());

    let zero_stats = ServerConnectionStats {
        max_connections: 0,
        ..Default::default()
    };
    assert_eq!(zero_stats.saturation_ratio(), 0.0);
    assert_eq!(zero_stats.headroom(), 0);
}

#[test]
fn test_pool_report_aggregates() {
    let report = PoolReport {
        is_pgbouncer: true,
        server_stats: ServerConnectionStats {
            total_connections: 20,
            max_connections: 100,
            superuser_reserved_connections: 3,
            ..Default::default()
        },
        pools: vec![
            PgBouncerPool {
                database: "app_db".to_string(),
                user: "app_user".to_string(),
                cl_active: 15,
                cl_waiting: 2,
                sv_active: 10,
                sv_idle: 5,
                sv_used: 0,
                sv_tested: 0,
                sv_login: 0,
                maxwait_seconds: 0.25,
                avg_query_time_ms: 1.5,
                pool_mode: "transaction".to_string(),
            },
            PgBouncerPool {
                database: "analytics_db".to_string(),
                user: "bi_user".to_string(),
                cl_active: 5,
                cl_waiting: 1,
                sv_active: 4,
                sv_idle: 1,
                sv_used: 0,
                sv_tested: 0,
                sv_login: 0,
                maxwait_seconds: 1.10,
                avg_query_time_ms: 45.0,
                pool_mode: "session".to_string(),
            },
        ],
        waiting_clients: vec![ClientWaitMetric {
            pid: 1234,
            database: "app_db".to_string(),
            user: "app_user".to_string(),
            application_name: "backend-service".to_string(),
            client_addr: Some("10.0.0.1".to_string()),
            wait_event_type: Some("Lock".to_string()),
            wait_event: Some("relation".to_string()),
            query: "UPDATE accounts SET balance = balance - 10".to_string(),
            state: "active".to_string(),
            duration_seconds: 4.5,
        }],
    };

    assert_eq!(report.total_client_active(), 20);
    assert_eq!(report.total_client_waiting(), 3);

    let table = report.render_table(PoolSection::All);
    assert!(table.contains("Ignatius Connection Pool & Saturation Telemetry"));
    assert!(table.contains("Mode: PgBouncer"));
    assert!(table.contains("app_db"));
    assert!(table.contains("backend-service"));

    let json = report.render_json();
    assert!(json.contains("\"is_pgbouncer\": true"));
    assert!(json.contains("\"cl_waiting\": 2"));

    let parsed: PoolReport = serde_json::from_str(&json).expect("valid pool json");
    assert_eq!(parsed, report);
}

#[test]
fn test_pool_section_parsing() {
    assert_eq!(PoolSection::from_str_loose("all"), Some(PoolSection::All));
    assert_eq!(
        PoolSection::from_str_loose("pools"),
        Some(PoolSection::Pools)
    );
    assert_eq!(
        PoolSection::from_str_loose("pool"),
        Some(PoolSection::Pools)
    );
    assert_eq!(
        PoolSection::from_str_loose("server"),
        Some(PoolSection::Server)
    );
    assert_eq!(
        PoolSection::from_str_loose("connections"),
        Some(PoolSection::Server)
    );
    assert_eq!(
        PoolSection::from_str_loose("saturation"),
        Some(PoolSection::Server)
    );
    assert_eq!(PoolSection::from_str_loose("wait"), Some(PoolSection::Wait));
    assert_eq!(
        PoolSection::from_str_loose("queues"),
        Some(PoolSection::Wait)
    );
    assert_eq!(PoolSection::from_str_loose("invalid_foo"), None);
}

#[test]
fn test_truncate_str() {
    assert_eq!(truncate_str("short", 10), "short");
    assert_eq!(truncate_str("exactlength", 11), "exactlength");
    assert_eq!(truncate_str("longerstringthanallowed", 10), "longers...");
}
