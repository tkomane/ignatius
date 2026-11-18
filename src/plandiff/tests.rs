use super::*;

const PG16_ANALYZED_WITH_BUFFERS_WAL_SETTINGS: &str = r#"[
  {
    "Plan": {
      "Node Type": "ModifyTable",
      "Operation": "Insert",
      "Relation Name": "orders",
      "Alias": "orders",
      "Startup Cost": 0.0,
      "Total Cost": 145.5,
      "Plan Rows": 0,
      "Plan Width": 0,
      "Actual Startup Time": 0.120,
      "Actual Total Time": 14.850,
      "Actual Rows": 0,
      "Actual Loops": 1,
      "Shared Hit Blocks": 310,
      "Shared Read Blocks": 40,
      "Shared Dirtied Blocks": 12,
      "Shared Written Blocks": 2,
      "Local Hit Blocks": 4,
      "Local Read Blocks": 1,
      "Temp Read Blocks": 0,
      "Temp Written Blocks": 0,
      "WAL Records": 55,
      "WAL FPI": 3,
      "WAL Bytes": 8192,
      "Plans": [
        {
          "Node Type": "Seq Scan",
          "Parent Relationship": "Outer",
          "Relation Name": "staging_orders",
          "Alias": "s",
          "Startup Cost": 0.0,
          "Total Cost": 120.0,
          "Plan Rows": 1000,
          "Plan Width": 48,
          "Actual Startup Time": 0.030,
          "Actual Total Time": 6.400,
          "Actual Rows": 1000,
          "Actual Loops": 1,
          "Shared Hit Blocks": 200,
          "Shared Read Blocks": 30,
          "WAL Records": 5,
          "WAL FPI": 1,
          "WAL Bytes": 1024
        }
      ]
    },
    "Planning": {
      "Shared Hit Blocks": 15,
      "Shared Read Blocks": 5,
      "Shared Dirtied Blocks": 0,
      "Shared Written Blocks": 0,
      "Local Hit Blocks": 0,
      "Local Read Blocks": 0,
      "Temp Read Blocks": 0,
      "Temp Written Blocks": 0
    },
    "Planning Time": 0.425,
    "Execution Time": 15.210,
    "Settings": {
      "work_mem": "16MB",
      "enable_seqscan": "on"
    }
  }
]"#;

const PG18_INDEX_IMPROVED_PLAN: &str = r#"[
  {
    "Plan": {
      "Node Type": "Index Scan",
      "Scan Direction": "Forward",
      "Index Name": "orders_customer_id_idx",
      "Relation Name": "orders",
      "Alias": "o",
      "Startup Cost": 0.43,
      "Total Cost": 8.45,
      "Plan Rows": 5,
      "Plan Width": 64,
      "Actual Startup Time": 0.025,
      "Actual Total Time": 0.140,
      "Actual Rows": 5,
      "Actual Loops": 1,
      "Index Cond": "(customer_id = 42)",
      "Shared Hit Blocks": 12,
      "Shared Read Blocks": 0,
      "WAL Records": 0,
      "WAL FPI": 0,
      "WAL Bytes": 0
    },
    "Planning": {
      "Shared Hit Blocks": 4,
      "Shared Read Blocks": 0
    },
    "Planning Time": 0.180,
    "Execution Time": 0.210,
    "Settings": {
      "work_mem": "64MB",
      "random_page_cost": "1.1"
    }
  }
]"#;

const PG18_SEQ_BASELINE_PLAN: &str = r#"[
  {
    "Plan": {
      "Node Type": "Seq Scan",
      "Relation Name": "orders",
      "Alias": "o",
      "Startup Cost": 0.0,
      "Total Cost": 2450.0,
      "Plan Rows": 100000,
      "Plan Width": 64,
      "Actual Startup Time": 0.040,
      "Actual Total Time": 48.600,
      "Actual Rows": 5,
      "Actual Loops": 1,
      "Filter": "(customer_id = 42)",
      "Shared Hit Blocks": 900,
      "Shared Read Blocks": 300,
      "WAL Records": 0,
      "WAL FPI": 0,
      "WAL Bytes": 0
    },
    "Planning": {
      "Shared Hit Blocks": 2,
      "Shared Read Blocks": 0
    },
    "Planning Time": 0.210,
    "Execution Time": 48.950,
    "Settings": {
      "work_mem": "4MB"
    }
  }
]"#;

