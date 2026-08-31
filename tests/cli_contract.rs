//! The non-interactive command-line contract.
//!
//! These run the real binary as a subprocess, because that is the only way to
//! prove what a script will actually observe: exit codes, which stream carries
//! what, and whether anything decorative leaks into piped output.
//!
//! Everything here works without a database except where noted.

// Tests report skips to the developer running them; that is what stderr is for.
#![allow(clippy::print_stderr)]

use std::env;
use std::process::{Command, Output};

fn binary() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ignatius"));
    // Never touch the developer's real configuration or local state while
    // testing. The history is a file of SQL, so the data directory matters as
    // much as the configuration one.
    command.env("IGNATIUS_CONFIG_DIR", temp_config_dir());
    command.env("IGNATIUS_DATA_DIR", temp_config_dir().join("data"));
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

fn command_stdout(command: &str, args: &[&str]) -> String {
    let output = Command::new(command)
        .args(args)
        .output()
        .unwrap_or_else(|error| panic!("run {command}: {error}"));
    assert!(
        output.status.success(),
        "{command} {args:?} failed: {}",
        stderr(&output)
    );
    stdout(&output)
}

fn identity_field<'a>(text: &'a str, label: &str) -> &'a str {
    text.lines()
        .find_map(|line| line.strip_prefix(label))
        .map(str::trim)
        .unwrap_or_else(|| panic!("missing {label} in {text}"))
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
fn the_history_can_be_found_read_and_cleared_without_a_database() {
    // A history is a file of SQL on a disk. Every command that exists is about
    // seeing it or removing it.
    let dir = std::env::temp_dir().join(format!("ignatius-history-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let run = |args: &[&str]| {
        binary()
            .env("IGNATIUS_DATA_DIR", &dir)
            .args(args)
            .output()
            .expect("run")
    };

    let output = run(&["history", "path"]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    let path = stdout(&output).trim().to_owned();
    assert!(path.ends_with("history.jsonl"), "{path}");
    assert!(path.starts_with(&dir.display().to_string()), "{path}");

    // Nothing recorded yet is a fact, not an error, and it goes to the message
    // stream so a pipe of the listing stays empty.
    let output = run(&["history", "list"]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert!(stdout(&output).is_empty(), "{}", stdout(&output));
    assert!(
        stderr(&output).contains("No statements"),
        "{}",
        stderr(&output)
    );

    // Clearing is irreversible, so it is refused without saying so explicitly.
    let output = run(&["history", "clear"]);
    assert_eq!(code(&output), 2, "a usage error: {}", stderr(&output));
    assert!(stderr(&output).contains("--yes"), "{}", stderr(&output));

    let output = run(&["history", "clear", "--yes"]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
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
fn config_init_writes_a_file_this_build_accepts_and_refuses_to_overwrite() {
    let dir = std::env::temp_dir().join(format!("ignatius-init-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create dir");

    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_ignatius"))
            .env("IGNATIUS_CONFIG_DIR", &dir)
            .env("IGNATIUS_DATA_DIR", dir.join("data"))
            .args(args)
            .output()
            .expect("run")
    };

    let output = run(&["config", "init"]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert!(stdout(&output).contains("Wrote"), "{}", stdout(&output));

    let written = std::fs::read_to_string(dir.join("config.toml")).expect("read");
    assert!(
        written.contains("No password belongs in this file"),
        "the one rule that matters is in the file itself"
    );

    // What it wrote, it accepts.
    let output = run(&["config", "validate"]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));

    // And it will not quietly replace what someone has written since.
    std::fs::write(dir.join("config.toml"), "[ui]\ntheme = \"light\"\n").expect("write");
    let output = run(&["config", "init"]);
    assert_eq!(code(&output), 2, "{}", stderr(&output));
    assert!(stderr(&output).contains("--force"), "{}", stderr(&output));
    assert!(
        std::fs::read_to_string(dir.join("config.toml"))
            .expect("read")
            .contains("light"),
        "the existing file is untouched"
    );

    let output = run(&["config", "init", "--force"]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_profile_names_a_connection_and_never_holds_a_password() {
    let dir = std::env::temp_dir().join(format!("ignatius-profiles-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create dir");

    let run = |toml: &str, args: &[&str]| {
        std::fs::write(dir.join("config.toml"), toml).expect("write");
        let mut command = Command::new(env!("CARGO_BIN_EXE_ignatius"));
        command
            .env("IGNATIUS_CONFIG_DIR", &dir)
            .env("IGNATIUS_DATA_DIR", dir.join("data"))
            .env_remove("PGHOST")
            .env_remove("PGPASSWORD");
        command.args(args).output().expect("run")
    };

    const PROFILES: &str = "[profiles.orders-prod]\n\
         host = \"127.0.0.1\"\n\
         port = 1\n\
         dbname = \"orders\"\n\
         environment = \"production\"\n\
         \n\
         [profiles.orders-dev]\n\
         host = \"127.0.0.1\"\n\
         port = 1\n";

    // The profile decides where to connect: this fails on the connection, not
    // on the name, and the failure names the host the profile gave.
    let output = run(PROFILES, &["connect", "--check", "@orders-prod"]);
    assert_ne!(code(&output), 3, "{}", stderr(&output));
    let report = format!("{}{}", stdout(&output), stderr(&output));
    assert!(report.contains("127.0.0.1"), "{report}");

    // An unknown one lists the ones that exist.
    let output = run(PROFILES, &["connect", "--check", "@orders-stage"]);
    assert_eq!(code(&output), 3, "{}", stderr(&output));
    assert!(
        stderr(&output).contains("orders-prod"),
        "{}",
        stderr(&output)
    );
    assert!(
        stderr(&output).contains("orders-dev"),
        "{}",
        stderr(&output)
    );

    // A profile and a target together are a usage error, not a guess.
    let output = run(
        PROFILES,
        &[
            "connect",
            "--check",
            "@orders-prod",
            "--profile",
            "orders-dev",
        ],
    );
    assert_eq!(code(&output), 2, "{}", stderr(&output));

    // A password in a profile is refused by name, with the routes that exist.
    let output = run(
        "[profiles.p]\nhost = \"127.0.0.1\"\npassword = \"hunter2\"\n",
        &["connect", "--check", "@p"],
    );
    assert_eq!(code(&output), 3, "{}", stderr(&output));
    let message = stderr(&output);
    assert!(message.contains(".pgpass"), "{message}");
    assert!(
        !message.contains("hunter2"),
        "the value is never repeated back: {message}"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_key_binding_that_would_quietly_do_nothing_stops_the_client() {
    // A file whose whole purpose is to say what the keyboard does must not
    // contain a line that does nothing. Each of these is refused before the
    // terminal is touched, so the message lands on an ordinary terminal.
    let dir = std::env::temp_dir().join(format!("ignatius-badkeys-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create dir");

    let run = |toml: &str| {
        std::fs::write(dir.join("config.toml"), toml).expect("write");
        Command::new(env!("CARGO_BIN_EXE_ignatius"))
            .env("IGNATIUS_CONFIG_DIR", &dir)
            .env("IGNATIUS_DATA_DIR", dir.join("data"))
            .args(["connect", "postgres://user@127.0.0.1:1/db"])
            .output()
            .expect("run")
    };

    let output = run("[keys]\nrun-everythng = \"f2\"\n");
    assert_eq!(code(&output), 3, "{}", stderr(&output));
    assert!(
        stderr(&output).contains("run-buffer"),
        "the real names are listed: {}",
        stderr(&output)
    );

    let output = run("[keys]\nquit = \"hyper+q\"\n");
    assert_eq!(code(&output), 3, "{}", stderr(&output));
    assert!(stderr(&output).contains("ctrl+r"), "{}", stderr(&output));

    let output = run("[keys]\ntoggle-help = \"ctrl+q\"\n");
    assert_eq!(code(&output), 3, "{}", stderr(&output));
    assert!(stderr(&output).contains("same key"), "{}", stderr(&output));

    // A file that is right is not refused: this one fails later, on the
    // connection, which is a different exit code entirely.
    let output = run("[keys]\nrun-buffer = [\"f2\", \"ctrl+r\"]\n");
    assert_ne!(code(&output), 3, "{}", stderr(&output));

    // And `config validate` answers the same way the client would, rather than
    // blessing a file the client then refuses to start with.
    let validate = |toml: &str| {
        std::fs::write(dir.join("config.toml"), toml).expect("write");
        Command::new(env!("CARGO_BIN_EXE_ignatius"))
            .env("IGNATIUS_CONFIG_DIR", &dir)
            .args(["config", "validate"])
            .output()
            .expect("run")
    };
    let output = validate("[keys]\nquit = \"hyper+q\"\n");
    assert_eq!(
        code(&output),
        3,
        "validate must refuse what connect refuses: {}",
        stdout(&output)
    );
    let output = validate("[keys]\nquit = \"ctrl+x\"\n");
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert!(stdout(&output).contains("valid"));

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
    assert!(
        text.starts_with(&format!("Ignatius {}\n", env!("CARGO_PKG_VERSION"))),
        "product version is not the Cargo version: {text}"
    );

    let source = identity_field(&text, "  source revision:");
    let (revision, state) = source
        .split_once(" (")
        .unwrap_or_else(|| panic!("source identity lacks state: {source}"));
    assert_eq!(
        revision,
        command_stdout("git", &["rev-parse", "HEAD"]).trim(),
        "source revision is not the checked-out revision"
    );
    let expected_state = if command_stdout("git", &["status", "--porcelain"])
        .trim()
        .is_empty()
    {
        "clean"
    } else {
        "modified"
    };
    assert_eq!(state.strip_suffix(')'), Some(expected_state));

    assert_eq!(
        identity_field(&text, "  build identity:"),
        env!("IGNATIUS_BUILD_IDENTITY")
    );

    let rustc = env::var("RUSTC").unwrap_or_else(|_| "rustc".to_owned());
    let rustc_verbose = command_stdout(&rustc, &["-vV"]);
    assert_eq!(
        identity_field(&text, "  target:"),
        identity_field(&rustc_verbose, "host:")
    );
    assert_eq!(
        identity_field(&text, "  compiler:"),
        command_stdout(&rustc, &["--version"]).trim()
    );
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
    fn plain_mode_never_asks_a_pipe_for_a_password() {
        // The rule that keeps a script from hanging: the question is asked of a
        // person or not at all. This test would not finish if that were wrong.
        use std::process::Stdio;

        let uri = uri_or_skip!();
        let Some((prefix, rest)) = uri.split_once("://") else {
            panic!("expected a URI");
        };
        let Some((_, host_and_db)) = rest.split_once('@') else {
            eprintln!("skipping: the test URI has no userinfo to replace");
            return;
        };
        let wrong = format!("{prefix}://ignatius_test:definitely-the-wrong-password@{host_and_db}");

        let child = binary()
            .args(["--plain", "connect", &wrong])
            .env("TERM", "dumb")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn");
        let output = child.wait_with_output().expect("wait");

        assert_eq!(
            code(&output),
            5,
            "it fails as an authentication problem rather than waiting: {}",
            stderr(&output)
        );
        assert!(
            !stderr(&output).contains("Password for"),
            "nothing was asked of a pipe: {}",
            stderr(&output)
        );
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
    fn a_profile_that_says_production_guards_the_database_without_being_asked_twice() {
        // This is what profiles are for. The classification is written down
        // once and cannot be forgotten on the day it matters.
        let uri = uri_or_skip!();
        let dir =
            std::env::temp_dir().join(format!("ignatius-profile-prod-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create dir");
        let (_, rest) = uri.split_once("://").expect("a URI");
        let (userinfo, host_and_db) = rest.split_once('@').expect("userinfo");
        let (user, password) = userinfo.split_once(':').expect("a password");
        let (host_port, database) = host_and_db.split_once('/').expect("a database");
        let (host, port) = host_port.split_once(':').expect("a port");
        std::fs::write(
            dir.join("config.toml"),
            format!(
                "[profiles.demo-prod]\nhost = \"{host}\"\nport = {port}\n\
                 dbname = \"{database}\"\nuser = \"{user}\"\nenvironment = \"production\"\n"
            ),
        )
        .expect("write");

        let run = |args: &[&str]| {
            Command::new(env!("CARGO_BIN_EXE_ignatius"))
                .env("IGNATIUS_CONFIG_DIR", &dir)
                .env("IGNATIUS_DATA_DIR", dir.join("data"))
                // The password comes from the environment, never from the
                // profile: that is the whole point of the refusal above.
                .env("PGPASSWORD", password)
                .args(args)
                .output()
                .expect("run")
        };

        let refused = run(&[
            "query",
            "@demo-prod",
            "-c",
            "UPDATE orders SET total = total",
        ]);
        assert_eq!(code(&refused), 2, "{}", stderr(&refused));
        assert!(
            stderr(&refused).contains("classified as production"),
            "{}",
            stderr(&refused)
        );

        // A read is never in the way, and the profile still decided where.
        let read = run(&["query", "@demo-prod", "-c", "SELECT 1 AS ok"]);
        assert_eq!(code(&read), 0, "{}", stderr(&read));
        assert!(stdout(&read).contains("ok"), "{}", stdout(&read));

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
    fn plain_mode_is_a_line_oriented_client_that_a_screen_reader_can_follow() {
        use std::io::Write;
        use std::process::Stdio;

        let uri = uri_or_skip!();
        let mut child = binary()
            .args(["--plain", "connect", &uri])
            // The mode exists for terminals that can do nothing else.
            .env("TERM", "dumb")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn");

        child
            .stdin
            .as_mut()
            .expect("stdin")
            .write_all(b"SELECT order_id\nFROM orders\nORDER BY order_id;\nBEGIN;\n\\q\n")
            .expect("write");

        let output = child.wait_with_output().expect("wait");
        assert_eq!(code(&output), 0, "{}", stderr(&output));

        let data = stdout(&output);
        let messages = stderr(&output);

        // Results are data; prompts and status are messages.
        assert!(data.contains("order_id"), "{data}");
        assert!(data.contains("(3 rows)"), "{data}");
        assert!(
            !data.contains('\u{1b}'),
            "plain mode must emit no control sequences at all"
        );
        assert!(
            !messages.contains('\u{1b}'),
            "not on the message stream either"
        );
        assert!(
            data.is_ascii(),
            "and nothing outside ASCII: {:?}",
            data.chars().filter(|c| !c.is_ascii()).collect::<String>()
        );

        // A multi-line statement is accumulated and shown as a continuation.
        assert!(messages.contains("... >"), "{messages}");
        // The transaction state reaches the prompt, in words.
        assert!(messages.contains("[in transaction]"), "{messages}");
        assert!(messages.contains("Connected to"), "{messages}");
    }

    #[test]
    fn plain_mode_asks_before_writing_to_a_production_target() {
        use std::io::Write;
        use std::process::Stdio;

        let uri = uri_or_skip!();
        let mut child = binary()
            .args(["--plain", "connect", "--environment", "production", &uri])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn");

        // An empty answer cancels, so nothing reaches the database.
        child
            .stdin
            .as_mut()
            .expect("stdin")
            .write_all(b"DELETE FROM orders;\n\n\\q\n")
            .expect("write");

        let output = child.wait_with_output().expect("wait");
        assert_eq!(code(&output), 0, "{}", stderr(&output));
        let messages = stderr(&output);
        assert!(messages.contains("destroys data"), "{messages}");
        assert!(messages.contains("classified as production"), "{messages}");
        assert!(
            messages.contains("Cancelled. Nothing was sent."),
            "an empty answer must cancel: {messages}"
        );
    }

    #[test]
    fn plain_mode_cancels_a_running_statement_and_keeps_the_transcript_safe() {
        use std::io::Write;
        use std::process::Stdio;

        let uri = uri_or_skip!();
        let mut child = binary()
            .args(["--plain", "connect", &uri])
            .env("TERM", "dumb")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn");

        child
            .stdin
            .as_mut()
            .expect("stdin")
            .write_all(b"SELECT pg_sleep(30);\n\\q\n")
            .expect("write");

        // The plain client must ask the server to stop the statement rather
        // than dying directly from SIGINT. The existing one-shot query test
        // uses the same server-side timing convention.
        std::thread::sleep(std::time::Duration::from_millis(1500));
        unsafe_free_interrupt(child.id());

        let output = child.wait_with_output().expect("wait");
        assert_eq!(
            code(&output),
            0,
            "plain mode leaves after \\q: {}",
            stderr(&output)
        );

        let data = stdout(&output);
        let messages = stderr(&output);
        assert!(data.is_empty(), "a cancelled statement wrote data: {data}");
        assert!(messages.contains("Query cancelled by server"), "{messages}");
        assert!(messages.contains("SQLSTATE: 57014"), "{messages}");
        assert!(messages.contains("nothing was retried"), "{messages}");
        assert!(!data.contains('\u{1b}'), "data contains a control sequence");
        assert!(
            !messages.contains('\u{1b}'),
            "messages contain a control sequence"
        );
    }

    #[test]
    fn a_session_records_what_ran_and_never_records_a_credential() {
        use std::io::Write;
        use std::process::Stdio;

        let uri = uri_or_skip!();
        let dir =
            std::env::temp_dir().join(format!("ignatius-history-session-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let session = |input: &str, extra: &[&str]| {
            let mut args = vec!["--plain"];
            args.extend_from_slice(extra);
            args.extend_from_slice(&["connect", &uri]);
            let mut child = binary()
                .env("IGNATIUS_DATA_DIR", &dir)
                .args(&args)
                .env("TERM", "dumb")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("spawn");
            child
                .stdin
                .as_mut()
                .expect("stdin")
                .write_all(input.as_bytes())
                .expect("write");
            child.wait_with_output().expect("wait")
        };

        let output = session("SELECT 1 AS recorded;\n\\q\n", &[]);
        assert_eq!(code(&output), 0, "{}", stderr(&output));

        // A statement that mentions a credential is refused, and the session is
        // told why rather than left to wonder where it went.
        let output = session("SELECT 'nothing' AS password_probe;\n\\q\n", &[]);
        assert_eq!(code(&output), 0, "{}", stderr(&output));
        assert!(
            stderr(&output).contains("mentions a credential"),
            "{}",
            stderr(&output)
        );

        // And a session told not to record does not.
        let output = session("SELECT 2 AS not_recorded;\n\\q\n", &["--no-history"]);
        assert_eq!(code(&output), 0, "{}", stderr(&output));
        assert!(
            stderr(&output).contains("not being recorded"),
            "a session that keeps no record says so: {}",
            stderr(&output)
        );

        let listing = binary()
            .env("IGNATIUS_DATA_DIR", &dir)
            .args(["history", "list"])
            .output()
            .expect("run");
        let text = stdout(&listing);
        assert!(text.contains("SELECT 1 AS recorded"), "{text}");
        assert!(
            !text.contains("AS password"),
            "a statement mentioning a credential reached the file: {text}"
        );
        assert!(
            !text.contains("not_recorded"),
            "a paused session wrote to the file: {text}"
        );

        // The file itself is the owner's alone.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let file = dir.join("history.jsonl");
            let mode = std::fs::metadata(&file)
                .expect("metadata")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600, "{}", file.display());
        }
    }

    #[test]
    fn plain_mode_preserves_failed_transaction_recovery_in_the_transcript() {
        use std::io::Write;
        use std::process::Stdio;

        let uri = uri_or_skip!();
        let mut child = binary()
            .args(["--plain", "connect", &uri])
            .env("TERM", "dumb")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn");

        child
            .stdin
            .as_mut()
            .expect("stdin")
            .write_all(b"BEGIN;\nSELECT * FROM definitely_not_a_table;\n\\q\n")
            .expect("write");

        let output = child.wait_with_output().expect("wait");
        assert_eq!(
            code(&output),
            0,
            "plain mode leaves after \\q: {}",
            stderr(&output)
        );

        let data = stdout(&output);
        let messages = stderr(&output);
        // Table formatting may report a no-row statement summary on stdout;
        // the diagnostic itself must remain on the message stream.
        assert!(
            !data.contains("Query error"),
            "diagnostic leaked to stdout: {data}"
        );
        assert!(!data.contains("42P01"), "SQLSTATE leaked to stdout: {data}");
        for expected in [
            "[in transaction]",
            "Query error",
            "SQLSTATE: 42P01",
            "Likely cause:",
            "Next:",
            "[transaction failed]",
            "ROLLBACK ends it",
        ] {
            assert!(
                messages.contains(expected),
                "{expected} missing from {messages}"
            );
        }
        assert!(!data.contains('\u{1b}'), "data contains a control sequence");
        assert!(
            !messages.contains('\u{1b}'),
            "messages contain a control sequence"
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
