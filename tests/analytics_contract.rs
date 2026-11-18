//! Acceptance contract for Feature 055: result analytics, Field Industrial HTML & Parquet exports.

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
        "ignatius-analytics-contract-{}-{}",
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
fn analytics_command_exports_json_xss_safe_html_and_valid_parquet_files() {
    let uri = uri_or_skip();
    if uri.is_empty() {
        return;
    }
    let root = root();

    // 1. JSON analytics profile over seeded orders table
    let json_out = binary(&root)
        .args([
            "analytics",
            &uri,
            "-c",
            "SELECT order_id, currency, total FROM public.orders ORDER BY order_id",
            "--format",
            "json",
        ])
        .output()
        .expect("run analytics json");
    assert_eq!(code(&json_out), 0, "stderr: {}", stderr(&json_out));
    let profile: serde_json::Value =
        serde_json::from_str(&stdout(&json_out)).expect("valid analytics json");
    assert_eq!(profile["column_count"], 3);

    // 2. HTML report escapes XSS payloads cleanly and enforces file overwrite protection
    let html_path = root.join("report.html");
    let html_run = binary(&root)
        .args([
            "analytics",
            &uri,
            "-c",
            "SELECT '<script>alert(1)</script>' AS xss, order_id FROM public.orders LIMIT 5",
            "--title",
            "Orders <Audit>",
            "--format",
            "html",
            "-o",
            html_path.to_str().expect("utf8"),
        ])
        .output()
        .expect("write html report");
    assert_eq!(code(&html_run), 0, "stderr: {}", stderr(&html_run));
    let html_bytes = std::fs::read_to_string(&html_path).expect("read html");
    assert!(html_bytes.contains("Orders &lt;Audit&gt;"));
    assert!(html_bytes.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
    assert!(!html_bytes.contains("<script>alert(1)</script>"));

    // 3. Binary Apache Parquet export starts and ends with PAR1 magic bytes
    let parquet_path = root.join("orders.parquet");
    let parquet_run = binary(&root)
        .args([
            "analytics",
            &uri,
            "-c",
            "SELECT order_id, currency FROM public.orders ORDER BY order_id LIMIT 10",
            "--format",
            "parquet",
            "-o",
            parquet_path.to_str().expect("utf8"),
        ])
        .output()
        .expect("write parquet");
    assert_eq!(code(&parquet_run), 0, "stderr: {}", stderr(&parquet_run));
    let pq = std::fs::read(&parquet_path).expect("read parquet bytes");
    assert!(pq.len() > 12);
    assert_eq!(&pq[..4], b"PAR1");
    assert_eq!(&pq[pq.len() - 4..], b"PAR1");

    let overwrite = binary(&root)
        .args([
            "analytics",
            &uri,
            "-c",
            "SELECT 1 AS n",
            "--format",
            "parquet",
            "-o",
            parquet_path.to_str().expect("utf8"),
        ])
        .output()
        .expect("refuse overwrite");
    assert_ne!(code(&overwrite), 0);

    let _ = std::fs::remove_dir_all(&root);
}
