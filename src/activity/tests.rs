use super::{
    ActivityViewMode, BackendProcess, DatabaseHealthMetrics, MAX_ACTIVITY_BACKENDS,
    ServerActivitySnapshot,
};

fn sample_backend(
    pid: i32,
    user: &str,
    state: &str,
    age_ms: i64,
    blocked_by: Vec<i32>,
    query: &str,
) -> BackendProcess {
    BackendProcess {
        pid,
        database: "ignatius_demo".to_owned(),
        username: user.to_owned(),
        application_name: "ignatius".to_owned(),
        client_addr: "127.0.0.1".to_owned(),
        state: state.to_owned(),
        wait_event_type: if blocked_by.is_empty() {
            None
        } else {
            Some("Lock".to_owned())
        },
        wait_event: if blocked_by.is_empty() {
            None
        } else {
            Some("transactionid".to_owned())
        },
        query_age_ms: age_ms,
        xact_age_ms: Some(age_ms),
        query: query.to_owned(),
        blocked_by,
        lock_summary: Some("ExclusiveLock on orders".to_owned()),
        is_self: false,
    }
}

#[test]
fn lock_blocker_tree_orders_root_holder_and_nested_waiters() {
    let snapshot = ServerActivitySnapshot {
        metrics: DatabaseHealthMetrics {
            database: "ignatius_demo".to_owned(),
            active_connections: 3,
            xact_commit: 120,
            xact_rollback: 2,
            cache_hit_bps: 9945,
            deadlocks: 0,
            temp_bytes: 0,
        },
        backends: vec![
            sample_backend(
                14288,
                "app_backend",
                "active",
                38_000,
                vec![14201],
                "UPDATE orders SET total_cents = 100 WHERE id = 1;",
            ),
            sample_backend(
                14201,
                "analyst_user",
                "idle in transaction",
                42_000,
                vec![],
                "BEGIN; UPDATE orders SET total_cents = 200 WHERE id = 1;",
            ),
            sample_backend(
                14302,
                "web_worker",
                "active",
                15_000,
                vec![14288],
                "SELECT * FROM orders WHERE id = 1 FOR UPDATE;",
            ),
        ],
        truncated_from: None,
    }
    .bounded();

    let tree = snapshot.lock_blocker_tree();
    assert_eq!(tree.len(), 3);
    assert_eq!(tree[0].backend.pid, 14201);
    assert_eq!(tree[0].depth, 0);
    assert_eq!(tree[0].blocking_pids, vec![14288]);

    assert_eq!(tree[1].backend.pid, 14288);
    assert_eq!(tree[1].depth, 1);
    assert_eq!(tree[1].blocking_pids, vec![14302]);

    assert_eq!(tree[2].backend.pid, 14302);
    assert_eq!(tree[2].depth, 2);
    assert!(tree[2].blocking_pids.is_empty());

    let rendered_tree = snapshot.render_text(ActivityViewMode::LocksTree, true);
    assert!(rendered_tree.contains("[BLOCKER] PID 14201 (analyst_user)"));
    assert!(rendered_tree.contains("└─ [WAITING] PID 14288 (app_backend)"));
    assert!(rendered_tree.contains("  └─ [WAITING] PID 14302 (web_worker)"));
}

#[test]
fn snapshot_bounding_and_json_rendering_are_deterministic() {
    let mut backends = Vec::new();
    for idx in 0..(MAX_ACTIVITY_BACKENDS + 15) {
        backends.push(sample_backend(
            1000 + i32::try_from(idx).unwrap_or(0),
            "role_a",
            "active",
            100,
            vec![],
            "SELECT 1;",
        ));
    }
    let snapshot = ServerActivitySnapshot {
        metrics: DatabaseHealthMetrics {
            database: "ignatius_demo".to_owned(),
            active_connections: 215,
            xact_commit: 500,
            xact_rollback: 1,
            cache_hit_bps: 9980,
            deadlocks: 0,
            temp_bytes: 0,
        },
        backends,
        truncated_from: None,
    }
    .bounded();

    assert_eq!(snapshot.backends.len(), MAX_ACTIVITY_BACKENDS);
    assert_eq!(snapshot.truncated_from, Some(MAX_ACTIVITY_BACKENDS + 15));

    let json = snapshot.render_json();
    assert!(json.contains("\"database\": \"ignatius_demo\""));
    assert!(json.contains("\"cache_hit_ratio\": \"99.80%\""));
}

#[test]
fn reviewed_signal_sql_renders_cancel_and_terminate_statements() {
    let backend = sample_backend(
        4242,
        "ignatius_test",
        "active",
        5_400,
        vec![],
        "SELECT pg_sleep(60);",
    );
    let cancel_sql = ServerActivitySnapshot::reviewed_backend_signal_sql(&backend, false);
    assert!(cancel_sql.contains("CANCEL running query for PID 4242"));
    assert!(cancel_sql.contains("SELECT pg_catalog.pg_cancel_backend(4242);"));

    let terminate_sql = ServerActivitySnapshot::reviewed_backend_signal_sql(&backend, true);
    assert!(terminate_sql.contains("TERMINATE session for PID 4242"));
    assert!(terminate_sql.contains("SELECT pg_catalog.pg_terminate_backend(4242);"));
}
