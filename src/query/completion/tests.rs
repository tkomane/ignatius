use super::*;

fn catalog() -> CompletionCatalog {
    CompletionCatalog {
        objects: vec![
            CatalogObject {
                kind: CatalogObjectKind::Table,
                schema: "public".into(),
                name: "orders".into(),
                readable: true,
                detail: None,
            },
            CatalogObject {
                kind: CatalogObjectKind::Table,
                schema: "public".into(),
                name: "order_items".into(),
                readable: true,
                detail: None,
            },
            CatalogObject {
                kind: CatalogObjectKind::Table,
                schema: "sales".into(),
                name: "orders".into(),
                readable: false,
                detail: None,
            },
            CatalogObject {
                kind: CatalogObjectKind::Table,
                schema: "public".into(),
                name: "we\"ird name".into(),
                readable: true,
                detail: None,
            },
        ],
        relations: vec![CatalogRelation {
            schema: "public".into(),
            name: "orders".into(),
            columns: vec![
                CatalogColumn {
                    name: "order_id".into(),
                    data_type: "bigint".into(),
                },
                CatalogColumn {
                    name: "Total Value".into(),
                    data_type: "numeric(12,2)".into(),
                },
            ],
        }],
    }
}

#[test]
fn relation_completion_is_prefix_ordered_and_bounded() {
    let sql = "SELECT * FROM ord";
    let result = complete(sql, sql.len(), Some(&catalog()));
    assert_eq!(result.scope, CompletionScope::Relations);
    assert_eq!(result.prefix, "ord");
    assert_eq!(result.candidates[0].label, "order_items");
    assert_eq!(result.candidates[1].label, "orders");
    assert!(result.candidates[0].kind != CandidateKind::Keyword);
    assert_eq!(
        result
            .candidates
            .last()
            .map(|candidate| candidate.label.as_str()),
        Some("ORDER")
    );
}

#[test]
fn alias_completion_offers_only_the_alias_relation_columns() {
    let sql = "SELECT * FROM orders o WHERE o.";
    let result = complete(sql, sql.len(), Some(&catalog()));
    assert_eq!(result.scope, CompletionScope::QualifiedColumns("o".into()));
    assert_eq!(
        result
            .candidates
            .iter()
            .map(|candidate| candidate.label.as_str())
            .collect::<Vec<_>>(),
        vec!["order_id", "Total Value"]
    );
    assert_eq!(result.candidates[1].insert_text, "\"Total Value\"");
}

#[test]
fn schema_completion_restricts_objects_to_that_schema() {
    let sql = "SELECT * FROM public.or";
    let result = complete(sql, sql.len(), Some(&catalog()));
    assert_eq!(result.scope, CompletionScope::Schema("public".into()));
    assert!(
        result
            .candidates
            .iter()
            .all(|candidate| candidate.schema.as_deref() == Some("public"))
    );
}

#[test]
fn a_relation_alias_shadows_a_schema_in_expressions() {
    let catalog = catalog();
    for (sql, qualifier) in [
        (
            "SELECT * FROM public.orders AS sales WHERE sales.To",
            "sales",
        ),
        (
            "SELECT * FROM public.orders AS sales WHERE coalesce(NULL, sales.To",
            "sales",
        ),
        (
            "SELECT * FROM public.orders AS sales JOIN public.orders o ON ARRAY[NULL, sales.To",
            "sales",
        ),
        (
            "SELECT * FROM public.orders AS SALES WHERE sales.To",
            "sales",
        ),
        (
            "SELECT * FROM public.orders AS sales WHERE SALES.To",
            "SALES",
        ),
        (
            r#"SELECT * FROM public.orders AS SALES WHERE "sales".To"#,
            "sales",
        ),
        (
            r#"SELECT * FROM public.orders AS "Sales" WHERE "Sales".To"#,
            "Sales",
        ),
    ] {
        let result = complete(sql, sql.len(), Some(&catalog));
        assert_eq!(
            result.scope,
            CompletionScope::QualifiedColumns(qualifier.into()),
            "{sql}"
        );
        assert_eq!(&sql[result.replacement], "To", "{sql}");
        assert_eq!(result.total_count, 2, "{sql}");
        assert_eq!(result.matching_count, 1, "{sql}");
        let [column] = result.candidates.as_slice() else {
            panic!("expected the aliased column: {sql}");
        };
        assert_eq!(column.label, "Total Value");
        assert_eq!(column.insert_text, "\"Total Value\"");
        assert_eq!(column.kind, CandidateKind::Column);
        assert_eq!(column.schema.as_deref(), Some("public"));
        assert_eq!(column.source.as_deref(), Some(qualifier));
        assert_eq!(column.detail.as_deref(), Some("numeric(12,2)"));
    }
}

#[test]
fn known_aliases_without_catalogue_columns_keep_empty_column_scope() {
    let catalog = catalog();
    for sql in [
        "SELECT * FROM public.unlisted AS sales WHERE sales.",
        "SELECT * FROM (/* scoped */ SELECT 1 AS unrelated) AS sales WHERE sales.",
        "WITH orders AS (SELECT 1 AS unrelated) SELECT * FROM orders AS sales WHERE sales.",
    ] {
        let result = complete(sql, sql.len(), Some(&catalog));
        assert_eq!(
            result.scope,
            CompletionScope::QualifiedColumns("sales".into()),
            "{sql}"
        );
        assert_eq!(result.matching_count, 0, "{sql}");
        assert!(result.candidates.is_empty(), "{sql}");
    }
}