#[test]
fn parses_pg16_buffers_wal_settings_and_planning_without_double_counting() {
    let snap = parse_extended_explain_json("Baseline (A)", PG16_ANALYZED_WITH_BUFFERS_WAL_SETTINGS)
        .expect("valid PG16 extended explain JSON");

    assert_eq!(snap.label, "Baseline (A)");
    assert!(snap.analyzed);
    assert_eq!(snap.planning_time_ms, Some(0.425));
    assert_eq!(snap.execution_time_ms, Some(15.210));
    assert_eq!(snap.total_cost, 145.5);
    assert_eq!(snap.startup_cost, 0.0);
    assert_eq!(snap.plan_rows, 0);
    assert_eq!(snap.actual_rows, Some(0));

    // Execution root (310) + Planning (15) = 325; child 200 is already included in root 310.
    assert_eq!(snap.shared_hit_blocks, 325);
    // Execution root (40) + Planning (5) = 45.
    assert_eq!(snap.shared_read_blocks, 45);
    assert_eq!(snap.shared_dirtied_blocks, 12);
    assert_eq!(snap.shared_written_blocks, 2);
    assert_eq!(snap.local_hit_blocks, 4);
    assert_eq!(snap.local_read_blocks, 1);
    assert_eq!(snap.temp_read_blocks, 0);
    assert_eq!(snap.temp_written_blocks, 0);

    // WAL counters come from inclusive root (55, 3, 8192) without double-counting child (5, 1, 1024).
    assert_eq!(snap.wal_records, 55);
    assert_eq!(snap.wal_fpi, 3);
    assert_eq!(snap.wal_bytes, 8192);

    // Hit ratio = 325 / (325 + 45) * 100.0 = 87.8378...
    let ratio = snap.buffer_hit_ratio_pct.expect("hit ratio computed");
    assert!((ratio - (325.0 / 370.0) * 100.0).abs() < 1e-6);

    assert_eq!(
        snap.settings.get("work_mem").map(String::as_str),
        Some("16MB")
    );
    assert_eq!(
        snap.settings.get("enable_seqscan").map(String::as_str),
        Some("on")
    );

    assert_eq!(snap.nodes.len(), 2);
    assert_eq!(snap.nodes[0].path, "0");
    assert_eq!(snap.nodes[0].depth, 0);
    assert_eq!(snap.nodes[0].node_type, "ModifyTable");
    assert_eq!(snap.nodes[0].wal_bytes, 8192);
    assert_eq!(snap.nodes[1].path, "0.0");
    assert_eq!(snap.nodes[1].depth, 1);
    assert_eq!(snap.nodes[1].node_type, "Seq Scan");
    assert_eq!(
        snap.nodes[1].relation_name.as_deref(),
        Some("staging_orders")
    );
    assert_eq!(snap.nodes[1].shared_hit_blocks, 200);
    assert_eq!(snap.nodes[1].shared_read_blocks, 30);
    assert_eq!(snap.nodes[1].wal_bytes, 1024);
}

#[test]
fn accumulates_leaf_counters_when_parent_omits_totals() {
    let json = r#"[{
      "Plan": {
        "Node Type": "Hash Join",
        "Startup Cost": 10.0,
        "Total Cost": 50.0,
        "Plan Rows": 100,
        "Plans": [
          {
            "Node Type": "Seq Scan",
            "Relation Name": "a",
            "Total Cost": 20.0,
            "Plan Rows": 100,
            "Shared Hit Blocks": 30,
            "Shared Read Blocks": 10,
            "WAL Records": 4,
            "WAL Bytes": 512
          },
          {
            "Node Type": "Hash",
            "Total Cost": 15.0,
            "Plan Rows": 50,
            "Temp Written Blocks": 12,
            "Plans": [
              {
                "Node Type": "Seq Scan",
                "Relation Name": "b",
                "Total Cost": 15.0,
                "Plan Rows": 50,
                "Shared Hit Blocks": 20,
                "Shared Read Blocks": 0,
                "WAL Records": 2,
                "WAL Bytes": 256
              }
            ]
          }
        ]
      }
    }]"#;

    let snap = parse_extended_explain_json("Leaf Accum", json).expect("valid tree");
    assert_eq!(snap.shared_hit_blocks, 50);
    assert_eq!(snap.shared_read_blocks, 10);
    assert_eq!(snap.temp_written_blocks, 12);
    assert_eq!(snap.wal_records, 6);
    assert_eq!(snap.wal_bytes, 768);
    assert_eq!(snap.nodes.len(), 4);
    assert_eq!(snap.nodes[0].path, "0");
    assert_eq!(snap.nodes[1].path, "0.0");
    assert_eq!(snap.nodes[2].path, "0.1");
    assert_eq!(snap.nodes[3].path, "0.1.0");
    assert_eq!(snap.nodes[3].depth, 2);
}

