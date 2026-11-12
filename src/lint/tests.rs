//! Sidecar tests for the SQL linter.

use super::*;

fn codes(sql: &str) -> Vec<String> {
    let report = lint_source(None, sql);
    report
        .findings
        .iter()
        .map(|finding| finding.code.clone())
        .collect()
}

fn assert_has(sql: &str, code: LintCode) {
    let report = lint_source(None, sql);
    assert!(
        report.findings.iter().any(|f| f.code == code.as_str()),
        "expected {code:?} in {sql:?}, got {:?}",
        codes(sql)
    );
}

fn assert_absent(sql: &str, code: LintCode) {
    let report = lint_source(None, sql);
    assert!(
        report.findings.iter().all(|f| f.code != code.as_str()),
        "did not expect {code:?} in {sql:?}, got {:?}",
        codes(sql)
    );
}

#[test]
fn every_error_rule_fires_on_its_signature() {
    assert_has("DELETE FROM orders;", LintCode::DeleteWithoutWhere);
    assert_has(
        "UPDATE orders SET note = 'x';",
        LintCode::UpdateWithoutWhere,
    );
    assert_has("TRUNCATE orders;", LintCode::TruncateTable);
}

#[test]
fn every_warning_rule_fires_on_its_signature() {
    assert_has("DROP TABLE orders;", LintCode::DropObject);
    assert_has("GRANT ALL ON orders TO app;", LintCode::GrantAll);
    assert_has(
        "CREATE INDEX orders_note ON orders(note);",
        LintCode::CreateIndexWithoutConcurrently,
    );
    assert_has(
        "REINDEX TABLE orders;",
        LintCode::ReindexWithoutConcurrently,
    );
    assert_has("VACUUM FULL orders;", LintCode::VacuumFull);
    assert_has("VACUUM (FULL, ANALYZE) orders;", LintCode::VacuumFull);
    assert_has(
        "SELECT id FROM orders WHERE note LIKE '%x';",
        LintCode::LeadingWildcardLike,
    );
    assert_has(
        "SELECT id FROM orders WHERE LOWER(note) = 'x';",
        LintCode::NonSargablePredicate,
    );
    assert_has(
        "SELECT a FROM orders, customers;",
        LintCode::ImplicitCrossJoin,
    );
}

#[test]
fn info_rules_fire_without_turning_style_into_an_error() {
    let report = lint_source(None, "SELECT * FROM orders;");
    assert!(
        report.findings.iter().any(|f| f.code == "select-star"),
        "{:?}",
        report.findings
    );
    assert_eq!(
        report
            .findings
            .iter()
            .find(|f| f.code == "select-star")
            .unwrap()
            .severity,
        LintSeverity::Info
    );

    assert_has("CREATE TABLE t (id int);", LintCode::MissingTransaction);
}

#[test]
fn an_explicit_transaction_silences_the_missing_transaction_rule() {
    assert_absent(
        "BEGIN; CREATE TABLE t (id int); COMMIT;",
        LintCode::MissingTransaction,
    );
}

#[test]
fn where_clauses_and_concurrent_indexes_are_not_flagged() {
    assert_absent(
        "DELETE FROM orders WHERE id = 1;",
        LintCode::DeleteWithoutWhere,
    );
    assert_absent(
        "UPDATE orders SET note = 'x' WHERE id = 1;",
        LintCode::UpdateWithoutWhere,
    );
    assert_absent(
        "CREATE INDEX CONCURRENTLY orders_note ON orders(note);",
        LintCode::CreateIndexWithoutConcurrently,
    );
    assert_absent(
        "REINDEX TABLE CONCURRENTLY orders;",
        LintCode::ReindexWithoutConcurrently,
    );
}

#[test]
fn protected_text_cannot_trigger_a_rule() {
    assert_absent(
        "SELECT 'DELETE FROM orders;' AS example;",
        LintCode::DeleteWithoutWhere,
    );
    assert_absent("-- DROP TABLE orders;\nSELECT 1;", LintCode::DropObject);
    assert_absent("/* TRUNCATE orders; */ SELECT 1;", LintCode::TruncateTable);
    assert_absent(
        "DO $$ BEGIN DELETE FROM orders; END $$;",
        LintCode::DeleteWithoutWhere,
    );
    assert_absent("SELECT 1 AS \"drop\";", LintCode::DropObject);
}

