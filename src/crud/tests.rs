use super::*;

fn sample_request() -> BrowseRequest {
    BrowseRequest {
        schema: "public".to_owned(),
        table: "orders".to_owned(),
        columns: vec!["id".to_owned(), "customer".to_owned(), "status".to_owned()],
        pk_columns: vec!["id".to_owned()],
        sort_column: None,
        sort_order: SortOrder::Asc,
        page_size: DEFAULT_PAGE_SIZE,
        cursor: None,
        direction: PageDirection::Next,
        filter_predicate: None,
    }
}

#[test]
fn constants_match_documented_bounds() {
    assert_eq!(DEFAULT_PAGE_SIZE, 50);
    assert_eq!(MAX_PAGE_SIZE, 500);
    assert_eq!(MAX_BATCH_MUTATIONS, 200);
}

#[test]
fn build_keyset_browse_sql_initial_page_and_clamping() {
    let mut req = sample_request();
    let sql = build_keyset_browse_sql(&req).expect("valid browse request");
    assert_eq!(
        sql,
        "SELECT \"id\", \"customer\", \"status\" FROM \"public\".\"orders\" ORDER BY \"id\" ASC LIMIT 51;"
    );

    req.page_size = 0;
    assert_eq!(req.effective_page_size(), 1);
    let min_sql = build_keyset_browse_sql(&req).expect("clamps 0 to 1");
    assert!(min_sql.ends_with("LIMIT 2;"));

    req.page_size = 9_999;
    assert_eq!(req.effective_page_size(), MAX_PAGE_SIZE);
    let max_sql = build_keyset_browse_sql(&req).expect("clamps oversized page");
    assert!(max_sql.ends_with("LIMIT 501;"));
}

#[test]
fn build_keyset_browse_sql_quotes_identifiers_and_supports_ctid_fallback() {
    let req = BrowseRequest {
        schema: "my\"schema".to_owned(),
        table: "odd\"table".to_owned(),
        columns: Vec::new(),
        pk_columns: Vec::new(),
        sort_column: Some("created\"at".to_owned()),
        sort_order: SortOrder::Desc,
        page_size: 25,
        cursor: None,
        direction: PageDirection::Next,
        filter_predicate: None,
    };
    let sql = build_keyset_browse_sql(&req).expect("ctid fallback query");
    assert_eq!(
        sql,
        "SELECT * FROM \"my\"\"schema\".\"odd\"\"table\" ORDER BY \"created\"\"at\" DESC, ctid DESC LIMIT 26;"
    );

    let mut no_sort = req;
    no_sort.sort_column = None;
    let sql_no_sort = build_keyset_browse_sql(&no_sort).expect("pure ctid fallback");
    assert!(sql_no_sort.contains("ORDER BY ctid DESC LIMIT 26;"));
}

#[test]
fn build_keyset_browse_sql_deduplicates_sort_column_from_pk_tiebreaker() {
    let mut req = sample_request();
    req.pk_columns = vec!["tenant_id".to_owned(), "id".to_owned()];
    req.sort_column = Some("tenant_id".to_owned());
    let sql = build_keyset_browse_sql(&req).expect("deduplicated sort column");
    assert!(sql.contains("ORDER BY \"tenant_id\" ASC, \"id\" ASC LIMIT 51;"));
}

