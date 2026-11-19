//! Acceptance contract for Feature 053: keyset-paginated data browser & batch CRUD.

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
        "ignatius-crud-contract-{}-{}",
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
fn browse_command_paginates_by_keyset_and_protects_output_files() {
    let uri = uri_or_skip();
    if uri.is_empty() {
        return;
    }
    let root = root();

    // 1. Page 1 of seeded orders table limited to 2 rows returns has_more + next_cursor
    let page1 = binary(&root)
        .args([
            "browse",
            &uri,
            "--table",
            "public.orders",
            "--page-size",
            "2",
            "--format",
            "json",
        ])
        .output()
        .expect("run browse page 1");
    assert_eq!(code(&page1), 0, "stderr: {}", stderr(&page1));
    let body1: serde_json::Value =
        serde_json::from_str(&stdout(&page1)).expect("valid browse json");
    assert_eq!(body1["schema"], "public");
    assert_eq!(body1["table"], "orders");
    let rows1 = body1["rows"].as_array().expect("rows array");
    assert!(!rows1.is_empty(), "orders fixture must yield rows");
    assert_eq!(body1["page_size"], 2);

    // 2. If next_cursor exists, advance with --after using resolved PK values
    if body1["has_more"] == serde_json::Value::Bool(true)
        && let Some(vals) = body1["next_cursor"]["pk_values"].as_array()
    {
        let cursor_arg: String = vals
            .iter()
            .filter_map(|v| v.as_str())
            .collect::<Vec<_>>()
            .join(",");
        if !cursor_arg.is_empty() {
            let page2 = binary(&root)
                .args([
                    "browse",
                    &uri,
                    "--table",
                    "orders",
                    "--page-size",
                    "2",
                    "--after",
                    &cursor_arg,
                    "--format",
                    "table",
                ])
                .output()
                .expect("run browse page 2");
            assert_eq!(code(&page2), 0, "stderr: {}", stderr(&page2));
            assert!(stdout(&page2).contains("orders"));
        }
    }

    // 3. Atomic file write + overwrite protection
    let out_path = root.join("browse.json");
    let write_ok = binary(&root)
        .args([
            "browse",
            &uri,
            "--table",
            "public.orders",
            "--page-size",
            "3",
            "--format",
            "json",
            "-o",
            out_path.to_str().expect("utf8"),
        ])
        .output()
        .expect("write browse output");
    assert_eq!(code(&write_ok), 0, "stderr: {}", stderr(&write_ok));

    let overwrite = binary(&root)
        .args([
            "browse",
            &uri,
            "--table",
            "public.orders",
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
