//! Unit tests for pure Object Tree quick-SQL builders.

use super::*;
use crate::query::parameters;

fn s(val: &str) -> String {
    val.to_owned()
}

#[test]
fn select_preview_uses_loaded_columns_or_falls_back_to_star() {
    let star = build_select_preview_sql("public", "orders", &[], DEFAULT_PREVIEW_LIMIT)
        .expect("star preview");
    assert_eq!(star, "SELECT *\nFROM \"public\".\"orders\"\nLIMIT 100;");

    let cols = build_select_preview_sql("Sales", "Order Items", &[s("id"), s("unit\"price")], 25)
        .expect("column preview");
    assert_eq!(
        cols,
        "SELECT \"id\", \"unit\"\"price\"\nFROM \"Sales\".\"Order Items\"\nLIMIT 25;"
    );

    let zero_limit = build_select_preview_sql("public", "orders", &[], 0).expect("clamped limit");
    assert!(zero_limit.ends_with("LIMIT 1;"), "{zero_limit}");
}

#[test]
fn count_sql_quotes_identifiers_safely() {
    let sql = build_count_sql("my-schema", "weird\"table").expect("count sql");
    assert_eq!(
        sql,
        "SELECT count(*) AS row_count\nFROM \"my-schema\".\"weird\"\"table\";"
    );
}

#[test]
fn insert_template_quotes_columns_and_generates_unique_valid_placeholders() {
    let cols = vec![s("id"), s("user name"), s("user-name"), s("1st_col")];
    let sql = build_insert_template_sql("public", "events", &cols).expect("insert template");
    assert_eq!(
        sql,
        "INSERT INTO \"public\".\"events\" (\"id\", \"user name\", \"user-name\", \"1st_col\")\nVALUES (:id, :user_name, :user_name_2, :p_1st_col);"
    );

    let discovered = parameters::discover(&sql).expect("valid placeholders");
    assert_eq!(
        discovered.names(),
        &["id", "user_name", "user_name_2", "p_1st_col"]
    );
}

#[test]
fn column_list_and_insert_template_require_non_empty_columns() {
    assert_eq!(build_column_list_sql(&[]), Err(ObjectSqlError::NoColumns));
    assert_eq!(
        build_insert_template_sql("public", "orders", &[]),
        Err(ObjectSqlError::NoColumns)
    );
    let cols = vec![s("id"), s("café")];
    assert_eq!(
        build_column_list_sql(&cols).expect("column list"),
        "\"id\", \"café\""
    );
}

#[test]
fn builders_reject_empty_identifiers_and_nul_bytes() {
    assert_eq!(
        build_count_sql("", "orders"),
        Err(ObjectSqlError::EmptyIdentifier { what: "schema" })
    );
    assert_eq!(
        build_count_sql("public", ""),
        Err(ObjectSqlError::EmptyIdentifier { what: "object" })
    );
    assert_eq!(
        build_column_list_sql(&[s("")]),
        Err(ObjectSqlError::EmptyIdentifier { what: "column" })
    );
    assert_eq!(
        build_count_sql("pub\0lic", "orders"),
        Err(ObjectSqlError::EmbeddedNul { what: "schema" })
    );
    assert_eq!(
        build_count_sql("public", "ord\0ers"),
        Err(ObjectSqlError::EmbeddedNul { what: "object" })
    );
    assert_eq!(
        build_select_preview_sql("public", "orders", &[s("a\0b")], 10),
        Err(ObjectSqlError::EmbeddedNul { what: "column" })
    );
}

#[test]
fn builders_enforce_column_and_byte_caps() {
    let too_many: Vec<String> = (0..=MAX_OBJECT_SQL_COLUMNS)
        .map(|i| format!("c_{i}"))
        .collect();
    assert_eq!(
        build_column_list_sql(&too_many),
        Err(ObjectSqlError::TooManyColumns {
            count: MAX_OBJECT_SQL_COLUMNS + 1,
            max: MAX_OBJECT_SQL_COLUMNS,
        })
    );
    assert_eq!(
        build_select_preview_sql("public", "wide", &too_many, 10),
        Err(ObjectSqlError::TooManyColumns {
            count: MAX_OBJECT_SQL_COLUMNS + 1,
            max: MAX_OBJECT_SQL_COLUMNS,
        })
    );

    let giant = "x".repeat(MAX_OBJECT_SQL_BYTES);
    assert!(matches!(
        build_count_sql("public", &giant),
        Err(ObjectSqlError::ExceedsByteCap { .. })
    ));
}

#[test]
fn relation_kind_predicates_match_postgresql_semantics() {
    for kind in [
        ObjectKind::Table,
        ObjectKind::View,
        ObjectKind::MaterializedView,
        ObjectKind::PartitionedTable,
        ObjectKind::ForeignTable,
    ] {
        assert!(is_queryable_relation(kind));
    }
    assert!(!is_queryable_relation(ObjectKind::Index));
    assert!(!is_queryable_relation(ObjectKind::Function));
    assert!(!is_queryable_relation(ObjectKind::Sequence));

    assert!(is_insertable_relation(ObjectKind::Table));
    assert!(is_insertable_relation(ObjectKind::PartitionedTable));
    assert!(is_insertable_relation(ObjectKind::ForeignTable));
    assert!(!is_insertable_relation(ObjectKind::View));
    assert!(!is_insertable_relation(ObjectKind::MaterializedView));
}