#[test]
fn build_keyset_browse_sql_composite_cursor_and_prev_reversal() {
    let mut req = sample_request();
    req.pk_columns = vec!["tenant_id".to_owned(), "id".to_owned()];
    req.cursor = Some(KeysetCursor {
        pk_columns: vec!["tenant_id".to_owned(), "id".to_owned()],
        pk_values: vec![Some("acme'co".to_owned()), Some("42".to_owned())],
    });

    let next_asc = build_keyset_browse_sql(&req).expect("composite next asc");
    assert!(!req.needs_row_reversal());
    assert!(next_asc.contains("WHERE (\"tenant_id\", \"id\") > ('acme''co', '42')"));
    assert!(next_asc.contains("ORDER BY \"tenant_id\" ASC, \"id\" ASC LIMIT 51;"));

    req.sort_order = SortOrder::Desc;
    let next_desc = build_keyset_browse_sql(&req).expect("composite next desc");
    assert!(next_desc.contains("WHERE (\"tenant_id\", \"id\") < ('acme''co', '42')"));
    assert!(next_desc.contains("ORDER BY \"tenant_id\" DESC, \"id\" DESC LIMIT 51;"));

    req.sort_order = SortOrder::Asc;
    req.direction = PageDirection::Prev;
    assert!(req.needs_row_reversal());
    let prev_asc = build_keyset_browse_sql(&req).expect("composite prev asc");
    assert!(prev_asc.contains("WHERE (\"tenant_id\", \"id\") < ('acme''co', '42')"));
    assert!(prev_asc.contains("ORDER BY \"tenant_id\" DESC, \"id\" DESC LIMIT 51;"));
    assert!(prev_asc.contains("reverse rows after trim"));

    req.sort_order = SortOrder::Desc;
    let prev_desc = build_keyset_browse_sql(&req).expect("composite prev desc");
    assert!(prev_desc.contains("WHERE (\"tenant_id\", \"id\") > ('acme''co', '42')"));
    assert!(prev_desc.contains("ORDER BY \"tenant_id\" ASC, \"id\" ASC LIMIT 51;"));
}

#[test]
fn build_keyset_browse_sql_rejects_invalid_identifiers_and_cursors() {
    let mut bad_schema = sample_request();
    bad_schema.schema = "  ".to_owned();
    let err = build_keyset_browse_sql(&bad_schema).expect_err("empty schema");
    assert_eq!(err.kind, DiagnosticKind::Usage);

    let mut no_pk_cursor = sample_request();
    no_pk_cursor.pk_columns.clear();
    no_pk_cursor.cursor = Some(KeysetCursor {
        pk_columns: vec!["id".to_owned()],
        pk_values: vec![Some("1".to_owned())],
    });
    let err = build_keyset_browse_sql(&no_pk_cursor).expect_err("cursor without pk");
    assert_eq!(err.kind, DiagnosticKind::Usage);

    let mut mismatch = sample_request();
    mismatch.cursor = Some(KeysetCursor {
        pk_columns: vec!["other_id".to_owned()],
        pk_values: vec![Some("1".to_owned())],
    });
    let err = build_keyset_browse_sql(&mismatch).expect_err("cursor column mismatch");
    assert_eq!(err.kind, DiagnosticKind::Usage);

    let mut null_cursor = sample_request();
    null_cursor.cursor = Some(KeysetCursor {
        pk_columns: vec!["id".to_owned()],
        pk_values: vec![None],
    });
    let err = build_keyset_browse_sql(&null_cursor).expect_err("null cursor value");
    assert_eq!(err.kind, DiagnosticKind::Usage);
}

#[test]
fn build_keyset_browse_sql_validates_filter_predicates() {
    let mut req = sample_request();
    req.filter_predicate = Some("status = 'O''Brien' AND (total > 10 OR vip = true)".to_owned());
    req.cursor = Some(KeysetCursor {
        pk_columns: vec!["id".to_owned()],
        pk_values: vec![Some("100".to_owned())],
    });
    let sql = build_keyset_browse_sql(&req).expect("safe single expression filter");
    assert!(
        sql.contains(
            "WHERE (status = 'O''Brien' AND (total > 10 OR vip = true)) AND \"id\" > '100'"
        )
    );

    for payload in [
        "1 = 1; DROP TABLE orders",
        "1 = 1 -- trailing comment",
        "1 = 1 /* block comment */",
        "id > 0) OR (1 = 1",
        "status = 'unterminated",
        "status = 'nul\0byte'",
    ] {
        req.filter_predicate = Some(payload.to_owned());
        let err = build_keyset_browse_sql(&req)
            .expect_err(&format!("expected filter rejection for {payload}"));
        assert_eq!(err.kind, DiagnosticKind::Usage);
    }
}

