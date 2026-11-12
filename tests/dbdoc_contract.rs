//! Acceptance contract for Feature 046: schema documentation generator.

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
        "ignatius-dbdoc-contract-{}-{}",
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
        "ignatius_docs_{}_{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

fn seed_schema(root: &Path, uri: &str, schema: &str) {
    let sql = format!(
        "CREATE SCHEMA {schema};
         CREATE TABLE {schema}.customers (
             customer_id integer PRIMARY KEY,
             email text NOT NULL UNIQUE
         );
         CREATE TABLE {schema}.orders (
             order_id integer PRIMARY KEY,
             customer_id integer NOT NULL REFERENCES {schema}.customers(customer_id),
             note text,
             total numeric(12,2) NOT NULL CHECK (total >= 0)
         );
         CREATE INDEX orders_note_idx ON {schema}.orders (note);
         COMMENT ON TABLE {schema}.orders IS 'Customer orders';
         COMMENT ON COLUMN {schema}.orders.note IS 'Free text';
         ALTER TABLE {schema}.orders ENABLE ROW LEVEL SECURITY;
         CREATE POLICY tenant_read ON {schema}.orders FOR SELECT USING (true);"
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

#[test]
fn markdown_documents_tables_columns_keys_indexes_constraints_and_rls() {
    let uri = uri_or_skip();
    if uri.is_empty() {
        return;
    }
    let root = root();
    let schema = schema_name();
    seed_schema(&root, &uri, &schema);

    let output = binary(&root)
        .args(["docs", &uri, "--schema", &schema])
        .output()
        .expect("run docs");
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains(&format!("# Data dictionary: ignatius_demo.{schema}")));
    assert!(text.contains("## customers"));
    assert!(text.contains("## orders"));
    assert!(text.contains("| 1 | order_id | integer | NO |"));
    assert!(text.contains("| PK |"));
    assert!(text.contains(&format!("{schema}.customers.customer_id")));
    assert!(text.contains("orders_note_idx"));
    assert!(text.contains("orders_total_check"));
    assert!(text.contains("Customer orders"));
    assert!(text.contains("Free text"));
    assert!(text.contains("tenant_read"));
    assert!(text.contains("Row-level security: enabled"));

    cleanup(&root, &uri, &schema);
}

#[test]
fn json_and_mermaid_are_machine_clean_and_complete() {
    let uri = uri_or_skip();
    if uri.is_empty() {
        return;
    }
    let root = root();
    let schema = schema_name();
    seed_schema(&root, &uri, &schema);

    let output = binary(&root)
        .args(["docs", &uri, "--schema", &schema, "--format", "json"])
        .output()
        .expect("run docs json");
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    assert!(!stdout(&output).contains('\u{1b}'));
    let value: serde_json::Value = serde_json::from_str(&stdout(&output)).expect("valid json");
    assert_eq!(value["version"], 1);
    assert_eq!(value["schema"], schema);
    assert_eq!(value["relations"].as_array().unwrap().len(), 2);

    let output = binary(&root)
        .args(["docs", &uri, "--schema", &schema, "--format", "mermaid"])
        .output()
        .expect("run docs mermaid");
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.starts_with("erDiagram\n"), "{text}");
    assert!(text.contains(&format!("\"{schema}.orders\" {{")), "{text}");
    assert!(
        text.contains(&format!(
            "\"{schema}.customers\" }}o--|| \"{schema}.orders\""
        )),
        "{text}"
    );

    cleanup(&root, &uri, &schema);
}

#[test]
fn output_files_are_protected_and_force_replaces_them() {
    let uri = uri_or_skip();
    if uri.is_empty() {
        return;
    }
    let root = root();
    let schema = schema_name();
    seed_schema(&root, &uri, &schema);
    let destination = root.join("data-dictionary.md");

    let first = binary(&root)
        .args(["docs", &uri, "--schema", &schema, "--output"])
        .arg(&destination)
        .output()
        .expect("write docs");
    assert_eq!(code(&first), 0, "stderr: {}", stderr(&first));
    assert!(stderr(&first).contains("Wrote 2 relation(s)"));
    let original = std::fs::read_to_string(&destination).expect("read docs");

    let second = binary(&root)
        .args(["docs", &uri, "--schema", &schema, "--output"])
        .arg(&destination)
        .output()
        .expect("refuse overwrite");
    assert_eq!(code(&second), 2, "stderr: {}", stderr(&second));
    assert!(stderr(&second).contains("refusing to overwrite"));
    assert_eq!(
        std::fs::read_to_string(&destination).expect("read"),
        original
    );

    let third = binary(&root)
        .args(["docs", &uri, "--schema", &schema, "--output"])
        .arg(&destination)
        .arg("--force")
        .output()
        .expect("force overwrite");
    assert_eq!(code(&third), 0, "stderr: {}", stderr(&third));

    cleanup(&root, &uri, &schema);
}

#[test]
fn an_empty_schema_says_so_and_a_missing_schema_is_a_usage_error() {
    let uri = uri_or_skip();
    if uri.is_empty() {
        return;
    }
    let root = root();
    let schema = schema_name();
    let output = query(&root, &uri, &format!("CREATE SCHEMA {schema}"));
    assert_eq!(code(&output), 0, "seed: {}", stderr(&output));

    let empty = binary(&root)
        .args(["docs", &uri, "--schema", &schema])
        .output()
        .expect("run docs empty");
    assert_eq!(code(&empty), 0, "stderr: {}", stderr(&empty));
    assert!(stdout(&empty).contains("_No relations are visible in this schema._"));

    let missing = binary(&root)
        .args(["docs", &uri, "--schema", "definitely_not_here"])
        .output()
        .expect("run docs missing");
    assert_eq!(code(&missing), 2, "stderr: {}", stderr(&missing));
    assert!(stderr(&missing).contains("does not exist or is not visible"));

    cleanup(&root, &uri, &schema);
}
