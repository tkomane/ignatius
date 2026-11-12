//! CLI contract tests for `ignatius seed` (`040-synthetic-test-fixture-generator`).

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
    let dir = std::env::temp_dir().join(format!("ignatius-seed-test-{}", std::process::id()));
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
fn seed_help_and_options_are_complete() {
    let output = binary()
        .args(["seed", "--help"])
        .output()
        .expect("run seed --help");

    assert_eq!(code(&output), 0);
    let help = stdout(&output);
    assert!(help.contains("--schema"));
    assert!(help.contains("--table"));
    assert!(help.contains("--rows"));
    assert!(help.contains("--seed"));
    assert!(help.contains("--insert"));
    assert!(help.contains("--format"));
    assert!(help.contains("--output"));
    assert!(help.contains("--force"));
    assert!(help.contains("sql"));
    assert!(help.contains("jsonl"));
    assert!(help.contains("csv"));
    assert!(help.contains("table"));
}

#[test]
fn seed_protects_existing_output_file_without_force() {
    let temp_file = temp_config_dir().join("existing_seed_file.sql");
    std::fs::write(&temp_file, "pre-existing seed data").expect("write temp file");

    let output = binary()
        .args([
            "seed",
            "-o",
            temp_file.to_str().unwrap(),
            "postgres://localhost:5432/postgres",
        ])
        .output()
        .expect("run seed with existing output file");

    assert_ne!(code(&output), 0);
    let err = stderr(&output);
    assert!(err.contains("refusing to overwrite existing file"));
    assert!(err.contains("--force"));
}

#[test]
fn seed_sql_generation_against_live_database() {
    let uri = uri_or_skip!();

    let output = binary()
        .args([
            "seed", &uri, "--rows", "3", "--seed", "42", "--format", "sql",
        ])
        .output()
        .expect("run seed sql format");

    assert_eq!(code(&output), 0, "seed command failed: {}", stderr(&output));
    let out = stdout(&output);
    assert!(out.contains("BEGIN;"));
    assert!(out.contains("COMMIT;"));
}

#[test]
fn seed_jsonl_generation_against_live_database() {
    let uri = uri_or_skip!();

    let output = binary()
        .args([
            "seed", &uri, "--rows", "2", "--seed", "77", "--format", "jsonl",
        ])
        .output()
        .expect("run seed jsonl format");

    assert_eq!(code(&output), 0, "seed command failed: {}", stderr(&output));
    let out = stdout(&output);
    assert!(out.contains("\"_table\""));
    assert!(out.contains("\"data\""));
}

#[test]
fn seed_table_summary_against_live_database() {
    let uri = uri_or_skip!();

    let output = binary()
        .args([
            "seed", &uri, "--rows", "5", "--seed", "123", "--format", "table",
        ])
        .output()
        .expect("run seed table format");

    assert_eq!(code(&output), 0, "seed command failed: {}", stderr(&output));
    let out = stdout(&output);
    assert!(out.contains("Seed: 123"));
    assert!(out.contains("COLUMNS"));
    assert!(out.contains("ROWS"));
}
