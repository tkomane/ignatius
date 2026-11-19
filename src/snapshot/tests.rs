use super::*;

fn col(
    name: &str,
    data_type: &str,
    nullable: bool,
    default_expr: Option<&str>,
    ordinal: i16,
) -> ColumnSnapshot {
    ColumnSnapshot {
        name: name.to_owned(),
        data_type: data_type.to_owned(),
        nullable,
        default_expr: default_expr.map(str::to_owned),
        ordinal,
    }
}

fn sample_baseline_snapshot() -> SchemaSnapshot {
    SchemaSnapshot {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        schema_filter: Some("public".to_owned()),
        fingerprint_sha256: String::new(),
        tables: vec![
            TableSnapshot {
                schema: "public".to_owned(),
                name: "orders".to_owned(),
                columns: vec![
                    col("id", "bigint", false, Some("nextval('orders_id_seq'::regclass)"), 1),
                    col("customer_id", "bigint", false, None, 2),
                    col("status", "public.order_status", false, Some("'pending'::public.order_status"), 3),
                    col("notes", "text", true, None, 4),
                ],
                constraints: vec![
                    ConstraintSnapshot {
                        name: "orders_pkey".to_owned(),
                        kind: "PRIMARY KEY".to_owned(),
                        definition: "PRIMARY KEY (id)".to_owned(),
                    },
                    ConstraintSnapshot {
                        name: "orders_amount_chk".to_owned(),
                        kind: "CHECK".to_owned(),
                        definition: "CHECK ((customer_id > 0))".to_owned(),
                    },
                ],
                indexes: vec![
                    IndexSnapshot {
                        name: "orders_pkey".to_owned(),
                        is_unique: true,
                        is_primary: true,
                        definition: "CREATE UNIQUE INDEX orders_pkey ON public.orders USING btree (id)".to_owned(),
                    },
                    IndexSnapshot {
                        name: "orders_customer_idx".to_owned(),
                        is_unique: false,
                        is_primary: false,
                        definition: "CREATE INDEX orders_customer_idx ON public.orders USING btree (customer_id)".to_owned(),
                    },
                ],
                rls_enabled: true,
                rls_forced: false,
                policies: vec![PolicySnapshot {
                    name: "orders_tenant_isolation".to_owned(),
                    cmd: "ALL".to_owned(),
                    permissive: true,
                    roles: vec!["app_user".to_owned(), "app_admin".to_owned()],
                    qual: Some("(customer_id = current_setting('app.customer_id')::bigint)".to_owned()),
                    with_check: Some("(customer_id = current_setting('app.customer_id')::bigint)".to_owned()),
                }],
            },
            TableSnapshot {
                schema: "public".to_owned(),
                name: "customers".to_owned(),
                columns: vec![
                    col("id", "bigint", false, None, 1),
                    col("email", "text", false, None, 2),
                ],
                constraints: vec![ConstraintSnapshot {
                    name: "customers_pkey".to_owned(),
                    kind: "PRIMARY KEY".to_owned(),
                    definition: "PRIMARY KEY (id)".to_owned(),
                }],
                indexes: vec![IndexSnapshot {
                    name: "customers_email_uidx".to_owned(),
                    is_unique: true,
                    is_primary: false,
                    definition: "CREATE UNIQUE INDEX customers_email_uidx ON public.customers USING btree (email)".to_owned(),
                }],
                rls_enabled: false,
                rls_forced: false,
                policies: Vec::new(),
            },
        ],
        views: vec![
            ViewSnapshot {
                schema: "public".to_owned(),
                name: "active_orders".to_owned(),
                materialized: false,
                definition_sha256: sha256_hex(b"SELECT id, customer_id FROM public.orders"),
            },
            ViewSnapshot {
                schema: "public".to_owned(),
                name: "daily_order_totals".to_owned(),
                materialized: true,
                definition_sha256: sha256_hex(b"SELECT customer_id, count(*) FROM public.orders GROUP BY 1"),
            },
        ],
        sequences: vec![SequenceSnapshot {
            schema: "public".to_owned(),
            name: "orders_id_seq".to_owned(),
            data_type: "bigint".to_owned(),
            start_value: 1,
            increment_by: 1,
            cycle: false,
        }],
        enums: vec![EnumSnapshot {
            schema: "public".to_owned(),
            name: "order_status".to_owned(),
            labels: vec!["pending".to_owned(), "paid".to_owned(), "shipped".to_owned()],
        }],
        truncated: false,
    }
}

