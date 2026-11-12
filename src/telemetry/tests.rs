//! Unit tests for `telemetry` domain module.

use super::*;

#[test]
fn checkpointer_stats_timed_and_backend_ratios() {
    let stats = CheckpointerStats {
        timed_checkpoints: 90,
        requested_checkpoints: 10,
        write_time_ms: 12000.0,
        sync_time_ms: 3500.0,
        buffers_checkpoint: 8000,
        buffers_clean: 1500,
        buffers_backend: 500,
        buffers_backend_fsync: 0,
        buffers_alloc: 25000,
    };

    assert!((stats.timed_ratio() - 90.0).abs() < 0.001);
    // Total written = 8000 + 1500 + 500 = 10000. Backend = 500 => 5.0%
    assert!((stats.backend_write_ratio() - 5.0).abs() < 0.001);

    let empty = CheckpointerStats::default();
    assert_eq!(empty.timed_ratio(), 100.0);
    assert_eq!(empty.backend_write_ratio(), 0.0);
}

#[test]
fn database_xact_stats_rollback_ratio() {
    let db = DatabaseXactStats {
        database_name: "prod_db".to_string(),
        xact_commit: 950,
        xact_rollback: 50,
        deadlocks: 2,
        conflicts: 0,
        temp_files: 10,
        temp_bytes: 1024 * 1024 * 20,
    };

    // 50 / 1000 = 5.0%
    assert!((db.rollback_ratio() - 5.0).abs() < 0.001);

    let zero = DatabaseXactStats::default();
    assert_eq!(zero.rollback_ratio(), 0.0);
}

#[test]
fn telemetry_report_aggregates_commits_and_rollbacks() {
    let mut report = TelemetryReport::default();
    report.databases.push(DatabaseXactStats {
        database_name: "db1".to_string(),
        xact_commit: 1000,
        xact_rollback: 50,
        deadlocks: 0,
        conflicts: 0,
        temp_files: 0,
        temp_bytes: 0,
    });
    report.databases.push(DatabaseXactStats {
        database_name: "db2".to_string(),
        xact_commit: 2000,
        xact_rollback: 50,
        deadlocks: 1,
        conflicts: 0,
        temp_files: 2,
        temp_bytes: 4096,
    });

    assert_eq!(report.total_commits(), 3000);
    assert_eq!(report.total_rollbacks(), 100);
    // 100 / 3100 = ~3.2258%
    assert!((report.aggregate_rollback_ratio() - 3.2258).abs() < 0.01);
}

#[test]
fn telemetry_section_parsing() {
    assert_eq!(
        TelemetrySection::from_str_loose("all"),
        Some(TelemetrySection::All)
    );
    assert_eq!(
        TelemetrySection::from_str_loose("checkpointer"),
        Some(TelemetrySection::Checkpointer)
    );
    assert_eq!(
        TelemetrySection::from_str_loose("ckpt"),
        Some(TelemetrySection::Checkpointer)
    );
    assert_eq!(
        TelemetrySection::from_str_loose("bgwriter"),
        Some(TelemetrySection::Checkpointer)
    );
    assert_eq!(
        TelemetrySection::from_str_loose("wal"),
        Some(TelemetrySection::Wal)
    );
    assert_eq!(
        TelemetrySection::from_str_loose("archiver"),
        Some(TelemetrySection::Wal)
    );
    assert_eq!(
        TelemetrySection::from_str_loose("transactions"),
        Some(TelemetrySection::Transactions)
    );
    assert_eq!(
        TelemetrySection::from_str_loose("xact"),
        Some(TelemetrySection::Transactions)
    );
    assert_eq!(
        TelemetrySection::from_str_loose("freeze"),
        Some(TelemetrySection::Freeze)
    );
    assert_eq!(
        TelemetrySection::from_str_loose("xid"),
        Some(TelemetrySection::Freeze)
    );
    assert_eq!(
        TelemetrySection::from_str_loose("replication"),
        Some(TelemetrySection::Replication)
    );
    assert_eq!(
        TelemetrySection::from_str_loose("standbys"),
        Some(TelemetrySection::Replication)
    );
    assert_eq!(TelemetrySection::from_str_loose("unknown_sec"), None);
}

#[test]
fn format_bytes_handles_various_scales() {
    assert_eq!(format_bytes(512), "512 B");
    assert_eq!(format_bytes(1024), "1.0 KB");
    assert_eq!(format_bytes(1024 * 1024 * 5), "5.0 MB");
    assert_eq!(format_bytes(1024 * 1024 * 1024 * 2), "2.0 GB");
}

#[test]
fn telemetry_report_renders_table_json_and_plain() {
    let mut report = TelemetryReport::default();
    report.wal.current_lsn = "0/15A20B0".to_string();
    report.wal.wal_bytes = 1024 * 1024 * 15;
    report.wal.archived_count = 142;
    report.checkpointer.timed_checkpoints = 100;
    report.checkpointer.requested_checkpoints = 5;

    report.xid_freeze.push(XidFreezeMetric {
        database_name: "ignatius_demo".to_string(),
        datfrozenxid_age: 15_000_000,
        autovacuum_freeze_max_age: 200_000_000,
        wraparound_pct: 0.75,
        oldest_table_name: Some("orders".to_string()),
        oldest_table_age: 14_900_000,
    });

    report.standbys.push(ReplicationStandby {
        pid: 1234,
        client_addr: Some("10.0.0.2".to_string()),
        application_name: "replica_1".to_string(),
        state: "streaming".to_string(),
        sync_state: "async".to_string(),
        sent_lsn: Some("0/15A20B0".to_string()),
        write_lag_bytes: 0,
        flush_lag_bytes: 0,
        replay_lag_bytes: 128,
        replay_lag_ms: Some(1.2),
    });

    // 1. Table
    let table = report.render_table(TelemetrySection::All);
    assert!(table.contains("Ignatius Transaction & WAL Observability"));
    assert!(table.contains("0/15A20B0"));
    assert!(table.contains("Checkpointer & Background Writer"));
    assert!(table.contains("Write-Ahead Log (WAL) & Archiver"));
    assert!(table.contains("Transaction ID (XID) Wraparound Forensics"));
    assert!(table.contains("Streaming Replication Standbys"));
    assert!(table.contains("replica_1"));

    // 2. JSON
    let json = report.render_json();
    assert!(json.contains("\"current_lsn\": \"0/15A20B0\""));
    assert!(json.contains("\"archived_count\": 142"));
    assert!(json.contains("\"standbys\":"));

    // 3. Plain
    let plain = report.render_plain(TelemetrySection::Wal);
    assert!(plain.contains("Write-Ahead Log (WAL) & Archiver"));
}
