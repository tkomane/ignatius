//! Acceptance contract for Feature 045: migration runner and ledger.

#![allow(clippy::print_stderr)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::{Mutex, MutexGuard};

/// The ledger and probe tables are shared by every test in this process; the
/// tests serialize their database work rather than trying to out-parallel each
/// other on one disposable server.
static DB_LOCK: Mutex<()> = Mutex::new(());

fn lock() -> MutexGuard<'static, ()> {
    DB_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

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
        "ignatius-migrate-contract-{}-{}",
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

fn cleanup(uri: &str, migrations: &Path) {
    let sql = "DROP TABLE IF EXISTS ignatius_schema_migrations; \
               DROP TABLE IF EXISTS ignatius_mig_alpha; \
               DROP TABLE IF EXISTS ignatius_mig_beta; \
               DROP TABLE IF EXISTS ignatius_mig_gamma;";
    let dir = root();
    let output = binary(&dir)
        .args(["query", uri, "-c", sql])
        .output()
        .expect("cleanup query");
    assert_eq!(code(&output), 0, "cleanup: {}", stderr(&output));
    for entry in std::fs::read_dir(migrations)
        .into_iter()
        .flatten()
        .flatten()
    {
        let _ = std::fs::remove_file(entry.path());
    }
}

fn write_migration(dir: &Path, name: &str, sql: &str) {
    std::fs::write(dir.join(name), sql).expect("write migration");
}

#[test]
fn apply_is_ordered_ledgered_and_idempotent() {
    let uri = uri_or_skip();
    if uri.is_empty() {
        return;
    }
    let _guard = lock();
    let state = root();
    let dir = state.join("migrations");
    std::fs::create_dir_all(&dir).expect("mkdir");
    cleanup(&uri, &dir);

    write_migration(
        &dir,
        "9001_alpha.sql",
        "CREATE TABLE ignatius_mig_alpha (id integer PRIMARY KEY);",
    );
    write_migration(
        &dir,
        "9002_beta.sql",
        "CREATE TABLE ignatius_mig_beta (id integer PRIMARY KEY);",
    );

    let status = binary(&state)
        .args(["migrate", "status", &uri, "--dir"])
        .arg(&dir)
        .output()
        .expect("status");
    assert_eq!(code(&status), 0, "{}", stderr(&status));
    assert!(stdout(&status).contains("pending"), "{}", stdout(&status));

    let status_json = binary(&state)
        .args(["migrate", "status", &uri, "--dir"])
        .arg(&dir)
        .args(["--format", "json"])
        .output()
        .expect("status json");
    assert_eq!(code(&status_json), 0, "{}", stderr(&status_json));
    let value: serde_json::Value =
        serde_json::from_str(&stdout(&status_json)).expect("valid status json");
    assert_eq!(value["pending"], 2);
    assert_eq!(value["clean"], true);

    let apply = binary(&state)
        .args(["migrate", "apply", &uri, "--dir"])
        .arg(&dir)
        .output()
        .expect("apply");
    assert_eq!(code(&apply), 0, "{}", stderr(&apply));
    assert!(stdout(&apply).contains("9001"), "{}", stdout(&apply));
    assert!(stdout(&apply).contains("9002"), "{}", stdout(&apply));

    let count = binary(&state)
        .args([
            "query",
            &uri,
            "-c",
            "SELECT count(*) FROM ignatius_schema_migrations",
            "--format",
            "csv",
        ])
        .output()
        .expect("ledger count");
    assert_eq!(code(&count), 0, "{}", stderr(&count));
    assert_eq!(stdout(&count), "count\n2\n");

    let second = binary(&state)
        .args(["migrate", "apply", &uri, "--dir"])
        .arg(&dir)
        .output()
        .expect("second apply");
    assert_eq!(code(&second), 0, "{}", stderr(&second));
    assert!(
        stdout(&second).contains("No pending migrations were applied."),
        "{}",
        stdout(&second)
    );

    let verify = binary(&state)
        .args(["migrate", "verify", &uri, "--dir"])
        .arg(&dir)
        .output()
        .expect("verify");
    assert_eq!(code(&verify), 0, "{}", stderr(&verify));
    assert!(stdout(&verify).contains("matches 2 file(s)"));
}

