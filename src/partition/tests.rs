//! Sidecar tests for the pure partition lifecycle model.

use super::*;

fn date(year: i32, month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, day).expect("valid date")
}

fn parent(strategy: PartitionStrategy, key_def: &str, key_type: &str) -> PartitionParent {
    PartitionParent {
        schema: "billing".to_owned(),
        table: "invoices".to_owned(),
        strategy,
        key_def: key_def.to_owned(),
        key_type: Some(key_type.to_owned()),
        partitions: Vec::new(),
    }
}

fn partition(name: &str, bound: &str) -> PartitionEntry {
    PartitionEntry {
        schema: "billing".to_owned(),
        name: name.to_owned(),
        bound: bound.to_owned(),
        total_bytes: 8192,
        estimated_rows: Some(10),
    }
}

fn monthly_range(from: &str, to: &str) -> String {
    format!("FOR VALUES FROM ('{from}') TO ('{to}')")
}

#[test]
fn calendar_periods_cross_month_year_and_leap_boundaries() {
    assert_eq!(
        PlanPeriod::Monthly.next(date(2026, 12, 1)),
        Some(date(2027, 1, 1))
    );
    assert_eq!(
        PlanPeriod::Monthly.next(date(2024, 2, 1)),
        Some(date(2024, 3, 1))
    );
    assert_eq!(
        PlanPeriod::Yearly.next(date(2026, 1, 1)),
        Some(date(2027, 1, 1))
    );
    assert_eq!(
        PlanPeriod::Weekly.next(date(2026, 12, 28)),
        Some(date(2027, 1, 4))
    );
    assert_eq!(
        PlanPeriod::Daily.next(date(2024, 2, 28)),
        Some(date(2024, 2, 29))
    );
    assert_eq!(
        PlanPeriod::Daily.next(date(2024, 2, 29)),
        Some(date(2024, 3, 1))
    );
}

#[test]
fn periods_require_an_aligned_start() {
    assert!(PlanPeriod::Monthly.is_aligned(date(2026, 12, 1)));
    assert!(!PlanPeriod::Monthly.is_aligned(date(2026, 12, 2)));
    assert!(PlanPeriod::Yearly.is_aligned(date(2026, 1, 1)));
    assert!(!PlanPeriod::Yearly.is_aligned(date(2026, 2, 1)));
    assert!(PlanPeriod::Weekly.is_aligned(date(2026, 11, 9)), "Monday");
    assert!(
        !PlanPeriod::Weekly.is_aligned(date(2026, 11, 10)),
        "Tuesday"
    );
    assert!(PlanPeriod::Daily.is_aligned(date(2026, 11, 10)));
}

#[test]
fn names_are_deterministic_per_period() {
    assert_eq!(
        PlanPeriod::Daily.name_suffix(date(2026, 1, 2)),
        "p2026_01_02"
    );
    assert_eq!(
        PlanPeriod::Weekly.name_suffix(date(2026, 1, 5)),
        "p2026_W02"
    );
    assert_eq!(
        PlanPeriod::Monthly.name_suffix(date(2026, 1, 1)),
        "p2026_01"
    );
    assert_eq!(PlanPeriod::Yearly.name_suffix(date(2026, 1, 1)), "p2026");
}

#[test]
fn long_names_are_truncated_to_the_identifier_limit_with_a_stable_suffix() {
    let long = "a".repeat(70);
    let first = fit_identifier(&long, "p2026_01");
    let second = fit_identifier(&long, "p2026_01");
    assert!(first.len() <= 63, "{} bytes", first.len());
    assert_eq!(first, second);
    assert!(first.ends_with(&format!(
        "{:08x}",
        fnv1a(format!("{long}_p2026_01").as_bytes())
    )));
    assert_ne!(first, fit_identifier(&long, "p2026_02"));
    assert_eq!(fit_identifier("invoices", "p2026_01"), "invoices_p2026_01");
}

#[test]
fn range_bounds_parse_dates_timestamps_and_infinities() {
    let parsed = parse_bound(&monthly_range("2026-11-01", "2026-12-01"));
    let PartitionBound::Range(bound) = parsed else {
        panic!("expected a range: {parsed:?}");
    };
    assert_eq!(
        bound.from,
        RangeValue::Point(date(2026, 11, 1).and_hms_opt(0, 0, 0).unwrap())
    );
    assert!(bound.overlaps(
        date(2026, 11, 15).and_hms_opt(0, 0, 0).unwrap(),
        date(2026, 11, 16).and_hms_opt(0, 0, 0).unwrap()
    ));

    let parsed = parse_bound("FOR VALUES FROM (MINVALUE) TO ('2026-01-01')");
    assert!(matches!(
        parsed,
        PartitionBound::Range(RangeBound {
            from: RangeValue::Min,
            ..
        })
    ));
    let parsed = parse_bound("FOR VALUES FROM ('2026-01-01') TO (MAXVALUE)");
    assert!(matches!(
        parsed,
        PartitionBound::Range(RangeBound {
            to: RangeValue::Max,
            ..
        })
    ));

    let parsed =
        parse_bound("FOR VALUES FROM ('2026-11-01 00:00:00+00') TO ('2026-12-01 00:00:00+00')");
    assert!(matches!(parsed, PartitionBound::Range(_)));
    assert_eq!(parse_bound("DEFAULT"), PartitionBound::Default);
    assert_eq!(
        parse_bound("FOR VALUES IN ('a', 'b')"),
        PartitionBound::Unparsed
    );
    assert_eq!(parse_bound("garbage"), PartitionBound::Unparsed);
}

