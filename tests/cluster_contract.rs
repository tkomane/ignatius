//! Acceptance contract for Feature 052: server config, HBA, extensions, tablespaces & FDWs.

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
        "ignatius-cluster-contract-{}-{}",
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
fn cluster_command_inspects_settings_hba_extensions_and_respects_filters() {
    let uri = uri_or_skip();
    if uri.is_empty() {
        return;
    }
    let root = root();

    // 1. Settings section with substring filter returns matching GUC parameter
    let settings = binary(&root)
        .args([
            "cluster",
            &uri,
            "--section",
            "settings",
            "--filter",
            "max_connections",
            "--format",
            "json",
        ])
        .output()
        .expect("run cluster settings");
    assert_eq!(code(&settings), 0, "stderr: {}", stderr(&settings));
    let body = stdout(&settings);
    let parsed: serde_json::Value = serde_json::from_str(&body).expect("valid json");
    let rows = parsed["settings"].as_array().expect("settings array");
    assert!(
        rows.iter().any(|s| s["name"] == "max_connections"),
        "expected max_connections in filtered cluster JSON: {body}"
    );

    // 2. Full plain/table cluster report executes cleanly and handles restricted HBA gracefully
    let table = binary(&root)
        .args(["cluster", &uri, "--section", "all", "--format", "table"])
        .output()
        .expect("run cluster table");
    assert_eq!(code(&table), 0, "stderr: {}", stderr(&table));
    let out = stdout(&table);
    assert!(out.contains("Server Settings") && out.contains("Extensions"));

    // 3. Atomic output & overwrite protection
    let out_path = root.join("cluster.json");
    let write_ok = binary(&root)
        .args([
            "cluster",
            &uri,
            "--section",
            "extensions",
            "--modified-only",
            "--format",
            "json",
            "-o",
            out_path.to_str().expect("utf8"),
        ])
        .output()
        .expect("write cluster json");
    assert_eq!(code(&write_ok), 0, "stderr: {}", stderr(&write_ok));

    let overwrite = binary(&root)
        .args([
            "cluster",
            &uri,
            "--format",
            "json",
            "-o",
            out_path.to_str().expect("utf8"),
        ])
        .output()
        .expect("refuse overwrite");
    assert_ne!(code(&overwrite), 0);

    let _ = std::fs::remove_dir_all(&root);
}