#[test]
fn a_schema_remains_available_where_no_matching_relation_is_visible() {
    let catalog = catalog();
    for sql in [
        "SELECT sales.or",
        "SELECT * FROM public.sales AS o WHERE sales.or",
        "SELECT (SELECT 1 FROM public.orders AS sales) WHERE sales.or",
        "SELECT (/* scoped */ SELECT 1 FROM public.orders AS sales) WHERE sales.or",
        "SELECT (-- scoped\nWITH inner_cte AS (SELECT 1) SELECT 1 FROM public.orders AS sales) WHERE sales.or",
        r#"SELECT * FROM public.orders AS "Sales" WHERE sales.or"#,
        r#"SELECT * FROM public."Sales" WHERE sales.or"#,
    ] {
        let result = complete(sql, sql.len(), Some(&catalog));
        assert_eq!(
            result.scope,
            CompletionScope::Schema("sales".into()),
            "{sql}"
        );
        let [object] = result.candidates.as_slice() else {
            panic!("expected one schema object: {sql}");
        };
        assert_eq!(object.label, "orders");
        assert_eq!(object.kind, CandidateKind::Object(CatalogObjectKind::Table));
    }
}

#[test]
fn a_schema_wins_in_qualified_relation_name_slots() {
    let catalog = catalog();
    for sql in [
        "SELECT * FROM sales.or",
        "SELECT * FROM public.orders AS sales JOIN sales.or",
        "SELECT * FROM public.orders AS sales JOIN LATERAL /* source */ sales.or",
        "SELECT * FROM public.orders AS sales JOIN ONLY (sales.or",
        "SELECT * FROM public.orders AS sales, sales.or",
        "SELECT * FROM public.orders AS sales JOIN public.orders o ON (TRUE), sales.or",
    ] {
        let result = complete(sql, sql.len(), Some(&catalog));
        assert_eq!(
            result.scope,
            CompletionScope::Schema("sales".into()),
            "{sql}"
        );
        assert_eq!(&sql[result.replacement], "or", "{sql}");
        let [object] = result.candidates.as_slice() else {
            panic!("expected one schema object: {sql}");
        };
        assert_eq!(object.label, "orders");
        assert_eq!(object.kind, CandidateKind::Object(CatalogObjectKind::Table));
    }
}

#[test]
fn strings_and_comments_are_hard_boundaries() {
    let string = "SELECT 'ord";
    let comment = "SELECT 1 -- ord";
    let block = "SELECT /* ord";
    assert!(
        complete(string, string.len(), Some(&catalog()))
            .candidates
            .is_empty()
    );
    assert!(
        complete(comment, comment.len(), Some(&catalog()))
            .candidates
            .is_empty()
    );
    assert!(
        complete(block, block.len(), Some(&catalog()))
            .candidates
            .is_empty()
    );
}

#[test]
fn ctes_are_offered_even_when_the_catalogue_has_no_cte_row() {
    let sql = "WITH recent_orders AS (SELECT 1) SELECT * FROM rec";
    let result = complete(sql, sql.len(), Some(&catalog()));
    assert!(result.candidates.iter().any(|candidate| {
        candidate.label == "recent_orders" && candidate.kind == CandidateKind::Cte
    }));
}

#[test]
fn hostile_names_are_quoted_for_acceptance() {
    let sql = "SELECT * FROM public.we";
    let result = complete(sql, sql.len(), Some(&catalog()));
    let candidate = result
        .candidates
        .iter()
        .find(|candidate| candidate.label == "we\"ird name")
        .expect("hostile name should be offered");
    assert_eq!(candidate.insert_text, "\"we\"\"ird name\"");
}

#[test]
fn no_catalogue_still_offers_keywords_without_claiming_objects() {
    let result = complete("SEL", 3, None);
    assert_eq!(result.scope, CompletionScope::Mixed);
    assert!(
        result
            .candidates
            .iter()
            .any(|candidate| candidate.label == "SELECT")
    );
    assert_eq!(result.total_count, highlight::KEYWORDS.len());
}

#[test]
fn a_prefix_longer_than_the_visible_limit_reports_the_true_match_count() {
    let mut catalog = catalog();
    for index in 0..20 {
        catalog.objects.push(CatalogObject {
            kind: CatalogObjectKind::Table,
            schema: "public".into(),
            name: format!("thing_{index:02}"),
            readable: true,
            detail: None,
        });
    }
    let sql = "SELECT * FROM thing_";
    let result = complete(sql, sql.len(), Some(&catalog));
    assert_eq!(result.matching_count, 20);
    assert_eq!(result.candidates.len(), MAX_VISIBLE_CANDIDATES);
}

#[test]
fn an_incomplete_previous_statement_does_not_make_completion_panic() {
    let sql = "SELECT ( FROM orders; SELECT * FROM ord";
    let result = complete(sql, sql.len(), Some(&catalog()));
    assert_eq!(result.scope, CompletionScope::Relations);
    assert!(
        result
            .candidates
            .iter()
            .any(|candidate| candidate.label == "orders")
    );
}

#[test]
fn context_ranges_are_utf8_boundaries() {
    let sql = "SELECT café";
    let result = complete(sql, sql.len(), None);
    assert!(sql.is_char_boundary(result.replacement.start));
    assert!(sql.is_char_boundary(result.replacement.end));
    assert_eq!(result.prefix, "café");
}
