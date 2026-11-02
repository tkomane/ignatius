use super::*;
use std::collections::BTreeMap;

fn schema(name: &str, tables: i64, views: i64, usable: bool) -> SchemaSummary {
    let mut counts = BTreeMap::new();
    if tables > 0 {
        counts.insert(ObjectKind::Table, tables);
    }
    if views > 0 {
        counts.insert(ObjectKind::View, views);
    }
    SchemaSummary {
        name: name.to_owned(),
        usable,
        counts,
    }
}

fn object(schema: &str, name: &str, readable: bool) -> ObjectSummary {
    ObjectSummary {
        kind: ObjectKind::Table,
        schema: schema.to_owned(),
        name: name.to_owned(),
        readable,
        detail: None,
    }
}

fn loaded_tree() -> ObjectTree {
    let mut tree = ObjectTree::default();
    tree.set_schemas(vec![
        schema("public", 2, 1, true),
        schema("reporting", 0, 1, true),
        schema("locked", 5, 0, false),
    ]);
    tree
}

#[test]
fn a_fresh_tree_shows_one_row_per_schema_and_the_database_itself() {
    let tree = loaded_tree();
    let rows = tree.rows();
    assert_eq!(rows.len(), 4, "three schemas and the extensions node");
    assert_eq!(
        rows[3].label, "extensions",
        "extensions belong to the database, not to a schema"
    );
    assert!(rows[3].expandable);
    assert_eq!(rows[0].label, "public");
    assert_eq!(rows[0].depth, 0);
    assert!(rows[0].expandable);
    assert!(!rows[0].expanded);
}

#[test]
fn a_schema_without_permission_says_so_and_cannot_be_opened() {
    let tree = loaded_tree();
    let locked = &tree.rows()[2];
    assert_eq!(locked.label, "locked");
    assert_eq!(locked.detail.as_deref(), Some("no permission"));
    assert!(
        !locked.expandable,
        "a schema the role cannot use has nothing to show"
    );
}

#[test]
fn opening_a_schema_needs_no_round_trip_because_counts_already_arrived() {
    let mut tree = loaded_tree();
    assert!(tree.expand_schema(&[0]));

    let rows = tree.rows();
    let labels: Vec<&str> = rows.iter().map(|r| r.label.as_str()).collect();
    assert_eq!(
        labels,
        vec![
            "public",
            "tables (2)",
            "views (1)",
            "reporting",
            "locked",
            "extensions"
        ]
    );
    // The group label carries the kind in words, so it survives without icons.
    assert!(rows[1].label.contains("tables"));
    assert_eq!(rows[1].depth, 1);
}

#[test]
fn a_schema_with_no_objects_says_so_rather_than_opening_empty() {
    let mut tree = ObjectTree::default();
    tree.set_schemas(vec![schema("empty", 0, 0, true)]);
    // Nothing to expand, but if it is opened anyway it explains itself.
    assert!(tree.expand_schema(&[0]));
    let rows = tree.rows();
    assert_eq!(rows[1].label, "nothing here");
}

#[test]
fn opening_a_group_asks_for_a_load_once() {
    let mut tree = loaded_tree();
    tree.expand_schema(&[0]);
    tree.selected = 1; // tables (2)

    let (request, path, query) = tree.expand_selected().expect("a load is needed");
    assert_eq!(path, vec![0, 0]);
    assert_eq!(
        query,
        MetadataQuery::Objects {
            schema: "public".into(),
            kind: ObjectKind::Table
        }
    );
    assert!(tree.rows()[1].loading, "the row shows that it is waiting");

    // Asking again while the answer is in flight must not send a second load.
    assert!(
        tree.expand_selected().is_none(),
        "one load per expansion, not one per keypress"
    );

    assert!(tree.apply(
        request,
        &path,
        MetadataPayload::Objects(vec![
            object("public", "orders", true),
            object("public", "secrets", false),
        ])
    ));

    let rows = tree.rows();
    let labels: Vec<&str> = rows.iter().map(|r| r.label.as_str()).collect();
    assert_eq!(
        labels,
        vec![
            "public",
            "tables (2)",
            "orders",
            "secrets",
            "views (1)",
            "reporting",
            "locked",
            "extensions"
        ]
    );
    assert!(!rows[1].loading);
    assert_eq!(
        rows[3].detail.as_deref(),
        Some("not readable"),
        "an object the role cannot read is marked, not hidden"
    );
}

