//! Acceptance contract for Feature 044: offline SQL linter.

#![allow(clippy::print_stderr)]

use std::path::PathBuf;
use std::process::{Command, Output};

fn unique() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

fn binary() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ignatius"));
    let state = std::env::temp_dir().join(format!(
        "ignatius-lint-state-{}-{}",
        std::process::id(),
        unique()
    ));
    command.env("IGNATIUS_CONFIG_DIR", &state);
    command.env("IGNATIUS_DATA_DIR", state.join("data"));
    command.env_remove("IGNATIUS_LOG");
    for key in [
        "PGHOST",
        "PGPORT",
        "PGDATABASE",
        "PGUSER",
        "PGPASSWORD",
        "PGSERVICE",
        "PGSSLMODE",
    ] {
        command.env_remove(key);
    }
    command
}

fn temp_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "ignatius-lint-contract-{}-{}",
        std::process::id(),
        unique()
    ));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
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
fn a_clean_file_exits_zero_and_says_there_are_no_findings() {
    let dir = temp_dir();
    let path = dir.join("clean.sql");
    std::fs::write(&path, "SELECT id FROM orders WHERE id = 1;\n").expect("write");

    let output = binary().arg("lint").arg(&path).output().expect("run lint");
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    assert!(stdout(&output).contains("No findings."));
}

