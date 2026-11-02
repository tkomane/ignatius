use super::*;

fn columns() -> Vec<UpdateColumn> {
    vec![
        UpdateColumn {
            name: "order_id".into(),
            primary_key: true,
        },
        UpdateColumn {
            name: "note".into(),
            primary_key: false,
        },
        UpdateColumn {
            name: "total".into(),
            primary_key: false,
        },
    ]
}

fn source(sql: &str, result_columns: &[&str], selected: usize) -> UpdateSource {
    parse_source(
        sql,
        &result_columns
            .iter()
            .map(|column| (*column).to_owned())
            .collect::<Vec<_>>(),
        selected,
    )
    .expect("source")
}

fn plan(replacement: &str) -> UpdatePlan {
    let result_columns = vec!["order_id".to_owned(), "note".to_owned()];
    let selected = source(
        "SELECT order_id, note FROM public.orders WHERE order_id = 1",
        &["order_id", "note"],
        1,
    );
    plan_update(
        &selected,
        "public",
        "orders",
        &result_columns,
        &[Cell::Text("1".into()), Cell::Text("old".into())],
        &columns(),
        replacement,
    )
    .expect("plan")
}

#[test]
fn parses_a_direct_schema_qualified_projection_and_alias() {
    let source = source(
        "SELECT o.order_id, o.note AS customer_note FROM public.orders o WHERE o.order_id = 1 ORDER BY o.order_id",
        &["order_id", "customer_note"],
        1,
    );
    assert_eq!(source.relation.schema.as_deref(), Some("public"));
    assert_eq!(source.relation.relation, "orders");
    assert_eq!(source.projected_columns, ["order_id", "note"]);
    assert_eq!(source.target_column, "note");
}

#[test]
fn star_uses_the_server_result_shape_and_bare_names_remain_unresolved() {
    let source = source("SELECT * FROM orders", &["order_id", "note"], 1);
    assert_eq!(source.relation.schema, None);
    assert_eq!(source.projected_columns, ["order_id", "note"]);
}

#[test]
fn comments_and_quoted_identifiers_do_not_change_the_source_shape() {
    let source = source(
        "/* read */ SELECT \"order_id\", \"note\" FROM \"public\".\"orders\"",
        &["order_id", "note"],
        0,
    );
    assert_eq!(source.relation.schema.as_deref(), Some("public"));
    assert_eq!(source.relation.relation, "orders");
    assert_eq!(source.target_column, "order_id");
}

#[test]
fn from_only_is_refused_without_breaking_a_quoted_only_relation() {
    for sql in [
        "SELECT id, note FROM ONLY orders",
        "SELECT id, note FROM ONLY public.orders",
        "SELECT id, note FROM ONLY orders AS o",
        "SELECT id, note FROM ONLY orders o",
        "SELECT id, note FROM ONLY (public.orders)",
    ] {
        assert_eq!(
            parse_source(sql, &["id".into(), "note".into()], 0),
            Err(UpdateRefusal::UnsupportedSource(
                "FROM ONLY is not editable through a result cell because an update can reach inherited rows"
            )),
            "{sql} must be refused as FROM ONLY"
        );
    }

    let quoted = source("SELECT id, note FROM \"only\"", &["id", "note"], 0);
    assert_eq!(quoted.relation.schema, None);
    assert_eq!(quoted.relation.relation, "only");

    for (sql, relation) in [
        ("SELECT id, note FROM onlys", "onlys"),
        ("SELECT id, note FROM only_t", "only_t"),
    ] {
        let accepted = source(sql, &["id", "note"], 0);
        assert_eq!(accepted.relation.relation, relation, "{sql} is not ONLY");
    }
}

#[test]
fn joins_expressions_ctes_and_set_operations_are_refused() {
    for sql in [
        "SELECT o.id, o.note FROM orders o JOIN notes n ON n.id = o.id",
        "SELECT id, lower(note) FROM orders",
        "WITH rows AS (SELECT id, note FROM orders) SELECT id, note FROM rows",
        "SELECT id, note FROM orders UNION SELECT id, note FROM archived_orders",
        "SELECT id, note FROM orders WHERE id = 1; DELETE FROM orders",
    ] {
        assert!(
            parse_source(sql, &["id".into(), "note".into()], 1).is_err(),
            "{sql} must be refused"
        );
    }
    assert!(
        parse_source(
            "SELECT id, note FROM orders;",
            &["id".into(), "note".into()],
            1
        )
        .is_ok()
    );
}

