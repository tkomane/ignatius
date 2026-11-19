//! Process and live PostgreSQL contracts for the local plan timeline.

#![allow(clippy::print_stderr)]

use std::path::Path;
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

fn json(output: &Output) -> serde_json::Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("timeline JSON")
}

#[test]
fn offline_empty_series_and_capture_refusals_leave_no_state() {
    let root = tempfile::tempdir().expect("temp root");
    let show = binary(root.path())
        .args([
            "plan-history",
            "show",
            "--series",
            "example",
            "--format",
            "json",
        ])
        .output()
        .expect("offline read");
    assert_eq!(
        json(&show)["observations"]
            .as_array()
            .expect("observations")
            .len(),
        0
    );
    for (sql, no_history) in [
        ("SELECT 1; SELECT 2", false),
        ("DELETE FROM orders", false),
        ("SELECT 1", true),
    ] {
        let mut command = binary(root.path());
        if no_history {
            command.arg("--no-history");
        }
        let refused = command
            .args([
                "plan-history",
                "capture",
                "--series",
                "example",
                "--schema",
                "public",
                "-c",
                sql,
            ])
            .output()
            .expect("capture");
        assert_eq!(
            refused.status.code(),
            Some(2),
            "{}",
            String::from_utf8_lossy(&refused.stderr)
        );
    }
    assert!(!root.path().join("data/plan-history").exists());
}

#[test]
fn captures_index_revision_and_read_only_measurement_then_forgets_offline() {
    let Ok(uri) = std::env::var("IGNATIUS_TEST_PG_URI") else {
        eprintln!("skipping: IGNATIUS_TEST_PG_URI is not set");
        return;
    };
    let root = tempfile::tempdir().expect("temp root");
    let schema = format!("ignatius_history_{}", std::process::id());
    let run_setup = |sql: &str| {
        let output = binary(root.path())
            .args(["query", &uri, "--allow-write", "-c", sql])
            .output()
            .expect("fixture SQL");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    run_setup(&format!(
        "CREATE SCHEMA {schema};
         CREATE TABLE {schema}.history_sample (id integer, marker text);
         INSERT INTO {schema}.history_sample
         SELECT i, 'timeline-contract-marker' FROM generate_series(1, 3000) AS i;
         ANALYZE {schema}.history_sample"
    ));
    let query = format!(
        "SELECT id FROM {schema}.history_sample WHERE id = 201 AND marker = 'timeline-contract-marker'"
    );
    let capture = |sql: &str, measure: bool, series: &str| {
        let mut command = binary(root.path());
        command.args([
            "plan-history",
            "capture",
            &uri,
            "--schema",
            &schema,
            "--series",
            series,
            "-c",
            sql,
            "--format",
            "json",
        ]);
        if measure {
            command.arg("--measure");
        }
        command.output().expect("capture")
    };
    let first = json(&capture(&query, false, "revisions"));
    run_setup(&format!(
        "CREATE INDEX history_sample_idx ON {schema}.history_sample (id)"
    ));
    let second = json(&capture(&query, false, "revisions"));
    let observations = second["observations"].as_array().expect("observations");
    assert_eq!(observations.len(), 2);
    assert_eq!(
        observations[0]["plan_sha256"],
        first["observations"][0]["plan_sha256"]
    );
    assert_eq!(observations[1]["changes"]["schema_changed"], true);
    assert_eq!(observations[1]["changes"]["plan_changed"], true);
    assert_eq!(observations[1]["changes"]["comparable"], true);
    let third = json(&capture(&query, true, "revisions"));
    let current = &third["observations"][2];
    assert_eq!(current["changes"]["comparable"], false);
    assert!(current["measurement"]["p95_ms"].is_number());
    assert!(current["measurement"]["shared_read_blocks"].is_number());
    assert!(current["measurement"]["wal_bytes"].is_number());

    let catalog = root.path().join("data/plan-history/timeline.json");
    let bytes = std::fs::read(&catalog).expect("stored catalog");
    let text = std::str::from_utf8(&bytes).expect("UTF-8");
    for secret in [
        "timeline-contract-marker",
        "history_sample",
        "SELECT",
        "postgres://",
    ] {
        assert!(!text.contains(secret));
    }
    let blocked = capture(&format!("{query} FOR UPDATE"), true, "read-lock");
    assert_eq!(
        blocked.status.code(),
        Some(7),
        "{}",
        String::from_utf8_lossy(&blocked.stderr)
    );
    assert!(!String::from_utf8_lossy(&blocked.stderr).contains("timeline-contract-marker"));
    assert_eq!(std::fs::read(&catalog).expect("catalog unchanged"), bytes);

    let removed = binary(root.path())
        .args(["plan-history", "forget", "--series", "revisions"])
        .output()
        .expect("offline forget");
    assert!(removed.status.success());
    assert!(
        !std::fs::read_to_string(&catalog)
            .expect("stored catalog")
            .contains("revisions")
    );
    run_setup(&format!("DROP SCHEMA {schema} CASCADE"));
}

#[test]
fn measures_prepared_read_over_verified_tls() {
    let Ok(uri) = std::env::var("IGNATIUS_TEST_PG_TLS_URI") else {
        eprintln!("skipping: IGNATIUS_TEST_PG_TLS_URI is not set");
        return;
    };
    let root = tempfile::tempdir().expect("temp root");
    let output = binary(root.path())
        .env(
            "PGSSLROOTCERT",
            Path::new(env!("CARGO_MANIFEST_DIR")).join("docker/tls/generated/ca.crt"),
        )
        .args([
            "plan-history",
            "capture",
            &uri,
            "--sslmode",
            "verify-full",
            "--series",
            "verified",
            "--schema",
            "public",
            "-c",
            "SELECT 'synthetic ; marker', \
             1 / pg_catalog.current_setting('transaction_read_only')::bool::int \
             FROM pg_catalog.generate_series(1, 3)",
            "--measure",
            "--format",
            "json",
        ])
        .output()
        .expect("TLS capture");
    let report = json(&output);
    let observations = report["observations"].as_array().expect("observations");
    assert_eq!(observations.len(), 1);
    assert!(observations[0]["measurement"]["p95_ms"].is_number());
    assert!(!String::from_utf8_lossy(&output.stdout).contains("synthetic ; marker"));
    let catalog = std::fs::read_to_string(root.path().join("data/plan-history/timeline.json"))
        .expect("stored catalog");
    assert!(!catalog.contains("synthetic ; marker"));
}
