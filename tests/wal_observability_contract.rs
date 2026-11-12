//! CLI contract tests for `ignatius wal-stats` (`036-transaction-and-wal-observability`).

#![allow(clippy::print_stderr)]

use std::process::{Command, Output};

fn binary() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ignatius"));
    command.env("IGNATIUS_CONFIG_DIR", temp_config_dir());
    command.env("IGNATIUS_DATA_DIR", temp_config_dir().join("data"));
    command.env_remove("IGNATIUS_LOG");
    for key in [
        "PGHOST",
        "PGPORT",
        "PGDATABASE",
        "PGUSER",
        "PGPASSWORD",
        "PGSERVICE",
        "PGSSLMODE",
    ] {
        command.env_remove(key);
    }
    command
}

fn temp_config_dir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ignatius-wal-stats-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create temp config dir");
    dir
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

macro_rules! uri_or_skip {
    () => {
        match std::env::var("IGNATIUS_TEST_PG_URI") {
            Ok(uri) if !uri.is_empty() => {
                let mut uri = uri;
                if uri.contains(":ignatius_test_password_do_not_use@") {
                    uri = uri.replace(
                        ":ignatius_test_password_do_not_use@",
                        ":not-a-real-password-disposable-container@",
                    );
                }
                if let Some(pos) = uri.find("?sslmode=disable") {
                    uri.truncate(pos);
                }
                uri
            }
            _ => {
                eprintln!("skipping: IGNATIUS_TEST_PG_URI is not set");
                return;
            }
        }
    };
}

#[test]
fn wal_stats_help_and_options_are_complete() {
    let output = binary()
        .args(["wal-stats", "--help"])
        .output()
        .expect("run wal-stats --help");

    assert_eq!(code(&output), 0);
    let help = stdout(&output);
    assert!(help.contains("--section"));
    assert!(help.contains("--format"));
    assert!(help.contains("--output"));
    assert!(help.contains("--force"));
    assert!(help.contains("all"));
    assert!(help.contains("checkpointer"));
    assert!(help.contains("wal"));
    assert!(help.contains("transactions"));
    assert!(help.contains("freeze"));
    assert!(help.contains("replication"));
}

#[test]
fn wal_stats_protects_existing_output_file_without_force() {
    let temp_file = temp_config_dir().join("existing_wal_report.txt");
    std::fs::write(&temp_file, "pre-existing content").expect("write temp file");

    let output = binary()
        .args([
            "wal-stats",
            "-o",
            temp_file.to_str().unwrap(),
            "postgres://localhost:5432/postgres",
        ])
        .output()
        .expect("run wal-stats with output path");

    assert_ne!(code(&output), 0);
    let err_str = stderr(&output);
    assert!(err_str.contains("refusing to overwrite existing file"));
    assert!(err_str.contains("--force"));

    let content = std::fs::read_to_string(&temp_file).expect("read temp file");
    assert_eq!(content, "pre-existing content");
}

#[test]
fn wal_stats_live_contract() {
    let uri = uri_or_skip!();

    // 1. Table format (all sections)
    let table_out = binary()
        .args(["wal-stats", &uri, "--format", "table"])
        .output()
        .expect("run wal-stats table");
    assert_eq!(code(&table_out), 0, "table failed: {}", stderr(&table_out));
    let table_str = stdout(&table_out);
    assert!(table_str.contains("Ignatius Transaction & WAL Observability"));
    assert!(table_str.contains("Checkpointer & Background Writer"));
    assert!(table_str.contains("Write-Ahead Log (WAL) & Archiver"));
    assert!(table_str.contains("Database Transactions & Spills"));
    assert!(table_str.contains("Transaction ID (XID) Wraparound Forensics"));

    // 2. JSON format
    let json_out = binary()
        .args(["wal-stats", &uri, "--format", "json"])
        .output()
        .expect("run wal-stats json");
    assert_eq!(code(&json_out), 0, "json failed: {}", stderr(&json_out));
    let json_str = stdout(&json_out);
    assert!(json_str.contains("\"checkpointer\":"));
    assert!(json_str.contains("\"wal\":"));
    assert!(json_str.contains("\"databases\":"));
    assert!(json_str.contains("\"xid_freeze\":"));
    assert!(json_str.contains("\"standbys\":"));

    // 3. Plain format with section filtering
    let plain_out = binary()
        .args(["wal-stats", &uri, "--format", "plain", "--section", "wal"])
        .output()
        .expect("run wal-stats plain wal");
    assert_eq!(code(&plain_out), 0, "plain failed: {}", stderr(&plain_out));
    let plain_str = stdout(&plain_out);
    assert!(plain_str.contains("Current LSN:"));

    // 4. Section filtering: freeze
    let freeze_out = binary()
        .args(["wal-stats", &uri, "--section", "freeze"])
        .output()
        .expect("run wal-stats freeze");
    assert_eq!(
        code(&freeze_out),
        0,
        "freeze failed: {}",
        stderr(&freeze_out)
    );
    let freeze_str = stdout(&freeze_out);
    assert!(freeze_str.contains("Transaction ID (XID) Wraparound Forensics"));

    // 5. Output file writing with force
    let out_file = temp_config_dir().join("generated_wal_report.json");
    let file_out = binary()
        .args([
            "wal-stats",
            &uri,
            "--format",
            "json",
            "-o",
            out_file.to_str().unwrap(),
            "--force",
        ])
        .output()
        .expect("run wal-stats to file");
    assert_eq!(
        code(&file_out),
        0,
        "file write failed: {}",
        stderr(&file_out)
    );
    let saved_json = std::fs::read_to_string(&out_file).expect("read written file");
    assert!(saved_json.contains("\"checkpointer\":"));
}