#[test]
fn a_dangerous_file_exits_one_and_names_the_rule() {
    let dir = temp_dir();
    let path = dir.join("dangerous.sql");
    std::fs::write(&path, "DELETE FROM orders;\n").expect("write");

    let output = binary().arg("lint").arg(&path).output().expect("run lint");
    assert_eq!(code(&output), 1, "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("error delete-without-where"), "{text}");
    assert!(text.contains("dangerous.sql:1:1"), "{text}");
}

#[test]
fn fail_on_never_reports_findings_without_failing_the_build() {
    let dir = temp_dir();
    let path = dir.join("dangerous.sql");
    std::fs::write(&path, "DELETE FROM orders;\n").expect("write");

    let output = binary()
        .args(["lint"])
        .arg(&path)
        .args(["--fail-on", "never"])
        .output()
        .expect("run lint");
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    assert!(stdout(&output).contains("delete-without-where"));
}

#[test]
fn json_and_sarif_are_machine_clean_and_parseable() {
    let dir = temp_dir();
    let path = dir.join("dangerous.sql");
    std::fs::write(&path, "UPDATE orders SET note = 'x';\n").expect("write");

    let output = binary()
        .args(["lint"])
        .arg(&path)
        .args(["--format", "json"])
        .output()
        .expect("run lint json");
    assert_eq!(code(&output), 1, "stderr: {}", stderr(&output));
    let value: serde_json::Value = serde_json::from_str(&stdout(&output)).expect("valid json");
    assert_eq!(value["version"], 1);
    assert_eq!(value["findings"][0]["code"], "update-without-where");
    assert_eq!(value["findings"][0]["severity"], "error");
    assert!(!stdout(&output).contains('\u{1b}'));

    let output = binary()
        .args(["lint"])
        .arg(&path)
        .args(["--format", "sarif"])
        .output()
        .expect("run lint sarif");
    assert_eq!(code(&output), 1, "stderr: {}", stderr(&output));
    let value: serde_json::Value = serde_json::from_str(&stdout(&output)).expect("valid sarif");
    assert_eq!(value["version"], "2.1.0");
    assert_eq!(
        value["runs"][0]["results"][0]["ruleId"],
        "update-without-where"
    );
}

#[test]
fn standard_input_and_directories_are_both_supported() {
    use std::io::Write as _;
    use std::process::Stdio;

    let mut child = binary()
        .args(["lint", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn lint stdin");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(b"DELETE FROM orders;\n")
        .expect("write stdin");
    let output = child.wait_with_output().expect("wait");
    assert_eq!(code(&output), 1, "stderr: {}", stderr(&output));
    assert!(stdout(&output).contains("delete-without-where"));
    assert!(stdout(&output).contains("<buffer>"), "{}", stdout(&output));

    let dir = temp_dir();
    std::fs::create_dir_all(dir.join("migrations")).expect("mkdir");
    std::fs::write(dir.join("migrations/001.sql"), "DELETE FROM a;\n").expect("write");
    std::fs::write(dir.join("migrations/002.sql"), "SELECT 1;\n").expect("write");
    let output = binary()
        .arg("lint")
        .arg(dir.join("migrations"))
        .output()
        .expect("run lint directory");
    assert_eq!(code(&output), 1, "stderr: {}", stderr(&output));
    assert!(stdout(&output).contains("001.sql"));
    assert!(!stdout(&output).contains("002.sql"));
}

#[test]
fn min_severity_hides_style_findings() {
    let dir = temp_dir();
    let path = dir.join("style.sql");
    std::fs::write(&path, "SELECT * FROM orders;\n").expect("write");

    let output = binary()
        .arg("lint")
        .arg(&path)
        .args(["--min-severity", "warning"])
        .output()
        .expect("run lint");
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    assert!(stdout(&output).contains("No findings."));

    let output = binary()
        .arg("lint")
        .arg(&path)
        .args(["--format", "json"])
        .output()
        .expect("run lint info");
    let value: serde_json::Value = serde_json::from_str(&stdout(&output)).expect("json");
    assert_eq!(value["findings"][0]["code"], "select-star");
}

#[test]
fn fail_on_is_evaluated_before_min_severity_hides_findings() {
    let dir = temp_dir();
    let path = dir.join("warning.sql");
    std::fs::write(&path, "DROP TABLE orders;\n").expect("write");

    let output = binary()
        .arg("lint")
        .arg(&path)
        .args(["--min-severity", "error", "--fail-on", "warning"])
        .output()
        .expect("run lint");
    assert_eq!(code(&output), 1, "stderr: {}", stderr(&output));
    assert!(stdout(&output).contains("No findings."));
}

#[test]
fn output_files_are_protected_and_force_replaces_them() {
    let dir = temp_dir();
    let path = dir.join("dangerous.sql");
    let report = dir.join("lint.sarif");
    std::fs::write(&path, "DELETE FROM orders;\n").expect("write");

    let first = binary()
        .args(["lint"])
        .arg(&path)
        .args(["--format", "sarif", "--output"])
        .arg(&report)
        .output()
        .expect("write report");
    assert_eq!(code(&first), 1, "stderr: {}", stderr(&first));
    let original = std::fs::read_to_string(&report).expect("read report");

    let second = binary()
        .args(["lint"])
        .arg(&path)
        .args(["--format", "sarif", "--output"])
        .arg(&report)
        .output()
        .expect("refuse overwrite");
    assert_eq!(code(&second), 2, "stderr: {}", stderr(&second));
    assert!(stderr(&second).contains("refusing to overwrite"));
    assert_eq!(std::fs::read_to_string(&report).expect("read"), original);

    let third = binary()
        .args(["lint"])
        .arg(&path)
        .args(["--format", "sarif", "--output"])
        .arg(&report)
        .arg("--force")
        .output()
        .expect("force overwrite");
    assert_eq!(code(&third), 1, "stderr: {}", stderr(&third));
}

#[test]
fn a_missing_path_is_a_usage_error_not_a_crash() {
    let output = binary()
        .args(["lint", "/definitely/not/here.sql"])
        .output()
        .expect("run lint");
    assert_eq!(code(&output), 2, "stderr: {}", stderr(&output));
    assert!(stderr(&output).contains("failed to read"));
}

#[cfg(unix)]
#[test]
fn a_symlinked_file_is_refused_without_following_it() {
    use std::os::unix::fs::symlink;

    let dir = temp_dir();
    let victim = dir.join("real.sql");
    std::fs::write(&victim, "DROP TABLE orders;\n").expect("write");
    let link = dir.join("link.sql");
    symlink(&victim, &link).expect("symlink");

    let output = binary().arg("lint").arg(&link).output().expect("run lint");
    assert_eq!(code(&output), 2, "stderr: {}", stderr(&output));
    assert!(stderr(&output).contains("symbolic link"));
}
