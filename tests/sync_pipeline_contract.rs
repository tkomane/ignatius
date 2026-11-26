//! CLI contract tests for `ignatius sync` (`039-cross-database-copy-and-sync-pipelines`).

#![allow(clippy::print_stderr)]

use std::path::Path;
use std::process::{Command, Output};

const UNREACHABLE_SOURCE: &str =
    "postgres://sync@127.0.0.1:1/source?sslmode=disable&connect_timeout=2";
const LITERAL_DESTINATION: &str =
    "postgres://sync@127.0.0.1:1/destination?sslmode=disable&connect_timeout=2";

fn binary() -> Command {
    binary_in(&temp_config_dir())
}

fn binary_in(root: &Path) -> Command {
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
    assert!(help.contains("--target-environment"));
    assert!(help.contains("--confirm-production"));
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

fn destination_profiles() -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("create isolated sync config");
    std::fs::write(
        root.path().join("config.toml"),
        r#"
[profiles.live]
host = "127.0.0.1"
port = 1
dbname = "destination"
user = "sync"
environment = "prod"
auth = "sync_destination_provider_does_not_exist"

[profiles.development]
host = "127.0.0.1"
port = 1
dbname = "destination"
user = "sync"
environment = "development"
"#,
    )
    .expect("write sync profiles");
    root
}

fn assert_production_refusal(output: &Output) {
    let message = stderr(output);
    assert_eq!(code(output), 2, "{message}");
    assert!(
        message.contains("destination is classified as production"),
        "{message}"
    );
    assert!(message.contains("--confirm-production"), "{message}");
    assert!(output.stdout.is_empty());
}

#[test]
fn sync_pipeline_refuses_production_append_before_authentication() {
    let root = destination_profiles();
    let report = root.path().join("sync-report.json");
    let args = [
        "sync",
        "--source",
        UNREACHABLE_SOURCE,
        "--auth",
        "sync_source_provider_does_not_exist",
        "--environment",
        "staging",
        "--target",
        "@live",
        "--format",
        "json",
        "--force",
    ];
    let refused = binary_in(root.path())
        .args(args)
        .arg("--output")
        .arg(&report)
        .output()
        .expect("run unconfirmed production append");
    assert_production_refusal(&refused);
    assert!(!report.exists());

    for opt in ["--confirm-production", "--dry-run"] {
        let allowed = binary_in(root.path())
            .args(args)
            .arg(opt)
            .output()
            .expect("reach auth with approval or dry run");
        let message = stderr(&allowed);
        assert_eq!(code(&allowed), 3, "{message}");
        assert!(
            message.contains("unknown authentication provider")
                && message.contains("sync_source_provider_does_not_exist"),
            "{message}"
        );
    }
}

#[test]
fn sync_pipeline_refuses_production_truncate_before_connecting() {
    let root = destination_profiles();
    let args = [
        "sync",
        "--source",
        UNREACHABLE_SOURCE,
        "--target",
        "@live",
        "--truncate",
        "--cascade",
    ];
    let refused = binary_in(root.path())
        .args(args)
        .output()
        .expect("run unconfirmed production truncate");
    assert_production_refusal(&refused);

    let confirmed = binary_in(root.path())
        .args(args)
        .arg("--confirm-production")
        .output()
        .expect("connect to source with approval");
    assert_eq!(code(&confirmed), 4, "{}", stderr(&confirmed));
}

#[test]
fn sync_pipeline_guards_a_literal_destination_classified_as_production() {
    let root = tempfile::tempdir().expect("create isolated sync config");
    let refused = binary_in(root.path())
        .args([
            "sync",
            "--source",
            UNREACHABLE_SOURCE,
            "--target",
            LITERAL_DESTINATION,
            "--target-environment",
            "production",
        ])
        .output()
        .expect("run unconfirmed sync to a literal production destination");
    assert_production_refusal(&refused);
}

#[test]
fn sync_pipeline_target_environment_requires_a_literal_destination() {
    let root = destination_profiles();
    let missing = binary_in(root.path())
        .args([
            "sync",
            "--source",
            UNREACHABLE_SOURCE,
            "--dry-run",
            "--target-environment",
            "production",
        ])
        .output()
        .expect("run with a classification and no destination");
    let message = stderr(&missing);
    assert_eq!(code(&missing), 2, "{message}");
    assert!(message.contains("--target <TARGET_TARGET>"), "{message}");

    let override_profile = binary_in(root.path())
        .args([
            "sync",
            "--source",
            UNREACHABLE_SOURCE,
            "--target",
            "@live",
            "--target-environment",
            "staging",
        ])
        .output()
        .expect("run with a destination profile and an environment override");
    let message = stderr(&override_profile);
    assert_eq!(code(&override_profile), 2, "{message}");
    assert!(
        message.contains("--target-environment requires a literal destination"),
        "{message}"
    );
}

#[test]
fn sync_pipeline_can_read_a_production_source_for_a_development_destination() {
    let root = destination_profiles();
    let output = binary_in(root.path())
        .args([
            "sync",
            "--source",
            UNREACHABLE_SOURCE,
            "--environment",
            "production",
            "--target",
            "@development",
        ])
        .output()
        .expect("reach source connection without requiring destination approval");
    assert_eq!(code(&output), 4, "{}", stderr(&output));
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
