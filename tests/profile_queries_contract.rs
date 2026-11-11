//! CLI contract tests for `ignatius profile-queries` (`034-query-performance-and-stat-statements`).

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
    let dir = std::env::temp_dir().join(format!("ignatius-perf-test-{}", std::process::id()));
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
fn profile_queries_help_and_options_are_complete() {
    let output = binary()
        .args(["profile-queries", "--help"])
        .output()
        .expect("run profile-queries --help");

    assert_eq!(code(&output), 0);
    let help = stdout(&output);
    assert!(help.contains("--sort"));
    assert!(help.contains("--limit"));
    assert!(help.contains("--min-calls"));
    assert!(help.contains("--format"));
    assert!(help.contains("--output"));
    assert!(help.contains("--force"));
    assert!(help.contains("--reset"));
}

#[test]
fn profile_queries_protects_existing_output_file_without_force() {
    let temp_file = temp_config_dir().join("existing_perf_report.txt");
    std::fs::write(&temp_file, "pre-existing content").expect("write temp file");

    let output = binary()
        .args([
            "profile-queries",
            "-o",
            temp_file.to_str().unwrap(),
            "postgres://localhost:5432/postgres",
        ])
        .output()
        .expect("run profile-queries with output path");

    assert_ne!(code(&output), 0);
    let err_str = stderr(&output);
    assert!(err_str.contains("refusing to overwrite existing file"));
    assert!(err_str.contains("--force"));

    let content = std::fs::read_to_string(&temp_file).expect("read temp file");
    assert_eq!(content, "pre-existing content");
}

#[test]
fn profile_queries_live_contract() {
    let uri = uri_or_skip!();

    // 1. Table format
    let table_out = binary()
        .args(["profile-queries", &uri, "--format", "table"])
        .output()
        .expect("run profile-queries table");
    assert_eq!(code(&table_out), 0, "table failed: {}", stderr(&table_out));
    let table_str = stdout(&table_out);
    assert!(
        table_str.contains("STATISTICS SUMMARY")
            || table_str.contains("No recorded queries")
            || table_str.contains("unavailable")
    );

    // 2. JSON format
    let json_out = binary()
        .args(["profile-queries", &uri, "--format", "json"])
        .output()
        .expect("run profile-queries json");
    assert_eq!(code(&json_out), 0, "json failed: {}", stderr(&json_out));
    let json_str = stdout(&json_out);
    assert!(json_str.contains("\"extension_available\":"));

    // 3. Plain format with sorting
    let plain_out = binary()
        .args([
            "profile-queries",
            &uri,
            "--format",
            "plain",
            "--sort",
            "mean",
        ])
        .output()
        .expect("run profile-queries plain");
    assert_eq!(code(&plain_out), 0, "plain failed: {}", stderr(&plain_out));

    // 4. Reset stats
    let reset_out = binary()
        .args(["profile-queries", &uri, "--reset"])
        .output()
        .expect("run profile-queries --reset");
    let reset_str = stdout(&reset_out);
    let reset_err = stderr(&reset_out);
    if code(&reset_out) == 0 {
        assert!(
            reset_str.contains("Reset pg_stat_statements query execution statistics successfully")
        );
    } else {
        assert!(
            reset_err.contains("pg_stat_statements")
                || reset_err.contains("function pg_stat_statements_reset() does not exist")
        );
    }
}
