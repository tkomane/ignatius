use super::*;
use crate::query::completion::{
    CatalogColumn, CatalogObject, CatalogObjectKind, CatalogRelation, CompletionCatalog,
};

fn sample_graph() -> SchemaGraph {
    SchemaGraph {
        tables: vec![
            DiagramTable {
                schema: "public".to_owned(),
                name: "customers".to_owned(),
                columns: vec![
                    DiagramColumn {
                        name: "customer_id".to_owned(),
                        data_type: "integer".to_owned(),
                        is_primary_key: true,
                        is_foreign_key: false,
                        is_unique: true,
                        nullable: false,
                    },
                    DiagramColumn {
                        name: "email".to_owned(),
                        data_type: "text".to_owned(),
                        is_primary_key: false,
                        is_foreign_key: false,
                        is_unique: true,
                        nullable: false,
                    },
                ],
            },
            DiagramTable {
                schema: "public".to_owned(),
                name: "orders".to_owned(),
                columns: vec![
                    DiagramColumn {
                        name: "order_id".to_owned(),
                        data_type: "bigint".to_owned(),
                        is_primary_key: true,
                        is_foreign_key: false,
                        is_unique: true,
                        nullable: false,
                    },
                    DiagramColumn {
                        name: "customer_id".to_owned(),
                        data_type: "integer".to_owned(),
                        is_primary_key: false,
                        is_foreign_key: true,
                        is_unique: false,
                        nullable: false,
                    },
                ],
            },
            DiagramTable {
                schema: "public".to_owned(),
                name: "order_items".to_owned(),
                columns: vec![
                    DiagramColumn {
                        name: "item_id".to_owned(),
                        data_type: "bigint".to_owned(),
                        is_primary_key: true,
                        is_foreign_key: false,
                        is_unique: true,
                        nullable: false,
                    },
                    DiagramColumn {
                        name: "order_id".to_owned(),
                        data_type: "bigint".to_owned(),
                        is_primary_key: false,
                        is_foreign_key: true,
                        is_unique: false,
                        nullable: false,
                    },
                ],
            },
            DiagramTable {
                schema: "reporting".to_owned(),
                name: "audit_log".to_owned(),
                columns: vec![DiagramColumn {
                    name: "audit_id".to_owned(),
                    data_type: "bigint".to_owned(),
                    is_primary_key: true,
                    is_foreign_key: false,
                    is_unique: true,
                    nullable: false,
                }],
            },
        ],
        edges: vec![
            ForeignKeyEdge {
                constraint_name: "fk_orders_customer".to_owned(),
                source_schema: "public".to_owned(),
                source_table: "orders".to_owned(),
                source_columns: vec!["customer_id".to_owned()],
                target_schema: "public".to_owned(),
                target_table: "customers".to_owned(),
                target_columns: vec!["customer_id".to_owned()],
                cardinality: Cardinality::OneToMany,
                nullable: false,
            },
            ForeignKeyEdge {
                constraint_name: "fk_items_order".to_owned(),
                source_schema: "public".to_owned(),
                source_table: "order_items".to_owned(),
                source_columns: vec!["order_id".to_owned()],
                target_schema: "public".to_owned(),
                target_table: "orders".to_owned(),
                target_columns: vec!["order_id".to_owned()],
                cardinality: Cardinality::OneToMany,
                nullable: false,
            },
        ],
        truncated_from: None,
    }
}

#[test]
fn mermaid_renderer_emits_valid_er_diagram_with_pk_fk_and_cardinality() {
    let rendered = sample_graph().render(DiagramFormat::Mermaid);
    assert!(rendered.starts_with("erDiagram\n"));
    assert!(
        rendered.contains("\"public.customers\" ||--|{ \"public.orders\" : \"fk_orders_customer\"")
    );
    assert!(rendered.contains("integer customer_id PK"));
    assert!(rendered.contains("integer customer_id FK"));
}

#[test]
fn dot_renderer_emits_graphviz_digraph_with_ports_and_edges() {
    let rendered = sample_graph().render(DiagramFormat::Dot);
    assert!(rendered.starts_with("digraph schema_er_diagram {"));
    assert!(rendered.contains(
        "\"public.orders\":\"customer_id\" -> \"public.customers\":\"customer_id\" [label=\"fk_orders_customer (1:N)\"];"
    ));
}

#[test]
fn svg_renderer_emits_self_contained_svg_with_cards_and_connectors() {
    let rendered = sample_graph().render(DiagramFormat::Svg);
    assert!(rendered.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\""));
    assert!(rendered.contains("public.customers"));
    assert!(rendered.contains("public.orders"));
    assert!(rendered.contains("marker-end=\"url(#fk-arrow)\""));
    assert!(rendered.trim_end().ends_with("</svg>"));
}

#[test]
fn unicode_and_ascii_renderers_distinguish_box_glyphs_and_list_edges() {
    let graph = sample_graph();
    let unicode = graph.render(DiagramFormat::Unicode);
    let ascii = graph.render(DiagramFormat::Ascii);

    assert!(unicode.contains('┌'));
    assert!(unicode.contains("──>"));
    assert!(!ascii.contains('┌'));
    assert!(ascii.contains("+---"));
    assert!(ascii.contains("-->"));
    assert!(
        ascii.contains("public.orders(customer_id) --> public.customers(customer_id) [1:N, mandatory, fk_orders_customer]")
    );
}

#[test]
fn neighborhood_filter_respects_one_hop_and_two_hop_boundaries() {
    let graph = sample_graph();
    let one_hop = graph.filter_neighborhood("customers", DiagramHopDepth::OneHop);
    let names_1: Vec<String> = one_hop
        .tables
        .iter()
        .map(DiagramTable::qualified_name)
        .collect();
    assert_eq!(names_1, vec!["public.customers", "public.orders"]);
    assert_eq!(one_hop.edges.len(), 1);

    let two_hops = graph.filter_neighborhood("customers", DiagramHopDepth::TwoHops);
    let names_2: Vec<String> = two_hops
        .tables
        .iter()
        .map(DiagramTable::qualified_name)
        .collect();
    assert_eq!(
        names_2,
        vec!["public.customers", "public.order_items", "public.orders"]
    );
    assert_eq!(two_hops.edges.len(), 2);
}

#[test]
fn schema_filter_and_table_cap_enforce_200_node_ceiling() {
    let graph = sample_graph().filter_schema("public");
    assert_eq!(graph.tables.len(), 3);

    let oversized = CompletionCatalog {
        objects: (0..225)
            .map(|idx| CatalogObject {
                schema: "public".to_owned(),
                name: format!("table_{idx:03}"),
                kind: CatalogObjectKind::Table,
                readable: true,
                detail: None,
            })
            .collect(),
        relations: (0..225)
            .map(|idx| CatalogRelation {
                schema: "public".to_owned(),
                name: format!("table_{idx:03}"),
                columns: vec![CatalogColumn {
                    name: "id".to_owned(),
                    data_type: "integer".to_owned(),
                }],
            })
            .collect(),
    };
    let bounded = SchemaGraph::from_completion_catalog(&oversized);
    assert_eq!(bounded.tables.len(), MAX_DIAGRAM_TABLES);
    assert_eq!(bounded.truncated_from, Some(225));
}