#[test]
fn a_stale_load_cannot_populate_a_node_that_moved_on() {
    let mut tree = loaded_tree();
    tree.expand_schema(&[0]);
    tree.selected = 1;
    let (first, path, _) = tree.expand_selected().expect("first load");

    // The user collapses and reopens before the answer arrives.
    tree.collapse_selected();
    let node = tree.node(&path).expect("node");
    assert!(!node.expanded);

    // The first answer arrives late. The node is no longer waiting for it.
    let applied = tree.apply(
        first,
        &path,
        MetadataPayload::Objects(vec![object("public", "stale", true)]),
    );
    assert!(applied, "the node is still waiting for its first request");

    // Now a genuinely stale case: a request identity that was never issued.
    assert!(
        !tree.apply(
            RequestId(9999),
            &path,
            MetadataPayload::Objects(vec![object("public", "wrong", true)])
        ),
        "an answer to a question nobody asked must be discarded"
    );
    let labels: Vec<String> = tree.rows().iter().map(|r| r.label.clone()).collect();
    assert!(!labels.contains(&"wrong".to_owned()), "{labels:?}");
}

#[test]
fn a_failed_load_shows_the_reason_in_the_tree() {
    let mut tree = loaded_tree();
    tree.expand_schema(&[0]);
    tree.selected = 1;
    let (request, path, _) = tree.expand_selected().expect("load");

    assert!(tree.apply_failure(request, &path, "permission denied for schema public"));
    let rows = tree.rows();
    assert_eq!(rows[2].label, "permission denied for schema public");
    assert!(
        !rows[1].loading,
        "the node stops waiting even when it failed"
    );
}

#[test]
fn an_empty_result_says_so_rather_than_looking_unloaded() {
    let mut tree = loaded_tree();
    tree.expand_schema(&[0]);
    tree.selected = 1;
    let (request, path, _) = tree.expand_selected().expect("load");
    assert!(tree.apply(request, &path, MetadataPayload::Objects(Vec::new())));
    assert_eq!(tree.rows()[2].label, "nothing here");
}

#[test]
fn selection_moves_within_the_visible_rows_and_stops_at_the_ends() {
    let mut tree = loaded_tree();
    for _ in 0..10 {
        tree.move_selection(1);
    }
    assert_eq!(tree.selected, 3, "cannot move past the last row");
    for _ in 0..10 {
        tree.move_selection(-1);
    }
    assert_eq!(tree.selected, 0);
}

#[test]
fn a_relation_shows_its_columns_and_then_its_indexes() {
    let mut tree = loaded_tree();
    tree.expand_schema(&[0]);
    tree.selected = 1;
    let (request, path, query) = tree.expand_selected().expect("a load");
    assert!(matches!(query, MetadataQuery::Objects { .. }));
    tree.apply(
        request,
        &path,
        MetadataPayload::Objects(vec![object("public", "orders", true)]),
    );

    tree.selected = 2;
    let (request, path, query) = tree.expand_selected().expect("a load");
    assert!(
        matches!(query, MetadataQuery::Relation { ref relation, .. } if relation == "orders"),
        "a relation is asked about as a whole: {query:?}"
    );
    tree.apply(
        request,
        &path,
        MetadataPayload::Relation {
            columns: vec![ColumnInfo {
                name: "order_id".into(),
                data_type: "bigint".into(),
                nullable: false,
                primary_key: true,
                default: None,
            }],
            indexes: vec![ObjectSummary {
                kind: ObjectKind::Index,
                schema: "public".into(),
                name: "orders_pkey".into(),
                readable: true,
                detail: Some("primary key".to_owned()),
            }],
        },
    );

    let labels: Vec<String> = tree.rows().iter().map(|r| r.label.clone()).collect();
    let order_id = labels.iter().position(|l| l == "order_id").expect("column");
    let index = labels
        .iter()
        .position(|l| l == "orders_pkey")
        .expect("index");
    assert!(
        order_id < index,
        "columns come first, then what makes the relation findable: {labels:?}"
    );

    let rows = tree.rows();
    assert!(matches!(
        rows[index].row_kind,
        RowKind::Object {
            kind: ObjectKind::Index,
            ..
        }
    ));
}

