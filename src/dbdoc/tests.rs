//! Sidecar tests for schema documentation rendering.

use super::*;

fn fixture() -> SchemaDocument {
    let mut orders = RelationDocument {
        name: "orders".to_owned(),
        kind: RelationKind::Table,
        comment: Some("Customer orders | one row per order".to_owned()),
        estimated_rows: Some(42),
        rls_enabled: true,
        rls_forced: false,
        columns: vec![
            ColumnDocument {
                ordinal: 2,
                name: "note".to_owned(),
                data_type: "text".to_owned(),
                nullable: true,
                default: None,
                primary_key: false,
                references: None,
                comment: Some("free text".to_owned()),
            },
            ColumnDocument {
                ordinal: 1,
                name: "order_id".to_owned(),
                data_type: "integer".to_owned(),
                nullable: false,
                default: Some("nextval('orders_order_id_seq'::regclass)".to_owned()),
                primary_key: true,
                references: None,
                comment: None,
            },
            ColumnDocument {
                ordinal: 3,
                name: "customer_id".to_owned(),
                data_type: "integer".to_owned(),
                nullable: false,
                default: None,
                primary_key: false,
                references: Some("public.customers.customer_id".to_owned()),
                comment: None,
            },
        ],
        indexes: vec![IndexDocument {
            name: "orders_pkey".to_owned(),
            definition: "CREATE UNIQUE INDEX orders_pkey ON public.orders USING btree (order_id)"
                .to_owned(),
            unique: true,
            primary: true,
        }],
        constraints: vec![ConstraintDocument {
            name: "orders_customer_fk".to_owned(),
            kind: "f".to_owned(),
            definition: "FOREIGN KEY (customer_id) REFERENCES customers(customer_id)".to_owned(),
        }],
        policies: vec![PolicyDocument {
            name: "tenant_read".to_owned(),
            command: "SELECT".to_owned(),
            roles: vec!["app".to_owned()],
            using: Some("tenant_id = current_setting('app.tenant')::int".to_owned()),
            with_check: None,
        }],
    };
    orders.sort();

    let customers = RelationDocument {
        name: "customers".to_owned(),
        kind: RelationKind::Table,
        comment: None,
        estimated_rows: Some(7),
        rls_enabled: false,
        rls_forced: false,
        columns: vec![ColumnDocument {
            ordinal: 1,
            name: "customer_id".to_owned(),
            data_type: "integer".to_owned(),
            nullable: false,
            default: None,
            primary_key: true,
            references: None,
            comment: None,
        }],
        indexes: Vec::new(),
        constraints: Vec::new(),
        policies: Vec::new(),
    };

    let mut document = SchemaDocument::new("demo", "public", "2026-11-12T10:00:00+02:00");
    document.relations = vec![orders, customers];
    document.sort();
    document
}

#[test]
fn markdown_lists_relations_columns_keys_indexes_constraints_and_policies() {
    let text = render_markdown(&fixture());

    assert!(text.contains("# Data dictionary: demo.public"));
    assert!(text.contains("- [customers](#customers)"));
    assert!(text.contains("- [orders](#orders)"));
    assert!(text.contains("| 1 | order_id | integer | NO |"));
    assert!(text.contains("| PK |"));
    assert!(text.contains("public.customers.customer_id"));
    assert!(text.contains("### Indexes"));
    assert!(text.contains("### Constraints"));
    assert!(text.contains("### Row-level security policies"));
    assert!(text.contains("tenant_read"));
    assert!(
        text.contains("Customer orders \\| one row per order"),
        "a pipe in a comment cannot break the table"
    );
    assert!(text.contains("catalog metadata. No row data is included."));
}

#[test]
fn markdown_orders_columns_by_ordinal_and_relations_by_name() {
    let text = render_markdown(&fixture());
    let order_id = text.find("| 1 | order_id").expect("order_id row");
    let note = text.find("| 2 | note").expect("note row");
    let customer = text.find("| 3 | customer_id").expect("customer row");
    assert!(order_id < note && note < customer);

    let customers = text.find("## customers").expect("customers section");
    let orders = text.find("## orders").expect("orders section");
    assert!(customers < orders, "relations are alphabetical");
}

#[test]
fn an_empty_schema_says_so_instead_of_rendering_an_empty_table() {
    let document = SchemaDocument::new("demo", "empty", "2026-11-12T10:00:00+02:00");
    let text = render_markdown(&document);
    assert!(text.contains("_No relations are visible in this schema._"));
}

#[test]
fn truncation_is_stated_in_the_document() {
    let mut document = fixture();
    document.truncated = true;
    let text = render_markdown(&document);
    assert!(text.contains("reached a relation or column bound"));
}

#[test]
fn json_round_trips_and_keeps_stable_field_names() {
    let document = fixture();
    let json = render_json(&document).expect("serialize");
    let value: serde_json::Value = serde_json::from_str(&json).expect("parse");
    assert_eq!(value["version"], DOCUMENT_VERSION);
    assert_eq!(value["database"], "demo");
    assert_eq!(value["schema"], "public");
    assert_eq!(value["relations"].as_array().unwrap().len(), 2);
    assert_eq!(value["relations"][1]["name"], "orders");
    assert_eq!(value["relations"][1]["kind"], "table");
    let restored: SchemaDocument = serde_json::from_str(&json).expect("round trip");
    assert_eq!(restored, document);
}

#[test]
fn mermaid_renders_entities_and_a_foreign_key_relationship() {
    let text = render_mermaid(&fixture());
    assert!(text.starts_with("erDiagram\n"));
    assert!(text.contains("\"public.orders\" {"));
    assert!(text.contains("\"public.customers\" {"));
    assert!(text.contains("integer order_id PK"));
    assert!(
        text.contains(
            "\"public.customers\" }o--|| \"public.orders\" : \"customer_id.customer_id\""
        ),
        "{text}"
    );
}

#[test]
fn hostile_text_cannot_escape_a_markdown_cell_or_mermaid_label() {
    assert_eq!(markdown_cell("a|b\nc\td\u{1b}"), "a\\|b<br>c d ");
    assert_eq!(mermaid_label("say \"hi\"\u{1b}"), "say #quot;hi#quot; ");
}

#[test]
fn count_reports_columns_across_relations() {
    assert_eq!(fixture().column_count(), 4);
}
