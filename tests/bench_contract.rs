//! Acceptance contract for Feature 056: query benchmarks and regression suite.

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
        "ignatius-bench-contract-{}-{}",
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
fn bench_command_measures_ab_queries_enforces_rollback_and_checks_regression_gate() {
    let uri = uri_or_skip();
    if uri.is_empty() {
        return;
    }
    let root = root();

    // 1. A/B JSON benchmark over seeded public.orders
    let ab = binary(&root)
        .args([
            "bench",
            &uri,
            "-c",
            "SELECT order_id, customer_id, total FROM public.orders",
            "--candidate",
            "SELECT order_id, customer_id, total FROM public.orders WHERE order_id = 1",
            "--warmup",
            "1",
            "--runs",
            "4",
            "--format",
            "json",
        ])
        .output()
        .expect("run ab bench");
    assert_eq!(code(&ab), 0, "stderr: {}", stderr(&ab));
    let parsed: serde_json::Value = serde_json::from_str(&stdout(&ab)).expect("valid bench json");
    assert_eq!(parsed["baseline"]["warmup_runs"], 1);
    assert_eq!(parsed["baseline"]["measured_runs"], 4);
    assert_eq!(parsed["baseline"]["rollback"], true);
    assert!(parsed["baseline"]["total_rows"].as_u64().unwrap_or(0) >= 12);
    assert_eq!(parsed["candidate"]["measured_runs"], 4);
    assert!(parsed["comparison"]["verdict"].is_string());
    assert_eq!(parsed["failed_gate"], false);

    // 2. Rollback safety verification: DML benchmark DELETEs inside BEGIN...ROLLBACK
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

    let dml_bench = binary(&root)
        .args([
            "bench",
            &uri,
            "-c",
            "DELETE FROM public.orders",
            "--warmup",
            "1",
            "--runs",
            "3",
            "--format",
            "json",
        ])
        .output()
        .expect("run dml bench");
    assert_eq!(code(&dml_bench), 0, "stderr: {}", stderr(&dml_bench));
    let dml_json: serde_json::Value =
        serde_json::from_str(&stdout(&dml_bench)).expect("valid dml bench json");
    assert_eq!(dml_json["baseline"]["rollback"], true);
    assert!(dml_json["baseline"]["total_rows"].as_u64().unwrap_or(0) >= 9);

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
    assert_eq!(code(&count_after), 0);
    assert_eq!(
        stdout(&count_before),
        stdout(&count_after),
        "default rollback mode must preserve all rows in public.orders"
    );

    // 3. Output file creation, overwrite protection, and --force
    let out_path = root.join("bench-report.txt");
    let file_out = binary(&root)
        .args([
            "bench",
            &uri,
            "-c",
            "SELECT order_id FROM public.orders ORDER BY order_id",
            "--warmup",
            "1",
            "--runs",
            "2",
            "--format",
            "table",
            "-o",
            out_path.to_str().expect("utf8"),
        ])
        .output()
        .expect("write bench report");
    assert_eq!(code(&file_out), 0, "stderr: {}", stderr(&file_out));
    let written = std::fs::read_to_string(&out_path).expect("read bench report");
    assert!(written.contains("QUERY BENCHMARK REPORT"));

    let refuse_overwrite = binary(&root)
        .args([
            "bench",
            &uri,
            "-c",
            "SELECT 1",
            "--warmup",
            "0",
            "--runs",
            "1",
            "-o",
            out_path.to_str().expect("utf8"),
        ])
        .output()
        .expect("refuse overwrite");
    assert_eq!(code(&refuse_overwrite), 2);

    let force_overwrite = binary(&root)
        .args([
            "bench",
            &uri,
            "-c",
            "SELECT 1",
            "--warmup",
            "0",
            "--runs",
            "1",
            "--format",
            "plain",
            "-o",
            out_path.to_str().expect("utf8"),
            "--force",
        ])
        .output()
        .expect("force overwrite");
    assert_eq!(
        code(&force_overwrite),
        0,
        "stderr: {}",
        stderr(&force_overwrite)
    );
    let updated = std::fs::read_to_string(&out_path).expect("read overwritten report");
    assert!(updated.contains("mode=single"));

    // 4. Regression gate failure returns exit code 1
    let gate_fail = binary(&root)
        .args([
            "bench",
            &uri,
            "-c",
            "SELECT 1",
            "--candidate",
            "SELECT pg_sleep(0.02)",
            "--warmup",
            "1",
            "--runs",
            "3",
            "--fail-on-regression-pct",
            "10",
            "--format",
            "json",
        ])
        .output()
        .expect("run gate failure bench");
    assert_eq!(code(&gate_fail), 1, "stderr: {}", stderr(&gate_fail));
    let gate_json: serde_json::Value =
        serde_json::from_str(&stdout(&gate_fail)).expect("valid gate json");
    assert_eq!(gate_json["failed_gate"], true);
    assert_eq!(gate_json["comparison"]["regression_detected"], true);

    // 5. Production target with --allow-write permits rollback-safe DML and leaves rows untouched
    let prod_allowed = binary(&root)
        .args([
            "bench",
            &uri,
            "--environment",
            "production",
            "--allow-write",
            "-c",
            "DELETE FROM public.orders",
            "--warmup",
            "0",
            "--runs",
            "1",
            "--format",
            "json",
        ])
        .output()
        .expect("run prod allowed bench");
    assert_eq!(code(&prod_allowed), 0, "stderr: {}", stderr(&prod_allowed));
    let count_after_prod = binary(&root)
        .args([
            "query",
            &uri,
            "-c",
            "SELECT count(*) AS n FROM public.orders",
            "--format",
            "json",
        ])
        .output()
        .expect("count after prod");
    assert_eq!(stdout(&count_before), stdout(&count_after_prod));

    // 6. Production target permits read-only benchmark queries without --allow-write
    let prod_read = binary(&root)
        .args([
            "bench",
            &uri,
            "--environment",
            "production",
            "-c",
            "SELECT 1",
            "--warmup",
            "0",
            "--runs",
            "1",
        ])
        .output()
        .expect("run prod read bench");
    assert_eq!(code(&prod_read), 0, "stderr: {}", stderr(&prod_read));

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn bench_production_target_refuses_writes_before_credential_fetch_or_connection() {
    let root = root();
    std::fs::write(
        root.join("config.toml"),
        "[auth.providers.never_called]\n\
         command = [\"ignatius-no-such-auth-tool\"]\n\
         \n\
         [profiles.prod_guard]\n\
         host = \"127.0.0.1\"\n\
         port = 1\n\
         dbname = \"orders\"\n\
         user = \"app\"\n\
         environment = \"production\"\n\
         auth = \"never_called\"\n",
    )
    .expect("write config.toml");

    for extra in [&[][..], &["--no-rollback"][..]] {
        let mut args = vec!["bench", "@prod_guard", "-c", "DELETE FROM public.orders"];
        args.extend_from_slice(extra);
        let refused = binary(&root)
            .args(args)
            .output()
            .expect("run refused bench");
        assert_eq!(
            code(&refused),
            2,
            "must fail with Usage (exit 2) before auth/connect: {}",
            stderr(&refused)
        );
        let message = stderr(&refused);
        assert!(message.contains("classified as production"), "{message}");
        assert!(message.contains("--allow-write"), "{message}");
        assert!(message.contains("not a security boundary"), "{message}");
    }

    // Candidate write with read baseline is also refused before credential fetch or connect.
    let cand_refused = binary(&root)
        .args([
            "bench",
            "@prod_guard",
            "-c",
            "SELECT 1",
            "--candidate",
            "UPDATE public.orders SET total = total",
        ])
        .output()
        .expect("run candidate refused bench");
    assert_eq!(code(&cand_refused), 2, "{}", stderr(&cand_refused));
    assert!(stderr(&cand_refused).contains("--allow-write"));

    let _ = std::fs::remove_dir_all(&root);
}
