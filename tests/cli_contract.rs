//! The non-interactive command-line contract.
//!
//! These run the real binary as a subprocess, because that is the only way to
//! prove what a script will actually observe: exit codes, which stream carries
//! what, and whether anything decorative leaks into piped output.
//!
//! Everything here works without a database except where noted.

// Tests report skips to the developer running them; that is what stderr is for.
#![allow(clippy::print_stderr)]

use std::process::{Command, Output};

fn binary() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ignatius"));
    // Never touch the developer's real configuration while testing.
    command.env("IGNATIUS_CONFIG_DIR", temp_config_dir());
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

/// A per-process temporary configuration directory.
fn temp_config_dir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ignatius-cli-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create temp config dir");
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
fn help_and_version_succeed_and_go_to_stdout() {
    let output = binary().arg("--help").output().expect("run");
    assert_eq!(code(&output), 0);
    assert!(
        stdout(&output).contains("PostgreSQL"),
        "{}",
        stdout(&output)
    );

    let output = binary().arg("--version").output().expect("run");
    assert_eq!(code(&output), 0);
    assert!(stdout(&output).contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn an_unknown_flag_is_a_usage_error() {
    let output = binary()
        .arg("--definitely-not-a-flag")
        .output()
        .expect("run");
    assert_eq!(code(&output), 2, "usage errors exit 2");
    assert!(
        stdout(&output).is_empty(),
        "usage errors do not pollute stdout"
    );
    assert!(!stderr(&output).is_empty());
}

#[test]
fn an_invalid_sslmode_is_a_usage_error_and_names_the_alternatives() {
    let output = binary()
        .args(["query", "-c", "SELECT 1", "--sslmode", "sort-of"])
        .output()
        .expect("run");
    assert_eq!(code(&output), 2);
    let message = stderr(&output);
    assert!(message.contains("sslmode"), "{message}");
    assert!(
        message.contains("verify-full"),
        "the safe option is named: {message}"
    );
}

#[test]
fn an_unreachable_server_exits_with_the_connection_code() {
    let output = binary()
        .args([
            "query",
            // Port 1 on loopback is not a PostgreSQL server.
            "postgres://someone@127.0.0.1:1/nothing?connect_timeout=2",
            "-c",
            "SELECT 1",
        ])
        .output()
        .expect("run");
    assert_eq!(
        code(&output),
        4,
        "connection failures exit 4: {}",
        stderr(&output)
    );
    assert!(stdout(&output).is_empty(), "no data on a failed connection");
    let message = stderr(&output);
    assert!(message.contains("Connection failed"), "{message}");
    assert!(
        message.contains("Next:"),
        "a failure must state a next action: {message}"
    );
}

#[test]
fn a_broken_configuration_file_exits_with_the_config_code() {
    let dir = std::env::temp_dir().join(format!("ignatius-badconfig-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create dir");
    std::fs::write(dir.join("config.toml"), "this is not = = toml\n").expect("write");

    let output = Command::new(env!("CARGO_BIN_EXE_ignatius"))
        .env("IGNATIUS_CONFIG_DIR", &dir)
        .args(["config", "validate"])
        .output()
        .expect("run");

    assert_eq!(code(&output), 3, "configuration errors exit 3");
    assert!(
        stderr(&output).contains("Configuration error"),
        "{}",
        stderr(&output)
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn secrets_never_appear_in_output_even_when_the_connection_fails() {
    const FAKE: &str = "hunter2-not-a-real-password";
    let output = binary()
        .args([
            "query",
            &format!("postgres://someone:{FAKE}@127.0.0.1:1/nothing?connect_timeout=2"),
            "-c",
            "SELECT 1",
            "--verbose",
        ])
        .output()
        .expect("run");

    let combined = format!("{}{}", stdout(&output), stderr(&output));
    assert!(
        !combined.contains(FAKE),
        "the password was echoed: {combined}"
    );
    assert!(
        combined.contains("[redacted]") || !combined.contains("someone:"),
        "{combined}"
    );
}

#[test]
fn query_without_sql_refuses_rather_than_reading_from_a_terminal() {
    use std::process::Stdio;
    // stdin is closed, which is the same shape as a non-interactive invocation
    // with nothing piped in.
    let output = binary()
        .args(["query", "postgres://x@127.0.0.1:1/y?connect_timeout=2"])
        .stdin(Stdio::null())
        .output()
        .expect("run");
    assert_eq!(code(&output), 2, "{}", stderr(&output));
    assert!(stderr(&output).contains("no SQL"), "{}", stderr(&output));
}

#[test]
fn completions_are_generated_for_every_documented_shell() {
    for shell in ["zsh", "bash", "fish", "powershell"] {
        let output = binary().args(["completion", shell]).output().expect("run");
        assert_eq!(code(&output), 0, "{shell} failed: {}", stderr(&output));
        let script = stdout(&output);
        assert!(
            script.contains("ignatius"),
            "{shell} produced no useful script"
        );
        assert!(stderr(&output).is_empty(), "{shell} wrote to stderr");
    }
}

#[test]
fn config_paths_and_doctor_work_on_a_machine_with_no_configuration() {
    let output = binary().args(["config", "paths"]).output().expect("run");
    assert_eq!(code(&output), 0);
    assert!(stdout(&output).contains("Configuration file:"));

    let output = binary().arg("doctor").output().expect("run");
    assert_eq!(
        code(&output),
        0,
        "a fresh machine is healthy: {}",
        stdout(&output)
    );
    assert!(stdout(&output).contains("Summary:"));
}

#[test]
fn doctor_json_is_valid_json_and_carries_the_same_checks() {
    let output = binary().args(["doctor", "--json"]).output().expect("run");
    assert_eq!(code(&output), 0);
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout(&output)).expect("doctor --json emits valid JSON");
    assert!(parsed["checks"].as_array().expect("checks").len() >= 6);
    assert_eq!(parsed["version"], env!("CARGO_PKG_VERSION"));
}

#[test]
fn version_verbose_separates_product_source_and_build_identity() {
    let output = binary()
        .args(["version", "--verbose"])
        .output()
        .expect("run");
    assert_eq!(code(&output), 0);
    let text = stdout(&output);
    for field in [
        "source revision:",
        "build identity:",
        "target:",
        "compiler:",
    ] {
        assert!(text.contains(field), "{field} missing from {text}");
    }
}

#[test]
fn piped_output_carries_no_colour_and_no_control_sequences() {
    let output = binary().args(["config", "paths"]).output().expect("run");
    assert!(
        !stdout(&output).contains('\u{1b}'),
        "piped output must be plain: {:?}",
        stdout(&output)
    );

    // NO_COLOR is honoured even when colour is asked for explicitly.
    let output = binary()
        .env("NO_COLOR", "1")
        .args(["--color", "always", "doctor"])
        .output()
        .expect("run");
    assert!(!stdout(&output).contains('\u{1b}'), "NO_COLOR was ignored");
}

#[test]
fn the_interactive_client_refuses_to_start_without_a_terminal() {
    let output = binary()
        .args(["connect", "postgres://x@127.0.0.1:1/y?connect_timeout=2"])
        .output()
        .expect("run");
    assert_eq!(code(&output), 2, "{}", stderr(&output));
    let message = stderr(&output);
    assert!(message.contains("needs a terminal"), "{message}");
    assert!(
        message.contains("query"),
        "it points at the non-interactive route: {message}"
    );
}

#[test]
fn a_security_relevant_parameter_is_refused_rather_than_ignored() {
    // gssencmode is genuinely unimplemented. Ignoring it would give weaker
    // protection than was asked for, so the connection is refused instead.
    let output = binary()
        .args([
            "query",
            "postgres://x@db.example.net/y?gssencmode=require",
            "-c",
            "SELECT 1",
        ])
        .output()
        .expect("run");
    assert_eq!(code(&output), 3, "{}", stderr(&output));
    assert!(
        stderr(&output).contains("gssencmode"),
        "{}",
        stderr(&output)
    );
}

// The remaining tests need the disposable PostgreSQL container.
mod with_server {
    use super::*;

    macro_rules! uri_or_skip {
        () => {
            match std::env::var("IGNATIUS_TEST_PG_URI") {
                Ok(uri) if !uri.is_empty() => uri,
                _ => {
                    eprintln!("skipping: IGNATIUS_TEST_PG_URI is not set");
                    return;
                }
            }
        };
    }

    #[test]
    fn a_successful_query_exits_zero_and_writes_only_data_to_stdout() {
        let uri = uri_or_skip!();
        let output = binary()
            .args(["query", &uri, "-c", "SELECT 1 AS one", "--format", "csv"])
            .output()
            .expect("run");
        assert_eq!(code(&output), 0, "{}", stderr(&output));
        assert_eq!(stdout(&output), "one\n1\n");
        assert!(stderr(&output).is_empty(), "{}", stderr(&output));
    }

    #[test]
    fn a_server_error_exits_with_the_query_code_and_keeps_stdout_clean() {
        let uri = uri_or_skip!();
        let output = binary()
            .args(["query", &uri, "-c", "SELECT * FROM no_such_table"])
            .output()
            .expect("run");
        assert_eq!(code(&output), 7, "query failures exit 7");
        assert!(
            stdout(&output).is_empty(),
            "stdout carried: {:?}",
            stdout(&output)
        );
        let message = stderr(&output);
        assert!(message.contains("Query error"), "{message}");
        assert!(message.contains("re-run with --verbose"), "{message}");
    }

    #[test]
    fn verbose_reveals_sqlstate_on_stderr() {
        let uri = uri_or_skip!();
        let output = binary()
            .args([
                "query",
                &uri,
                "-c",
                "SELECT * FROM no_such_table",
                "--verbose",
            ])
            .output()
            .expect("run");
        assert!(stderr(&output).contains("42P01"), "{}", stderr(&output));
    }

    #[test]
    fn machine_formats_round_trip_through_a_parser() {
        let uri = uri_or_skip!();
        let output = binary()
            .args([
                "query",
                &uri,
                "-c",
                "SELECT order_id, note FROM orders ORDER BY order_id",
                "--format",
                "json",
            ])
            .output()
            .expect("run");
        assert_eq!(code(&output), 0, "{}", stderr(&output));
        let parsed: serde_json::Value = serde_json::from_str(&stdout(&output)).expect("valid json");
        let rows = parsed[0]["rows"].as_array().expect("rows");
        assert_eq!(rows.len(), 3);
        assert!(rows[1]["note"].is_null(), "SQL NULL survives as JSON null");
    }

    #[test]
    fn stdin_is_accepted_as_a_source_of_sql() {
        use std::io::Write;
        use std::process::Stdio;

        let uri = uri_or_skip!();
        let mut child = binary()
            .args(["query", &uri, "--format", "csv"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn");
        child
            .stdin
            .as_mut()
            .expect("stdin")
            .write_all(b"SELECT 2 AS two;")
            .expect("write");
        let output = child.wait_with_output().expect("wait");

        assert_eq!(code(&output), 0, "{}", stderr(&output));
        assert_eq!(stdout(&output), "two\n2\n");
    }

    #[test]
    fn a_wrong_password_exits_with_the_authentication_code() {
        let uri = uri_or_skip!();
        let Some((prefix, rest)) = uri.split_once("://") else {
            panic!("expected a URI");
        };
        let Some((_, host_and_db)) = rest.split_once('@') else {
            eprintln!("skipping: the test URI has no userinfo to replace");
            return;
        };
        let wrong = format!("{prefix}://ignatius_test:definitely-the-wrong-password@{host_and_db}");

        let output = binary()
            .args(["query", &wrong, "-c", "SELECT 1"])
            .output()
            .expect("run");
        assert_eq!(
            code(&output),
            5,
            "authentication failures exit 5, distinctly from connection failures: {}",
            stderr(&output)
        );
        assert!(
            stderr(&output).contains("Authentication failed"),
            "{}",
            stderr(&output)
        );
        assert!(
            !stderr(&output).contains("definitely-the-wrong-password"),
            "the password was echoed"
        );
    }

    #[test]
    fn requiring_tls_from_a_server_without_it_exits_with_the_tls_code() {
        let uri = uri_or_skip!();
        let output = binary()
            .args(["query", &format!("{uri}?sslmode=require"), "-c", "SELECT 1"])
            .output()
            .expect("run");
        assert_eq!(
            code(&output),
            6,
            "a refusal to encrypt exits 6, not 4: {}",
            stderr(&output)
        );
        let message = stderr(&output);
        assert!(message.contains("TLS failed"), "{message}");
        assert!(
            message.contains("not retried without"),
            "it must say it did not fall back: {message}"
        );
    }

    #[test]
    fn interrupting_a_long_query_exits_with_the_cancellation_code() {
        use std::process::Stdio;

        let uri = uri_or_skip!();
        let child = binary()
            .args(["query", &uri, "-c", "SELECT pg_sleep(30)"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn");

        // Give it time to reach the server, then interrupt it the way a person
        // at a terminal would.
        std::thread::sleep(std::time::Duration::from_millis(1500));
        unsafe_free_interrupt(child.id());

        let output = child.wait_with_output().expect("wait");
        assert_eq!(
            code(&output),
            8,
            "an interrupted statement exits 8 rather than dying on a signal: {}",
            stderr(&output)
        );
        assert!(stderr(&output).contains("Cancelled"), "{}", stderr(&output));
    }

    /// Sends SIGINT without linking libc, by asking the system `kill` to do it.
    ///
    /// The crate denies `unsafe`, and this is a test helper, so shelling out is
    /// both simpler and honest about what it does.
    fn unsafe_free_interrupt(pid: u32) {
        let status = Command::new("kill")
            .args(["-INT", &pid.to_string()])
            .status()
            .expect("send SIGINT");
        assert!(status.success(), "could not interrupt the child");
    }

    #[test]
    fn an_export_writes_the_file_and_reports_the_count_on_stderr() {
        let uri = uri_or_skip!();
        let dir = std::env::temp_dir().join(format!("ignatius-export-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let destination = dir.join("orders.csv");
        let _ = std::fs::remove_file(&destination);

        let output = binary()
            .args([
                "query",
                &uri,
                "-c",
                "SELECT order_id, total FROM orders ORDER BY order_id",
                "--format",
                "csv",
                "-o",
                &destination.display().to_string(),
            ])
            .output()
            .expect("run");

        assert_eq!(code(&output), 0, "{}", stderr(&output));
        assert!(
            stdout(&output).is_empty(),
            "an export writes no data to stdout"
        );
        assert!(
            stderr(&output).contains("Exported 3 row(s)"),
            "{}",
            stderr(&output)
        );

        let written = std::fs::read_to_string(&destination).expect("read");
        assert!(
            written.starts_with("order_id,total\n1,1245.00\n"),
            "{written}"
        );
        assert!(
            !destination.with_extension("csv.partial").exists(),
            "the partial file is gone once the export is complete"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn an_interrupted_export_keeps_its_rows_and_exits_with_the_export_code() {
        use std::process::Stdio;

        let uri = uri_or_skip!();
        let dir = std::env::temp_dir().join(format!("ignatius-partial-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let destination = dir.join("big.csv");
        let _ = std::fs::remove_file(&destination);

        // A result large enough that it is certainly still streaming when the
        // interrupt arrives.
        let child = binary()
            .args([
                "query",
                &uri,
                "-c",
                "SELECT generate_series(1, 4000000) AS n, repeat('x', 200) AS pad",
                "--format",
                "csv",
                "-o",
                &destination.display().to_string(),
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn");

        std::thread::sleep(std::time::Duration::from_millis(2500));
        let status = Command::new("kill")
            .args(["-INT", &child.id().to_string()])
            .status()
            .expect("send SIGINT");
        assert!(status.success());

        let output = child.wait_with_output().expect("wait");
        assert_eq!(
            code(&output),
            9,
            "an interrupted export exits 9: {}",
            stderr(&output)
        );

        let message = stderr(&output);
        assert!(message.contains("Export incomplete"), "{message}");
        assert!(
            message.contains(".partial"),
            "it names the partial file: {message}"
        );

        let partial = std::path::PathBuf::from(format!("{}.partial", destination.display()));
        assert!(partial.exists(), "the rows already written are kept");
        assert!(
            !destination.exists(),
            "an incomplete export never occupies the destination, where it would look finished"
        );
        assert!(
            std::fs::metadata(&partial).expect("metadata").len() > 0,
            "something was actually written before the interrupt"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_write_to_a_production_target_is_refused_until_it_is_asked_for() {
        let uri = uri_or_skip!();

        let refused = binary()
            .args([
                "query",
                &uri,
                "--environment",
                "production",
                "-c",
                "UPDATE orders SET total = total",
            ])
            .output()
            .expect("run");
        assert_eq!(code(&refused), 2, "{}", stderr(&refused));
        let message = stderr(&refused);
        assert!(message.contains("classified as production"), "{message}");
        assert!(
            message.contains("--allow-write"),
            "it names the way through: {message}"
        );
        assert!(
            message.contains("not a security boundary"),
            "it must not overstate itself: {message}"
        );

        // Reads are never in the way.
        let read = binary()
            .args([
                "query",
                &uri,
                "--environment",
                "production",
                "-c",
                "SELECT 1",
                "--format",
                "csv",
            ])
            .output()
            .expect("run");
        assert_eq!(code(&read), 0, "{}", stderr(&read));

        // And a deliberate write goes through.
        let allowed = binary()
            .args([
                "query",
                &uri,
                "--environment",
                "production",
                "--allow-write",
                "-c",
                "UPDATE orders SET total = total",
            ])
            .output()
            .expect("run");
        assert_eq!(code(&allowed), 0, "{}", stderr(&allowed));
    }

    #[test]
    fn a_read_only_session_is_refused_by_the_server_not_by_a_guess() {
        let uri = uri_or_skip!();
        let output = binary()
            .args([
                "query",
                &uri,
                "--read-only",
                "-c",
                "INSERT INTO orders (customer_id, total) VALUES (1, 1)",
                "--verbose",
            ])
            .output()
            .expect("run");

        assert_eq!(
            code(&output),
            7,
            "the server rejected it: {}",
            stderr(&output)
        );
        let message = stderr(&output);
        assert!(message.contains("read-only transaction"), "{message}");
        assert!(
            message.contains("25006"),
            "PostgreSQL's own error, not ours: {message}"
        );
    }

    #[test]
    fn connect_check_reports_stages_and_succeeds_against_a_live_server() {
        let uri = uri_or_skip!();
        let output = binary()
            .args(["connect", "--check", &uri])
            .output()
            .expect("run");
        assert_eq!(code(&output), 0, "{}", stdout(&output));
        let report = stdout(&output);
        for stage in ["target", "address resolution", "tcp", "postgres", "session"] {
            assert!(report.contains(stage), "{stage} missing from {report}");
        }
        assert!(report.contains("Summary:"));
    }
}