#[test]
fn cursor_extraction_handles_rows_and_cells() {
    let cols = vec!["name".to_owned(), "id".to_owned(), "tenant".to_owned()];
    let pks = vec!["tenant".to_owned(), "id".to_owned()];
    let row = vec![
        Some("alice".to_owned()),
        Some("7".to_owned()),
        Some("t1".to_owned()),
    ];
    let cursor = extract_cursor_from_row(&cols, &pks, &row).expect("extract cursor");
    assert_eq!(
        cursor,
        KeysetCursor {
            pk_columns: pks.clone(),
            pk_values: vec![Some("t1".to_owned()), Some("7".to_owned())],
        }
    );

    let cells = vec![
        Cell::Text("alice".to_owned()),
        Cell::Text("7".to_owned()),
        Cell::Text("t1".to_owned()),
    ];
    assert_eq!(extract_cursor_from_cells(&cols, &pks, &cells), Some(cursor));

    let null_cells = vec![
        Cell::Text("alice".to_owned()),
        Cell::Null,
        Cell::Text("t1".to_owned()),
    ];
    assert_eq!(extract_cursor_from_cells(&cols, &pks, &null_cells), None);
    assert_eq!(extract_cursor_from_row(&cols, &[], &row), None);
}

#[test]
fn batch_crud_plan_builds_escaped_transactional_sql() {
    let plan = BatchCrudPlan {
        schema: "public".to_owned(),
        table: "users".to_owned(),
        pk_columns: vec!["tenant_id".to_owned(), "id".to_owned()],
        mutations: vec![
            RowMutation::Insert {
                values: vec![
                    ("tenant_id".to_owned(), Some("t1".to_owned())),
                    (
                        "name".to_owned(),
                        Some("O'Reilly; DROP TABLE users;--".to_owned()),
                    ),
                    ("notes".to_owned(), None),
                ],
            },
            RowMutation::Update {
                pk_match: vec![
                    ("id".to_owned(), "42".to_owned()),
                    ("tenant_id".to_owned(), "t1".to_owned()),
                ],
                changes: vec![
                    ("name".to_owned(), Some("Alice".to_owned())),
                    ("notes".to_owned(), None),
                ],
            },
            RowMutation::Delete {
                pk_match: vec![
                    ("tenant_id".to_owned(), "t1".to_owned()),
                    ("id".to_owned(), "99".to_owned()),
                ],
            },
        ],
        dry_run_rollback: true,
    };

    let sql = plan.build_reviewed_sql().expect("valid dry-run batch plan");
    assert!(sql.contains("BEGIN;\n"));
    assert!(sql.contains(
        "INSERT INTO \"public\".\"users\" (\"tenant_id\", \"name\", \"notes\") VALUES ('t1', 'O''Reilly; DROP TABLE users;--', NULL);"
    ));
    assert!(sql.contains(
        "UPDATE \"public\".\"users\" SET \"name\" = 'Alice', \"notes\" = NULL WHERE \"tenant_id\" = 't1' AND \"id\" = '42';"
    ));
    assert!(sql.contains(
        "DELETE FROM \"public\".\"users\" WHERE \"tenant_id\" = 't1' AND \"id\" = '99';"
    ));
    assert!(sql.ends_with("ROLLBACK; -- dry-run\n"));

    let mut commit_plan = plan;
    commit_plan.dry_run_rollback = false;
    let commit_sql = commit_plan.build_reviewed_sql().expect("commit plan");
    assert!(commit_sql.ends_with("COMMIT;\n"));
}