#[test]
fn finalize_is_canonical_idempotent_and_deterministic() {
    let ordered = sample_baseline_snapshot().finalize();
    assert_eq!(ordered.fingerprint_sha256.len(), 64);
    assert!(
        ordered
            .fingerprint_sha256
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
    );
    assert!(ordered.validate_fingerprint());
    assert_eq!(ordered.tables[0].name, "customers");
    assert_eq!(ordered.tables[1].name, "orders");
    assert_eq!(
        ordered.tables[1].policies[0].roles,
        vec!["app_admin".to_owned(), "app_user".to_owned()]
    );

    let mut scrambled = sample_baseline_snapshot();
    scrambled.tables.reverse();
    for table in &mut scrambled.tables {
        table.columns.reverse();
        table.constraints.reverse();
        table.indexes.reverse();
        for pol in &mut table.policies {
            pol.roles.reverse();
        }
        table.policies.reverse();
    }
    scrambled.views.reverse();
    scrambled.sequences.reverse();
    scrambled.enums.reverse();

    let finalized_scrambled = scrambled.finalize();
    assert_eq!(ordered, finalized_scrambled);
    assert_eq!(
        ordered.fingerprint_sha256,
        finalized_scrambled.fingerprint_sha256
    );
    assert_eq!(ordered.clone().finalize(), ordered);
}

#[test]
fn fingerprint_detects_enum_order_and_tampering() {
    let mut a = sample_baseline_snapshot().finalize();
    let mut b = sample_baseline_snapshot();
    b.enums[0].labels.swap(0, 1);
    let b = b.finalize();

    assert_ne!(a.fingerprint_sha256, b.fingerprint_sha256);

    a.tables[0].rls_enabled = true;
    assert!(!a.validate_fingerprint());
}

#[test]
fn diff_identical_snapshots_reports_no_drift() {
    let expected = sample_baseline_snapshot().finalize();
    let actual = sample_baseline_snapshot().finalize();
    let report = diff_snapshots(&expected, &actual);

    assert!(report.fingerprints_match);
    assert_eq!(report.expected_fingerprint, expected.fingerprint_sha256);
    assert_eq!(report.actual_fingerprint, actual.fingerprint_sha256);
    assert!(report.findings.is_empty());
    assert_eq!(report.breaking_count, 0);
    assert_eq!(report.warning_count, 0);
    assert_eq!(report.info_count, 0);
    assert!(!report.has_drift());
    assert!(!report.exceeds_threshold(DriftFailThreshold::Any));
    assert!(!report.exceeds_threshold(DriftFailThreshold::Never));
}

#[test]
fn diff_detects_all_breaking_drift_rules() {
    let expected = sample_baseline_snapshot().finalize();
    let mut actual = sample_baseline_snapshot();

    // 1. Drop table public.customers
    actual.tables.retain(|t| t.name != "customers");
    // 2. Drop column `notes`, flip `customer_id` nullable true, change `status` type
    let orders = actual
        .tables
        .iter_mut()
        .find(|t| t.name == "orders")
        .unwrap();
    orders.columns.retain(|c| c.name != "notes");
    orders
        .columns
        .iter_mut()
        .find(|c| c.name == "customer_id")
        .unwrap()
        .nullable = true;
    orders
        .columns
        .iter_mut()
        .find(|c| c.name == "status")
        .unwrap()
        .data_type = "text".to_owned();
    // 3. Disable RLS and drop RLS policy
    orders.rls_enabled = false;
    orders.policies.clear();
    // 4. Drop view and drop sequence
    actual.views.retain(|v| v.name != "active_orders");
    actual.sequences.clear();

    let actual = actual.finalize();
    let report = diff_snapshots(&expected, &actual);

    assert!(!report.fingerprints_match);
    assert!(report.breaking_count >= 8);
    assert!(report.exceeds_threshold(DriftFailThreshold::Breaking));

    let kinds: Vec<(&str, &str)> = report
        .findings
        .iter()
        .filter(|f| f.severity == DriftSeverity::Breaking)
        .map(|f| (f.kind.as_str(), f.object.as_str()))
        .collect();

    assert!(kinds.contains(&("table_dropped", "public.customers")));
    assert!(kinds.contains(&("column_dropped", "public.orders.notes")));
    assert!(kinds.contains(&("column_nullability_changed", "public.orders.customer_id")));
    assert!(kinds.contains(&("column_type_changed", "public.orders.status")));
    assert!(kinds.contains(&("rls_disabled", "public.orders")));
    assert!(kinds.contains(&("policy_dropped", "public.orders.orders_tenant_isolation")));
    assert!(kinds.contains(&("view_dropped", "public.active_orders")));
    assert!(kinds.contains(&("sequence_dropped", "public.orders_id_seq")));
}

