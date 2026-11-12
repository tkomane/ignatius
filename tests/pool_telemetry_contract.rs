//! CLI contract tests for `ignatius pool-stats` (`037-connection-pool-and-pgbouncer-telemetry`).

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
    let dir = std::env::temp_dir().join(format!("ignatius-pool-stats-test-{}", std::process::id()));
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
fn pool_stats_help_and_options_are_complete() {
    let output = binary()
        .args(["pool-stats", "--help"])
        .output()
        .expect("run pool-stats --help");

    assert_eq!(code(&output), 0);
    let help = stdout(&output);
    assert!(help.contains("--section"));
    assert!(help.contains("--format"));
    assert!(help.contains("--output"));
    assert!(help.contains("--force"));
    assert!(help.contains("all"));
    assert!(help.contains("pools"));
    assert!(help.contains("server"));
    assert!(help.contains("wait"));
}

#[test]
fn pool_stats_protects_existing_output_file_without_force() {
    let temp_file = temp_config_dir().join("existing_pool_report.txt");
    std::fs::write(&temp_file, "pre-existing content").expect("write temp file");

    let output = binary()
        .args([
            "pool-stats",
            "-o",
            temp_file.to_str().unwrap(),
            "postgres://localhost:5432/postgres",
        ])
        .output()
        .expect("run pool-stats with output path");

    assert_ne!(code(&output), 0);
    let err_str = stderr(&output);
    assert!(err_str.contains("refusing to overwrite existing file"));
    assert!(err_str.contains("--force"));

    let content = std::fs::read_to_string(&temp_file).expect("read temp file");
    assert_eq!(content, "pre-existing content");
}

#[test]
fn pool_stats_live_contract() {
    let uri = uri_or_skip!();

    // 1. Table format (all sections)
    let table_out = binary()
        .args(["pool-stats", &uri, "--format", "table"])
        .output()
        .expect("run pool-stats table");
    assert_eq!(code(&table_out), 0, "table failed: {}", stderr(&table_out));
    let table_str = stdout(&table_out);
    assert!(table_str.contains("Ignatius Connection Pool & Saturation Telemetry"));
    assert!(table_str.contains("Connections:"));
    assert!(table_str.contains("Headroom:"));
    assert!(table_str.contains("Server Connection Saturation & States"));

    // 2. JSON format
    let json_out = binary()
        .args(["pool-stats", &uri, "--format", "json"])
        .output()
        .expect("run pool-stats json");
    assert_eq!(code(&json_out), 0, "json failed: {}", stderr(&json_out));
    let json_str = stdout(&json_out);
    assert!(json_str.contains("\"server_stats\":"));
    assert!(json_str.contains("\"pools\":"));
    assert!(json_str.contains("\"waiting_clients\":"));

    // 3. Plain format with section filtering: server
    let plain_out = binary()
        .args([
            "pool-stats",
            &uri,
            "--format",
            "plain",
            "--section",
            "server",
        ])
        .output()
        .expect("run pool-stats plain server");
    assert_eq!(code(&plain_out), 0, "plain failed: {}", stderr(&plain_out));
    let plain_str = stdout(&plain_out);
    assert!(plain_str.contains("Server Connection Saturation & States"));

    // 4. Section filtering: pools
    let pools_out = binary()
        .args(["pool-stats", &uri, "--section", "pools"])
        .output()
        .expect("run pool-stats pools");
    assert_eq!(code(&pools_out), 0, "pools failed: {}", stderr(&pools_out));
    let pools_str = stdout(&pools_out);
    assert!(pools_str.contains("Ignatius Connection Pool & Saturation Telemetry"));

    // 5. Output file writing with force
    let out_file = temp_config_dir().join("generated_pool_report.json");
    let file_out = binary()
        .args([
            "pool-stats",
            &uri,
            "--format",
            "json",
            "-o",
            out_file.to_str().unwrap(),
            "--force",
        ])
        .output()
        .expect("run pool-stats to file");
    assert_eq!(
        code(&file_out),
        0,
        "file write failed: {}",
        stderr(&file_out)
    );
    let saved_json = std::fs::read_to_string(&out_file).expect("read written file");
    assert!(saved_json.contains("\"server_stats\":"));
}