#[test]
fn batch_crud_plan_refuses_keyless_writes_and_invalid_batches() {
    let empty_plan = BatchCrudPlan {
        schema: "public".to_owned(),
        table: "users".to_owned(),
        pk_columns: vec!["id".to_owned()],
        mutations: Vec::new(),
        dry_run_rollback: true,
    };
    assert_eq!(
        empty_plan
            .build_reviewed_sql()
            .expect_err("empty batch")
            .kind,
        DiagnosticKind::Usage
    );

    let too_many = BatchCrudPlan {
        schema: "public".to_owned(),
        table: "users".to_owned(),
        pk_columns: vec!["id".to_owned()],
        mutations: vec![
            RowMutation::Delete {
                pk_match: vec![("id".to_owned(), "1".to_owned())],
            };
            MAX_BATCH_MUTATIONS + 1
        ],
        dry_run_rollback: true,
    };
    assert_eq!(
        too_many
            .build_reviewed_sql()
            .expect_err("exceeds batch cap")
            .kind,
        DiagnosticKind::Usage
    );

    let keyless_update = BatchCrudPlan {
        schema: "public".to_owned(),
        table: "users".to_owned(),
        pk_columns: Vec::new(),
        mutations: vec![RowMutation::Update {
            pk_match: vec![("id".to_owned(), "1".to_owned())],
            changes: vec![("name".to_owned(), Some("bob".to_owned()))],
        }],
        dry_run_rollback: true,
    };
    assert_eq!(
        keyless_update
            .build_reviewed_sql()
            .expect_err("keyless update")
            .kind,
        DiagnosticKind::Usage
    );

    let keyless_delete = BatchCrudPlan {
        schema: "public".to_owned(),
        table: "users".to_owned(),
        pk_columns: Vec::new(),
        mutations: vec![RowMutation::Delete {
            pk_match: vec![("id".to_owned(), "1".to_owned())],
        }],
        dry_run_rollback: true,
    };
    assert_eq!(
        keyless_delete
            .build_reviewed_sql()
            .expect_err("keyless delete")
            .kind,
        DiagnosticKind::Usage
    );

    let partial_pk = BatchCrudPlan {
        schema: "public".to_owned(),
        table: "users".to_owned(),
        pk_columns: vec!["tenant_id".to_owned(), "id".to_owned()],
        mutations: vec![RowMutation::Delete {
            pk_match: vec![("id".to_owned(), "1".to_owned())],
        }],
        dry_run_rollback: true,
    };
    assert_eq!(
        partial_pk
            .build_reviewed_sql()
            .expect_err("partial pk match")
            .kind,
        DiagnosticKind::Usage
    );

    let empty_changes = BatchCrudPlan {
        schema: "public".to_owned(),
        table: "users".to_owned(),
        pk_columns: vec!["id".to_owned()],
        mutations: vec![RowMutation::Update {
            pk_match: vec![("id".to_owned(), "1".to_owned())],
            changes: Vec::new(),
        }],
        dry_run_rollback: true,
    };
    assert_eq!(
        empty_changes
            .build_reviewed_sql()
            .expect_err("empty update changes")
            .kind,
        DiagnosticKind::Usage
    );
}

#[test]
fn browse_page_renderers_produce_plain_ascii_and_distinguish_null() {
    let page = BrowsePage {
        schema: "public".to_owned(),
        table: "users".to_owned(),
        columns: vec!["id".to_owned(), "name".to_owned(), "bio".to_owned()],
        pk_columns: vec!["id".to_owned()],
        rows: vec![
            vec![Some("1".to_owned()), Some("alice\x1b[31m".to_owned()), None],
            vec![
                Some("2".to_owned()),
                Some(String::new()),
                Some("NULL".to_owned()),
            ],
        ],
        page_size: 50,
        has_more: true,
        has_prev: false,
        next_cursor: Some(KeysetCursor {
            pk_columns: vec!["id".to_owned()],
            pk_values: vec![Some("2".to_owned())],
        }),
        prev_cursor: None,
    };

    let table = page.render_table();
    assert!(table.is_ascii());
    assert!(table.contains("[null]"));
    assert!(table.contains("alice\\x1B[31m"));
    assert!(table.contains("next_cursor: id=2"));

    let plain = page.render_plain();
    assert!(plain.is_ascii());
    assert!(plain.contains("bio=[null]"));
    assert!(plain.contains("name=\"\""));
    assert!(plain.contains("bio=NULL"));

    let json = page.render_json();
    assert!(json.is_ascii());
    let round_trip: BrowsePage = serde_json::from_str(&json).expect("valid BrowsePage JSON");
    assert_eq!(round_trip, page);
}