#[test]
fn findings_carry_a_one_based_line_and_column_from_the_source() {
    let report = lint_source(None, "SELECT 1;\n\nDELETE FROM orders;");
    let finding = report
        .findings
        .iter()
        .find(|f| f.code == "delete-without-where")
        .expect("finding");
    assert_eq!(finding.line, 3);
    assert_eq!(finding.column, 1);
    assert_eq!(finding.statement_index, 1);
    assert_eq!(finding.excerpt, "DELETE FROM orders;");
}

#[test]
fn findings_are_sorted_by_line_column_and_code() {
    let report = lint_source(None, "SELECT * FROM t;\nDROP TABLE t;\nDELETE FROM t;");
    let keys: Vec<(usize, usize, &str)> = report
        .findings
        .iter()
        .map(|f| (f.line, f.column, f.code.as_str()))
        .collect();
    let mut sorted = keys.clone();
    sorted.sort();
    assert_eq!(keys, sorted);
}

#[test]
fn severity_thresholds_filter_and_count() {
    let mut report = lint_source(None, "SELECT * FROM t; DROP TABLE t;");
    assert_eq!(report.count(LintSeverity::Info), 2);
    assert!(report.meets(LintSeverity::Warning));
    report.retain_at_or_above(LintSeverity::Warning);
    assert!(
        report
            .findings
            .iter()
            .all(|f| f.severity != LintSeverity::Info)
    );
}

#[test]
fn table_json_and_sarif_render_the_same_findings() {
    let report = lint_source(Some("migrations/001.sql"), "DELETE FROM orders;");
    let table = render_table(&report);
    assert!(table.contains("migrations/001.sql:1:1 error delete-without-where"));

    let json: serde_json::Value =
        serde_json::from_str(&render_json(&report).expect("json")).expect("parse json");
    assert_eq!(json["version"], 1);
    assert_eq!(json["findings"][0]["code"], "delete-without-where");
    assert_eq!(json["findings"][0]["severity"], "error");

    let sarif: serde_json::Value =
        serde_json::from_str(&render_sarif(&report).expect("sarif")).expect("parse sarif");
    assert_eq!(sarif["version"], "2.1.0");
    assert_eq!(
        sarif["runs"][0]["results"][0]["ruleId"],
        "delete-without-where"
    );
    assert_eq!(sarif["runs"][0]["results"][0]["level"], "error");
    assert_eq!(
        sarif["runs"][0]["results"][0]["locations"][0]["physicalLocation"]["region"]["startLine"],
        1
    );
    assert_eq!(
        sarif["runs"][0]["tool"]["driver"]["rules"]
            .as_array()
            .unwrap()
            .len(),
        13
    );
}

#[test]
fn an_empty_or_clean_input_reports_nothing() {
    assert!(lint_source(None, "").findings.is_empty());
    assert!(
        lint_source(None, "SELECT id FROM orders WHERE id = 1;")
            .findings
            .is_empty()
    );
    assert_eq!(
        render_table(&lint_source(None, "SELECT 1;")),
        "No findings.\n"
    );
}

#[test]
fn files_and_directories_are_linted_in_stable_order() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("b.sql"), "DROP TABLE b;").expect("write b");
    std::fs::write(dir.path().join("a.sql"), "DELETE FROM a;").expect("write a");
    std::fs::write(dir.path().join("ignored.txt"), "DELETE FROM a;").expect("write txt");
    let report = lint_paths(&[dir.path().to_path_buf()]).expect("lint dir");
    assert_eq!(report.files, 2);
    assert!(
        report.findings[0]
            .file
            .as_deref()
            .unwrap()
            .ends_with("a.sql")
    );
    assert!(
        report.findings[1]
            .file
            .as_deref()
            .unwrap()
            .ends_with("b.sql")
    );
}

#[test]
fn an_oversized_file_is_refused_before_reading_it() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("large.sql");
    let file = std::fs::File::create(&path).expect("create");
    file.set_len(MAX_FILE_BYTES as u64 + 1).expect("set len");
    let error = lint_file(&path).expect_err("must refuse");
    assert!(error.headline.contains("over the"));
}

#[cfg(unix)]
#[test]
fn a_symbolic_link_is_refused_rather_than_followed() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().expect("tempdir");
    let victim = dir.path().join("real.sql");
    std::fs::write(&victim, "DELETE FROM t;").expect("write");
    let link = dir.path().join("link.sql");
    symlink(&victim, &link).expect("symlink");
    let error = lint_file(&link).expect_err("must refuse");
    assert!(error.headline.contains("symbolic link"));
}
