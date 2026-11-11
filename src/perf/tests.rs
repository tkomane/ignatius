use super::*;

fn sample_entry(
    query_id: i64,
    query: &str,
    calls: u64,
    mean_ms: f64,
    hit: u64,
    read: u64,
) -> StatStatementEntry {
    StatStatementEntry {
        query_id,
        query: query.to_owned(),
        calls,
        total_exec_time_ms: mean_ms * (calls as f64),
        min_exec_time_ms: mean_ms * 0.5,
        max_exec_time_ms: mean_ms * 1.5,
        mean_exec_time_ms: mean_ms,
        stddev_exec_time_ms: mean_ms * 0.1,
        rows: calls * 10,
        shared_blks_hit: hit,
        shared_blks_read: read,
        shared_blks_dirtied: 5,
        shared_blks_written: 2,
    }
}

#[test]
fn hit_ratio_calculates_correctly_and_handles_zero_blocks() {
    let empty = sample_entry(1, "SELECT 1", 10, 0.5, 0, 0);
    assert_eq!(empty.hit_ratio(), 100.0);

    let all_hit = sample_entry(2, "SELECT * FROM users", 5, 2.0, 100, 0);
    assert_eq!(all_hit.hit_ratio(), 100.0);

    let half = sample_entry(3, "SELECT * FROM orders", 5, 10.0, 50, 50);
    assert_eq!(half.hit_ratio(), 50.0);

    let all_read = sample_entry(4, "SELECT * FROM big_table", 1, 100.0, 0, 100);
    assert_eq!(all_read.hit_ratio(), 0.0);
}

#[test]
fn execution_time_formatting_adapts_to_magnitudes() {
    let fast = sample_entry(1, "SELECT 1", 1, 0.42, 10, 0);
    assert_eq!(fast.format_mean_time(), "0.42ms");
    assert_eq!(fast.format_total_time(), "0.4ms");

    let moderate = sample_entry(2, "SELECT * FROM items", 1, 1500.0, 10, 0);
    assert_eq!(moderate.format_mean_time(), "1.50s");
    assert_eq!(moderate.format_total_time(), "1.50s");

    let long = StatStatementEntry {
        query_id: 3,
        query: "VACUUM FULL".to_owned(),
        calls: 1,
        total_exec_time_ms: 7200000.0, // 2 hours
        min_exec_time_ms: 7200000.0,
        max_exec_time_ms: 7200000.0,
        mean_exec_time_ms: 7200000.0,
        stddev_exec_time_ms: 0.0,
        rows: 0,
        shared_blks_hit: 0,
        shared_blks_read: 0,
        shared_blks_dirtied: 0,
        shared_blks_written: 0,
    };
    assert_eq!(long.format_total_time(), "2.0h");
}

#[test]
fn query_snippet_compresses_whitespace_and_bounds_length() {
    let multiline = sample_entry(
        1,
        "SELECT id,\n       name,\n       email\nFROM users",
        1,
        1.0,
        10,
        0,
    );
    assert_eq!(
        multiline.query_snippet(100),
        "SELECT id, name, email FROM users"
    );

    let truncated = multiline.query_snippet(15);
    assert!(truncated.ends_with("..."));
    assert!(truncated.chars().count() <= 15);
}

#[test]
fn sort_order_parsing_accepts_aliases_and_rejects_unknown() {
    assert_eq!(
        PerformanceSortOrder::from_str_loose("mean"),
        Some(PerformanceSortOrder::MeanTime)
    );
    assert_eq!(
        PerformanceSortOrder::from_str_loose("avg"),
        Some(PerformanceSortOrder::MeanTime)
    );
    assert_eq!(
        PerformanceSortOrder::from_str_loose("total"),
        Some(PerformanceSortOrder::TotalTime)
    );
    assert_eq!(
        PerformanceSortOrder::from_str_loose("calls"),
        Some(PerformanceSortOrder::Calls)
    );
    assert_eq!(
        PerformanceSortOrder::from_str_loose("hit-ratio"),
        Some(PerformanceSortOrder::HitRatio)
    );
    assert_eq!(
        PerformanceSortOrder::from_str_loose("io"),
        Some(PerformanceSortOrder::IoRead)
    );
    assert_eq!(PerformanceSortOrder::from_str_loose("unknown-flag"), None);
}

#[test]
fn snapshot_aggregates_telemetry_and_orders_entries() {
    let q1 = sample_entry(1, "SELECT 1", 100, 0.5, 90, 10); // mean 0.5, total 50, calls 100, hit 90%
    let q2 = sample_entry(2, "SELECT 2", 10, 20.0, 50, 50); // mean 20.0, total 200, calls 10, hit 50%
    let q3 = sample_entry(3, "SELECT 3", 1, 100.0, 10, 90); // mean 100.0, total 100, calls 1, hit 10%

    let snapshot_mean = QueryPerformanceSnapshot::new(
        vec![q1.clone(), q2.clone(), q3.clone()],
        true,
        None,
        PerformanceSortOrder::MeanTime,
    );
    assert_eq!(snapshot_mean.total_calls, 111);
    assert_eq!(snapshot_mean.queries[0].query_id, 3); // highest mean (100.0)
    assert_eq!(snapshot_mean.queries[1].query_id, 2); // (20.0)
    assert_eq!(snapshot_mean.queries[2].query_id, 1); // (0.5)

    let mut snapshot_total = snapshot_mean.clone();
    snapshot_total.reorder(PerformanceSortOrder::TotalTime);
    assert_eq!(snapshot_total.queries[0].query_id, 2); // highest total (200.0)

    let mut snapshot_calls = snapshot_mean.clone();
    snapshot_calls.reorder(PerformanceSortOrder::Calls);
    assert_eq!(snapshot_calls.queries[0].query_id, 1); // highest calls (100)

    let mut snapshot_hit = snapshot_mean.clone();
    snapshot_hit.reorder(PerformanceSortOrder::HitRatio);
    assert_eq!(snapshot_hit.queries[0].query_id, 3); // worst hit ratio first (10%)
}

#[test]
fn reset_sql_is_exact() {
    assert_eq!(
        QueryPerformanceSnapshot::build_reset_sql(),
        "SELECT pg_stat_statements_reset();"
    );
}

#[test]
fn unavailable_snapshot_renders_actionable_instructions() {
    let unavail =
        QueryPerformanceSnapshot::unavailable("extension 'pg_stat_statements' is not created");
    assert!(!unavail.extension_available);
    let table = unavail.render_table();
    assert!(table.contains("Query performance statistics are unavailable"));
    assert!(table.contains("CREATE EXTENSION pg_stat_statements;"));
}

#[test]
fn snapshot_roundtrips_through_json() {
    let q = sample_entry(1, "SELECT 1", 10, 1.5, 95, 5);
    let original =
        QueryPerformanceSnapshot::new(vec![q], true, None, PerformanceSortOrder::MeanTime);
    let json = original.render_json().expect("serialize JSON");
    let deserialized: QueryPerformanceSnapshot =
        serde_json::from_str(&json).expect("deserialize JSON");
    assert_eq!(original, deserialized);
}
