use super::*;
use crate::query::result::{ResultRetention, ResultSet};
use crate::query::value::Cell;

fn make_result(columns: &[&str], rows: Vec<Vec<Cell>>) -> ResultSet {
    let mut set = ResultSet::new(columns.iter().map(|s| (*s).to_owned()).collect(), 1000);
    for row in rows {
        set.push(row);
    }
    set
}

#[test]
fn keyed_alignment_detects_added_removed_changed_and_unchanged_rows() {
    let base_set = make_result(
        &["id", "name", "status"],
        vec![
            vec![
                Cell::Text("1".into()),
                Cell::Text("alice".into()),
                Cell::Text("active".into()),
            ],
            vec![
                Cell::Text("2".into()),
                Cell::Text("bob".into()),
                Cell::Text("pending".into()),
            ],
            vec![
                Cell::Text("3".into()),
                Cell::Text("carol".into()),
                Cell::Null,
            ],
        ],
    );
    let pinned = PinnedResult::capture(
        &base_set,
        Some("SELECT id, name, status FROM users ORDER BY id"),
        Some("id".into()),
        Some("local".into()),
    )
    .expect("capture baseline");

    let curr_set = make_result(
        &["id", "name", "status"],
        vec![
            vec![
                Cell::Text("1".into()),
                Cell::Text("alice".into()),
                Cell::Text("active".into()),
            ],
            vec![
                Cell::Text("3".into()),
                Cell::Text("carol".into()),
                Cell::Text("".into()),
            ],
            vec![
                Cell::Text("4".into()),
                Cell::Text("dave".into()),
                Cell::Text("new".into()),
            ],
        ],
    );

    let diff = diff_results(&pinned, &curr_set, Some("id"));
    assert_eq!(
        diff.alignment,
        AlignmentMode::Keyed {
            column: "id".into()
        }
    );
    assert_eq!(diff.unchanged_count, 1);
    assert_eq!(diff.removed_count, 1);
    assert_eq!(diff.changed_count, 1);
    assert_eq!(diff.added_count, 1);
    assert!(diff.has_differences());
    assert!(!diff.schema_delta.has_changes());

    // NULL vs empty string must be classified as changed, never equal.
    let changed = diff
        .entries
        .iter()
        .find(|e| matches!(e.kind, DiffRowKind::Changed { .. }))
        .expect("changed row");
    assert_eq!(changed.key_value.as_deref(), Some("3"));
    let DiffRowKind::Changed { cell_diffs, .. } = &changed.kind else {
        unreachable!();
    };
    assert_eq!(cell_diffs.len(), 1);
    assert_eq!(cell_diffs[0].column, "status");
    assert_eq!(cell_diffs[0].baseline, Cell::Null);
    assert_eq!(cell_diffs[0].current, Cell::Text(String::new()));
}

#[test]
fn duplicate_or_null_key_falls_back_explicitly_to_positional_alignment() {
    let base_set = make_result(
        &["id", "val"],
        vec![
            vec![Cell::Text("1".into()), Cell::Text("a".into())],
            vec![Cell::Text("1".into()), Cell::Text("b".into())],
        ],
    );
    let pinned = PinnedResult::capture(&base_set, None, None, None).unwrap();
    let curr_set = make_result(
        &["id", "val"],
        vec![
            vec![Cell::Text("1".into()), Cell::Text("a".into())],
            vec![Cell::Text("2".into()), Cell::Text("c".into())],
        ],
    );

    let dup_diff = diff_results(&pinned, &curr_set, Some("id"));
    assert!(matches!(
        dup_diff.alignment,
        AlignmentMode::Positional { reason: Some(ref r) } if r.contains("duplicate")
    ));
    assert_eq!(dup_diff.unchanged_count, 1);
    assert_eq!(dup_diff.changed_count, 1);

    let null_base = make_result(
        &["id", "val"],
        vec![vec![Cell::Null, Cell::Text("a".into())]],
    );
    let null_pinned = PinnedResult::capture(&null_base, None, None, None).unwrap();
    let null_diff = diff_results(&null_pinned, &curr_set, Some("id"));
    assert!(matches!(
        null_diff.alignment,
        AlignmentMode::Positional { reason: Some(ref r) } if r.contains("NULL")
    ));

    let missing_diff = diff_results(&pinned, &curr_set, Some("nonexistent"));
    assert!(matches!(
        missing_diff.alignment,
        AlignmentMode::Positional { reason: Some(ref r) } if r.contains("missing")
    ));
}

#[test]
fn schema_delta_tracks_added_and_removed_columns() {
    let base_set = make_result(
        &["id", "legacy_col"],
        vec![vec![Cell::Text("1".into()), Cell::Text("old".into())]],
    );
    let pinned =
        PinnedResult::capture(&base_set, Some("SELECT id, legacy_col FROM t"), None, None).unwrap();
    let curr_set = make_result(
        &["id", "new_col"],
        vec![vec![Cell::Text("1".into()), Cell::Text("fresh".into())]],
    );

    let diff = diff_results(&pinned, &curr_set, Some("id"));
    assert_eq!(diff.schema_delta.shared_columns, vec!["id"]);
    assert_eq!(diff.schema_delta.removed_columns, vec!["legacy_col"]);
    assert_eq!(diff.schema_delta.added_columns, vec!["new_col"]);
    assert!(diff.has_differences());
    assert_eq!(diff.candidate_key_columns, vec!["id"]);
}

#[test]
fn unavailable_result_cannot_be_pinned() {
    let unavailable = ResultSet::unavailable(3, 100);
    let err = PinnedResult::capture(&unavailable, None, None, None).unwrap_err();
    assert!(err.contains("unavailable"));
}

