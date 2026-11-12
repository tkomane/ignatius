//! Acceptance contract for Feature 048: partition lifecycle.

#![allow(clippy::print_stderr)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn binary(root: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ignatius"));
    command.env("IGNATIUS_CONFIG_DIR", root);
    command.env("IGNATIUS_DATA_DIR", root.join("data"));
    command.env_remove("IGNATIUS_LOG");
    for key in [
        "PGHOST",
        "PGPORT",
        "PGDATABASE",
        "PGUSER",
        "PGPASSWORD",
        "PGSERVICE",
        "PGSSLMODE",
        "PGPASSFILE",
    ] {
        command.env_remove(key);
    }
    command
}

fn root() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "ignatius-partition-contract-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).expect("create temp root");
    dir
}

fn uri_or_skip() -> String {
    match std::env::var("IGNATIUS_TEST_PG_URI") {
        Ok(uri) if !uri.is_empty() => uri,
        _ => {
            eprintln!("skipping: IGNATIUS_TEST_PG_URI is not set");
            String::new()
        }
    }
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn code(output: &Output) -> i32 {
    output.status.code().expect("process exited normally")
}

fn query(root: &Path, uri: &str, sql: &str) -> Output {
    binary(root)
        .args(["query", uri, "-c", sql])
        .output()
        .expect("run query")
}

fn schema_name() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    format!(
        "ignatius_part_{}_{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

fn seed(root: &Path, uri: &str, schema: &str) {
    let sql = format!(
        "CREATE SCHEMA {schema};
         CREATE TABLE {schema}.events (
             id bigint,
             occurred_on date NOT NULL
         ) PARTITION BY RANGE (occurred_on);
         CREATE TABLE {schema}.events_p2024_12 PARTITION OF {schema}.events
             FOR VALUES FROM ('2024-12-01') TO ('2025-01-01');
         CREATE TABLE {schema}.events_p2026_11 PARTITION OF {schema}.events
             FOR VALUES FROM ('2026-11-01') TO ('2026-12-01');
         CREATE TABLE {schema}.events_p2026_12 PARTITION OF {schema}.events
             FOR VALUES FROM ('2026-12-01') TO ('2027-01-01');
         CREATE TABLE {schema}.events_default PARTITION OF {schema}.events DEFAULT;
         INSERT INTO {schema}.events VALUES (1, '2026-11-15');
         CREATE TABLE {schema}.regions (
             id bigint,
             region text NOT NULL
         ) PARTITION BY LIST (region);
         CREATE TABLE {schema}.regions_a PARTITION OF {schema}.regions FOR VALUES IN ('a');
         CREATE TABLE {schema}.times (
             id bigint,
             at timestamptz NOT NULL
         ) PARTITION BY RANGE (at);
         CREATE TABLE {schema}.times_p2026_11 PARTITION OF {schema}.times
             FOR VALUES FROM ('2026-11-01 00:00:00+00') TO ('2026-12-01 00:00:00+00');"
    );
    let output = query(root, uri, &sql);
    assert_eq!(code(&output), 0, "seed: {}", stderr(&output));
}

fn cleanup(root: &Path, uri: &str, schema: &str) {
    let output = query(
        root,
        uri,
        &format!("DROP SCHEMA IF EXISTS {schema} CASCADE"),
    );
    assert_eq!(code(&output), 0, "cleanup: {}", stderr(&output));
}

fn run_plan(root: &Path, uri: &str, table: &str, options: &[&str]) -> Output {
    let mut command = binary(root);
    command.args(["partition", "plan", uri, "--table", table]);
    command.args(options);
    command.output().expect("run partition plan")
}

#[test]
fn status_reports_keys_bounds_sizes_and_default_partitions() {
    let uri = uri_or_skip();
    if uri.is_empty() {
        return;
    }
    let root = root();
    let schema = schema_name();
    seed(&root, &uri, &schema);

    let output = binary(&root)
        .args(["partition", "status", &uri, "--schema", &schema])
        .output()
        .expect("run partition status");
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains(&format!("{schema}.events")), "{text}");
    assert!(text.contains("[RANGE]"), "{text}");
    assert!(text.contains("events_p2026_11"), "{text}");
    assert!(text.contains("events_default"), "{text}");
    assert!(text.contains("DEFAULT"), "{text}");
    assert!(
        text.contains("FOR VALUES FROM ('2026-11-01') TO ('2026-12-01')"),
        "{text}"
    );
    assert!(text.contains(&format!("{schema}.regions")), "{text}");
    assert!(text.contains("[LIST]"), "{text}");

    let output = binary(&root)
        .args([
            "partition",
            "status",
            &uri,
            "--schema",
            &schema,
            "--format",
            "json",
        ])
        .output()
        .expect("run partition status json");
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    let value: serde_json::Value = serde_json::from_str(&stdout(&output)).expect("valid json");
    let events = value["parents"]
        .as_array()
        .expect("parents")
        .iter()
        .find(|parent| parent["table"] == "events")
        .expect("events parent");
    assert_eq!(events["strategy"], "range");
    assert_eq!(events["key_def"], "RANGE (occurred_on)");
    assert_eq!(events["partitions"].as_array().map(Vec::len), Some(4));
    assert!(
        events["partitions"]
            .as_array()
            .expect("partitions")
            .iter()
            .any(|partition| partition["bound"] == "DEFAULT")
    );

    cleanup(&root, &uri, &schema);
}

#[test]
fn a_plan_generates_reviewed_sql_and_changes_nothing() {
    let uri = uri_or_skip();
    if uri.is_empty() {
        return;
    }
    let root = root();
    let schema = schema_name();
    seed(&root, &uri, &schema);

    let before = binary(&root)
        .args([
            "partition",
            "status",
            &uri,
            "--schema",
            &schema,
            "--format",
            "json",
        ])
        .output()
        .expect("status before");
    assert_eq!(code(&before), 0, "stderr: {}", stderr(&before));

    let output = run_plan(
        &root,
        &uri,
        &format!("{schema}.events"),
        &[
            "--period",
            "monthly",
            "--start",
            "2027-01-01",
            "--count",
            "3",
        ],
    );
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    let sql = stdout(&output);
    assert!(
        sql.contains("-- Generated by ignatius partition plan"),
        "{sql}"
    );
    assert!(sql.contains(&format!(
        "CREATE TABLE IF NOT EXISTS \"{schema}\".\"events_p2027_01\" PARTITION OF \"{schema}\".\"events\" FOR VALUES FROM ('2027-01-01') TO ('2027-02-01');"
    )));
    assert!(sql.contains("events_p2027_03"));
    assert!(sql.contains("-- A default partition exists."));
    assert_eq!(sql.matches("CREATE TABLE").count(), 3);
    assert!(stderr(&output).contains("nothing was executed"));

    let after = binary(&root)
        .args([
            "partition",
            "status",
            &uri,
            "--schema",
            &schema,
            "--format",
            "json",
        ])
        .output()
        .expect("status after");
    assert_eq!(code(&after), 0, "stderr: {}", stderr(&after));
    assert_eq!(
        stdout(&before),
        stdout(&after),
        "a plan must not change the catalog"
    );

    cleanup(&root, &uri, &schema);
}

#[test]
fn an_overlap_or_an_unsupported_shape_is_refused() {
    let uri = uri_or_skip();
    if uri.is_empty() {
        return;
    }
    let root = root();
    let schema = schema_name();
    seed(&root, &uri, &schema);

    let overlap = run_plan(
        &root,
        &uri,
        &format!("{schema}.events"),
        &[
            "--period",
            "monthly",
            "--start",
            "2026-11-01",
            "--count",
            "1",
        ],
    );
    assert_eq!(code(&overlap), 2, "stderr: {}", stderr(&overlap));
    assert!(stderr(&overlap).contains("overlaps existing partition"));
    assert!(stderr(&overlap).contains("events_p2026_11"));

    let list = run_plan(
        &root,
        &uri,
        &format!("{schema}.regions"),
        &[
            "--period",
            "monthly",
            "--start",
            "2027-01-01",
            "--count",
            "1",
        ],
    );
    assert_eq!(code(&list), 2, "stderr: {}", stderr(&list));
    assert!(stderr(&list).contains("only RANGE partitioning"));

    let unqualified = binary(&root)
        .args([
            "partition",
            "plan",
            &uri,
            "--table",
            "events",
            "--period",
            "monthly",
            "--start",
            "2027-01-01",
        ])
        .output()
        .expect("run unqualified");
    assert_eq!(code(&unqualified), 2, "stderr: {}", stderr(&unqualified));
    assert!(stderr(&unqualified).contains("schema.table"));

    cleanup(&root, &uri, &schema);
}

#[test]
fn expiry_and_timestamptz_literals_are_generated_without_execution() {
    let uri = uri_or_skip();
    if uri.is_empty() {
        return;
    }
    let root = root();
    let schema = schema_name();
    seed(&root, &uri, &schema);

    let output = run_plan(
        &root,
        &uri,
        &format!("{schema}.events"),
        &[
            "--period",
            "monthly",
            "--start",
            "2027-01-01",
            "--count",
            "1",
            "--expire-before",
            "2025-01-01",
        ],
    );
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    let sql = stdout(&output);
    assert!(
        sql.contains(&format!(
            "ALTER TABLE \"{schema}\".\"events\" DETACH PARTITION \"{schema}\".\"events_p2024_12\";"
        )),
        "{sql}"
    );
    assert!(
        sql.contains(&format!("DROP TABLE \"{schema}\".\"events_p2024_12\";")),
        "{sql}"
    );
    assert!(!sql.contains("events_p2026_11;\nDROP"));

    let timestamps = run_plan(
        &root,
        &uri,
        &format!("{schema}.times"),
        &[
            "--period",
            "monthly",
            "--start",
            "2026-12-01",
            "--count",
            "1",
        ],
    );
    assert_eq!(code(&timestamps), 0, "stderr: {}", stderr(&timestamps));
    assert!(
        stdout(&timestamps)
            .contains("FROM ('2026-12-01 00:00:00+00') TO ('2027-01-01 00:00:00+00')"),
        "{}",
        stdout(&timestamps)
    );

    cleanup(&root, &uri, &schema);
}
