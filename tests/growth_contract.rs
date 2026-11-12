//! Acceptance contract for Feature 049: retention and growth audit.

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
        "ignatius-growth-contract-{}-{}",
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
        "ignatius_growth_{}_{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

fn seed(root: &Path, uri: &str, schema: &str) {
    let sql = format!(
        "CREATE SCHEMA {schema};
         CREATE TABLE {schema}.events (
             id bigint PRIMARY KEY,
             note text NOT NULL
         ) WITH (autovacuum_enabled = false);
         INSERT INTO {schema}.events
             SELECT g, 'payload-marker-value'
             FROM generate_series(1, 20000) AS g;
         DELETE FROM {schema}.events WHERE id > 8000;
         CREATE SEQUENCE {schema}.events_id_seq MAXVALUE 1000;
         SELECT setval('{schema}.events_id_seq', 990, true);
         CREATE SEQUENCE {schema}.cycle_seq MAXVALUE 100 CYCLE;
         SELECT setval('{schema}.cycle_seq', 99, true);"
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

fn rows(root: &Path, uri: &str, sql: &str) -> String {
    let output = query(root, uri, sql);
    assert_eq!(code(&output), 0, "probe: {}", stderr(&output));
    stdout(&output)
}

#[test]
fn the_audit_reports_sizes_dead_tuples_sequences_and_findings() {
    let uri = uri_or_skip();
    if uri.is_empty() {
        return;
    }
    let root = root();
    let schema = schema_name();
    seed(&root, &uri, &schema);

    let before_rows = rows(
        &root,
        &uri,
        &format!("SELECT count(*) FROM {schema}.events"),
    );
    let before_file = rows(
        &root,
        &uri,
        &format!(
            "SELECT c.relfilenode::text FROM pg_catalog.pg_class c \
             JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
             WHERE n.nspname = '{schema}' AND c.relname = 'events'"
        ),
    );

    let output = binary(&root)
        .args([
            "growth",
            &uri,
            "--schema",
            &schema,
            "--large-table-bytes",
            "1",
            "--format",
            "json",
        ])
        .output()
        .expect("run growth");
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(
        !text.contains("payload-marker-value"),
        "the audit must not read table rows: {text}"
    );
    let value: serde_json::Value = serde_json::from_str(&text).expect("valid json");

    let relations = value["relations"].as_array().expect("relations");
    let events = relations
        .iter()
        .find(|entry| entry["table"] == "events")
        .expect("events relation");
    assert!(events["total_bytes"].as_u64().unwrap_or(0) > 0);
    let catalog_toast = rows(
        &root,
        &uri,
        &format!(
            "SELECT pg_catalog.pg_total_relation_size(c.reltoastrelid)::bigint \
             FROM pg_catalog.pg_class c \
             JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
             WHERE n.nspname = '{schema}' AND c.relname = 'events'"
        ),
    );
    let catalog_toast: u64 = catalog_toast
        .lines()
        .find_map(|line| line.trim().parse().ok())
        .expect("toast size probe");
    assert_eq!(
        events["toast_bytes"].as_u64(),
        Some(catalog_toast),
        "toast bytes must match the catalog: {events}"
    );
    let component_sum = events["heap_bytes"].as_u64().unwrap_or(0)
        + events["index_bytes"].as_u64().unwrap_or(0)
        + events["toast_bytes"].as_u64().unwrap_or(0);
    assert!(
        events["total_bytes"].as_u64().unwrap_or(0) >= component_sum,
        "component sizes cannot exceed the total: {events}"
    );
    assert!(
        events["dead_tuples"].as_i64().unwrap_or(0) >= 10_000,
        "seed should leave vacuum debt: {events}"
    );
    assert!(
        events["dead_ratio"].as_f64().unwrap_or(0.0) >= 0.20,
        "dead ratio: {events}"
    );

    let sequences = value["sequences"].as_array().expect("sequences");
    let critical = sequences
        .iter()
        .find(|entry| entry["name"] == "events_id_seq")
        .expect("events sequence");
    assert_eq!(critical["state"], "CRITICAL");
    assert_eq!(critical["remaining"], 10);
    let cyclic = sequences
        .iter()
        .find(|entry| entry["name"] == "cycle_seq")
        .expect("cycle sequence");
    assert_eq!(cyclic["state"], "CYCLIC");

    let codes: Vec<&str> = value["findings"]
        .as_array()
        .expect("findings")
        .iter()
        .filter_map(|finding| finding["code"].as_str())
        .collect();
    assert!(codes.contains(&"vacuum-debt"), "findings: {codes:?}");
    assert!(
        codes.contains(&"sequence-exhaustion"),
        "findings: {codes:?}"
    );
    assert!(
        codes.contains(&"partitioning-candidate"),
        "findings: {codes:?}"
    );
    assert!(
        value["findings"]
            .as_array()
            .expect("findings")
            .iter()
            .all(|finding| finding["object"] != format!("{schema}.cycle_seq"))
    );

    let after_rows = rows(
        &root,
        &uri,
        &format!("SELECT count(*) FROM {schema}.events"),
    );
    let after_file = rows(
        &root,
        &uri,
        &format!(
            "SELECT c.relfilenode::text FROM pg_catalog.pg_class c \
             JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
             WHERE n.nspname = '{schema}' AND c.relname = 'events'"
        ),
    );
    assert_eq!(before_rows, after_rows, "the audit changed table rows");
    assert_eq!(before_file, after_file, "the audit rewrote the relation");

    cleanup(&root, &uri, &schema);
}

#[test]
fn thresholds_control_the_exit_code_and_the_schema_filter_is_exact() {
    let uri = uri_or_skip();
    if uri.is_empty() {
        return;
    }
    let root = root();
    let schema = schema_name();
    seed(&root, &uri, &schema);
    let other = format!("{schema}_other");
    let seeded = query(&root, &uri, &format!("CREATE SCHEMA {other}"));
    assert_eq!(code(&seeded), 0, "seed other: {}", stderr(&seeded));

    let failing = binary(&root)
        .args([
            "growth",
            &uri,
            "--schema",
            &schema,
            "--large-table-bytes",
            "1",
            "--fail-on",
            "critical",
        ])
        .output()
        .expect("run growth failing");
    assert_eq!(code(&failing), 1, "stderr: {}", stderr(&failing));
    assert!(stderr(&failing).contains("threshold was met"));
    assert!(stdout(&failing).contains("Sequence headroom"));
    assert!(stdout(&failing).contains("CRITICAL"));

    let passing = binary(&root)
        .args([
            "growth",
            &uri,
            "--schema",
            &schema,
            "--large-table-bytes",
            "1",
            "--fail-on",
            "never",
        ])
        .output()
        .expect("run growth passing");
    assert_eq!(code(&passing), 0, "stderr: {}", stderr(&passing));

    let filtered = binary(&root)
        .args(["growth", &uri, "--schema", &other, "--format", "json"])
        .output()
        .expect("run growth filtered");
    assert_eq!(code(&filtered), 0, "stderr: {}", stderr(&filtered));
    let value: serde_json::Value =
        serde_json::from_str(&stdout(&filtered)).expect("valid filtered json");
    assert_eq!(value["relations"].as_array().map(Vec::len), Some(0));
    assert_eq!(value["sequences"].as_array().map(Vec::len), Some(0));
    assert_eq!(value["findings"].as_array().map(Vec::len), Some(0));

    cleanup(&root, &uri, &other);
    cleanup(&root, &uri, &schema);
}

#[test]
fn out_of_range_options_are_refused_before_connecting() {
    let root = root();
    for arguments in [
        vec!["growth", "postgres://127.0.0.1:1/none", "--top", "0"],
        vec!["growth", "postgres://127.0.0.1:1/none", "--top", "201"],
        vec![
            "growth",
            "postgres://127.0.0.1:1/none",
            "--large-table-bytes",
            "0",
        ],
    ] {
        let output = binary(&root)
            .args(&arguments)
            .output()
            .expect("run growth bounds");
        assert_eq!(
            code(&output),
            2,
            "arguments {arguments:?} should be refused: {}",
            stderr(&output)
        );
    }
}
