use super::*;

#[test]
fn table_key_formatting() {
    let key = TableKey::new("public", "users");
    assert_eq!(key.display(), "public.users");
    assert_eq!(key.qualified_sql(), "\"public\".\"users\"");

    let escaped = TableKey::new("my\"schema", "order\"items");
    assert_eq!(
        escaped.qualified_sql(),
        "\"my\"\"schema\".\"order\"\"items\""
    );
}

#[test]
fn topological_ordering_linear_chain() {
    let users = TableKey::new("public", "users");
    let orders = TableKey::new("public", "orders");
    let items = TableKey::new("public", "order_items");

    let tables = vec![items.clone(), orders.clone(), users.clone()];
    let edges = vec![
        ForeignKeyEdge {
            from: items.clone(),
            to: orders.clone(),
            constraint_name: "fk_items_orders".to_string(),
        },
        ForeignKeyEdge {
            from: orders.clone(),
            to: users.clone(),
            constraint_name: "fk_orders_users".to_string(),
        },
    ];

    let graph = DependencyGraph::build(&tables, &edges);
    let plan = graph.compute_sync_plan();

    assert!(!plan.has_cycles);
    assert!(plan.cyclic_tables.is_empty());

    // Insert order: users -> orders -> items
    let users_idx = plan.insert_order.iter().position(|k| k == &users).unwrap();
    let orders_idx = plan.insert_order.iter().position(|k| k == &orders).unwrap();
    let items_idx = plan.insert_order.iter().position(|k| k == &items).unwrap();
    assert!(
        users_idx < orders_idx,
        "users must be inserted before orders"
    );
    assert!(
        orders_idx < items_idx,
        "orders must be inserted before items"
    );

    // Truncate order: items -> orders -> users
    let users_trunc = plan
        .truncate_order
        .iter()
        .position(|k| k == &users)
        .unwrap();
    let orders_trunc = plan
        .truncate_order
        .iter()
        .position(|k| k == &orders)
        .unwrap();
    let items_trunc = plan
        .truncate_order
        .iter()
        .position(|k| k == &items)
        .unwrap();
    assert!(
        items_trunc < orders_trunc,
        "items must be truncated before orders"
    );
    assert!(
        orders_trunc < users_trunc,
        "orders must be truncated before users"
    );
}

#[test]
fn topological_ordering_diamond_graph() {
    let a = TableKey::new("public", "a");
    let b = TableKey::new("public", "b");
    let c = TableKey::new("public", "c");
    let d = TableKey::new("public", "d");

    let tables = vec![d.clone(), c.clone(), b.clone(), a.clone()];
    let edges = vec![
        ForeignKeyEdge {
            from: b.clone(),
            to: a.clone(),
            constraint_name: "fk_b_a".to_string(),
        },
        ForeignKeyEdge {
            from: c.clone(),
            to: a.clone(),
            constraint_name: "fk_c_a".to_string(),
        },
        ForeignKeyEdge {
            from: d.clone(),
            to: b.clone(),
            constraint_name: "fk_d_b".to_string(),
        },
        ForeignKeyEdge {
            from: d.clone(),
            to: c.clone(),
            constraint_name: "fk_d_c".to_string(),
        },
    ];

    let graph = DependencyGraph::build(&tables, &edges);
    let plan = graph.compute_sync_plan();

    assert!(!plan.has_cycles);
    let pos_a = plan.insert_order.iter().position(|k| k == &a).unwrap();
    let pos_b = plan.insert_order.iter().position(|k| k == &b).unwrap();
    let pos_c = plan.insert_order.iter().position(|k| k == &c).unwrap();
    let pos_d = plan.insert_order.iter().position(|k| k == &d).unwrap();

    assert!(pos_a < pos_b);
    assert!(pos_a < pos_c);
    assert!(pos_b < pos_d);
    assert!(pos_c < pos_d);
}

#[test]
fn self_referential_foreign_keys_do_not_block_topological_sort() {
    let emp = TableKey::new("public", "employees");
    let tables = vec![emp.clone()];
    let edges = vec![ForeignKeyEdge {
        from: emp.clone(),
        to: emp.clone(),
        constraint_name: "fk_manager".to_string(),
    }];

    let graph = DependencyGraph::build(&tables, &edges);
    let plan = graph.compute_sync_plan();

    assert!(!plan.has_cycles);
    assert_eq!(plan.insert_order, vec![emp]);
}

#[test]
fn circular_dependencies_detected() {
    let t1 = TableKey::new("public", "table_1");
    let t2 = TableKey::new("public", "table_2");

    let tables = vec![t1.clone(), t2.clone()];
    let edges = vec![
        ForeignKeyEdge {
            from: t1.clone(),
            to: t2.clone(),
            constraint_name: "fk_1_2".to_string(),
        },
        ForeignKeyEdge {
            from: t2.clone(),
            to: t1.clone(),
            constraint_name: "fk_2_1".to_string(),
        },
    ];

    let graph = DependencyGraph::build(&tables, &edges);
    let plan = graph.compute_sync_plan();

    assert!(plan.has_cycles);
    assert_eq!(plan.cyclic_tables.len(), 2);
    assert_eq!(plan.insert_order.len(), 2);
}

#[test]
fn format_helpers() {
    assert_eq!(truncate_str("short", 10), "short");
    assert_eq!(
        truncate_str("very_long_string_that_exceeds", 10),
        "very_lo..."
    );

    assert_eq!(format_bytes(500), "500 B");
    assert_eq!(format_bytes(2048), "2.00 KiB");
    assert_eq!(format_bytes(5 * 1024 * 1024), "5.00 MiB");
    assert_eq!(format_bytes(3 * 1024 * 1024 * 1024), "3.00 GiB");
}

#[test]
fn sync_report_renderers() {
    let report = SyncReport {
        source_endpoint: "localhost:5432/source_db".to_string(),
        target_endpoint: "localhost:5432/target_db".to_string(),
        total_rows: 1500,
        total_bytes: 450000,
        total_duration_ms: 1250,
        has_cycles: false,
        table_progress: vec![SyncTableProgress {
            key: TableKey::new("public", "users"),
            status: SyncTableStatus::Completed,
            rows_copied: 1500,
            bytes_transferred: 450000,
            duration_ms: 1250,
            message: None,
        }],
    };

    let table = report.render_table();
    assert!(table.contains("Ignatius Cross-Database Sync Pipeline"));
    assert!(table.contains("source_db"));
    assert!(table.contains("users"));
    assert!(table.contains("COMPLETED"));

    let plain = report.render_plain();
    assert_eq!(table, plain);

    let json = report.render_json();
    assert!(json.contains("\"total_rows\": 1500"));
    assert!(json.contains("\"Completed\""));
}