#[test]
fn parses_estimate_only_plan_without_buffers_and_bounds_label() {
    let long_sql = format!("SELECT * FROM orders WHERE note = '{}'", "x".repeat(200));
    let json = r#"[{
      "Plan": {
        "Node Type": "Seq Scan",
        "Relation Name": "orders",
        "Startup Cost": 0.0,
        "Total Cost": 25.5,
        "Plan Rows": 42.0
      },
      "Planning Time": 0.15,
      "Settings": { "enable_hashjoin": "off" }
    }]"#;

    let snap = parse_extended_explain_json(&long_sql, json).expect("valid estimate");
    assert!(!snap.analyzed);
    assert_eq!(snap.execution_time_ms, None);
    assert_eq!(snap.buffer_hit_ratio_pct, None);
    assert_eq!(snap.plan_rows, 42);
    assert!(snap.label.len() <= MAX_LABEL_CHARS);
    assert!(snap.label.ends_with("..."));
}

#[test]
fn enforces_response_too_large_and_malformed_inputs() {
    let oversized = " ".repeat(MAX_PLAN_JSON_BYTES + 1);
    let err = parse_extended_explain_json("TooBig", &oversized)
        .expect_err("payload over MAX_PLAN_JSON_BYTES must fail");
    assert_eq!(err.kind, DiagnosticKind::Query);
    assert!(
        err.technical
            .iter()
            .any(|f| f.label == "PlanParseError" && f.value == "ResponseTooLarge")
    );

    let malformed = parse_extended_explain_json("BadJson", "{not valid json")
        .expect_err("malformed json must fail");
    assert_eq!(malformed.kind, DiagnosticKind::Query);
    assert!(
        malformed
            .technical
            .iter()
            .any(|f| f.label == "PlanParseError" && f.value == "MalformedJson")
    );

    let missing_root = parse_extended_explain_json("NoPlan", r#"[{"Planning Time": 1.0}]"#)
        .expect_err("missing Plan root must fail");
    assert_eq!(missing_root.kind, DiagnosticKind::Query);
    assert!(
        missing_root
            .technical
            .iter()
            .any(|f| f.label == "PlanParseError" && f.value == "MissingRoot")
    );

    let blank_node = parse_extended_explain_json("BlankNode", r#"[{"Plan":{"Node Type":"   "}}]"#)
        .expect_err("blank Node Type must fail");
    assert_eq!(blank_node.kind, DiagnosticKind::Query);
    assert!(
        blank_node
            .technical
            .iter()
            .any(|f| f.label == "PlanParseError" && f.value == "InvalidNode")
    );
}

#[test]
fn bounds_flattened_nodes_to_max_plan_nodes() {
    let mut children = String::new();
    for i in 0..600 {
        if i > 0 {
            children.push(',');
        }
        children.push_str(
            r#"{"Node Type":"Result","Startup Cost":0.0,"Total Cost":1.0,"Plan Rows":1}"#,
        );
    }
    let payload = format!(
        r#"[{{"Plan":{{"Node Type":"Append","Startup Cost":0.0,"Total Cost":600.0,"Plan Rows":600,"Plans":[{children}]}}}}]"#
    );
    let snap = parse_extended_explain_json("WideTree", &payload).expect("valid wide plan");
    assert_eq!(snap.nodes.len(), MAX_PLAN_NODES);
}

#[test]
fn detects_seq_to_index_scan_improvement_and_settings_diff() {
    let baseline =
        parse_extended_explain_json("Baseline (A)", PG18_SEQ_BASELINE_PLAN).expect("baseline");
    let candidate =
        parse_extended_explain_json("Candidate (B)", PG18_INDEX_IMPROVED_PLAN).expect("candidate");

    let report = diff_plans(baseline, candidate);
    assert_eq!(report.verdict, DiffVerdict::Improvement);
    assert!(report.total_cost_delta < -2400.0);
    assert!(report.execution_time_delta_ms.expect("exec delta") < -48.0);
    assert_eq!(report.shared_read_delta, -300);
    assert!(
        report
            .node_changes
            .iter()
            .any(|c| c.severity == DiffVerdict::Improvement
                && c.description.contains("Seq Scan -> Index Scan"))
    );
    assert_eq!(report.setting_changes.len(), 2);
    assert_eq!(report.setting_changes[0].name, "random_page_cost");
    assert_eq!(report.setting_changes[0].baseline, None);
    assert_eq!(report.setting_changes[0].candidate.as_deref(), Some("1.1"));
    assert_eq!(report.setting_changes[1].name, "work_mem");
    assert_eq!(report.setting_changes[1].baseline.as_deref(), Some("4MB"));
    assert_eq!(report.setting_changes[1].candidate.as_deref(), Some("64MB"));
}

#[test]
fn detects_index_to_seq_regression_and_sort_temp_spill() {
    let baseline =
        parse_extended_explain_json("Baseline (A)", PG18_INDEX_IMPROVED_PLAN).expect("baseline");
    let spilled_candidate_json = r#"[{
      "Plan": {
        "Node Type": "Sort",
        "Startup Cost": 3200.0,
        "Total Cost": 3500.0,
        "Plan Rows": 100000,
        "Actual Total Time": 95.4,
        "Actual Rows": 100000,
        "Temp Written Blocks": 420,
        "Shared Hit Blocks": 900,
        "Shared Read Blocks": 300,
        "Plans": [
          {
            "Node Type": "Seq Scan",
            "Relation Name": "orders",
            "Startup Cost": 0.0,
            "Total Cost": 2450.0,
            "Plan Rows": 100000,
            "Actual Total Time": 48.6,
            "Actual Rows": 100000,
            "Shared Hit Blocks": 900,
            "Shared Read Blocks": 300
          }
        ]
      },
      "Planning Time": 0.25,
      "Execution Time": 96.1
    }]"#;
    let candidate =
        parse_extended_explain_json("Candidate (B)", spilled_candidate_json).expect("candidate");

    let report = diff_plans(baseline, candidate);
    assert_eq!(report.verdict, DiffVerdict::Regression);
    assert!(report.total_cost_delta > 3000.0);
    assert!(report.execution_time_delta_ms.expect("exec delta") > 90.0);
    assert_eq!(report.temp_written_delta, 420);
    assert!(
        report
            .node_changes
            .iter()
            .any(|c| c.severity == DiffVerdict::Regression
                && c.description.contains("Index Scan -> Seq Scan"))
    );
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.contains("temp") || f.contains("Temp") || f.contains("spill"))
    );
}