#[test]
fn diff_detects_nullable_to_not_null_flip_as_breaking() {
    let expected = sample_baseline_snapshot().finalize();
    let mut actual = sample_baseline_snapshot();
    actual
        .tables
        .iter_mut()
        .find(|t| t.name == "orders")
        .unwrap()
        .columns
        .iter_mut()
        .find(|c| c.name == "notes")
        .unwrap()
        .nullable = false;
    let actual = actual.finalize();

    let report = diff_snapshots(&expected, &actual);
    assert_eq!(report.breaking_count, 1);
    assert_eq!(report.findings[0].severity, DriftSeverity::Breaking);
    assert_eq!(report.findings[0].kind, "column_nullability_changed");
    assert_eq!(report.findings[0].object, "public.orders.notes");
}

#[test]
fn diff_detects_warning_and_info_rules() {
    let expected = sample_baseline_snapshot().finalize();
    let mut actual = sample_baseline_snapshot();

    let orders = actual
        .tables
        .iter_mut()
        .find(|t| t.name == "orders")
        .unwrap();
    // Warning: drop index, drop constraint, modify default
    orders.indexes.retain(|i| i.name != "orders_customer_idx");
    orders.constraints.retain(|c| c.name != "orders_amount_chk");
    orders
        .columns
        .iter_mut()
        .find(|c| c.name == "status")
        .unwrap()
        .default_expr = Some("'paid'::public.order_status".to_owned());
    // Info: add column, add index
    orders
        .columns
        .push(col("external_ref", "text", true, None, 5));
    orders.indexes.push(IndexSnapshot {
        name: "orders_status_idx".to_owned(),
        is_unique: false,
        is_primary: false,
        definition: "CREATE INDEX orders_status_idx ON public.orders USING btree (status)"
            .to_owned(),
    });

    // Warning: enum label removal; Info: enum label addition
    actual.enums[0].labels = vec![
        "pending".to_owned(),
        "refunded".to_owned(),
        "shipped".to_owned(),
    ];

    // Info: add table, view, sequence, enum
    actual.tables.push(TableSnapshot {
        schema: "public".to_owned(),
        name: "audit_log".to_owned(),
        columns: vec![col("id", "bigint", false, None, 1)],
        constraints: Vec::new(),
        indexes: Vec::new(),
        rls_enabled: false,
        rls_forced: false,
        policies: Vec::new(),
    });
    actual.views.push(ViewSnapshot {
        schema: "public".to_owned(),
        name: "recent_orders".to_owned(),
        materialized: false,
        definition_sha256: sha256_hex(b"SELECT id FROM public.orders"),
    });
    actual.sequences.push(SequenceSnapshot {
        schema: "public".to_owned(),
        name: "audit_id_seq".to_owned(),
        data_type: "bigint".to_owned(),
        start_value: 1,
        increment_by: 1,
        cycle: false,
    });
    actual.enums.push(EnumSnapshot {
        schema: "public".to_owned(),
        name: "audit_action".to_owned(),
        labels: vec!["insert".to_owned(), "delete".to_owned()],
    });

    let actual = actual.finalize();
    let report = diff_snapshots(&expected, &actual);

    assert_eq!(report.breaking_count, 0);
    assert_eq!(report.warning_count, 4);
    assert_eq!(report.info_count, 7);
    assert!(!report.exceeds_threshold(DriftFailThreshold::Breaking));
    assert!(report.exceeds_threshold(DriftFailThreshold::Warning));
    assert!(report.exceeds_threshold(DriftFailThreshold::Any));
    assert!(!report.exceeds_threshold(DriftFailThreshold::Never));

    // Verify findings are ordered Breaking -> Warning -> Info
    let mut prev_rank = u8::MAX;
    for finding in &report.findings {
        assert!(finding.severity.rank() <= prev_rank);
        prev_rank = finding.severity.rank();
    }

    assert!(
        report
            .findings
            .iter()
            .any(|f| f.severity == DriftSeverity::Warning && f.kind == "enum_label_removed")
    );
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.severity == DriftSeverity::Info && f.kind == "enum_label_added")
    );
    assert!(
        !report
            .findings
            .iter()
            .any(|f| f.kind == "enum_labels_reordered")
    );
}