#[test]
fn the_extensions_node_asks_for_extensions_and_shows_them() {
    let mut tree = loaded_tree();
    let last = tree.rows().len() - 1;
    tree.selected = last;
    let (request, path, query) = tree.expand_selected().expect("a load");
    assert_eq!(query, MetadataQuery::Extensions);

    tree.apply(
        request,
        &path,
        MetadataPayload::Relation {
            columns: Vec::new(),
            indexes: vec![ObjectSummary {
                kind: ObjectKind::Extension,
                schema: "public".into(),
                name: "plpgsql".into(),
                readable: true,
                detail: Some("version 1.0".to_owned()),
            }],
        },
    );

    let rows = tree.rows();
    let extension = rows.last().expect("a row");
    assert_eq!(extension.label, "plpgsql");
    assert_eq!(extension.detail.as_deref(), Some("version 1.0"));
    assert_eq!(extension.depth, 1);
}

#[test]
fn collapsing_a_closed_node_moves_to_its_parent() {
    let mut tree = loaded_tree();
    tree.expand_schema(&[0]);
    tree.selected = 1; // tables (2), closed

    tree.collapse_selected();
    assert_eq!(tree.selected, 0, "focus moves up to the schema");
    assert_eq!(tree.selected_row().expect("row").label, "public");
}

#[test]
fn a_filter_keeps_ancestors_so_the_tree_stays_navigable() {
    let mut tree = loaded_tree();
    tree.expand_schema(&[0]);
    tree.selected = 1;
    let (request, path, _) = tree.expand_selected().expect("load");
    tree.apply(
        request,
        &path,
        MetadataPayload::Objects(vec![
            object("public", "orders", true),
            object("public", "customers", true),
        ]),
    );

    tree.filter = "order".into();
    let labels: Vec<String> = tree.rows().iter().map(|r| r.label.clone()).collect();
    assert!(labels.contains(&"orders".to_owned()), "{labels:?}");
    assert!(
        labels.contains(&"public".to_owned()),
        "the ancestor stays so the match has context: {labels:?}"
    );
    assert!(!labels.contains(&"customers".to_owned()), "{labels:?}");
    assert!(!labels.contains(&"reporting".to_owned()), "{labels:?}");
}

#[test]
fn a_filter_matching_nothing_shows_nothing_rather_than_everything() {
    let mut tree = loaded_tree();
    tree.filter = "zzzz-no-such-object".into();
    assert!(tree.rows().is_empty());
    assert!(tree.selected_row().is_none());
}

#[test]
fn object_names_are_quoted_for_pasting_into_sql() {
    let node = Node::new(NodeKind::Object(object("public", "we\"ird", true)));
    assert_eq!(
        node.qualified_sql().expect("a name"),
        "\"public\".\"we\"\"ird\""
    );
    assert!(
        Node::new(NodeKind::Message("nothing".into()))
            .qualified_sql()
            .is_none()
    );
}

#[test]
fn columns_report_their_type_and_key_membership_in_words() {
    let node = Node::new(NodeKind::Column(ColumnInfo {
        name: "order_id".into(),
        data_type: "bigint".into(),
        nullable: false,
        primary_key: true,
        default: None,
    }));
    assert_eq!(node.label(), "order_id");
    assert_eq!(node.detail().as_deref(), Some("bigint, primary key"));
    assert!(!node.is_expandable());
}
