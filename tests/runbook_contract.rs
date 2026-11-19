//! Acceptance contract for Feature 058: Markdown SQL runbook runner and write-guard safety.

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
        "ignatius-runbook-contract-{}-{}",
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
fn runbook_dry_run_works_offline_and_live_execution_enforces_expectations_and_write_guard() {
    let root = root();

    // 1. Offline --dry-run (no database URI needed!)
    let guarded_md = root.join("guarded.md");
    std::fs::write(
        &guarded_md,
        "# Orders Playbook\n\n\
         ## Count\n\
         ```sql\n\
         -- @name: count-orders\n\
         -- @expect-rows: 1\n\
         SELECT count(*) AS total_orders FROM public.orders;\n\
         ```\n\n\
         ## Purge\n\
         ```sql\n\
         -- @name: delete-orders\n\
         DELETE FROM public.orders;\n\
         ```\n",
    )
    .expect("write guarded runbook");

    let dry_run = binary(&root)
        .args([
            "runbook",
            "-f",
            guarded_md.to_str().expect("utf8"),
            "--dry-run",
            "--format",
            "json",
        ])
        .output()
        .expect("run offline dry-run");
    assert_eq!(code(&dry_run), 0, "stderr: {}", stderr(&dry_run));
    let dry_json: serde_json::Value =
        serde_json::from_str(&stdout(&dry_run)).expect("valid dry-run json");
    assert_eq!(dry_json["dry_run"], true);
    assert_eq!(dry_json["allow_writes"], false);
    assert_eq!(dry_json["blocked_count"], 1);
    assert_eq!(dry_json["cells"][0]["status"], "planned");
    assert_eq!(dry_json["cells"][1]["status"], "blocked-by-write-guard");

    let uri = uri_or_skip();
    if uri.is_empty() {
        let _ = std::fs::remove_dir_all(&root);
        return;
    }

    // 2. Live read-only runbook with passing @expect-rows and --format markdown export
    let readonly_md = root.join("readonly.md");
    std::fs::write(
        &readonly_md,
        "# Orders Health Check\n\n\
         ## Row Count\n\
         ```sql\n\
         -- @name: count-orders\n\
         -- @expect-rows: 1\n\
         SELECT count(*) AS n FROM public.orders;\n\
         ```\n\n\
         ## Seeded Customer Check\n\
         ```sql\n\
         -- @name: seeded-orders\n\
         -- @expect-rows: 3\n\
         SELECT order_id, customer_id, total FROM public.orders WHERE customer_id IN (10482, 10483, 10484) ORDER BY order_id;\n\
         ```\n",
    )
    .expect("write readonly runbook");

    let report_md_path = root.join("runbook-report.md");
    let live_ok = binary(&root)
        .args([
            "runbook",
            &uri,
            "-f",
            readonly_md.to_str().expect("utf8"),
            "--format",
            "markdown",
            "-o",
            report_md_path.to_str().expect("utf8"),
        ])
        .output()
        .expect("run readonly runbook");
    assert_eq!(code(&live_ok), 0, "stderr: {}", stderr(&live_ok));
    let report_md = std::fs::read_to_string(&report_md_path).expect("read markdown report");
    assert!(report_md.contains("# Runbook Report:"));
    assert!(report_md.contains("count-orders"));
    assert!(report_md.contains("seeded-orders"));
    assert!(report_md.contains("| passed |"));

    // 3. @expect-rows failure exits code 1
    let bad_expect_md = root.join("bad-expect.md");
    std::fs::write(
        &bad_expect_md,
        "# Failing Expectation\n\n\
         ```sql\n\
         -- @name: bad-count\n\
         -- @expect-rows: 99\n\
         SELECT count(*) FROM public.orders;\n\
         ```\n",
    )
    .expect("write bad expect runbook");
    let expect_fail = binary(&root)
        .args([
            "runbook",
            &uri,
            "-f",
            bad_expect_md.to_str().expect("utf8"),
            "--format",
            "json",
        ])
        .output()
        .expect("run bad expect runbook");
    assert_eq!(code(&expect_fail), 1, "stderr: {}", stderr(&expect_fail));
    let expect_json: serde_json::Value =
        serde_json::from_str(&stdout(&expect_fail)).expect("valid failure json");
    assert_eq!(expect_json["failed_count"], 1);
    assert_eq!(expect_json["cells"][0]["status"], "failed");
    assert!(
        expect_json["cells"][0]["message"]
            .as_str()
            .is_some_and(|m| m.contains("expected 99 row(s), got 1"))
    );

    // 4. Write-guard blocks DELETE without --allow-writes (exits code 1, leaves DB untouched)
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

    let guard_block = binary(&root)
        .args([
            "runbook",
            &uri,
            "-f",
            guarded_md.to_str().expect("utf8"),
            "--format",
            "json",
        ])
        .output()
        .expect("run guarded runbook");
    assert_eq!(code(&guard_block), 1, "stderr: {}", stderr(&guard_block));
    let guard_json: serde_json::Value =
        serde_json::from_str(&stdout(&guard_block)).expect("valid guard json");
    assert_eq!(guard_json["passed_count"], 1);
    assert_eq!(guard_json["blocked_count"], 1);
    assert_eq!(guard_json["cells"][0]["status"], "passed");
    assert_eq!(guard_json["cells"][1]["status"], "blocked-by-write-guard");

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
    assert_eq!(stdout(&count_before), stdout(&count_after));

    // 5. --allow-writes executes a session-local write cell cleanly, and --stop-on-error halts on error
    let allow_write_md = root.join("temp-write.md");
    std::fs::write(
        &allow_write_md,
        "# Temp Scratch Write\n\n\
         ```sql\n\
         -- @name: temp-insert\n\
         -- @expect-rows: 2\n\
         CREATE TEMP TABLE ignatius_rb_tmp (n integer); INSERT INTO ignatius_rb_tmp VALUES (1), (2);\n\
         ```\n",
    )
    .expect("write temp-write runbook");
    let allow_ok = binary(&root)
        .args([
            "runbook",
            &uri,
            "-f",
            allow_write_md.to_str().expect("utf8"),
            "--allow-writes",
            "--format",
            "json",
        ])
        .output()
        .expect("run allow-writes");
    assert_eq!(code(&allow_ok), 0, "stderr: {}", stderr(&allow_ok));
    let allow_json: serde_json::Value =
        serde_json::from_str(&stdout(&allow_ok)).expect("valid allow json");
    assert_eq!(allow_json["passed_count"], 1);
    assert_eq!(allow_json["blocked_count"], 0);

    let stop_md = root.join("stop-on-error.md");
    std::fs::write(
        &stop_md,
        "# Stop On Error\n\n\
         ```sql\n\
         -- @name: broken\n\
         SELECT * FROM public.nonexistent_runbook_table;\n\
         ```\n\n\
         ```sql\n\
         -- @name: never-runs\n\
         SELECT 1;\n\
         ```\n",
    )
    .expect("write stop-on-error runbook");
    let stop_res = binary(&root)
        .args([
            "runbook",
            &uri,
            "-f",
            stop_md.to_str().expect("utf8"),
            "--stop-on-error",
            "--format",
            "json",
        ])
        .output()
        .expect("run stop-on-error");
    assert_eq!(code(&stop_res), 1, "stderr: {}", stderr(&stop_res));
    let stop_json: serde_json::Value =
        serde_json::from_str(&stdout(&stop_res)).expect("valid stop json");
    assert_eq!(stop_json["failed_count"], 1);
    assert_eq!(stop_json["skipped_count"], 1);
    assert_eq!(stop_json["cells"][0]["status"], "failed");
    assert_eq!(stop_json["cells"][1]["status"], "skipped");

    let _ = std::fs::remove_dir_all(&root);
}