#[test]
fn an_edited_applied_migration_is_refused_before_any_write() {
    let uri = uri_or_skip();
    if uri.is_empty() {
        return;
    }
    let _guard = lock();
    let state = root();
    let dir = state.join("migrations");
    std::fs::create_dir_all(&dir).expect("mkdir");
    cleanup(&uri, &dir);

    write_migration(
        &dir,
        "9001_alpha.sql",
        "CREATE TABLE ignatius_mig_alpha (id integer PRIMARY KEY);",
    );
    let first = binary(&state)
        .args(["migrate", "apply", &uri, "--dir"])
        .arg(&dir)
        .output()
        .expect("apply");
    assert_eq!(code(&first), 0, "{}", stderr(&first));

    write_migration(
        &dir,
        "9001_alpha.sql",
        "CREATE TABLE ignatius_mig_alpha (id integer PRIMARY KEY, note text);",
    );
    let verify = binary(&state)
        .args(["migrate", "verify", &uri, "--dir"])
        .arg(&dir)
        .output()
        .expect("verify");
    assert_eq!(code(&verify), 1, "{}", stderr(&verify));
    assert!(stderr(&verify).contains("drifted: 9001"));

    let dry = binary(&state)
        .args(["migrate", "apply", &uri, "--dir"])
        .arg(&dir)
        .arg("--dry-run")
        .output()
        .expect("dry run drift");
    assert_eq!(
        code(&dry),
        2,
        "a dry run must not bless a drifted ledger: {}",
        stderr(&dry)
    );
    assert!(stderr(&dry).contains("does not match the files"));

    write_migration(
        &dir,
        "9002_beta.sql",
        "CREATE TABLE ignatius_mig_beta (id integer PRIMARY KEY);",
    );
    let apply = binary(&state)
        .args(["migrate", "apply", &uri, "--dir"])
        .arg(&dir)
        .output()
        .expect("apply drift");
    assert_eq!(code(&apply), 2, "{}", stderr(&apply));
    assert!(stderr(&apply).contains("does not match the files"));
    let beta = binary(&state)
        .args([
            "query",
            &uri,
            "-c",
            "SELECT to_regclass('public.ignatius_mig_beta')::text",
            "--format",
            "csv",
        ])
        .output()
        .expect("beta probe");
    assert_eq!(stdout(&beta), "to_regclass\n\n", "no migration ran");
}

#[test]
fn a_failed_migration_rolls_back_and_leaves_no_ledger_row() {
    let uri = uri_or_skip();
    if uri.is_empty() {
        return;
    }
    let _guard = lock();
    let state = root();
    let dir = state.join("migrations");
    std::fs::create_dir_all(&dir).expect("mkdir");
    cleanup(&uri, &dir);

    write_migration(
        &dir,
        "9001_gamma.sql",
        "CREATE TABLE ignatius_mig_gamma (id integer); \
         INSERT INTO no_such_table VALUES (1);",
    );
    let apply = binary(&state)
        .args(["migrate", "apply", &uri, "--dir"])
        .arg(&dir)
        .output()
        .expect("apply");
    assert_eq!(code(&apply), 7, "{}", stderr(&apply));
    assert!(stderr(&apply).contains("9001"), "{}", stderr(&apply));

    let gamma = binary(&state)
        .args([
            "query",
            &uri,
            "-c",
            "SELECT to_regclass('public.ignatius_mig_gamma')::text",
            "--format",
            "csv",
        ])
        .output()
        .expect("gamma probe");
    assert_eq!(
        stdout(&gamma),
        "to_regclass\n\n",
        "the prefix was rolled back"
    );

    let ledger = binary(&state)
        .args([
            "query",
            &uri,
            "-c",
            "SELECT count(*) FROM ignatius_schema_migrations",
            "--format",
            "csv",
        ])
        .output()
        .expect("ledger probe");
    assert_eq!(stdout(&ledger), "count\n0\n");
}

#[test]
fn dry_run_and_transaction_control_never_write() {
    let uri = uri_or_skip();
    if uri.is_empty() {
        return;
    }
    let _guard = lock();
    let state = root();
    let dir = state.join("migrations");
    std::fs::create_dir_all(&dir).expect("mkdir");
    cleanup(&uri, &dir);

    write_migration(
        &dir,
        "9001_alpha.sql",
        "CREATE TABLE ignatius_mig_alpha (id integer);",
    );
    let dry = binary(&state)
        .args(["migrate", "apply", &uri, "--dir"])
        .arg(&dir)
        .arg("--dry-run")
        .output()
        .expect("dry run");
    assert_eq!(code(&dry), 0, "{}", stderr(&dry));
    assert!(stdout(&dry).contains("CREATE TABLE ignatius_mig_alpha"));
    assert!(stderr(&dry).contains("nothing was applied"));
    let alpha = binary(&state)
        .args([
            "query",
            &uri,
            "-c",
            "SELECT to_regclass('public.ignatius_mig_alpha')::text",
            "--format",
            "csv",
        ])
        .output()
        .expect("alpha probe");
    assert_eq!(stdout(&alpha), "to_regclass\n\n");

    write_migration(
        &dir,
        "9001_alpha.sql",
        "BEGIN; CREATE TABLE ignatius_mig_alpha (id integer); COMMIT;",
    );
    let refused = binary(&state)
        .args(["migrate", "apply", &uri, "--dir"])
        .arg(&dir)
        .output()
        .expect("transaction control");
    assert_eq!(code(&refused), 2, "{}", stderr(&refused));
    assert!(stderr(&refused).contains("contains transaction control"));
    let alpha = binary(&state)
        .args([
            "query",
            &uri,
            "-c",
            "SELECT to_regclass('public.ignatius_mig_alpha')::text",
            "--format",
            "csv",
        ])
        .output()
        .expect("alpha probe");
    assert_eq!(stdout(&alpha), "to_regclass\n\n");
}
