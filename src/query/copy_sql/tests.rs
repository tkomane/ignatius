use super::*;

#[test]
fn where_clause_formats_null_empty_and_escaped_literals() {
    assert_eq!(
        build_where_clause("user_id", &Cell::Null),
        Ok("WHERE \"user_id\" IS NULL".to_owned())
    );
    assert_eq!(
        build_where_clause("name", &Cell::Text(String::new())),
        Ok("WHERE \"name\" = ''".to_owned())
    );
    assert_eq!(
        build_where_clause("quote\"col", &Cell::Text("O'Reilly".to_owned())),
        Ok("WHERE \"quote\"\"col\" = 'O''Reilly'".to_owned())
    );
    assert_eq!(
        build_where_clause("literal_null", &Cell::Text("NULL".to_owned())),
        Ok("WHERE \"literal_null\" = 'NULL'".to_owned())
    );
}

#[test]
fn where_clause_refuses_nul_bytes_and_oversized_payloads() {
    assert!(build_where_clause("col", &Cell::Text("abc\0def".to_owned())).is_err());
    assert!(build_where_clause("bad\0col", &Cell::Null).is_err());

    let exact_limit = "a".repeat(MAX_COPY_SQL_BYTES);
    assert!(build_where_clause("c", &Cell::Text(exact_limit)).is_err());

    let fits = "a".repeat(MAX_COPY_SQL_BYTES - "WHERE \"c\" = ''".len());
    assert!(build_where_clause("c", &Cell::Text(fits)).is_ok());
}

#[test]
fn in_list_deduplicates_in_encounter_order_and_ignores_nulls() {
    let cells = [
        Cell::Null,
        Cell::Text("beta".to_owned()),
        Cell::Text("alpha".to_owned()),
        Cell::Text("beta".to_owned()),
        Cell::Null,
        Cell::Text("it's".to_owned()),
        Cell::Text(String::new()),
    ];
    let refs: Vec<&Cell> = cells.iter().collect();

    let (sql, count, truncated) =
        build_in_list("status", &refs, DEFAULT_IN_LIST_MAX_VALUES).expect("valid IN list");
    assert_eq!(sql, "\"status\" IN ('beta', 'alpha', 'it''s', '')");
    assert_eq!(count, 4);
    assert!(!truncated);
}

#[test]
fn in_list_caps_at_max_values_and_reports_truncation() {
    let owned: Vec<Cell> = (0..250).map(|i| Cell::Text(format!("v{i}"))).collect();
    let refs: Vec<&Cell> = owned.iter().collect();

    let (sql, count, truncated) =
        build_in_list("id", &refs, DEFAULT_IN_LIST_MAX_VALUES).expect("capped IN list");
    assert_eq!(count, 200);
    assert!(truncated);
    assert!(sql.starts_with("\"id\" IN ('v0', 'v1',"));
    assert!(sql.contains("'v199'"));
    assert!(!sql.contains("'v200'"));
}

#[test]
fn in_list_refuses_all_null_zero_limit_nul_bytes_and_oversized() {
    let only_nulls = [Cell::Null, Cell::Null];
    let null_refs: Vec<&Cell> = only_nulls.iter().collect();
    assert!(build_in_list("col", &null_refs, 200).is_err());

    let empty_refs: Vec<&Cell> = Vec::new();
    assert!(build_in_list("col", &empty_refs, 200).is_err());

    let valid = [Cell::Text("ok".to_owned())];
    let valid_refs: Vec<&Cell> = valid.iter().collect();
    assert!(build_in_list("col", &valid_refs, 0).is_err());

    let with_nul = [Cell::Text("ok".to_owned()), Cell::Text("bad\0".to_owned())];
    let nul_refs: Vec<&Cell> = with_nul.iter().collect();
    assert!(build_in_list("col", &nul_refs, 200).is_err());

    let huge = [
        Cell::Text("a".repeat(600_000)),
        Cell::Text("b".repeat(600_000)),
    ];
    let huge_refs: Vec<&Cell> = huge.iter().collect();
    assert!(build_in_list("col", &huge_refs, 200).is_err());
}

#[test]
fn row_json_is_compact_deterministic_and_distinguishes_null_from_empty() {
    let columns = vec![
        "z_col".to_owned(),
        "a_null".to_owned(),
        "b_empty".to_owned(),
        "c_lit".to_owned(),
    ];
    let row = vec![
        Cell::Text("42".to_owned()),
        Cell::Null,
        Cell::Text(String::new()),
        Cell::Text("NULL".to_owned()),
    ];
    let json = build_row_json(&columns, &row).expect("valid row JSON");
    assert_eq!(
        json,
        r#"{"a_null":null,"b_empty":"","c_lit":"NULL","z_col":"42"}"#
    );
}

#[test]
fn row_json_enforces_one_mib_bound() {
    let columns = vec!["big".to_owned()];
    let row = vec![Cell::Text("x".repeat(MAX_COPY_SQL_BYTES))];
    assert!(build_row_json(&columns, &row).is_err());
}
