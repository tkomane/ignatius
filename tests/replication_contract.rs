//! Acceptance contract for Feature 051: logical replication and slot safety manager.

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
        "ignatius-replication-contract-{}-{}",
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
fn replication_command_outputs_table_and_json_and_protects_files() {
    let uri = uri_or_skip();
    if uri.is_empty() {
        return;
    }
    let root = root();

    // 1. Default table report succeeds against PG 18 container
    let table = binary(&root)
        .args(["replication", &uri, "--section", "all", "--format", "table"])
        .output()
        .expect("run replication table");
    assert_eq!(code(&table), 0, "stderr: {}", stderr(&table));
    assert!(stdout(&table).contains("Publications"));

    // 2. JSON report exports atomically to file and refuses overwrite without --force
    let out_path = root.join("replication.json");
    let first = binary(&root)
        .args([
            "replication",
            &uri,
            "--section",
            "slots",
            "--format",
            "json",
            "-o",
            out_path.to_str().expect("utf8"),
        ])
        .output()
        .expect("write replication json");
    assert_eq!(code(&first), 0, "stderr: {}", stderr(&first));
    let written = std::fs::read_to_string(&out_path).expect("read output json");
    let parsed: serde_json::Value = serde_json::from_str(&written).expect("valid json");
    assert!(parsed.get("slots").is_some());

    let overwrite = binary(&root)
        .args([
            "replication",
            &uri,
            "--format",
            "json",
            "-o",
            out_path.to_str().expect("utf8"),
        ])
        .output()
        .expect("refuse overwrite");
    assert_ne!(code(&overwrite), 0, "must refuse overwrite without --force");

    let forced = binary(&root)
        .args([
            "replication",
            &uri,
            "--format",
            "json",
            "-o",
            out_path.to_str().expect("utf8"),
            "--force",
        ])
        .output()
        .expect("force overwrite");
    assert_eq!(code(&forced), 0, "stderr: {}", stderr(&forced));

    let _ = std::fs::remove_dir_all(&root);
}