#[test]
fn formatters_emit_text_markdown_and_schema_v1_json() {
    let base_set = make_result(
        &["id", "amount"],
        vec![
            vec![Cell::Text("1".into()), Cell::Text("100".into())],
            vec![Cell::Text("2".into()), Cell::Null],
        ],
    );
    let pinned =
        PinnedResult::capture(&base_set, Some("SELECT id, amount FROM orders"), None, None)
            .unwrap();
    let curr_set = make_result(
        &["id", "amount"],
        vec![
            vec![Cell::Text("1".into()), Cell::Text("150".into())],
            vec![Cell::Text("3".into()), Cell::Text("300".into())],
        ],
    );

    let diff = diff_results(&pinned, &curr_set, Some("id"));

    let text = diff.render_text();
    assert!(text.contains("[+]"));
    assert!(text.contains("[-]"));
    assert!(text.contains("[~]"));
    assert!(text.contains("keyed (id)"));
    assert!(!text.contains('\u{2014}'));
    assert!(!text.contains('\u{2013}'));

    let md = diff.render_markdown();
    assert!(md.contains("| `[~]` |"));
    assert!(md.contains("| `[-]` |"));
    assert!(md.contains("| `[+]` |"));

    let json_str = diff.render_json();
    let parsed: serde_json::Value = serde_json::from_str(&json_str).expect("valid diff JSON");
    assert_eq!(parsed["schema_version"], 1);
    assert_eq!(parsed["counts"]["changed"], 1);
    assert_eq!(parsed["counts"]["removed"], 1);
    assert_eq!(parsed["counts"]["added"], 1);
    assert_eq!(parsed["counts"]["unchanged"], 0);
}

#[test]
fn parses_ignatius_json_export_array_of_objects_and_ndjson() {
    let ignatius_obj = r#"{"columns":["id","name"],"rows":[{"id":"1","name":"a"},{"id":"2","name":null}],"rows_seen":2,"truncated":false}"#;
    let parsed_obj = parse_result_str(ignatius_obj, "obj.json").expect("ignatius obj");
    assert_eq!(parsed_obj.columns, vec!["id", "name"]);
    assert_eq!(parsed_obj.rows.len(), 2);
    assert_eq!(parsed_obj.rows[1][1], Cell::Null);

    let ignatius_arr = format!("[{ignatius_obj}]");
    let parsed_arr = parse_result_str(&ignatius_arr, "arr.json").expect("ignatius export array");
    assert_eq!(parsed_arr.columns, vec!["id", "name"]);
    assert_eq!(parsed_arr.rows.len(), 2);

    let row_objects = r#"[{"id": 10, "ok": true}, {"id": 20, "note": null}]"#;
    let parsed_rows = parse_result_str(row_objects, "rows.json").expect("row objects");
    assert_eq!(parsed_rows.columns, vec!["id", "ok", "note"]);
    assert_eq!(parsed_rows.rows[0][1], Cell::Text("true".into()));
    assert_eq!(parsed_rows.rows[1][2], Cell::Null);

    let ndjson = "{\"id\": 1, \"name\": \"x\"}\n{\"id\": 2, \"name\": \"y\"}\n";
    let parsed_ndjson = parse_result_str(ndjson, "rows.ndjson").expect("ndjson");
    assert_eq!(parsed_ndjson.rows.len(), 2);
    assert_eq!(parsed_ndjson.rows[1][1], Cell::Text("y".into()));
}

#[test]
fn rejects_malformed_deeply_nested_and_multi_result_json() {
    let malformed = parse_result_str("{bad json", "bad.json").unwrap_err();
    assert_eq!(malformed.kind, DiagnosticKind::Usage);

    let deep = format!("{}{}", "[".repeat(80), "]".repeat(80));
    let deep_err = parse_result_str(&deep, "deep.json").unwrap_err();
    assert_eq!(deep_err.kind, DiagnosticKind::Usage);

    let multi = r#"[{"columns":["a"],"rows":[]},{"columns":["b"],"rows":[]}]"#;
    let multi_err = parse_result_str(multi, "multi.json").unwrap_err();
    assert_eq!(multi_err.kind, DiagnosticKind::Usage);
}

#[test]
fn pinned_capture_marks_truncated_when_source_was_truncated() {
    let mut set = ResultSet::new(vec!["id".into()], 1);
    assert!(set.push(vec![Cell::Text("1".into())]));
    assert!(!set.push(vec![Cell::Text("2".into())]));
    assert_eq!(set.retention, ResultRetention::RowCap);

    let pinned = PinnedResult::capture(&set, None, None, None).unwrap();
    assert!(pinned.truncated);
    assert_eq!(pinned.retained_rows, 1);
    assert_eq!(pinned.rows_seen, 2);
}

#[cfg(unix)]
#[test]
fn parse_result_file_rejects_symlinks_and_missing_files() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("ignatius-result-diff-test-{unique}"));
    std::fs::create_dir_all(&dir).unwrap();
    let real = dir.join("real.json");
    let link = dir.join("link.json");
    std::fs::write(&real, r#"[{"id":1}]"#).unwrap();
    std::os::unix::fs::symlink(&real, &link).unwrap();

    assert!(parse_result_file(&real).is_ok());
    let link_err = parse_result_file(&link).unwrap_err();
    assert_eq!(link_err.kind, DiagnosticKind::Usage);

    let missing_err = parse_result_file(&dir.join("missing.json")).unwrap_err();
    assert_eq!(missing_err.kind, DiagnosticKind::Usage);

    let _ = std::fs::remove_dir_all(&dir);
}