#[test]
fn enum_label_insertion_does_not_false_trigger_reorder_warning() {
    let expected = SchemaSnapshot {
        enums: vec![EnumSnapshot {
            schema: "public".to_owned(),
            name: "tier".to_owned(),
            labels: vec!["free".to_owned(), "pro".to_owned()],
        }],
        ..SchemaSnapshot::default()
    }
    .finalize();

    let actual = SchemaSnapshot {
        enums: vec![EnumSnapshot {
            schema: "public".to_owned(),
            name: "tier".to_owned(),
            labels: vec!["free".to_owned(), "starter".to_owned(), "pro".to_owned()],
        }],
        ..SchemaSnapshot::default()
    }
    .finalize();

    let report = diff_snapshots(&expected, &actual);
    assert_eq!(report.warning_count, 0);
    assert_eq!(report.info_count, 1);
    assert_eq!(report.findings[0].kind, "enum_label_added");
}

#[test]
fn threshold_parse_loose_and_exceeds_threshold() {
    assert_eq!(
        DriftFailThreshold::parse_loose("breaking"),
        Some(DriftFailThreshold::Breaking)
    );
    assert_eq!(
        DriftFailThreshold::parse_loose("  ERROR "),
        Some(DriftFailThreshold::Breaking)
    );
    assert_eq!(
        DriftFailThreshold::parse_loose("warn"),
        Some(DriftFailThreshold::Warning)
    );
    assert_eq!(
        DriftFailThreshold::parse_loose("ALL"),
        Some(DriftFailThreshold::Any)
    );
    assert_eq!(
        DriftFailThreshold::parse_loose("none"),
        Some(DriftFailThreshold::Never)
    );
    assert_eq!(DriftFailThreshold::parse_loose("invalid"), None);

    let info_only = SnapshotDiffReport {
        expected_fingerprint: "a".to_owned(),
        actual_fingerprint: "b".to_owned(),
        fingerprints_match: false,
        findings: vec![DriftFinding {
            severity: DriftSeverity::Info,
            kind: "table_added".to_owned(),
            object: "public.new_tbl".to_owned(),
            detail: "Table added".to_owned(),
        }],
        breaking_count: 0,
        warning_count: 0,
        info_count: 1,
    };
    assert!(!info_only.exceeds_threshold(DriftFailThreshold::Breaking));
    assert!(!info_only.exceeds_threshold(DriftFailThreshold::Warning));
    assert!(info_only.exceeds_threshold(DriftFailThreshold::Any));
    assert!(!info_only.exceeds_threshold(DriftFailThreshold::Never));
}