#[test]
fn overlap_is_half_open_and_respects_infinity() {
    let bound = match parse_bound(&monthly_range("2026-11-01", "2026-12-01")) {
        PartitionBound::Range(bound) => bound,
        other => panic!("unexpected {other:?}"),
    };
    let point = |day| date(2026, 11, day).and_hms_opt(0, 0, 0).unwrap();
    assert!(!bound.overlaps(point(1), point(1)), "an empty interval");
    assert!(
        bound.overlaps(point(1), point(2)),
        "the lower bound is included"
    );
    assert!(bound.overlaps(point(30), date(2026, 12, 2).and_hms_opt(0, 0, 0).unwrap()));
    assert!(!bound.overlaps(
        date(2026, 12, 1).and_hms_opt(0, 0, 0).unwrap(),
        date(2026, 12, 2).and_hms_opt(0, 0, 0).unwrap()
    ));
}

#[test]
fn a_single_column_key_is_extracted_and_quoted_names_are_unquoted() {
    assert_eq!(
        parse_single_column_key("RANGE (created_at)"),
        Some("created_at".to_owned())
    );
    assert_eq!(
        parse_single_column_key("RANGE (\"Order Date\")"),
        Some("Order Date".to_owned())
    );
    assert_eq!(parse_single_column_key("RANGE (a, b)"), None);
    assert_eq!(
        parse_single_column_key("RANGE (date_trunc('day'::text, ts))"),
        None
    );
    assert_eq!(parse_single_column_key("HASH (id)"), Some("id".to_owned()));
}

#[test]
fn a_monthly_plan_generates_aligned_reviewable_sql() {
    let parent = parent(PartitionStrategy::Range, "RANGE (created_at)", "date");
    let plan =
        plan_partitions(&parent, PlanPeriod::Monthly, date(2026, 11, 1), 3, None).expect("plan");
    assert_eq!(plan.creates.len(), 3);
    assert_eq!(plan.statement_count(), 3);
    assert_eq!(plan.creates[0].name, "invoices_p2026_11");
    assert_eq!(plan.creates[2].name, "invoices_p2027_01");
    let sql = plan.render_sql();
    assert!(sql.contains(
        "CREATE TABLE IF NOT EXISTS \"billing\".\"invoices_p2026_11\" PARTITION OF \"billing\".\"invoices\" FOR VALUES FROM ('2026-11-01') TO ('2026-12-01');"
    ));
    assert!(sql.contains("-- Period: monthly, start: 2026-11-01, count: 3"));
    assert!(!sql.contains("DETACH"));
}

#[test]
fn timestamp_key_literals_are_unambiguous() {
    let parent = parent(
        PartitionStrategy::Range,
        "RANGE (created_at)",
        "timestamp with time zone",
    );
    let plan =
        plan_partitions(&parent, PlanPeriod::Daily, date(2026, 11, 1), 1, None).expect("plan");
    let sql = plan.render_sql();
    assert!(
        sql.contains("FROM ('2026-11-01 00:00:00+00') TO ('2026-11-02 00:00:00+00')"),
        "{sql}"
    );
}

#[test]
fn an_overlapping_interval_is_refused_with_the_existing_partition() {
    let mut parent = parent(PartitionStrategy::Range, "RANGE (created_at)", "date");
    parent.partitions.push(partition(
        "invoices_p2026_11",
        &monthly_range("2026-11-01", "2026-12-01"),
    ));
    let error = plan_partitions(&parent, PlanPeriod::Monthly, date(2026, 11, 1), 1, None)
        .expect_err("must refuse");
    assert!(matches!(error, PlanError::Overlap { .. }));
    assert!(error.to_string().contains("invoices_p2026_11"));
}

