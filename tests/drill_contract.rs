//! Acceptance contract for Feature 047: restore drills.

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
        "ignatius-drill-contract-{}-{}",
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

fn with_database(uri: &str, database: &str) -> String {
    let (base, query) = match uri.split_once('?') {
        Some((base, query)) => (base, Some(query)),
        None => (uri, None),
    };
    let prefix = &base[..base.rfind('/').expect("URI has a database path")];
    match query {
        Some(query) => format!("{prefix}/{database}?{query}"),
        None => format!("{prefix}/{database}"),
    }
}

fn query(root: &Path, uri: &str, sql: &str) -> Output {
    binary(root)
        .args(["query", uri, "-c", sql])
        .output()
        .expect("run query")
}

fn query_json(root: &Path, uri: &str, sql: &str) -> serde_json::Value {
    let output = binary(root)
        .args(["query", uri, "--format", "json", "-c", sql])
        .output()
        .expect("run query json");
    assert_eq!(code(&output), 0, "query: {}", stderr(&output));
    serde_json::from_str(&stdout(&output)).expect("valid query JSON")
}

fn database_exists(root: &Path, admin_uri: &str, name: &str) -> bool {
    let value = query_json(
        root,
        admin_uri,
        &format!("SELECT EXISTS (SELECT 1 FROM pg_database WHERE datname = '{name}') AS present"),
    );
    value[0]["rows"][0]["present"].as_str() == Some("t")
}

