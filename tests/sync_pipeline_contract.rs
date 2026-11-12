//! CLI contract tests for `ignatius sync` (`039-cross-database-copy-and-sync-pipelines`).

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
    let dir = std::env::temp_dir().join(format!("ignatius-sync-test-{}", std::process::id()));
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
fn sync_pipeline_help_and_options_are_complete() {
    let output = binary()
        .args(["sync", "--help"])
        .output()
        .expect("run sync --help");

    assert_eq!(code(&output), 0);
    let help = stdout(&output);
    assert!(help.contains("--source"));
    assert!(help.contains("--target"));
    assert!(help.contains("--schema"));
    assert!(help.contains("--table"));
    assert!(help.contains("--truncate"));
    assert!(help.contains("--cascade"));
    assert!(help.contains("--dry-run"));
    assert!(help.contains("--format"));
    assert!(help.contains("--output"));
    assert!(help.contains("--force"));
    assert!(help.contains("table"));
    assert!(help.contains("json"));
    assert!(help.contains("plain"));
}

#[test]
fn sync_pipeline_protects_existing_output_file_without_force() {
    let temp_file = temp_config_dir().join("existing_sync_report.txt");
    std::fs::write(&temp_file, "pre-existing sync report").expect("write temp file");

    let output = binary()
        .args([
            "sync",
            "--dry-run",
            "-o",
            temp_file.to_str().unwrap(),
            "postgres://localhost:5432/postgres",
        ])
        .output()
        .expect("run sync with existing output file");

    assert_ne!(code(&output), 0);
    let err = stderr(&output);
    assert!(err.contains("refusing to overwrite existing file"));
    assert!(err.contains("--force"));
}

#[test]
fn sync_pipeline_refuses_when_target_missing_and_not_dry_run() {
    let output = binary()
        .args(["sync", "-s", "postgres://localhost:5432/postgres"])
        .output()
        .expect("run sync without target");

    assert_ne!(code(&output), 0);
    let err = stderr(&output);
    assert!(err.contains("a target database connection is required"));
}

#[test]
fn sync_pipeline_dry_run_against_live_database() {
    let uri = uri_or_skip!();

    let output = binary()
        .args(["sync", "--dry-run", "-s", &uri, "--format", "json"])
        .output()
        .expect("run sync --dry-run");

    assert_eq!(code(&output), 0, "sync command failed: {}", stderr(&output));
    let out = stdout(&output);
    assert!(out.contains("\"total_rows\""));
    assert!(out.contains("\"table_progress\""));
}