#[test]
fn duplicate_and_mismatched_result_columns_are_refused() {
    assert!(matches!(
        parse_source("SELECT id, id FROM orders", &["id".into(), "id".into()], 0),
        Err(UpdateRefusal::DuplicateColumn(_))
    ));
    assert_eq!(
        parse_source(
            "SELECT id, note FROM orders",
            &["id".into(), "renamed".into()],
            0
        ),
        Err(UpdateRefusal::ProjectionMismatch)
    );
}

#[test]
fn missing_and_null_primary_keys_are_refused() {
    let selected = source("SELECT note FROM orders", &["note"], 0);
    assert_eq!(
        plan_update(
            &selected,
            "public",
            "orders",
            &["note".into()],
            &[Cell::Text("old".into())],
            &columns(),
            "new",
        ),
        Err(UpdateRefusal::PrimaryKeyNotProjected("order_id".into()))
    );

    let source = source(
        "SELECT order_id, note FROM orders",
        &["order_id", "note"],
        1,
    );
    assert_eq!(
        plan_update(
            &source,
            "public",
            "orders",
            &["order_id".into(), "note".into()],
            &[Cell::Null, Cell::Text("old".into())],
            &columns(),
            "new",
        ),
        Err(UpdateRefusal::NullPrimaryKey("order_id".into()))
    );
}

#[test]
fn primary_key_cell_is_not_an_editable_target() {
    let source = source(
        "SELECT order_id, note FROM orders",
        &["order_id", "note"],
        0,
    );
    assert_eq!(
        plan_update(
            &source,
            "public",
            "orders",
            &["order_id".into(), "note".into()],
            &[Cell::Text("1".into()), Cell::Text("old".into())],
            &columns(),
            "2",
        ),
        Err(UpdateRefusal::PrimaryKeyTarget("order_id".into()))
    );
}

#[test]
fn plan_quotes_identifiers_binds_literal_values_and_keeps_debug_safe() {
    let value = "new ' note \\\\ line\n ; --";
    let plan = plan(value);
    assert_eq!(
        plan.sql_template,
        "UPDATE \"public\".\"orders\" SET \"note\" = :__ignatius_new_value WHERE \"order_id\" = :__ignatius_key_0"
    );
    let bound = plan.bound_sql().expect("bound SQL");
    assert!(bound.contains("new \\\' note"), "{bound}");
    assert!(bound.contains("\\\\ line\\n"), "{bound}");
    let debug = format!("{plan:?}");
    assert!(debug.contains("sql_template"));
    assert!(!debug.contains("new ' note"));
    assert!(!debug.contains("old"));
}

#[test]
fn empty_values_are_valid_but_nul_and_oversized_values_are_refused() {
    let empty = plan("");
    assert!(
        empty
            .bound_sql()
            .expect("empty literal")
            .contains("\"note\" = E''")
    );
    // The planner intentionally does not expose the value in the refusal.
    let refusal = plan_update(
        &source(
            "SELECT order_id, note FROM orders",
            &["order_id", "note"],
            1,
        ),
        "public",
        "orders",
        &["order_id".into(), "note".into()],
        &[Cell::Text("1".into()), Cell::Text("old".into())],
        &columns(),
        "safe\0marker",
    )
    .expect_err("NUL");
    assert_eq!(refusal, UpdateRefusal::NulValue);
    assert!(!format!("{refusal:?}").contains("safe"));
    let refusal = plan_update(
        &source(
            "SELECT order_id, note FROM orders",
            &["order_id", "note"],
            1,
        ),
        "public",
        "orders",
        &["order_id".into(), "note".into()],
        &[Cell::Text("1".into()), Cell::Text("old".into())],
        &columns(),
        &"x".repeat(MAX_UPDATE_SQL_BYTES),
    )
    .expect_err("bound");
    assert!(matches!(refusal, UpdateRefusal::TooLarge { .. }));
}
