use super::*;

const PLAIN: &str = r#"[
  {
    "Plan": {
      "Node Type": "Nested Loop",
      "Startup Cost": 0.42,
      "Total Cost": 18.4,
      "Plan Rows": 4,
      "Plan Width": 32,
      "Plans": [
        {
          "Node Type": "Seq Scan",
          "Relation Name": "accounts",
          "Alias": "a",
          "Startup Cost": 0.0,
          "Total Cost": 24.0,
          "Plan Rows": 100,
          "Plan Width": 16,
          "Filter": "(owner = 'demo')"
        },
        {
          "Node Type": "Index Scan",
          "Relation Name": "orders",
          "Index Name": "orders_owner_idx",
          "Startup Cost": 0.2,
          "Total Cost": 5.1,
          "Plan Rows": 4,
          "Plan Width": 24,
          "Index Cond": "(owner_id = a.id)"
        }
      ]
    },
    "Planning Time": 0.31
  }
]"#;

const ANALYZED: &str = r#"[
  {
    "Plan": {
      "Node Type": "Nested Loop",
      "Startup Cost": 0.42,
      "Total Cost": 18.4,
      "Plan Rows": 4,
      "Actual Startup Time": 0.1,
      "Actual Total Time": 12.0,
      "Actual Rows": 4,
      "Actual Loops": 2,
      "Plans": [{
        "Node Type": "Seq Scan",
        "Plan Rows": 100,
        "Actual Total Time": 7.0,
        "Actual Rows": 1000,
        "Actual Loops": 2
      }]
    },
    "Planning Time": 0.31,
    "Execution Time": 25.7
  }
]"#;

#[test]
fn parses_the_postgres_envelope_and_preserves_tree_order() {
    let document = PlanDocument::parse(PLAIN, false).expect("valid plan");
    assert!(!document.analyzed);
    assert_eq!(document.node_count, 3);
    assert_eq!(document.root.node_type, "Nested Loop");
    assert_eq!(document.root.children[0].node_type, "Seq Scan");
    assert_eq!(
        document.root.children[1].index.as_deref(),
        Some("orders_owner_idx")
    );
    assert_eq!(document.planning_time_ms, Some(0.31));
    assert_eq!(document.attention_path, vec![0]);
    assert_eq!(document.attention_basis(), AttentionBasis::EstimatedCost);
}

#[test]
fn missing_metrics_stay_unavailable_and_known_facts_are_retained() {
    let document = PlanDocument::parse(
        r#"[{"Plan":{"Node Type":"Result","Plans":[],"Unknown":42,"Filter":"x"}}]"#,
        false,
    )
    .expect("valid plan");
    let node = &document.root;
    assert_eq!(node.total_cost, None);
    assert_eq!(node.actual_rows, None);
    assert_eq!(node.facts[0].label, "Filter");
    assert_eq!(node.facts[0].value, "x");
}

#[test]
fn analyzed_metrics_compare_rows_per_loop_and_mark_measured_time() {
    let document = PlanDocument::parse(ANALYZED, true).expect("valid analyzed plan");
    let root = &document.root;
    assert_eq!(root.actual_rows, Some(4.0));
    assert_eq!(root.actual_loops, Some(2.0));
    assert_eq!(root.estimate_ratio(), Some(1.0));
    assert!(root.children[0].estimate_mismatch());
    assert_eq!(document.execution_time_ms, Some(25.7));
    assert_eq!(document.attention_path, Vec::<usize>::new());
    assert_eq!(document.attention_basis(), AttentionBasis::MeasuredTime);
}

#[test]
fn zero_observed_rows_can_be_a_real_tenfold_mismatch() {
    let document = PlanDocument::parse(
        r#"[{"Plan":{"Node Type":"Index Scan","Plan Rows":20,"Actual Rows":0,"Actual Loops":1}}]"#,
        true,
    )
    .expect("valid plan");
    assert_eq!(document.root.estimate_ratio(), Some(0.0));
    assert!(document.root.estimate_mismatch());
}

#[test]
fn malformed_or_missing_roots_are_not_partial_successes() {
    for json in ["not json", "{}", "[]", "[{\"Plan\":{}}]"] {
        assert!(matches!(
            PlanDocument::parse(json, false),
            Err(PlanParseError::MalformedJson)
                | Err(PlanParseError::MissingRoot)
                | Err(PlanParseError::InvalidNode)
        ));
    }
}

#[test]
fn response_size_is_bounded_before_parsing() {
    let json = " ".repeat(MAX_PLAN_JSON_BYTES + 1);
    assert_eq!(
        PlanDocument::parse(&json, false),
        Err(PlanParseError::ResponseTooLarge)
    );
}

#[test]
fn deep_and_wide_plans_keep_a_valid_prefix_and_say_so() {
    let mut nested = String::from(r#"{"Node Type":"Result"}"#);
    for _ in 0..(MAX_PLAN_DEPTH + 2) {
        nested = format!(r#"{{"Node Type":"Result","Plans":[{nested}]}}"#);
    }
    let deep = format!(r#"[{{"Plan":{nested}}}]"#);
    let deep_document = PlanDocument::parse(&deep, false).expect("bounded deep plan");
    assert!(deep_document.truncated);
    assert!(deep_document.node_count <= MAX_PLAN_DEPTH);

    let children = (0..(MAX_PLAN_NODES + 4))
        .map(|_| "{\"Node Type\":\"Result\"}")
        .collect::<Vec<_>>()
        .join(",");
    let wide = format!("[{{\"Plan\":{{\"Node Type\":\"Append\",\"Plans\":[{children}]}}}}]");
    let wide_document = PlanDocument::parse(&wide, false).expect("bounded wide plan");
    assert!(wide_document.truncated);
    assert_eq!(wide_document.node_count, MAX_PLAN_NODES);
    assert_eq!(wide_document.root.children.len(), MAX_PLAN_NODES - 1);
}
