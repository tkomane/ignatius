//! Acceptance contract for Feature 050: psql meta-commands and bounded watch mode.

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
        "ignatius-metacommand-contract-{}-{}",
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
fn metacommand_catalog_queries_and_refusals_work_against_live_postgres() {
    let uri = uri_or_skip();
    if uri.is_empty() {
        return;
    }
    let root = root();

    // 1. \dt lists seeded tables cleanly in JSON format
    let dt = binary(&root)
        .args(["query", &uri, "-c", "\\dt", "--format", "json"])
        .output()
        .expect("run \\dt");
    assert_eq!(code(&dt), 0, "stderr: {}", stderr(&dt));
    let dt_out = stdout(&dt);
    assert!(
        dt_out.contains("orders"),
        "\\dt should list seeded orders table: {dt_out}"
    );

    // 2. \d+ orders describes columns, storage, table_size, and indexes
    let d_plus = binary(&root)
        .args(["query", &uri, "-c", "\\d+ orders", "--format", "json"])
        .output()
        .expect("run \\d+ orders");
    assert_eq!(code(&d_plus), 0, "stderr: {}", stderr(&d_plus));
    let dp_out = stdout(&d_plus);
    assert!(
        dp_out.contains("table_size") && dp_out.contains("storage"),
        "\\d+ should include extended attributes: {dp_out}"
    );

    // 3. \conninfo returns live database & user session facts
    let conninfo = binary(&root)
        .args(["query", &uri, "-c", "\\conninfo", "--format", "json"])
        .output()
        .expect("run \\conninfo");
    assert_eq!(code(&conninfo), 0, "stderr: {}", stderr(&conninfo));
    assert!(stdout(&conninfo).contains("database"));

    // 4. \? outputs built-in meta-command reference
    let help = binary(&root)
        .args(["query", &uri, "-c", "\\?", "--format", "table"])
        .output()
        .expect("run \\?");
    assert_eq!(code(&help), 0, "stderr: {}", stderr(&help));
    assert!(stdout(&help).contains("\\watch"));

    // 5. Shell escape \! is refused explicitly with exit code 2
    let shell = binary(&root)
        .args(["query", &uri, "-c", "\\! id"])
        .output()
        .expect("run \\! refusal");
    assert_eq!(code(&shell), 2, "shell escape must exit 2");

    // 6. TUI-only \watch is refused cleanly in headless query mode with exit code 2
    let watch = binary(&root)
        .args(["query", &uri, "-c", "SELECT 1; \\watch 2"])
        .output()
        .expect("run \\watch refusal");
    assert_eq!(code(&watch), 2, "headless \\watch must exit 2");

    let _ = std::fs::remove_dir_all(&root);
}