fn unique_name(label: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    format!(
        "ignatius_drill_{label}_{}_{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

fn create_database(root: &Path, admin_uri: &str, name: &str) {
    let output = query(root, admin_uri, &format!("CREATE DATABASE \"{name}\""));
    assert_eq!(code(&output), 0, "create database: {}", stderr(&output));
}

fn drop_database(root: &Path, admin_uri: &str, name: &str) {
    let output = query(
        root,
        admin_uri,
        &format!("DROP DATABASE IF EXISTS \"{name}\" WITH (FORCE)"),
    );
    assert_eq!(code(&output), 0, "drop database: {}", stderr(&output));
}

fn seed(root: &Path, uri: &str) {
    let sql = "CREATE SCHEMA sales;
         CREATE TABLE sales.orders (
             id bigint PRIMARY KEY,
             customer text NOT NULL,
             total numeric(12,2) NOT NULL
         );
         CREATE INDEX orders_customer_idx ON sales.orders (customer);
         CREATE TABLE sales.customers (
             id bigint PRIMARY KEY,
             name text NOT NULL
         );
         CREATE SEQUENCE sales.orders_id_seq;
         CREATE VIEW sales.recent_orders AS
             SELECT id, customer FROM sales.orders WHERE id > 0;
         INSERT INTO sales.customers VALUES (1, 'Ada'), (2, 'Grace');
         INSERT INTO sales.orders VALUES (1, 'Ada', 10.00), (2, 'Grace', 20.00), (3, 'Ada', 30.00);";
    let output = query(root, uri, sql);
    assert_eq!(code(&output), 0, "seed: {}", stderr(&output));
}

fn backup(root: &Path, uri: &str, destination: &Path, format: Option<&str>) {
    let mut command = binary(root);
    command.args(["backup", uri, "-o"]).arg(destination);
    if let Some(format) = format {
        command.args(["--format", format]);
    }
    let output = command.output().expect("run backup");
    assert_eq!(code(&output), 0, "backup: {}", stderr(&output));
}

fn drill(root: &Path, uri: &str, archive: &Path, options: &[&str]) -> Output {
    let mut command = binary(root);
    command.args(["restore-drill", uri, "--backup"]);
    command.arg(archive);
    command.args(options);
    command.output().expect("run restore drill")
}

fn copy_directory(source: &Path, destination: &Path) {
    std::fs::create_dir_all(destination).expect("create copy directory");
    for entry in std::fs::read_dir(source).expect("read source directory") {
        let entry = entry.expect("directory entry");
        let target = destination.join(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            copy_directory(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).expect("copy archive file");
        }
    }
}

#[test]
fn a_passing_drill_compares_counts_and_drops_the_scratch_database() {
    let uri = uri_or_skip();
    if uri.is_empty() {
        return;
    }
    let root = root();
    let source = unique_name("pass");
    let source_uri = with_database(&uri, &source);
    create_database(&root, &uri, &source);
    seed(&root, &source_uri);
    let archive = root.join("source.dump");
    backup(&root, &source_uri, &archive, None);

    let output = drill(&root, &source_uri, &archive, &["--format", "json"]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    let report: serde_json::Value = serde_json::from_str(&stdout(&output)).expect("valid report");
    assert_eq!(report["outcome"], "passed");
    assert!(report["toc_entries"].as_u64().unwrap_or(0) > 0);
    assert_eq!(report["source_relations"], report["restored_relations"]);
    assert_eq!(report["source_relations"], 4);
    assert!(report["restored_bytes"].as_u64().unwrap_or(0) > 0);
    assert_eq!(report["scratch_kept"], false);
    let scratch = report["scratch_database"].as_str().expect("scratch name");
    assert!(
        scratch.starts_with(&format!("{source}_ignatius_drill_")),
        "{scratch}"
    );
    assert!(
        !database_exists(&root, &uri, scratch),
        "scratch was not dropped"
    );
    assert!(stderr(&output).contains("scratch database dropped"));

    let rows = query_json(
        &root,
        &source_uri,
        "SELECT count(*)::bigint AS n FROM sales.orders",
    );
    assert_eq!(rows[0]["rows"][0]["n"].as_str(), Some("3"));

    drop_database(&root, &uri, &source);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn keep_leaves_a_queryable_scratch_database() {
    let uri = uri_or_skip();
    if uri.is_empty() {
        return;
    }
    let root = root();
    let source = unique_name("keep");
    let scratch = unique_name("scratch");
    let source_uri = with_database(&uri, &source);
    create_database(&root, &uri, &source);
    seed(&root, &source_uri);
    let archive = root.join("keep.tar");
    backup(&root, &source_uri, &archive, Some("tar"));

    let output = drill(
        &root,
        &source_uri,
        &archive,
        &["--keep", "--scratch-database", &scratch, "--format", "json"],
    );
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    let report: serde_json::Value = serde_json::from_str(&stdout(&output)).expect("valid report");
    assert_eq!(report["outcome"], "passed");
    assert_eq!(report["archive_format"], "tar");
    assert_eq!(report["scratch_database"], scratch);
    assert_eq!(report["scratch_kept"], true);
    assert!(
        database_exists(&root, &uri, &scratch),
        "the scratch database should be kept"
    );

    let scratch_uri = with_database(&uri, &scratch);
    let rows = query_json(
        &root,
        &scratch_uri,
        "SELECT count(*)::bigint AS n FROM sales.orders",
    );
    assert_eq!(rows[0]["rows"][0]["n"].as_str(), Some("3"));

    drop_database(&root, &uri, &scratch);
    drop_database(&root, &uri, &source);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_damaged_directory_archive_fails_keeps_the_scratch_and_can_be_retried_with_drop() {
    let uri = uri_or_skip();
    if uri.is_empty() {
        return;
    }
    let root = root();
    let source = unique_name("fail");
    let scratch = unique_name("failed");
    let source_uri = with_database(&uri, &source);
    create_database(&root, &uri, &source);
    seed(&root, &source_uri);
    let archive = root.join("directory");
    backup(&root, &source_uri, &archive, Some("directory"));

    let damaged = root.join("damaged");
    copy_directory(&archive, &damaged);
    let mut removed = false;
    for entry in std::fs::read_dir(&damaged).expect("read damaged directory") {
        let entry = entry.expect("directory entry");
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.ends_with(".dat.gz") && name != "toc.dat" {
            std::fs::remove_file(entry.path()).expect("remove data file");
            removed = true;
            break;
        }
    }
    assert!(removed, "the directory archive should contain a data file");

    let output = drill(
        &root,
        &source_uri,
        &damaged,
        &["--scratch-database", &scratch, "--format", "json"],
    );
    assert_eq!(code(&output), 1, "stderr: {}", stderr(&output));
    let report: serde_json::Value = serde_json::from_str(&stdout(&output)).expect("valid report");
    assert_eq!(report["outcome"], "failed");
    assert_eq!(report["scratch_kept"], true);
    assert!(!report["failure"].as_str().unwrap_or_default().is_empty());
    assert!(
        database_exists(&root, &uri, &scratch),
        "a failed drill keeps its scratch database for inspection"
    );

    let retry_scratch = unique_name("retry");
    let dropped = drill(
        &root,
        &source_uri,
        &damaged,
        &["--scratch-database", &retry_scratch, "--drop-on-failure"],
    );
    assert_eq!(code(&dropped), 1, "stderr: {}", stderr(&dropped));
    assert!(!database_exists(&root, &uri, &retry_scratch));

    let rows = query_json(
        &root,
        &source_uri,
        "SELECT count(*)::bigint AS n FROM sales.orders",
    );
    assert_eq!(rows[0]["rows"][0]["n"].as_str(), Some("3"));

    drop_database(&root, &uri, &scratch);
    drop_database(&root, &uri, &source);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn dry_run_changes_nothing_and_invalid_inputs_are_refused() {
    let uri = uri_or_skip();
    if uri.is_empty() {
        return;
    }
    let root = root();
    let source = unique_name("dry");
    let scratch = unique_name("dry_scratch");
    let source_uri = with_database(&uri, &source);
    create_database(&root, &uri, &source);
    seed(&root, &source_uri);
    let archive = root.join("dry.dump");
    backup(&root, &source_uri, &archive, None);

    let output = drill(
        &root,
        &source_uri,
        &archive,
        &["--scratch-database", &scratch, "--dry-run"],
    );
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("no changes will be made"));
    assert!(text.contains("CREATE DATABASE"));
    assert!(text.contains("--exit-on-error"));
    assert!(text.contains("PGPASSWORD=[redacted]"));
    assert!(text.contains("inspect"));
    assert!(!database_exists(&root, &uri, &scratch));

    let plain = root.join("plain.sql");
    std::fs::write(&plain, b"SELECT 1;\n").expect("write plain sql");
    let refused = drill(&root, &source_uri, &plain, &[]);
    assert_eq!(code(&refused), 2, "stderr: {}", stderr(&refused));
    assert!(stderr(&refused).contains("plain SQL"));

    let same = drill(
        &root,
        &source_uri,
        &archive,
        &["--scratch-database", &source],
    );
    assert_eq!(code(&same), 2, "stderr: {}", stderr(&same));
    assert!(stderr(&same).contains("cannot be the source"));

    let system = drill(
        &root,
        &source_uri,
        &archive,
        &["--scratch-database", "postgres"],
    );
    assert_eq!(code(&system), 2, "stderr: {}", stderr(&system));
    assert!(stderr(&system).contains("system database"));

    drop_database(&root, &uri, &scratch);
    drop_database(&root, &uri, &source);
    let _ = std::fs::remove_dir_all(&root);
}
