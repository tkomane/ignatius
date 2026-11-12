//! Sidecar tests for migration discovery, checksums and planning.

use super::*;

fn write(dir: &Path, name: &str, sql: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, sql).expect("write migration");
    path
}

fn record(file: &MigrationFile) -> MigrationRecord {
    MigrationRecord {
        version: file.raw_version.clone(),
        name: file.name.clone(),
        checksum: file.checksum.clone(),
        applied_at: "2026-11-12 10:00:00+00".to_owned(),
        execution_ms: 4,
    }
}

#[test]
fn files_are_discovered_in_numeric_order_and_checksummed() {
    let dir = tempfile::tempdir().expect("tempdir");
    write(dir.path(), "010_second.sql", "SELECT 10;");
    write(dir.path(), "002_first.sql", "SELECT 2;");
    write(dir.path(), "README.md", "not a migration");

    let files = discover(dir.path()).expect("discover");
    assert_eq!(files.len(), 2);
    assert_eq!(files[0].version, 2);
    assert_eq!(files[0].raw_version, "002");
    assert_eq!(files[0].name, "first");
    assert_eq!(files[0].checksum.len(), 64);
    assert!(files[0].checksum.chars().all(|c| c.is_ascii_hexdigit()));
    assert_eq!(files[1].version, 10);
}

#[test]
fn a_file_without_a_numeric_prefix_is_refused() {
    let dir = tempfile::tempdir().expect("tempdir");
    write(dir.path(), "create_users.sql", "CREATE TABLE users ();");
    let error = discover(dir.path()).expect_err("must refuse");
    assert!(
        error
            .headline
            .contains("does not start with a numeric version")
    );
}

#[test]
fn duplicate_versions_are_refused_before_any_apply() {
    let dir = tempfile::tempdir().expect("tempdir");
    write(dir.path(), "001_one.sql", "SELECT 1;");
    write(dir.path(), "01_other.sql", "SELECT 2;");
    let files = discover(dir.path()).expect("discover");
    let error = MigrationPlan::new(files, Vec::new()).expect_err("must refuse");
    assert!(error.headline.contains("appears more than once"));
}

#[test]
fn checksums_change_when_the_bytes_change() {
    assert_eq!(checksum(b"SELECT 1;"), checksum(b"SELECT 1;"));
    assert_ne!(checksum(b"SELECT 1;"), checksum(b"SELECT 1; "));
}

#[test]
fn transaction_control_is_detected_outside_protected_text() {
    assert!(transaction_control("BEGIN; SELECT 1;").is_some());
    assert!(transaction_control("COMMIT;").is_some());
    assert!(transaction_control("SAVEPOINT x;").is_some());
    assert_eq!(
        transaction_control("SELECT 'BEGIN;';"),
        None,
        "a string literal is not transaction control"
    );
    assert_eq!(
        transaction_control("DO $$ BEGIN PERFORM 1; END $$;"),
        None,
        "a dollar body is not transaction control"
    );
    assert_eq!(
        transaction_control("-- COMMIT;\nSELECT 1;"),
        None,
        "a comment is not transaction control"
    );
}

#[test]
fn a_plan_classifies_applied_pending_drifted_and_missing() {
    let dir = tempfile::tempdir().expect("tempdir");
    write(dir.path(), "001_one.sql", "SELECT 1;");
    write(dir.path(), "002_two.sql", "SELECT 2;");
    write(dir.path(), "003_three.sql", "SELECT 3;");
    let mut files = discover(dir.path()).expect("discover");

    let applied = record(&files[0]);
    let second = record(&files[1]);
    files[1].checksum = "changed".to_owned();
    let records = vec![
        applied,
        second,
        MigrationRecord {
            version: "004".to_owned(),
            name: "gone".to_owned(),
            checksum: "missing".to_owned(),
            applied_at: "2026-11-12 10:00:00+00".to_owned(),
            execution_ms: 1,
        },
    ];
    let plan = MigrationPlan::new(files, records).expect("plan");

    let states: Vec<(&str, MigrationState)> = plan
        .entries
        .iter()
        .map(|entry| (entry.raw_version(), entry.state))
        .collect();
    assert_eq!(
        states,
        vec![
            ("001", MigrationState::Applied),
            ("002", MigrationState::Drifted),
            ("003", MigrationState::Pending),
            ("004", MigrationState::Missing),
        ]
    );
    assert!(!plan.is_clean());
    assert_eq!(plan.applied(), 1);
    assert_eq!(plan.pending(None).len(), 1);
    assert_eq!(plan.drifted().len(), 1);
    assert_eq!(plan.missing().len(), 1);
}

#[test]
fn up_to_caps_the_pending_plan_inclusively() {
    let dir = tempfile::tempdir().expect("tempdir");
    write(dir.path(), "001_one.sql", "SELECT 1;");
    write(dir.path(), "002_two.sql", "SELECT 2;");
    write(dir.path(), "003_three.sql", "SELECT 3;");
    let files = discover(dir.path()).expect("discover");
    let plan = MigrationPlan::new(files, Vec::new()).expect("plan");
    let pending = plan.pending(Some(2));
    assert_eq!(pending.len(), 2);
    assert_eq!(pending[1].raw_version(), "002");
}

#[test]
fn renderers_state_the_plan_and_the_sql() {
    let dir = tempfile::tempdir().expect("tempdir");
    write(dir.path(), "001_one.sql", "CREATE TABLE one (id int);");
    write(dir.path(), "002_two.sql", "CREATE TABLE two (id int);");
    let files = discover(dir.path()).expect("discover");
    let plan = MigrationPlan::new(files, Vec::new()).expect("plan");

    let table = render_status_table(&plan);
    assert!(table.contains("VERSION"));
    assert!(table.contains("pending"));
    assert!(render_plan_table(&plan, Some(1)).contains("001"));
    assert!(!render_plan_table(&plan, Some(1)).contains("002"));

    let sql = render_plan_sql(&plan, None);
    assert!(sql.contains("-- ---- 001 one"));
    assert!(sql.contains("CREATE TABLE one"));
    assert!(sql.contains("CREATE TABLE two"));

    let json: serde_json::Value =
        serde_json::from_str(&render_status_json(&plan).expect("json")).expect("parse");
    assert_eq!(json["version"], 1);
    assert_eq!(json["clean"], true);
    assert_eq!(json["pending"], 2);
    assert_eq!(json["entries"][0]["state"], "pending");
}

#[test]
fn an_empty_directory_is_a_clean_empty_plan() {
    let dir = tempfile::tempdir().expect("tempdir");
    let files = discover(dir.path()).expect("discover");
    assert!(files.is_empty());
    let plan = MigrationPlan::new(files, Vec::new()).expect("plan");
    assert!(plan.is_clean());
    assert_eq!(
        render_status_table(&plan),
        "VERSION  STATE    NAME\nNo pending migrations.\n"
    );
}
