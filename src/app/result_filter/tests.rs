use super::*;

fn sample_set() -> ResultSet {
    let mut set = ResultSet::new(
        vec![
            "id".into(),
            "status".into(),
            "amount".into(),
            "notes".into(),
            "created_at".into(),
        ],
        100,
    );
    set.push(vec![
        Cell::Text("1".into()),
        Cell::Text("paid".into()),
        Cell::Text("125.50".into()),
        Cell::Text("null".into()), // literal text "null", NOT SQL NULL
        Cell::Text("2026-11-28 12:30:00".into()),
    ]);
    set.push(vec![
        Cell::Text("2".into()),
        Cell::Text("failed".into()),
        Cell::Text("40".into()),
        Cell::Null, // actual SQL NULL
        Cell::Text("https://example.test:8443/pay".into()),
    ]);
    set.push(vec![
        Cell::Text("3".into()),
        Cell::Text("pending".into()),
        Cell::Text("250".into()),
        Cell::Text("vip customer".into()),
        Cell::Text("2026-11-28 14:00:00".into()),
    ]);
    set
}

#[test]
fn plain_substrings_timestamps_urls_and_negation_work() {
    let set = sample_set();
    assert_eq!(filter_rows(&set, "paid"), vec![0]);
    assert_eq!(filter_rows(&set, "12:30:00"), vec![0]);
    assert_eq!(filter_rows(&set, "https://example.test:8443"), vec![1]);
    assert_eq!(filter_rows(&set, "!failed"), vec![0, 2]);
    assert_eq!(filter_rows(&set, "\"vip customer\""), vec![2]);
}

#[test]
fn column_predicates_distinguish_null_exact_not_equal_and_numeric() {
    let set = sample_set();

    // SQL NULL vs literal string "null"
    assert_eq!(filter_rows(&set, "notes:null"), vec![1]);
    assert_eq!(filter_rows(&set, "notes:is:null"), vec![1]);
    assert_eq!(filter_rows(&set, "notes:=null"), vec![0]);
    assert_eq!(filter_rows(&set, "notes:!null"), vec![0, 2]);
    assert_eq!(filter_rows(&set, "notes:notnull"), vec![0, 2]);

    // Exact vs substring vs !=
    assert_eq!(filter_rows(&set, "status:=paid"), vec![0]);
    assert_eq!(filter_rows(&set, "status:p"), vec![0, 2]);
    assert_eq!(filter_rows(&set, "status:!=failed"), vec![0, 2]);

    // Numeric comparisons
    assert_eq!(filter_rows(&set, "amount:>100"), vec![0, 2]);
    assert_eq!(filter_rows(&set, "amount:>=125.5"), vec![0, 2]);
    assert_eq!(filter_rows(&set, "amount:<100"), vec![1]);
    assert_eq!(filter_rows(&set, "amount:<=40"), vec![1]);
    assert!(filter_rows(&set, "status:>10").is_empty());

    // Combined AND terms
    assert_eq!(filter_rows(&set, "amount:>100 !paid"), vec![2]);
}

#[test]
fn unknown_column_with_explicit_operator_returns_empty_and_reports_column() {
    let set = sample_set();
    let parsed = ParsedResultFilter::parse("missing_col:=abc", &set.columns);
    assert_eq!(parsed.unknown_columns, vec!["missing_col".to_owned()]);
    assert!(filter_rows(&set, "missing_col:=abc").is_empty());

    let parsed_gt = ParsedResultFilter::parse("nope:>5", &set.columns);
    assert_eq!(parsed_gt.unknown_columns, vec!["nope".to_owned()]);
    assert!(filter_rows(&set, "nope:>5").is_empty());
}

#[test]
fn cell_filter_builder_round_trips_spaces_quotes_and_null() {
    let mut set = ResultSet::new(vec!["user name".into(), "note".into()], 10);
    set.push(vec![Cell::Text("alice \"dev\"".into()), Cell::Null]);
    set.push(vec![Cell::Text("bob".into()), Cell::Text("ok".into())]);

    let expr_text = build_cell_filter_expression("user name", &set.rows[0][0]);
    assert_eq!(filter_rows(&set, &expr_text), vec![0]);

    let expr_null = build_cell_filter_expression("note", &set.rows[0][1]);
    assert_eq!(expr_null, "note:null");
    assert_eq!(filter_rows(&set, &expr_null), vec![0]);
}

#[test]
fn filter_bounds_cap_terms_and_bytes() {
    let set = sample_set();
    let many_terms = (0..30)
        .map(|i| format!("t{i}"))
        .collect::<Vec<_>>()
        .join(" ");
    let parsed = ParsedResultFilter::parse(&many_terms, &set.columns);
    assert!(parsed.truncated);

    let long_filter = "a".repeat(MAX_FILTER_LEN + 200);
    let parsed_long = ParsedResultFilter::parse(&long_filter, &set.columns);
    assert!(parsed_long.truncated);
}
