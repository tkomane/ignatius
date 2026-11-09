use super::*;

#[test]
fn canonical_drop_statement_quotes_identifiers_and_supports_function_signatures() {
    assert_eq!(
        canonical_drop_statement(ObjectKind::Table, "public", "orders", None),
        "DROP TABLE IF EXISTS \"public\".\"orders\" CASCADE;"
    );
    assert_eq!(
        canonical_drop_statement(
            ObjectKind::Function,
            "reporting",
            "calc_tax",
            Some("integer, numeric")
        ),
        "DROP FUNCTION IF EXISTS \"reporting\".\"calc_tax\"(integer, numeric) CASCADE;"
    );
    assert_eq!(
        canonical_drop_statement(ObjectKind::Sequence, "public", "orders_id_seq", None),
        "DROP SEQUENCE IF EXISTS \"public\".\"orders_id_seq\" CASCADE;"
    );
}

#[test]
fn sequence_and_enum_formatters_emit_deterministic_postgres_ddl() {
    let seq = format_sequence_ddl(SequenceDdlSpec {
        schema: "public",
        name: "orders_order_id_seq",
        data_type: "bigint",
        start_value: 1,
        increment_by: 1,
        min_value: 1,
        max_value: 9_223_372_036_854_775_807,
        cache_size: 1,
        is_cycled: false,
    });
    assert!(seq.starts_with("CREATE SEQUENCE \"public\".\"orders_order_id_seq\""));
    assert!(seq.contains("AS bigint"));
    assert!(seq.contains("NO CYCLE;"));

    let enum_ddl = format_enum_ddl(
        "public",
        "order_status",
        &[
            "pending".to_owned(),
            "shipped".to_owned(),
            "it's_done".to_owned(),
        ],
    );
    assert_eq!(
        enum_ddl,
        "CREATE TYPE \"public\".\"order_status\" AS ENUM ('pending', 'shipped', 'it''s_done');"
    );
}

#[test]
fn ddl_bundle_orders_categories_and_renders_reverse_drop_headers() {
    let bundle = DdlBundle {
        schema: "public".to_owned(),
        target_object: None,
        objects: vec![
            DdlObject {
                category: DdlCategory::View,
                kind: ObjectKind::View,
                schema: "public".to_owned(),
                name: "recent_orders".to_owned(),
                drop_statement: "DROP VIEW IF EXISTS \"public\".\"recent_orders\" CASCADE;"
                    .to_owned(),
                create_statement:
                    "CREATE OR REPLACE VIEW \"public\".\"recent_orders\" AS\nSELECT * FROM orders;"
                        .to_owned(),
                dependent_statements: Vec::new(),
            },
            DdlObject {
                category: DdlCategory::Table,
                kind: ObjectKind::Table,
                schema: "public".to_owned(),
                name: "orders".to_owned(),
                drop_statement: "DROP TABLE IF EXISTS \"public\".\"orders\" CASCADE;".to_owned(),
                create_statement: "CREATE TABLE \"public\".\"orders\" (\n    \"id\" bigint NOT NULL\n);"
                    .to_owned(),
                dependent_statements: vec![
                    "ALTER TABLE \"public\".\"orders\" ADD CONSTRAINT \"orders_pkey\" PRIMARY KEY (id);"
                        .to_owned(),
                ],
            },
            DdlObject {
                category: DdlCategory::Type,
                kind: ObjectKind::Table,
                schema: "public".to_owned(),
                name: "order_status".to_owned(),
                drop_statement: "DROP TYPE IF EXISTS \"public\".\"order_status\" CASCADE;"
                    .to_owned(),
                create_statement:
                    "CREATE TYPE \"public\".\"order_status\" AS ENUM ('pending', 'paid');"
                        .to_owned(),
                dependent_statements: Vec::new(),
            },
        ],
        truncated_from: None,
    }
    .bounded();

    let rendered = bundle.render_sql(true);
    let drop_view_pos = rendered.find("DROP VIEW IF EXISTS").expect("drop view");
    let drop_table_pos = rendered.find("DROP TABLE IF EXISTS").expect("drop table");
    let drop_type_pos = rendered.find("DROP TYPE IF EXISTS").expect("drop type");
    assert!(
        drop_view_pos < drop_table_pos && drop_table_pos < drop_type_pos,
        "reverse drop order must drop views before tables and tables before types"
    );

    let create_type_pos = rendered.find("CREATE TYPE").expect("create type");
    let create_table_pos = rendered.find("CREATE TABLE").expect("create table");
    let create_view_pos = rendered
        .find("CREATE OR REPLACE VIEW")
        .expect("create view");
    assert!(
        create_type_pos < create_table_pos && create_table_pos < create_view_pos,
        "forward creation order must create types before tables and tables before views"
    );
}