#[test]
fn detects_mixed_and_unchanged_verdicts() {
    let baseline =
        parse_extended_explain_json("Baseline (A)", PG18_SEQ_BASELINE_PLAN).expect("baseline");
    let identical =
        parse_extended_explain_json("Candidate (B)", PG18_SEQ_BASELINE_PLAN).expect("identical");

    let unchanged_report = diff_plans(baseline.clone(), identical);
    assert_eq!(unchanged_report.verdict, DiffVerdict::Unchanged);
    assert_eq!(unchanged_report.total_cost_delta, 0.0);
    assert_eq!(unchanged_report.execution_time_delta_ms, Some(0.0));
    assert!(unchanged_report.setting_changes.is_empty());
    assert!(unchanged_report.node_changes.is_empty());

    // Faster execution time via index scan, but introduces a Materialize temp spill.
    let mixed_candidate_json = r#"[{
      "Plan": {
        "Node Type": "Materialize",
        "Startup Cost": 0.43,
        "Total Cost": 180.0,
        "Plan Rows": 5000,
        "Actual Total Time": 12.0,
        "Actual Rows": 5000,
        "Temp Written Blocks": 85,
        "Shared Hit Blocks": 120,
        "Shared Read Blocks": 10,
        "Plans": [
          {
            "Node Type": "Index Scan",
            "Relation Name": "orders",
            "Index Name": "orders_customer_id_idx",
            "Total Cost": 150.0,
            "Plan Rows": 5000,
            "Actual Total Time": 8.0,
            "Actual Rows": 5000
          }
        ]
      },
      "Execution Time": 12.5
    }]"#;
    let mixed_candidate =
        parse_extended_explain_json("Candidate (B)", mixed_candidate_json).expect("mixed");
    let mixed_report = diff_plans(baseline, mixed_candidate);
    assert_eq!(mixed_report.verdict, DiffVerdict::Mixed);
    assert_eq!(mixed_report.temp_written_delta, 85);
}

#[test]
fn renders_strictly_plain_ascii_table_plain_and_json() {
    let baseline =
        parse_extended_explain_json("SELECT \x1b[31m*\x1b[0m FROM café", PG18_SEQ_BASELINE_PLAN)
            .expect("baseline");
    let candidate = parse_extended_explain_json("Candidate \u{202E}B", PG18_INDEX_IMPROVED_PLAN)
        .expect("candidate");

    let report = diff_plans(baseline, candidate);

    let table = report.render_table();
    assert!(table.is_ascii(), "render_table must be strictly ASCII");
    assert!(table.contains("PLAN DIFF VERDICT: Improvement"));
    assert!(table.contains("METRIC"));
    assert!(table.contains("Settings Diff"));
    assert!(table.contains("Node Changes"));
    assert!(table.contains("work_mem"));
    assert!(!table.contains('\x1b'));

    let plain = report.render_plain();
    assert!(plain.is_ascii(), "render_plain must be strictly ASCII");
    assert!(plain.contains("verdict=Improvement"));
    assert!(plain.contains("setting: work_mem a=4MB b=64MB"));

    let json = report.render_json();
    assert!(json.is_ascii(), "render_json must be strictly ASCII");
    let roundtrip: PlanDiffReport =
        serde_json::from_str(&json).expect("render_json must be valid deserializable JSON");
    assert_eq!(roundtrip.verdict, DiffVerdict::Improvement);
    assert_eq!(roundtrip.setting_changes.len(), 2);
}
