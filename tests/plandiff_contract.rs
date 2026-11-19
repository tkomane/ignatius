//! Acceptance contract for Feature 054: EXPLAIN (BUFFERS, WAL, SETTINGS) & A/B Plan Diff.

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
        "ignatius-plandiff-contract-{}-{}",
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
fn plan_diff_command_compares_plans_with_buffers_and_rolls_back_analyze() {
    let uri = uri_or_skip();
    if uri.is_empty() {
        return;
    }
    let root = root();

    // 1. Compare full table scan vs point lookup with --analyze (BUFFERS + WAL + SETTINGS)
    let diff = binary(&root)
        .args([
            "plan-diff",
            &uri,
            "--baseline",
            "SELECT * FROM public.orders",
            "--candidate",
            "SELECT * FROM public.orders WHERE order_id = 1",
            "--analyze",
            "--format",
            "json",
        ])
        .output()
        .expect("run plan-diff analyze");
    assert_eq!(code(&diff), 0, "stderr: {}", stderr(&diff));
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout(&diff)).expect("valid plan-diff json");
    assert_eq!(parsed["baseline"]["analyzed"], true);
    assert_eq!(parsed["candidate"]["analyzed"], true);
    assert!(parsed.get("verdict").is_some());

    // 2. Rollback safety verification: analyzed DELETE does NOT delete rows!
    let count_before = binary(&root)
        .args([
            "query",
            &uri,
            "-c",
            "SELECT count(*) AS n FROM public.orders",
            "--format",
            "json",
        ])
        .output()
        .expect("count before");
    assert_eq!(code(&count_before), 0);

    let dml_explain = binary(&root)
        .args([
            "plan-diff",
            &uri,
            "--baseline",
            "DELETE FROM public.orders",
            "--analyze",
            "--format",
            "table",
        ])
        .output()
        .expect("analyzed delete explain");
    assert_eq!(code(&dml_explain), 0, "stderr: {}", stderr(&dml_explain));

    let count_after = binary(&root)
        .args([
            "query",
            &uri,
            "-c",
            "SELECT count(*) AS n FROM public.orders",
            "--format",
            "json",
        ])
        .output()
        .expect("count after");
    assert_eq!(
        stdout(&count_before),
        stdout(&count_after),
        "rollback must preserve all rows"
    );

    let _ = std::fs::remove_dir_all(&root);
}