#[test]
fn finalize_enforces_snapshot_bounds_and_sets_truncated() {
    let mut oversized_tables = SchemaSnapshot::new(None);
    for i in 0..=MAX_SNAPSHOT_TABLES {
        oversized_tables.tables.push(TableSnapshot {
            schema: "public".to_owned(),
            name: format!("tbl_{i:04}"),
            columns: Vec::new(),
            constraints: Vec::new(),
            indexes: Vec::new(),
            rls_enabled: false,
            rls_forced: false,
            policies: Vec::new(),
        });
    }
    let finalized_tables = oversized_tables.finalize();
    assert!(finalized_tables.truncated);
    assert_eq!(finalized_tables.tables.len(), MAX_SNAPSHOT_TABLES);

    let mut oversized_cols = SchemaSnapshot::new(Some("public".to_owned()));
    let mut columns = Vec::with_capacity(MAX_SNAPSHOT_COLUMNS + 1);
    for i in 0..=MAX_SNAPSHOT_COLUMNS {
        columns.push(col(
            &format!("c_{i:04}"),
            "integer",
            true,
            None,
            (i + 1) as i16,
        ));
    }
    oversized_cols.tables.push(TableSnapshot {
        schema: "public".to_owned(),
        name: "wide_table".to_owned(),
        columns,
        constraints: Vec::new(),
        indexes: Vec::new(),
        rls_enabled: false,
        rls_forced: false,
        policies: Vec::new(),
    });
    let finalized_cols = oversized_cols.finalize();
    assert!(finalized_cols.truncated);
    assert_eq!(finalized_cols.total_columns(), MAX_SNAPSHOT_COLUMNS);

    let mut oversized_idx_pol = SchemaSnapshot::new(Some("public".to_owned()));
    let mut indexes = Vec::with_capacity(MAX_SNAPSHOT_INDEXES + 1);
    for i in 0..=MAX_SNAPSHOT_INDEXES {
        indexes.push(IndexSnapshot {
            name: format!("idx_{i:04}"),
            is_unique: false,
            is_primary: false,
            definition: format!("CREATE INDEX idx_{i:04} ON public.t (id)"),
        });
    }
    let mut policies = Vec::with_capacity(MAX_SNAPSHOT_POLICIES + 1);
    for i in 0..=MAX_SNAPSHOT_POLICIES {
        policies.push(PolicySnapshot {
            name: format!("pol_{i:04}"),
            cmd: "SELECT".to_owned(),
            permissive: true,
            roles: vec!["PUBLIC".to_owned()],
            qual: Some("true".to_owned()),
            with_check: None,
        });
    }
    oversized_idx_pol.tables.push(TableSnapshot {
        schema: "public".to_owned(),
        name: "t".to_owned(),
        columns: Vec::new(),
        constraints: Vec::new(),
        indexes,
        rls_enabled: true,
        rls_forced: false,
        policies,
    });
    let finalized_idx_pol = oversized_idx_pol.finalize();
    assert!(finalized_idx_pol.truncated);
    assert_eq!(finalized_idx_pol.total_indexes(), MAX_SNAPSHOT_INDEXES);
    assert_eq!(finalized_idx_pol.total_policies(), MAX_SNAPSHOT_POLICIES);
}

#[test]
fn json_round_trip_and_version_validation() {
    let original = sample_baseline_snapshot().finalize();
    let json = original.render_json();
    assert!(json.is_ascii());

    let parsed = SchemaSnapshot::from_json(&json).expect("valid snapshot JSON");
    assert_eq!(parsed, original);
    assert!(parsed.validate_fingerprint());

    let bad_version = json.replace("\"schema_version\": 1", "\"schema_version\": 99");
    let err = SchemaSnapshot::from_json(&bad_version).unwrap_err();
    assert_eq!(err.kind, DiagnosticKind::Usage);

    let deeply_nested = "[".repeat(MAX_JSON_NESTING + 2) + &"]".repeat(MAX_JSON_NESTING + 2);
    let nest_err = SchemaSnapshot::from_json(&deeply_nested).unwrap_err();
    assert_eq!(nest_err.kind, DiagnosticKind::Usage);
}

#[test]
fn renderers_emit_strictly_plain_ascii_and_escape_hostile_input() {
    let expected = sample_baseline_snapshot().finalize();
    let mut actual = sample_baseline_snapshot();
    actual.tables.push(TableSnapshot {
        schema: "public".to_owned(),
        name: "evil\x1b[31m_tbl_\u{202E}caf\u{00E9}".to_owned(),
        columns: vec![col("id", "integer", false, None, 1)],
        constraints: Vec::new(),
        indexes: Vec::new(),
        rls_enabled: false,
        rls_forced: false,
        policies: Vec::new(),
    });
    let actual = actual.finalize();

    let report = diff_snapshots(&expected, &actual);
    let table_out = report.render_table();
    let plain_out = report.render_plain();
    let json_out = report.render_json();
    let summary_out = actual.render_summary();

    for output in [&table_out, &plain_out, &json_out, &summary_out] {
        assert!(output.is_ascii(), "output contained non-ASCII: {output}");
        assert!(!output.contains('\x1b'), "output contained raw ESC byte");
    }

    assert!(table_out.contains("DRIFT DETECTED"));
    assert!(table_out.contains("table_added"));
    assert!(plain_out.contains("status=drift"));
    assert!(plain_out.contains("finding: [INFO]"));

    let parsed: serde_json::Value = serde_json::from_str(&json_out).expect("valid JSON");
    assert_eq!(parsed["fingerprints_match"], serde_json::Value::Bool(false));
}
