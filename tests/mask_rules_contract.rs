//! CLI contract tests for `ignatius mask-rules` (`038-data-masking-and-anonymization-rules`).

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
    let dir = std::env::temp_dir().join(format!("ignatius-mask-rules-test-{}", std::process::id()));
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
fn mask_rules_help_and_options_are_complete() {
    let output = binary()
        .args(["mask-rules", "--help"])
        .output()
        .expect("run mask-rules --help");

    assert_eq!(code(&output), 0);
    let help = stdout(&output);
    assert!(help.contains("--schema"));
    assert!(help.contains("--view-prefix"));
    assert!(help.contains("--format"));
    assert!(help.contains("--output"));
    assert!(help.contains("--force"));
    assert!(help.contains("table"));
    assert!(help.contains("json"));
    assert!(help.contains("plain"));
    assert!(help.contains("sql"));
}

#[test]
fn mask_rules_protects_existing_output_file_without_force() {
    let temp_file = temp_config_dir().join("existing_mask_report.txt");
    std::fs::write(&temp_file, "pre-existing content").expect("write temp file");

    let output = binary()
        .args([
            "mask-rules",
            "-o",
            temp_file.to_str().unwrap(),
            "postgres://localhost:5432/postgres",
        ])
        .output()
        .expect("run mask-rules with output path");

    assert_ne!(code(&output), 0);
    let err_str = stderr(&output);
    assert!(err_str.contains("refusing to overwrite existing file"));
    assert!(err_str.contains("--force"));

    let content = std::fs::read_to_string(&temp_file).expect("read temp file");
    assert_eq!(content, "pre-existing content");
}

#[test]
fn mask_rules_live_contract() {
    let uri = uri_or_skip!();

    // 1. Table format
    let table_out = binary()
        .args(["mask-rules", &uri, "--format", "table"])
        .output()
        .expect("run mask-rules table");
    assert_eq!(code(&table_out), 0, "table failed: {}", stderr(&table_out));
    let table_str = stdout(&table_out);
    assert!(table_str.contains("Ignatius Data Masking & PII Forensics"));

    // 2. JSON format
    let json_out = binary()
        .args(["mask-rules", &uri, "--format", "json"])
        .output()
        .expect("run mask-rules json");
    assert_eq!(code(&json_out), 0, "json failed: {}", stderr(&json_out));
    let json_str = stdout(&json_out);
    assert!(json_str.contains("\"candidates\":"));
    assert!(json_str.contains("\"policies\":"));

    // 3. Plain format
    let plain_out = binary()
        .args(["mask-rules", &uri, "--format", "plain"])
        .output()
        .expect("run mask-rules plain");
    assert_eq!(code(&plain_out), 0, "plain failed: {}", stderr(&plain_out));
    let plain_str = stdout(&plain_out);
    assert!(plain_str.contains("Ignatius Data Masking & PII Forensics"));

    // 4. SQL format
    let sql_out = binary()
        .args(["mask-rules", &uri, "--format", "sql"])
        .output()
        .expect("run mask-rules sql");
    assert_eq!(code(&sql_out), 0, "sql failed: {}", stderr(&sql_out));
    let sql_str = stdout(&sql_out);
    assert!(sql_str.contains("-- Ignatius Data Masking View Definitions"));

    // 5. Output file writing with force
    let out_file = temp_config_dir().join("generated_mask_views.sql");
    let file_out = binary()
        .args([
            "mask-rules",
            &uri,
            "--format",
            "sql",
            "-o",
            out_file.to_str().unwrap(),
            "--force",
        ])
        .output()
        .expect("run mask-rules to file");
    assert_eq!(
        code(&file_out),
        0,
        "file write failed: {}",
        stderr(&file_out)
    );
    let saved_sql = std::fs::read_to_string(&out_file).expect("read written file");
    assert!(saved_sql.contains("-- Ignatius Data Masking View Definitions"));
}