#[test]
fn an_unparsable_existing_bound_refuses_the_plan_rather_than_guessing() {
    let mut parent = parent(PartitionStrategy::Range, "RANGE (created_at)", "date");
    parent.partitions.push(partition(
        "invoices_odd",
        "FOR VALUES FROM (date_trunc('month', now())) TO (MAXVALUE)",
    ));
    let error = plan_partitions(&parent, PlanPeriod::Monthly, date(2026, 11, 1), 1, None)
        .expect_err("must refuse");
    assert!(matches!(error, PlanError::UnparsableBound { .. }));
}

#[test]
fn unsupported_strategy_key_and_type_are_refused() {
    let list = parent(PartitionStrategy::List, "LIST (region)", "text");
    assert!(matches!(
        plan_partitions(&list, PlanPeriod::Monthly, date(2026, 11, 1), 1, None),
        Err(PlanError::UnsupportedStrategy(PartitionStrategy::List))
    ));

    let multi = parent(PartitionStrategy::Range, "RANGE (a, b)", "date");
    assert!(matches!(
        plan_partitions(&multi, PlanPeriod::Monthly, date(2026, 11, 1), 1, None),
        Err(PlanError::UnsupportedKey(_))
    ));

    let text = parent(PartitionStrategy::Range, "RANGE (code)", "text");
    assert!(matches!(
        plan_partitions(&text, PlanPeriod::Monthly, date(2026, 11, 1), 1, None),
        Err(PlanError::UnsupportedKeyType(_))
    ));
}

#[test]
fn misaligned_starts_and_out_of_range_counts_are_refused() {
    let parent = parent(PartitionStrategy::Range, "RANGE (created_at)", "date");
    assert!(matches!(
        plan_partitions(&parent, PlanPeriod::Monthly, date(2026, 11, 2), 1, None),
        Err(PlanError::StartNotAligned { .. })
    ));
    assert!(matches!(
        plan_partitions(&parent, PlanPeriod::Monthly, date(2026, 11, 1), 0, None),
        Err(PlanError::InvalidCount(0))
    ));
    assert!(matches!(
        plan_partitions(&parent, PlanPeriod::Monthly, date(2026, 11, 1), 121, None),
        Err(PlanError::InvalidCount(121))
    ));
}

#[test]
fn expiry_selects_fully_expired_children_and_never_one_with_maxvalue() {
    let mut parent = parent(PartitionStrategy::Range, "RANGE (created_at)", "date");
    parent.partitions.push(partition(
        "invoices_p2024_11",
        &monthly_range("2024-11-01", "2024-12-01"),
    ));
    parent.partitions.push(partition(
        "invoices_p2024_12",
        &monthly_range("2024-12-01", "2025-01-01"),
    ));
    parent.partitions.push(partition(
        "invoices_p2027_01",
        "FOR VALUES FROM ('2027-01-01') TO (MAXVALUE)",
    ));
    parent
        .partitions
        .push(partition("invoices_default", "DEFAULT"));
    let plan = plan_partitions(
        &parent,
        PlanPeriod::Monthly,
        date(2026, 11, 1),
        1,
        Some(date(2025, 1, 1)),
    )
    .expect("plan");
    assert_eq!(plan.expires.len(), 2);
    assert_eq!(plan.statement_count(), 5);
    assert!(plan.has_default);
    let sql = plan.render_sql();
    assert!(sql.contains("-- A default partition exists."));
    assert!(sql.contains(
        "ALTER TABLE \"billing\".\"invoices\" DETACH PARTITION \"billing\".\"invoices_p2024_11\";"
    ));
    assert!(sql.contains("DROP TABLE \"billing\".\"invoices_p2024_12\";"));
    assert!(!sql.contains("invoices_p2027_01"));
}

#[test]
fn status_renders_empty_table_json_and_qualified_names() {
    let empty = PartitionSnapshot::empty();
    assert_eq!(empty.render_table(), "No partitioned tables found.\n");
    let parsed: serde_json::Value = serde_json::from_str(&empty.render_json()).expect("valid JSON");
    assert_eq!(parsed["parents"].as_array().map(Vec::len), Some(0));

    let mut parent = parent(PartitionStrategy::Range, "RANGE (created_at)", "date");
    parent.partitions.push(partition(
        "invoices_p2026_11",
        &monthly_range("2026-11-01", "2026-12-01"),
    ));
    let snapshot = PartitionSnapshot {
        parents: vec![parent],
        ..PartitionSnapshot::empty()
    }
    .sorted();
    let table = snapshot.render_table();
    assert!(table.contains("billing.invoices"));
    assert!(table.contains("[RANGE]"));
    assert!(table.contains("invoices_p2026_11"));
    let parsed: serde_json::Value =
        serde_json::from_str(&snapshot.render_json()).expect("valid JSON");
    assert_eq!(parsed["parents"][0]["table"], "invoices");
    assert_eq!(
        parsed["parents"][0]["partitions"][0]["name"],
        "invoices_p2026_11"
    );
}
