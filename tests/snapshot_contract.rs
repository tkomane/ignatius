//! Acceptance contract for Feature 057: canonical schema snapshots and drift guard.

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
        "ignatius-snapshot-contract-{}-{}",
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

#[test]
fn snapshot_capture_and_drift_check_contract_without_mutating_database() {
    let uri = uri_or_skip();
    if uri.is_empty() {
        return;
    }
    let root = root();

    // 1. Capture baseline snapshot JSON for public schema
    let snap_path = root.join("public-snapshot.json");
    let capture = binary(&root)
        .args([
            "snapshot",
            "capture",
            &uri,
            "--schema",
            "public",
            "-o",
            snap_path.to_str().expect("utf8"),
        ])
        .output()
        .expect("capture snapshot");
    assert_eq!(code(&capture), 0, "stderr: {}", stderr(&capture));
    let raw_snap = std::fs::read_to_string(&snap_path).expect("read snapshot json");
    let snap_json: serde_json::Value =
        serde_json::from_str(&raw_snap).expect("valid snapshot json");
    assert_eq!(snap_json["schema_version"], 1);
    assert_eq!(snap_json["schema_filter"], "public");
    assert_eq!(
        snap_json["fingerprint_sha256"].as_str().map(str::len),
        Some(64)
    );
    let tables = snap_json["tables"].as_array().expect("tables array");
    let orders = tables
        .iter()
        .find(|t| t["name"] == "orders")
        .expect("public.orders captured");
    let col_names: Vec<&str> = orders["columns"]
        .as_array()
        .expect("columns array")
        .iter()
        .filter_map(|c| c["name"].as_str())
        .collect();
    for expected_col in [
        "order_id",
        "customer_id",
        "total",
        "currency",
        "note",
        "created_at",
    ] {
        assert!(
            col_names.contains(&expected_col),
            "missing column {expected_col}: {col_names:?}"
        );
    }

    // Also test table and plain formats on capture
    let table_cap = binary(&root)
        .args([
            "snapshot", "capture", &uri, "--schema", "public", "--format", "table",
        ])
        .output()
        .expect("capture table");
    assert_eq!(code(&table_cap), 0, "stderr: {}", stderr(&table_cap));
    assert!(stdout(&table_cap).contains("public.orders"));

    // 2. Check live DB against itself -> exit 0, zero drift
    let check_clean = binary(&root)
        .args([
            "snapshot",
            "check",
            &uri,
            "--against",
            snap_path.to_str().expect("utf8"),
            "--format",
            "json",
        ])
        .output()
        .expect("check clean snapshot");
    assert_eq!(code(&check_clean), 0, "stderr: {}", stderr(&check_clean));
    let clean_json: serde_json::Value =
        serde_json::from_str(&stdout(&check_clean)).expect("valid drift json");
    assert_eq!(clean_json["fingerprints_match"], true);
    assert_eq!(clean_json["breaking_count"], 0);
    assert_eq!(clean_json["warning_count"], 0);
    assert_eq!(clean_json["info_count"], 0);
    assert_eq!(clean_json["findings"].as_array().map(Vec::len), Some(0));

    // 3. Mutate expected JSON locally (synthetic column + changed fingerprint) without mutating DB
    let mut drifted = snap_json.clone();
    drifted["fingerprint_sha256"] = serde_json::Value::String("0".repeat(64));
    let drifted_tables = drifted["tables"].as_array_mut().expect("tables mut");
    let orders_mut = drifted_tables
        .iter_mut()
        .find(|t| t["name"] == "orders")
        .expect("orders mut");
    orders_mut["columns"]
        .as_array_mut()
        .expect("columns mut")
        .push(serde_json::json!({
            "name": "phantom_col",
            "data_type": "text",
            "nullable": true,
            "default_expr": null,
            "ordinal": 99
        }));
    let drifted_path = root.join("drifted-expected.json");
    std::fs::write(
        &drifted_path,
        serde_json::to_string_pretty(&drifted).expect("serialize drifted"),
    )
    .expect("write drifted snapshot");

    let report_path = root.join("drift-report.json");
    let check_drift = binary(&root)
        .args([
            "snapshot",
            "check",
            &uri,
            "--against",
            drifted_path.to_str().expect("utf8"),
            "--fail-on",
            "breaking",
            "--format",
            "json",
            "-o",
            report_path.to_str().expect("utf8"),
        ])
        .output()
        .expect("check drifted snapshot");
    assert_eq!(code(&check_drift), 1, "stderr: {}", stderr(&check_drift));
    let drift_report: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&report_path).expect("read drift report"))
            .expect("valid drift report json");
    assert_eq!(drift_report["fingerprints_match"], false);
    assert!(drift_report["breaking_count"].as_u64().unwrap_or(0) >= 1);
    let findings = drift_report["findings"].as_array().expect("findings array");
    assert!(
        findings
            .iter()
            .any(|f| f["kind"] == "column_dropped" && f["object"] == "public.orders.phantom_col"),
        "expected column_dropped finding on public.orders.phantom_col: {findings:?}"
    );

    // 4. Overwrite protection and --fail-on never
    let refuse_overwrite = binary(&root)
        .args([
            "snapshot",
            "check",
            &uri,
            "--against",
            drifted_path.to_str().expect("utf8"),
            "-o",
            report_path.to_str().expect("utf8"),
        ])
        .output()
        .expect("refuse overwrite");
    assert_eq!(code(&refuse_overwrite), 2);

    let pass_never = binary(&root)
        .args([
            "snapshot",
            "check",
            &uri,
            "--against",
            drifted_path.to_str().expect("utf8"),
            "--fail-on",
            "never",
            "--format",
            "table",
            "-o",
            report_path.to_str().expect("utf8"),
            "--force",
        ])
        .output()
        .expect("check with fail-on never");
    assert_eq!(code(&pass_never), 0, "stderr: {}", stderr(&pass_never));
    let table_report = std::fs::read_to_string(&report_path).expect("read table drift report");
    assert!(table_report.contains("DRIFT DETECTED"));
    assert!(table_report.contains("public.orders.phantom_col"));

    let _ = std::fs::remove_dir_all(&root);
}
