//! CLI contract tests for `ignatius index-advisor` (`035-index-advisor-and-bloat-forensics`).

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
    let dir = std::env::temp_dir().join(format!("ignatius-advisor-test-{}", std::process::id()));
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
fn index_advisor_help_and_options_are_complete() {
    let output = binary()
        .args(["index-advisor", "--help"])
        .output()
        .expect("run index-advisor --help");

    assert_eq!(code(&output), 0);
    let help = stdout(&output);
    assert!(help.contains("--section"));
    assert!(help.contains("--format"));
    assert!(help.contains("--output"));
    assert!(help.contains("--force"));
    assert!(help.contains("--min-size"));
}

#[test]
fn index_advisor_protects_existing_output_file_without_force() {
    let temp_file = temp_config_dir().join("existing_advisor_report.txt");
    std::fs::write(&temp_file, "pre-existing content").expect("write temp file");

    let output = binary()
        .args([
            "index-advisor",
            "-o",
            temp_file.to_str().unwrap(),
            "postgres://localhost:5432/postgres",
        ])
        .output()
        .expect("run index-advisor with output path");

    assert_ne!(code(&output), 0);
    let err_str = stderr(&output);
    assert!(err_str.contains("refusing to overwrite existing file"));
    assert!(err_str.contains("--force"));

    let content = std::fs::read_to_string(&temp_file).expect("read temp file");
    assert_eq!(content, "pre-existing content");
}

#[test]
fn index_advisor_live_contract() {
    let uri = uri_or_skip!();

    // 1. Table format (all sections)
    let table_out = binary()
        .args(["index-advisor", &uri, "--format", "table"])
        .output()
        .expect("run index-advisor table");
    assert_eq!(code(&table_out), 0, "table failed: {}", stderr(&table_out));
    let table_str = stdout(&table_out);
    assert!(table_str.contains("Ignatius Index Advisor & Bloat Forensics"));
    assert!(table_str.contains("Health Score:"));
    assert!(table_str.contains("Reclaimable:"));

    // 2. JSON format
    let json_out = binary()
        .args(["index-advisor", &uri, "--format", "json"])
        .output()
        .expect("run index-advisor json");
    assert_eq!(code(&json_out), 0, "json failed: {}", stderr(&json_out));
    let json_str = stdout(&json_out);
    assert!(json_str.contains("\"redundant_indexes\":"));
    assert!(json_str.contains("\"bloated_indexes\":"));
    assert!(json_str.contains("\"unindexed_foreign_keys\":"));

    // 3. Plain format with section filtering
    let plain_out = binary()
        .args([
            "index-advisor",
            &uri,
            "--format",
            "plain",
            "--section",
            "redundant",
        ])
        .output()
        .expect("run index-advisor plain redundant");
    assert_eq!(code(&plain_out), 0, "plain failed: {}", stderr(&plain_out));

    // 4. Section filtering: bloat with min-size
    let bloat_out = binary()
        .args([
            "index-advisor",
            &uri,
            "--section",
            "bloat",
            "--min-size",
            "10",
        ])
        .output()
        .expect("run index-advisor bloat min-size");
    assert_eq!(code(&bloat_out), 0, "bloat failed: {}", stderr(&bloat_out));

    // 5. Output file writing with force
    let out_file = temp_config_dir().join("generated_advisor.json");
    let file_out = binary()
        .args([
            "index-advisor",
            &uri,
            "--format",
            "json",
            "-o",
            out_file.to_str().unwrap(),
            "--force",
        ])
        .output()
        .expect("run index-advisor to file");
    assert_eq!(
        code(&file_out),
        0,
        "file write failed: {}",
        stderr(&file_out)
    );
    let saved_json = std::fs::read_to_string(&out_file).expect("read written file");
    assert!(saved_json.contains("\"redundant_indexes\":"));
}
