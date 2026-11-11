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
fn insert_output_requires_an_explicit_table_before_connection_work() {
    for args in [
        vec!["query", "-c", "SELECT 1", "--format", "insert"],
        vec![
            "query",
            "-c",
            "SELECT 1",
            "--format",
            "insert",
            "--insert-table",
            "",
        ],
        vec![
            "query",
            "-c",
            "SELECT 1",
            "--format",
            "insert",
            "--insert-table",
            "orders",
            "--no-header",
        ],
    ] {
        let output = binary().args(args).output().expect("run");
        assert_eq!(code(&output), 2, "{}", stderr(&output));
        assert!(stdout(&output).is_empty(), "usage errors stay off stdout");
        assert!(
            stderr(&output).contains("INSERT output") || stderr(&output).contains("insert-table"),
            "{}",
            stderr(&output)
        );
        assert!(
            !stderr(&output).contains("Connection failed"),
            "validation must happen before a target is contacted: {}",
            stderr(&output)
        );
    }
}

#[test]
fn named_parameter_mappings_fail_before_connection_or_prompting() {
    let cases: &[(&[&str], &str)] = &[
        (
            &[
                "query",
                "postgres://app@127.0.0.1:1/does-not-connect",
                "-c",
                "SELECT :customer_id",
            ],
            "no --param-env mappings",
        ),
        (
            &[
                "query",
                "postgres://app@127.0.0.1:1/does-not-connect",
                "-c",
                "SELECT :customer_id",
                "--param-env",
                "other=PATH",
            ],
            "unknown parameter",
        ),
        (
            &[
                "query",
                "postgres://app@127.0.0.1:1/does-not-connect",
                "-c",
                "SELECT :customer_id",
                "--param-env",
                "customer_id=PATH",
                "--param-env",
                "customer_id=PATH",
            ],
            "mapped more than once",
        ),
        (
            &[
                "query",
                "postgres://app@127.0.0.1:1/does-not-connect",
                "-c",
                "SELECT :customer_id",
                "--param-env",
                "customer_id=PATH=OTHER",
            ],
            "NAME=VARIABLE",
        ),
        (
            &[
                "query",
                "postgres://app@127.0.0.1:1/does-not-connect",
                "-c",
                "SELECT :customer_id",
                "--param-env",
                "customer_id=IGNATIUS_FEATURE_022_MISSING",
            ],
            "is not set",
        ),
    ];

    for (args, expected) in cases {
        let mut command = binary();
        command.env_remove("IGNATIUS_FEATURE_022_MISSING");
        let output = command.args(*args).output().expect("run");
        assert_eq!(code(&output), 2, "{}", stderr(&output));
        assert!(stdout(&output).is_empty(), "usage errors stay off stdout");
        let message = stderr(&output);
        assert!(
            message.contains(expected),
            "expected {expected:?}: {message}"
        );
        assert!(
            !message.contains("could not reach") && !message.contains("Connection failed"),
            "mapping validation must precede connection work: {message}"
        );
    }
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
fn retained_result_refresh_stays_out_of_noninteractive_machine_output() {
    let marker = "synthetic-refresh-value";
    let output = binary()
        .env("IGNATIUS_REFRESH_TEST_VALUE", marker)
        .args([
            "query",
            "postgres://someone@127.0.0.1:1/nothing?connect_timeout=2",
            "-c",
            "SELECT :value AS value",
            "--param-env",
            "value=IGNATIUS_REFRESH_TEST_VALUE",
            "--format",
            "csv",
        ])
        .output()
        .expect("run");
    assert_eq!(code(&output), 4, "{}", stderr(&output));
    assert!(stdout(&output).is_empty(), "failed query emitted data");
    let combined = format!("{}{}", stdout(&output), stderr(&output));
    assert!(
        !combined.contains(marker),
        "parameter value leaked: {combined}"
    );
    assert!(
        !combined.contains("Refreshing retained result"),
        "an interactive-only state reached machine output: {combined}"
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
fn density_configuration_is_strict_and_refused_before_terminal_acquisition() {
    let state = tempfile::tempdir().expect("isolated density configuration");
    let dir = state.path();
    for value in ["comfortable", "compact"] {
        std::fs::write(
            dir.join("config.toml"),
            format!("schema_version = 1\n[ui]\ndensity = \"{value}\"\n"),
        )
        .unwrap();
        let output = binary()
            .env("IGNATIUS_CONFIG_DIR", dir)
            .args(["config", "validate"])
            .output()
            .unwrap();
        assert_eq!(code(&output), 0, "{}", stderr(&output));
    }
    std::fs::write(
        dir.join("config.toml"),
        "schema_version = 1\n[ui]\ndensity = \"dense\"\n",
    )
    .unwrap();
    for args in [vec!["config", "validate"], vec!["connect"]] {
        let output = binary()
            .env("IGNATIUS_CONFIG_DIR", dir)
            .args(args)
            .output()
            .unwrap();
        assert_eq!(code(&output), 3, "{}", stderr(&output));
        let text = format!("{}{}", stdout(&output), stderr(&output));
        assert!(
            text.contains("comfortable") && text.contains("compact"),
            "{text}"
        );
        assert!(
            !text.contains("\u{1b}[?1049h"),
            "configuration refusal entered alternate screen"
        );
    }
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
    assert!(
        written.contains("[clipboard]") && written.contains("osc52 = false"),
        "the starter file exposes the safe clipboard default"
    );

    let output = run(&["config", "show"]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert!(
        stdout(&output).contains("[clipboard]") && stdout(&output).contains("osc52 = false"),
        "config show exposes the clipboard preference: {}",
        stdout(&output)
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
fn named_profiles_remain_explicit_in_noninteractive_routes() {
    let dir = std::env::temp_dir().join(format!(
        "ignatius-picker-cli-contract-{}",
        std::process::id()
    ));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).expect("create directory");
    std::fs::write(
        dir.join("config.toml"),
        "[profiles.orders-dev]\nhost = \"127.0.0.1\"\nport = 1\ndbname = \"orders\"\n",
    )
    .expect("write profile");

    let output = Command::new(env!("CARGO_BIN_EXE_ignatius"))
        .env("IGNATIUS_CONFIG_DIR", &dir)
        .env("IGNATIUS_DATA_DIR", dir.join("data"))
        .args(["query", "@orders-dev", "-c", "SELECT 1", "--format", "json"])
        .output()
        .expect("run named route");
    assert_eq!(
        code(&output),
        4,
        "the named route reaches connection: {}",
        stderr(&output)
    );
    assert!(stdout(&output).is_empty(), "failed query data on stdout");
    let combined = format!("{}{}", stdout(&output), stderr(&output));
    assert!(!combined.contains("Connection picker"), "{combined}");
    assert!(
        combined.contains("127.0.0.1"),
        "the profile route was used: {combined}"
    );

    let output = Command::new(env!("CARGO_BIN_EXE_ignatius"))
        .env("IGNATIUS_CONFIG_DIR", &dir)
        .env("IGNATIUS_DATA_DIR", dir.join("data"))
        .args(["query", "--profile", "orders-dev", "-c", "SELECT 1"])
        .output()
        .expect("run profile flag");
    assert_eq!(
        code(&output),
        4,
        "the profile flag remains a route: {}",
        stderr(&output)
    );
    assert!(!format!("{}{}", stdout(&output), stderr(&output)).contains("Connection picker"));

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
fn guided_discovery_stays_outside_plain_machine_and_history_routes() {
    // Feature 015 belongs to the full-screen presentation. A piped invocation
    // must keep the existing line-oriented and machine-readable boundaries.
    let uri = "postgres://x@127.0.0.1:1/y?connect_timeout=2";

    let output = binary()
        .env("TERM", "dumb")
        .args(["--plain", "connect", uri])
        .output()
        .expect("run");
    assert_eq!(code(&output), 4, "{}", stderr(&output));
    let plain = format!("{}{}", stdout(&output), stderr(&output));
    assert!(!plain.contains("Command palette"), "{plain}");
    assert!(!plain.contains('\u{1b}'), "{plain:?}");

    let output = binary()
        .args(["query", uri, "-c", "SELECT 1", "--format", "json"])
        .output()
        .expect("run");
    assert_eq!(code(&output), 4, "{}", stderr(&output));
    assert!(stdout(&output).is_empty(), "{}", stdout(&output));
    let machine = format!("{}{}", stdout(&output), stderr(&output));
    assert!(!machine.contains("Command palette"), "{machine}");
    assert!(
        !machine.contains("EXPLAIN"),
        "plan output must stay interactive: {machine}"
    );
    assert!(!machine.contains('\u{1b}'), "{machine:?}");

    let output = binary()
        .args(["query", uri, "-c", "SELECT 1", "--format", "ndjson"])
        .output()
        .expect("run");
    assert_eq!(code(&output), 4, "{}", stderr(&output));
    assert!(stdout(&output).is_empty(), "{}", stdout(&output));
    let machine = format!("{}{}", stdout(&output), stderr(&output));
    assert!(
        !machine.contains("EXPLAIN"),
        "plan output must stay interactive: {machine}"
    );
    assert!(!machine.contains('\u{1b}'), "{machine:?}");

    let output = binary().args(["history", "list"]).output().expect("run");
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert!(stdout(&output).is_empty(), "{}", stdout(&output));
    assert!(
        !stderr(&output).contains("Command palette"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn clipboard_transport_stays_outside_plain_and_machine_routes() {
    let dir = std::env::temp_dir().join(format!("ignatius-clipboard-cli-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).expect("create config dir");
    std::fs::write(dir.join("config.toml"), "[clipboard]\nosc52 = true\n")
        .expect("write opt-in config");
    let uri = "postgres://x@127.0.0.1:1/y?connect_timeout=2";

    for args in [
        vec!["--plain", "connect", uri],
        vec!["query", uri, "-c", "SELECT 1", "--format", "json"],
        vec!["query", uri, "-c", "SELECT 1", "--format", "ndjson"],
        vec!["query", uri, "-c", "SELECT 1", "--output", "rows.csv"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_ignatius"))
            .env("IGNATIUS_CONFIG_DIR", &dir)
            .env("IGNATIUS_DATA_DIR", dir.join("data"))
            .env("TERM", "dumb")
            .args(&args)
            .output()
            .expect("run");
        let combined = format!("{}{}", stdout(&output), stderr(&output));
        assert!(
            !combined.contains("\u{1b}]52;"),
            "machine/plain route emitted OSC 52 for {args:?}: {combined:?}"
        );
    }

    let output = Command::new(env!("CARGO_BIN_EXE_ignatius"))
        .env("IGNATIUS_CONFIG_DIR", &dir)
        .env("IGNATIUS_DATA_DIR", dir.join("data"))
        .args(["history", "list"])
        .output()
        .expect("run history");
    let combined = format!("{}{}", stdout(&output), stderr(&output));
    assert!(
        !combined.contains("\u{1b}]52;"),
        "history emitted OSC 52: {combined:?}"
    );

    std::fs::remove_dir_all(dir).ok();
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
                Ok(uri) if !uri.is_empty() => {
                    let mut uri = uri;
                    if uri.contains(":ignatius_test_password_do_not_use@") {
                        uri = uri.replace(
                            ":ignatius_test_password_do_not_use@",
                            ":not-a-real-password-disposable-container@",
                        );
                    }
                    if let Some(pos) = uri.find("?sslmode=disable") {
                        uri.truncate(pos);
                    }
                    uri
                }
                _ => {
                    eprintln!("skipping: IGNATIUS_TEST_PG_URI is not set");
                    return;
                }
            }
        };
    }

    mod buffered_retention {
        use super::*;

        const SQL: &str = "SELECT 'w10-retention-row'::text AS value FROM generate_series(1, 2)";
        const FORMATS: &[&str] = &["csv", "tsv", "ndjson", "markdown", "insert"];
        const INSERT_ROW: &str =
            "INSERT INTO \"retained_snapshot\" (\"value\") VALUES (E'w10-retention-row');\n";

        fn status(ordinal: usize, retained: usize, received: u64) -> String {
            format!(
                "Result set {ordinal} truncated: retained {retained} of {received} received rows. For a future complete streaming export, choose --output before execution.\n"
            )
        }

        fn command(uri: &str, state: &tempfile::TempDir) -> Command {
            let fields = ignatius::connection::parse_connection_string(uri)
                .expect("synthetic fixture connection is valid");
            let mut command = Command::new(env!("CARGO_BIN_EXE_ignatius"));
            command.env("IGNATIUS_CONFIG_DIR", state.path().join("config"));
            command.env("IGNATIUS_DATA_DIR", state.path().join("data"));
            command.env_remove("IGNATIUS_LOG");
            command.env_remove("IGNATIUS_TEST_PG_URI");
            for name in [
                "PGHOST",
                "PGPORT",
                "PGDATABASE",
                "PGUSER",
                "PGPASSWORD",
                "PGAPPNAME",
                "PGSSLMODE",
                "PGCONNECT_TIMEOUT",
                "PGSERVICE",
                "PGSERVICEFILE",
                "PGSYSCONFDIR",
                "PGPASSFILE",
                "PGSSLROOTCERT",
                "PGSSLCERT",
                "PGSSLKEY",
                "PGREQUIRESSL",
                "PGCHANNELBINDING",
                "PGGSSENCMODE",
            ] {
                command.env_remove(name);
            }
            command.env("PGPASSFILE", state.path().join("absent.pgpass"));
            for (field, variable) in [
                ("host", "PGHOST"),
                ("port", "PGPORT"),
                ("dbname", "PGDATABASE"),
                ("user", "PGUSER"),
                ("password", "PGPASSWORD"),
                ("sslmode", "PGSSLMODE"),
                ("sslrootcert", "PGSSLROOTCERT"),
                ("sslcert", "PGSSLCERT"),
                ("sslkey", "PGSSLKEY"),
                ("connect_timeout", "PGCONNECT_TIMEOUT"),
                ("channel_binding", "PGCHANNELBINDING"),
                ("gssencmode", "PGGSSENCMODE"),
            ] {
                if let Some(value) = fields.get(field) {
                    command.env(variable, value);
                }
            }
            command.args(["query", "--read-only"]);
            command
        }

        fn options(command: &mut Command, format: &str, cap: usize) {
            command.args(["--format", format, "--max-rows", &cap.to_string()]);
            if format == "insert" {
                command.args(["--insert-table", "retained_snapshot"]);
            }
        }

        fn expected(format: &str, rows: usize) -> String {
            match format {
                "csv" | "tsv" => format!("value\n{}", "w10-retention-row\n".repeat(rows)),
                "ndjson" => "{\"value\":\"w10-retention-row\"}\n".repeat(rows),
                "markdown" => format!(
                    "| value |\n| --- |\n{}",
                    "| w10-retention-row |\n".repeat(rows)
                ),
                "insert" => vec![INSERT_ROW; rows].join("\n"),
                _ => unreachable!(),
            }
        }

        fn reports_truncation(format: &str) {
            let uri = uri_or_skip!();
            let state = tempfile::tempdir().expect("private child state");
            let mut child = command(&uri, &state);
            options(&mut child, format, 1);
            let output = child.args(["-c", SQL]).output().expect("run isolated CLI");
            assert_eq!(code(&output), 0, "{}", stderr(&output));
            assert_eq!(
                stdout(&output),
                expected(format, 1),
                "compatible result bytes"
            );
            assert_eq!(stderr(&output), status(1, 1, 2));
            assert!(!stderr(&output).contains("w10-retention-row"));
        }

        #[test]
        fn csv_reports_retained_and_received_rows() {
            reports_truncation("csv");
        }
        #[test]
        fn tsv_reports_retained_and_received_rows() {
            reports_truncation("tsv");
        }
        #[test]
        fn ndjson_reports_retained_and_received_rows() {
            reports_truncation("ndjson");
        }
        #[test]
        fn markdown_reports_retained_and_received_rows() {
            reports_truncation("markdown");
        }
        #[test]
        fn insert_reports_retained_and_received_rows() {
            reports_truncation("insert");
        }

        #[test]
        fn nontruncated_results_have_no_new_status() {
            let uri = uri_or_skip!();
            for format in FORMATS {
                let state = tempfile::tempdir().expect("private child state");
                let mut child = command(&uri, &state);
                options(&mut child, format, 2);
                let output = child.args(["-c", SQL]).output().expect("run isolated CLI");
                assert_eq!(code(&output), 0, "{}", stderr(&output));
                assert_eq!(stdout(&output), expected(format, 2));
                assert!(stderr(&output).is_empty());
            }
        }

        #[test]
        fn zero_cap_reports_received_rows_without_inventing_data() {
            let uri = uri_or_skip!();
            for format in FORMATS {
                let state = tempfile::tempdir().expect("private child state");
                let mut child = command(&uri, &state);
                options(&mut child, format, 0);
                let output = child.args(["-c", SQL]).output().expect("run isolated CLI");
                assert_eq!(code(&output), 0, "{}", stderr(&output));
                assert_eq!(stdout(&output), expected(format, 0));
                assert_eq!(stderr(&output), status(1, 0, 2));
            }
        }

        #[test]
        fn no_header_does_not_silence_the_separate_status() {
            let uri = uri_or_skip!();
            let state = tempfile::tempdir().expect("private child state");
            let mut child = command(&uri, &state);
            options(&mut child, "csv", 1);
            let output = child
                .args(["-c", SQL, "--no-header"])
                .output()
                .expect("run isolated CLI");
            assert_eq!(code(&output), 0, "{}", stderr(&output));
            assert_eq!(stdout(&output), "w10-retention-row\n");
            assert_eq!(stderr(&output), status(1, 1, 2));
        }

        #[test]
        fn successful_prefix_status_survives_a_later_error_in_every_format() {
            let uri = uri_or_skip!();
            for format in FORMATS {
                let state = tempfile::tempdir().expect("private child state");
                let mut child = command(&uri, &state);
                options(&mut child, format, 1);
                child.arg("--verbose");
                let output = child
                    .args(["-c", &format!("{SQL}; SELECT 1 / 0")])
                    .output()
                    .expect("run isolated CLI");
                assert_eq!(code(&output), 7);
                assert_eq!(stdout(&output), expected(format, 1));
                let error = stderr(&output);
                assert!(error.starts_with(&status(1, 1, 2)), "{error}");
                assert!(
                    error.contains("division by zero") && error.contains("22012"),
                    "{error}"
                );
                assert!(!error.contains("w10-retention-row"));
            }
        }

        #[test]
        fn result_ordinals_ignore_command_only_statements_before_a_later_error() {
            let uri = uri_or_skip!();
            let state = tempfile::tempdir().expect("private child state");
            let mut child = command(&uri, &state);
            options(&mut child, "csv", 1);
            child.arg("--verbose");
            let output = child.args(["-c", "SET application_name = 'w10-status'; SELECT generate_series(1, 2) AS n; SET application_name = 'w10-status'; SELECT generate_series(3, 5) AS n; SELECT 1 / 0"]).output().expect("run isolated CLI");
            assert_eq!(code(&output), 7);
            assert_eq!(stdout(&output), "n\n1\n\nn\n3\n");
            let error = stderr(&output);
            assert!(
                error.starts_with(&format!("{}{}", status(1, 1, 2), status(2, 1, 3))),
                "{error}"
            );
            assert!(
                error.contains("division by zero") && error.contains("22012"),
                "{error}"
            );
            assert!(!error.contains("Result set 3 truncated"));
        }

        #[test]
        fn file_streaming_remains_complete_with_a_zero_buffer_cap() {
            let uri = uri_or_skip!();
            for format in ["csv", "tsv", "ndjson", "insert"] {
                let state = tempfile::tempdir().expect("private child state");
                let destination = state.path().join(format!("complete.{format}"));
                let mut child = command(&uri, &state);
                options(&mut child, format, 0);
                let output = child
                    .args(["-c", SQL, "--output"])
                    .arg(&destination)
                    .output()
                    .expect("stream isolated file");
                assert_eq!(code(&output), 0, "{}", stderr(&output));
                assert!(stdout(&output).is_empty());
                let file_data = expected(format, 2);
                assert_eq!(
                    std::fs::read_to_string(&destination).expect("completed file"),
                    file_data
                );
                assert_eq!(
                    stderr(&output),
                    format!("Exported 2 row(s) to {}\n", destination.display())
                );
                assert!(
                    !destination
                        .with_extension(format!("{format}.partial"))
                        .exists()
                );
            }
        }

        #[test]
        fn unsupported_markdown_streaming_keeps_its_existing_refusal() {
            let uri = uri_or_skip!();
            let state = tempfile::tempdir().expect("private child state");
            let destination = state.path().join("unsupported.md");
            let mut child = command(&uri, &state);
            options(&mut child, "markdown", 0);
            let output = child
                .args(["-c", SQL, "--output"])
                .arg(&destination)
                .output()
                .expect("run isolated CLI");
            assert_eq!(code(&output), 2);
            assert!(stdout(&output).is_empty());
            assert!(stderr(&output).contains("markdown cannot be written as a stream"));
            assert!(!stderr(&output).contains("truncated:"));
            assert!(!destination.exists());
        }

        #[test]
        fn json_and_table_keep_only_their_existing_inband_counts() {
            let uri = uri_or_skip!();
            for format in ["json", "table"] {
                let state = tempfile::tempdir().expect("private child state");
                let mut child = command(&uri, &state);
                options(&mut child, format, 1);
                let output = child.args(["-c", SQL]).output().expect("run isolated CLI");
                assert_eq!(code(&output), 0, "{}", stderr(&output));
                assert!(stderr(&output).is_empty());
                if format == "json" {
                    let data: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
                    assert_eq!(data[0]["rows_seen"], 2);
                    assert_eq!(data[0]["truncated"], true);
                    assert_eq!(data[0]["rows"].as_array().unwrap().len(), 1);
                } else {
                    assert!(stdout(&output).contains("showing 1 of 2 rows"));
                }
            }
        }

        #[test]
        fn refused_multi_result_ndjson_has_no_success_status() {
            let uri = uri_or_skip!();
            let state = tempfile::tempdir().expect("private child state");
            let mut child = command(&uri, &state);
            options(&mut child, "ndjson", 1);
            let output = child
                .args(["-c", &format!("{SQL}; {SQL}")])
                .output()
                .expect("run isolated CLI");
            assert_eq!(code(&output), 2);
            assert!(stdout(&output).is_empty());
            assert!(stderr(&output).contains("ndjson cannot represent"));
            assert!(!stderr(&output).contains("truncated:"));
        }
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
    fn debug_logging_never_captures_driver_sql_or_result_values() {
        const MARKER: &str = "release-log-sql-and-row-marker";
        let uri = uri_or_skip!();
        let dir =
            std::env::temp_dir().join(format!("ignatius-log-contract-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let data_dir = dir.join("data");
        let sql = format!("SELECT '{MARKER}' AS result_value");
        let output = binary()
            .env("IGNATIUS_CONFIG_DIR", &dir)
            .env("IGNATIUS_DATA_DIR", &data_dir)
            .env("IGNATIUS_LOG", "debug")
            .args(["query", &uri, "-c", &sql])
            .output()
            .expect("run");
        assert_eq!(code(&output), 0, "{}", stderr(&output));

        let log_path = data_dir.join("logs/ignatius.log");
        let log = std::fs::read_to_string(&log_path).expect("read isolated log");
        assert!(log.contains("job=1 statement_chars="), "{log}");
        assert!(!log.contains(MARKER), "SQL or result value reached {log}");
        assert!(
            !log.contains("tokio_postgres"),
            "dependency trace reached {log}"
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&log_path)
                .expect("log metadata")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600, "{}", log_path.display());
        }

        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn debug_logging_never_captures_expanded_parameter_values() {
        const SENTINEL: &str = "w04-sentinel-4f2a";
        const VARIABLE: &str = "W04_EXPANSION_SENTINEL";
        let uri = uri_or_skip!();
        let dir = std::env::temp_dir().join(format!(
            "ignatius-param-log-contract-{}",
            std::process::id()
        ));
        std::fs::remove_dir_all(&dir).ok();
        let data_dir = dir.join("data");
        let mapping = format!("sentinel={VARIABLE}");
        let sql = "SELECT :sentinel AS value";
        let output = binary()
            .env("IGNATIUS_CONFIG_DIR", &dir)
            .env("IGNATIUS_DATA_DIR", &data_dir)
            .env("IGNATIUS_LOG", "debug")
            .env(VARIABLE, SENTINEL)
            .args([
                "query",
                &uri,
                "-c",
                sql,
                "--param-env",
                &mapping,
                "--format",
                "csv",
            ])
            .output()
            .expect("run");
        assert_eq!(code(&output), 0, "{}", stderr(&output));
        assert_eq!(
            stdout(&output),
            format!("value\n{SENTINEL}\n"),
            "the supplied value must appear as ordinary result data"
        );

        assert!(
            !stderr(&output).contains(SENTINEL),
            "expanded parameter value reached stderr: {}",
            stderr(&output)
        );

        let log_path = data_dir.join("logs/ignatius.log");
        let log = std::fs::read_to_string(&log_path).expect("read isolated log");
        assert!(
            log.contains("job=1 statement_chars="),
            "the process wrote no value-free statement descriptor to {}: {log}",
            log_path.display()
        );
        assert!(
            !log.contains(SENTINEL),
            "expanded parameter value reached the log: {log}"
        );
        assert!(
            !log.contains("E'"),
            "an escaped expanded literal reached the log: {log}"
        );
        assert!(
            !log.contains("tokio_postgres"),
            "dependency trace reached the log: {log}"
        );

        std::fs::remove_dir_all(dir).ok();
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
    fn a_failed_json_query_keeps_stdout_empty_and_emits_one_diagnostic_object() {
        let uri = uri_or_skip!();
        let dir = std::env::temp_dir().join(format!(
            "ignatius-json-error-contract-{}",
            std::process::id()
        ));
        std::fs::remove_dir_all(&dir).ok();
        let data_dir = dir.join("data");
        let output = binary()
            .env("IGNATIUS_CONFIG_DIR", &dir)
            .env("IGNATIUS_DATA_DIR", &data_dir)
            .env("IGNATIUS_LOG", "debug")
            .args([
                "query",
                &uri,
                "-c",
                "SELECT * FROM no_such_table",
                "--format",
                "json",
            ])
            .output()
            .expect("run");
        assert_eq!(code(&output), 7, "query failures exit 7");
        assert!(
            stdout(&output).is_empty(),
            "stdout carried: {:?}",
            stdout(&output)
        );

        let diagnostic: serde_json::Value =
            serde_json::from_str(stderr(&output).trim()).expect("one JSON diagnostic on stderr");
        assert_eq!(diagnostic["kind"], "query");
        assert_eq!(diagnostic["statement_number"], 1);
        assert!(diagnostic["position"].is_number());
        assert!(diagnostic["technical"].as_array().is_some_and(|fields| {
            fields
                .iter()
                .any(|field| field["label"] == "SQLSTATE" && field["value"] == "42P01")
        }));
        let log = std::fs::read_to_string(data_dir.join("logs/ignatius.log"))
            .expect("isolated diagnostic log");
        assert_eq!(
            log.matches("job=1 statement_chars=").count(),
            1,
            "a failed query is not retried automatically: {log}"
        );
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn failed_json_constraint_diagnostics_keep_server_object_fields() {
        let uri = uri_or_skip!();
        let output = binary()
            .args([
                "query",
                &uri,
                "-c",
                "INSERT INTO orders (order_id, customer_id, total) VALUES (1, 99999, 1.00)",
                "--format",
                "json",
            ])
            .output()
            .expect("run");
        assert_eq!(code(&output), 7, "constraint failures exit 7");
        assert!(
            stdout(&output).is_empty(),
            "stdout carried: {:?}",
            stdout(&output)
        );

        let diagnostic: serde_json::Value =
            serde_json::from_str(stderr(&output).trim()).expect("one JSON diagnostic on stderr");
        assert_eq!(diagnostic["object"]["schema"], "public");
        assert_eq!(diagnostic["object"]["table"], "orders");
        assert_eq!(diagnostic["object"]["constraint"], "orders_pkey");
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
    fn insert_output_is_explicit_quoted_and_streamable() {
        let uri = uri_or_skip!();
        let output = binary()
            .args([
                "query",
                &uri,
                "-c",
                "SELECT order_id, note FROM orders ORDER BY order_id",
                "--format",
                "insert",
                "--insert-table",
                "orders",
            ])
            .output()
            .expect("run");
        assert_eq!(code(&output), 0, "{}", stderr(&output));
        assert!(
            stdout(&output).starts_with("INSERT INTO \"orders\""),
            "{}",
            stdout(&output)
        );
        assert!(
            stdout(&output).contains("\"order_id\""),
            "{}",
            stdout(&output)
        );
        assert!(stdout(&output).contains("NULL"), "{}", stdout(&output));
        assert!(
            !stdout(&output).contains('\x1b'),
            "machine output is undecorated"
        );
        assert!(stderr(&output).is_empty(), "{}", stderr(&output));

        let dir =
            std::env::temp_dir().join(format!("ignatius-insert-export-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create dir");
        let destination = dir.join("orders.sql");
        let streamed = binary()
            .args([
                "query",
                &uri,
                "-c",
                "SELECT order_id, note FROM orders ORDER BY order_id",
                "--format",
                "insert",
                "--insert-table",
                "orders",
                "--output",
                &destination.display().to_string(),
            ])
            .output()
            .expect("stream");
        assert_eq!(code(&streamed), 0, "{}", stderr(&streamed));
        assert!(stdout(&streamed).is_empty(), "{}", stdout(&streamed));
        assert!(
            stderr(&streamed).contains("Exported 3 row(s)"),
            "{}",
            stderr(&streamed)
        );
        assert_eq!(
            std::fs::read_to_string(&destination).expect("read"),
            stdout(&output)
        );
        std::fs::remove_dir_all(&dir).ok();
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
    fn generated_insert_output_is_accepted_when_applied_explicitly() {
        let uri = uri_or_skip!();
        let setup = binary()
            .args([
                "query",
                &uri,
                "-c",
                "CREATE TABLE IF NOT EXISTS insert_contract_probe (order_id integer, note text); TRUNCATE insert_contract_probe",
            ])
            .output()
            .expect("setup");
        assert_eq!(code(&setup), 0, "{}", stderr(&setup));

        let dir =
            std::env::temp_dir().join(format!("ignatius-insert-apply-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create dir");
        let destination = dir.join("probe.sql");
        let generated = binary()
            .args([
                "query",
                &uri,
                "-c",
                "SELECT order_id, note FROM orders ORDER BY order_id",
                "--format",
                "insert",
                "--insert-table",
                "insert_contract_probe",
                "--output",
                &destination.display().to_string(),
            ])
            .output()
            .expect("generate");
        assert_eq!(code(&generated), 0, "{}", stderr(&generated));

        // Applying the file is a separate, explicit user action. The client
        // generated it as data and did not execute it as part of the export.
        let applied = binary()
            .args(["query", &uri, "--file", &destination.display().to_string()])
            .output()
            .expect("apply");
        assert_eq!(code(&applied), 0, "{}", stderr(&applied));

        let count = binary()
            .args([
                "query",
                &uri,
                "-c",
                "SELECT count(*) AS inserted FROM insert_contract_probe",
                "--format",
                "csv",
            ])
            .output()
            .expect("count");
        assert_eq!(code(&count), 0, "{}", stderr(&count));
        assert_eq!(stdout(&count), "inserted\n3\n");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn an_interrupted_export_keeps_its_rows_and_exits_with_the_export_code() {
        use std::process::Stdio;

        let uri = uri_or_skip!();
        let dir = tempfile::tempdir().expect("isolated partial export");
        let destination = dir.path().join("stream.csv");
        let partial = std::path::PathBuf::from(format!("{}.partial", destination.display()));

        // Emit a bounded prefix that exceeds both protocol and file buffers,
        // then hold the tail on the server. Large row counts plus a fixed delay
        // race fast servers and can interrupt an already completed export.
        let mut child = binary()
            .args([
                "query",
                &uri,
                "-c",
                "SELECT n, repeat('x', 200) AS pad FROM generate_series(1, 1000) AS n \
                 UNION ALL SELECT 1001, repeat('x', 200) FROM pg_sleep(30)",
                "--format",
                "csv",
                "-o",
                &destination.display().to_string(),
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn");

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            if std::fs::metadata(&partial).is_ok_and(|metadata| metadata.len() > 8192) {
                break;
            }
            if child.try_wait().expect("poll export child").is_some() {
                let output = child.wait_with_output().expect("read early exit");
                panic!(
                    "export ended before its written-row barrier: {}",
                    stderr(&output)
                );
            }
            if std::time::Instant::now() >= deadline {
                child.kill().expect("stop only the owned export child");
                let output = child.wait_with_output().expect("read bounded failure");
                panic!(
                    "export did not write its bounded prefix: {}",
                    stderr(&output)
                );
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        unsafe_free_interrupt(child.id());

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

        assert!(partial.exists(), "the rows already written are kept");
        assert!(
            !destination.exists(),
            "an incomplete export never occupies the destination, where it would look finished"
        );
        let written = std::fs::read_to_string(&partial).expect("retained partial data");
        let mut lines = written.lines();
        assert_eq!(lines.next(), Some("n,pad"));
        let rows = lines.count();
        assert!(
            (1..=1000).contains(&rows),
            "the emitted prefix is retained: {rows}"
        );
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
    fn plain_mode_completion_is_explicit_numbered_and_does_not_run_until_sql_is_finished() {
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
            .write_all(b"SELECT * FROM ord\n\\complete\n\\use orders\n;\n\\q\n")
            .expect("write");

        let output = child.wait_with_output().expect("wait");
        assert_eq!(code(&output), 0, "{}", stderr(&output));
        let messages = stderr(&output);
        assert!(messages.contains("Completion for"), "{messages}");
        assert!(messages.contains("orders"), "{messages}");
        assert!(messages.contains("Completion inserted"), "{messages}");
        assert!(stdout(&output).contains("(3 rows)"), "{}", stdout(&output));
        assert!(!messages.contains('\u{1b}'), "{messages:?}");
        assert!(
            messages.is_ascii(),
            "plain completion must be ASCII: {messages:?}"
        );
    }

    #[test]
    fn plain_mode_formats_pending_sql_on_the_message_stream_without_running_until_finished() {
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
            .write_all(b"SELECT 1 AS one WHERE 1=1\n\\format\n;\n\\q\n")
            .expect("write");

        let output = child.wait_with_output().expect("wait");
        assert_eq!(code(&output), 0, "{}", stderr(&output));

        let data = stdout(&output);
        let messages = stderr(&output);
        assert!(messages.contains("Formatted SQL buffer:"), "{messages}");
        assert!(messages.contains("WHERE 1 = 1"), "{messages}");
        assert!(data.contains("one"), "{data}");
        assert!(data.contains("(1 row)"), "{data}");
        assert!(!data.contains("Formatted SQL"), "{data}");
        assert!(!data.contains('\u{1b}'), "{data:?}");
        assert!(!messages.contains('\u{1b}'), "{messages:?}");
        assert!(messages.is_ascii(), "{messages:?}");
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

    fn sanitize_live_uri(uri: &str) -> String {
        if uri.contains(":ignatius_test_password_do_not_use@") {
            uri.replace(
                ":ignatius_test_password_do_not_use@",
                ":not-a-real-password-disposable-container@",
            )
        } else {
            uri.to_owned()
        }
    }

    fn resolve_live_tls_uri(uri: &str) -> String {
        let uri = sanitize_live_uri(uri);
        let manifest = env!("CARGO_MANIFEST_DIR");
        let fallback_cert = format!("{manifest}/docker/tls/generated/ca.crt");
        if let Some(idx) = uri.find("sslrootcert=") {
            let prefix = &uri[..idx + "sslrootcert=".len()];
            let after = &uri[idx + "sslrootcert=".len()..];
            let (path_str, rest) = match after.find('&') {
                Some(amp) => (&after[..amp], &after[amp..]),
                None => (after, ""),
            };
            let resolved_path = if let Some(stripped) = path_str.strip_prefix("./") {
                format!("{manifest}/{stripped}")
            } else if !path_str.starts_with('/') {
                format!("{manifest}/{path_str}")
            } else {
                path_str.to_owned()
            };
            let actual_path = if !std::path::Path::new(&resolved_path).exists()
                && std::path::Path::new(&fallback_cert).exists()
            {
                fallback_cert
            } else {
                resolved_path
            };
            format!("{prefix}{actual_path}{rest}")
        } else if std::path::Path::new(&fallback_cert).exists() && uri.contains("55433") {
            let delimiter = if uri.contains('?') { "&" } else { "?" };
            format!("{uri}{delimiter}sslmode=verify-full&sslrootcert={fallback_cert}")
        } else {
            uri
        }
    }

    #[test]
    fn plain_mode_live_tls_verify_full_reports_cipher_and_runs_query() {
        use std::io::Write;
        use std::process::Stdio;

        let uri = match std::env::var("IGNATIUS_TEST_PG_TLS_URI") {
            Ok(uri) if !uri.is_empty() => uri,
            _ => {
                eprintln!("skipping: IGNATIUS_TEST_PG_TLS_URI is not set");
                return;
            }
        };
        let tls_uri = resolve_live_tls_uri(&uri);

        let mut child = binary()
            .args(["--plain", "connect", &tls_uri])
            .env("TERM", "dumb")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn plain tls client");

        child
            .stdin
            .as_mut()
            .expect("stdin")
            .write_all(b"SELECT current_database(), current_user;\n\\q\n")
            .expect("write");

        let output = child.wait_with_output().expect("wait");
        assert_eq!(code(&output), 0, "{}", stderr(&output));

        let data = stdout(&output);
        let messages = stderr(&output);

        assert!(data.contains("ignatius_demo"), "database in data: {data}");
        assert!(data.contains("ignatius_test"), "user in data: {data}");
        assert!(
            messages.contains("sslmode=verify-full"),
            "verify-full in messages: {messages}"
        );
        assert!(
            messages.contains("TLS")
                && (messages.contains("TLS_AES")
                    || messages.contains("using")
                    || messages.contains("cipher")),
            "cipher or protocol in messages: {messages}"
        );
        assert!(
            !data.contains('\u{1b}'),
            "data contains control sequence: {data}"
        );
        assert!(
            !messages.contains('\u{1b}'),
            "messages contain control sequence: {messages}"
        );
    }

    #[test]
    fn plain_mode_live_production_write_guard_refuses_unconfirmed_delete() {
        use std::io::Write;
        use std::process::Stdio;

        let uri = uri_or_skip!();
        let uri = sanitize_live_uri(&uri);
        let dir = std::env::temp_dir().join(format!("ignatius-prod-guard-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create dir");

        let (_, rest) = uri.split_once("://").expect("a URI");
        let (userinfo, host_and_db) = rest.split_once('@').expect("userinfo");
        let (user, password) = userinfo.split_once(':').expect("a password");
        let (host_port, database) = host_and_db.split_once('/').expect("a database");
        let (host, port) = host_port.split_once(':').expect("a port");
        let database = database.split_once('?').map_or(database, |(db, _)| db);

        std::fs::write(
            dir.join("config.toml"),
            format!(
                "[profiles.prod_sim]\nhost = \"{host}\"\nport = {port}\n\
                 dbname = \"{database}\"\nuser = \"{user}\"\nenvironment = \"production\"\n"
            ),
        )
        .expect("write config");

        let mut child = Command::new(env!("CARGO_BIN_EXE_ignatius"))
            .env("IGNATIUS_CONFIG_DIR", &dir)
            .env("IGNATIUS_DATA_DIR", dir.join("data"))
            .env("PGPASSWORD", password)
            .env("TERM", "dumb")
            .args(["--plain", "connect", "--profile", "prod_sim"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn plain production client");

        child
            .stdin
            .as_mut()
            .expect("stdin")
            .write_all(b"DELETE FROM orders;\nno\nSELECT count(*) FROM orders;\n\\q\n")
            .expect("write");

        let output = child.wait_with_output().expect("wait");
        assert_eq!(code(&output), 0, "{}", stderr(&output));

        let data = stdout(&output);
        let messages = stderr(&output);

        assert!(
            messages.contains("Connected to") && messages.contains("PROD"),
            "connected to PROD banner: {messages}"
        );
        assert!(
            messages.contains("Cancelled. Nothing was sent."),
            "cancelled message: {messages}"
        );
        assert!(
            data.contains("3") || data.contains("5"),
            "orders count unchanged in stdout: {data}"
        );
        assert!(
            !data.contains('\u{1b}'),
            "data contains control sequence: {data}"
        );
        assert!(
            !messages.contains('\u{1b}'),
            "messages contain control sequence: {messages}"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn plain_mode_live_syntax_error_recovers_and_runs_next_statement() {
        use std::io::Write;
        use std::process::Stdio;

        let uri = uri_or_skip!();
        let uri = sanitize_live_uri(&uri);

        let mut child = binary()
            .args(["--plain", "connect", &uri])
            .env("TERM", "dumb")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn plain syntax recovery client");

        child
            .stdin
            .as_mut()
            .expect("stdin")
            .write_all(b"SELEC broken;\nSELECT 42 AS answer;\n\\q\n")
            .expect("write");

        let output = child.wait_with_output().expect("wait");
        assert_eq!(
            code(&output),
            0,
            "plain REPL exits 0 after syntax error recovery and \\q: {}",
            stderr(&output)
        );

        let data = stdout(&output);
        let messages = stderr(&output);

        assert!(messages.contains("42601"), "SQLSTATE 42601 in {messages}");
        assert!(
            messages.contains("line 1, column 1"),
            "line 1, column 1 in {messages}"
        );
        assert!(data.contains("42"), "42 answer in stdout: {data}");
        assert!(
            !data.contains('\u{1b}'),
            "data contains control sequence: {data}"
        );
        assert!(
            !messages.contains('\u{1b}'),
            "messages contain control sequence: {messages}"
        );
    }

    #[test]
    fn diagram_subcommand_exports_mermaid_dot_svg_and_ascii_against_live_postgres() {
        let uri = uri_or_skip!();

        // 1. Export mermaid diagram to stdout
        let output = binary()
            .arg("diagram")
            .arg(&uri)
            .arg("--schema")
            .arg("public")
            .arg("--format")
            .arg("mermaid")
            .output()
            .expect("run ignatius diagram mermaid");

        assert_eq!(
            code(&output),
            0,
            "diagram mermaid exits zero: {}",
            stderr(&output)
        );
        let mermaid_out = stdout(&output);
        assert!(
            mermaid_out.starts_with("erDiagram\n"),
            "stdout starts with erDiagram\\n: {mermaid_out}"
        );
        assert!(
            mermaid_out.contains("public.orders"),
            "stdout contains public.orders: {mermaid_out}"
        );

        // 2. Export svg diagram to a temporary file
        let temp_svg = temp_config_dir().join(format!("test_diagram_{}.svg", std::process::id()));
        let _ = std::fs::remove_file(&temp_svg);

        let output_svg = binary()
            .arg("diagram")
            .arg(&uri)
            .arg("--schema")
            .arg("public")
            .arg("--format")
            .arg("svg")
            .arg("--output")
            .arg(&temp_svg)
            .output()
            .expect("run ignatius diagram svg");

        assert_eq!(
            code(&output_svg),
            0,
            "diagram svg exits zero: {}",
            stderr(&output_svg)
        );
        assert!(temp_svg.exists(), "SVG output file was created");
        let svg_content = std::fs::read_to_string(&temp_svg).expect("read svg content");
        assert!(
            svg_content.starts_with("<svg"),
            "SVG file starts with <svg: {svg_content}"
        );
        let svg_err = stderr(&output_svg);
        assert!(
            svg_err.contains("Wrote SVG diagram"),
            "stderr reports wrote svg diagram: {svg_err}"
        );

        let _ = std::fs::remove_file(&temp_svg);
    }

    #[test]
    fn diagram_subcommand_refuses_existing_output_file_without_force() {
        let uri = uri_or_skip!();

        let temp_file = temp_config_dir().join(format!(
            "test_diagram_refuse_{}.mermaid",
            std::process::id()
        ));
        std::fs::write(&temp_file, "preserved original content").expect("write original file");

        let output = binary()
            .arg("diagram")
            .arg(&uri)
            .arg("--schema")
            .arg("public")
            .arg("--format")
            .arg("mermaid")
            .arg("--output")
            .arg(&temp_file)
            .output()
            .expect("run ignatius diagram refuse without force");

        assert_ne!(
            code(&output),
            0,
            "diagram without --force must exit non-zero when output file exists"
        );
        let content = std::fs::read_to_string(&temp_file).expect("read preserved file");
        assert_eq!(
            content, "preserved original content",
            "existing file content must be left untouched"
        );

        let _ = std::fs::remove_file(&temp_file);
    }

    #[test]
    fn ddl_subcommand_exports_object_and_schema_ddl_with_optional_drop_headers() {
        let uri = uri_or_skip!();

        // 1. Export single table DDL with drop header to stdout
        let output = binary()
            .arg("ddl")
            .arg(&uri)
            .arg("public.orders")
            .arg("--drop")
            .output()
            .expect("run ignatius ddl public.orders --drop");

        assert_eq!(
            code(&output),
            0,
            "ddl public.orders exits zero: {}",
            stderr(&output)
        );
        let sql_out = stdout(&output);
        assert!(
            sql_out.contains("DROP TABLE IF EXISTS \"public\".\"orders\" CASCADE;"),
            "stdout contains drop table statement: {sql_out}"
        );
        assert!(
            sql_out.contains("CREATE TABLE \"public\".\"orders\""),
            "stdout contains create table statement: {sql_out}"
        );

        // 2. Export entire public schema DDL to a file
        let temp_ddl = temp_config_dir().join(format!("test_ddl_{}.sql", std::process::id()));
        let _ = std::fs::remove_file(&temp_ddl);

        let output_file = binary()
            .arg("ddl")
            .arg(&uri)
            .arg("--schema")
            .arg("public")
            .arg("--output")
            .arg(&temp_ddl)
            .output()
            .expect("run ignatius ddl schema output");

        assert_eq!(
            code(&output_file),
            0,
            "ddl schema output exits zero: {}",
            stderr(&output_file)
        );
        assert!(temp_ddl.exists(), "DDL output file was created");
        let ddl_content = std::fs::read_to_string(&temp_ddl).expect("read ddl content");
        assert!(
            ddl_content.contains("CREATE TABLE \"public\".\"orders\""),
            "DDL contains create table orders: {ddl_content}"
        );
        assert!(
            ddl_content.contains("CREATE OR REPLACE VIEW \"public\".\"recent_orders\""),
            "DDL contains create or replace view recent_orders: {ddl_content}"
        );
        let ddl_err = stderr(&output_file);
        assert!(
            ddl_err.contains("Wrote DDL"),
            "stderr reports wrote DDL: {ddl_err}"
        );

        let _ = std::fs::remove_file(&temp_ddl);
    }

    #[test]
    fn ddl_subcommand_refuses_existing_output_file_without_force() {
        let uri = uri_or_skip!();

        let temp_file =
            temp_config_dir().join(format!("test_ddl_refuse_{}.sql", std::process::id()));
        std::fs::write(&temp_file, "preserved ddl content").expect("write original file");

        let output = binary()
            .arg("ddl")
            .arg(&uri)
            .arg("orders")
            .arg("--output")
            .arg(&temp_file)
            .output()
            .expect("run ignatius ddl refuse without force");

        assert_ne!(
            code(&output),
            0,
            "ddl without --force must exit non-zero when output file exists"
        );
        let content = std::fs::read_to_string(&temp_file).expect("read preserved file");
        assert_eq!(
            content, "preserved ddl content",
            "existing file content must be left untouched"
        );

        let _ = std::fs::remove_file(&temp_file);
    }

    #[test]
    fn activity_subcommand_renders_backends_locks_and_json_and_protects_existing_files() {
        let uri = uri_or_skip!();

        // 1. Run ignatius activity <uri> (default table format)
        let output = binary()
            .arg("activity")
            .arg(&uri)
            .output()
            .expect("run ignatius activity");

        assert_eq!(code(&output), 0, "activity exits zero: {}", stderr(&output));
        let out = stdout(&output);
        assert!(
            out.contains("Database:"),
            "stdout contains Database header: {out}"
        );
        assert!(
            out.contains("Cache Hit:"),
            "stdout contains Cache Hit: {out}"
        );
        assert!(out.contains("PID"), "stdout contains PID column: {out}");
        assert!(out.contains("STATE"), "stdout contains STATE column: {out}");

        // 2. Run ignatius activity <uri> --locks
        let locks_output = binary()
            .arg("activity")
            .arg(&uri)
            .arg("--locks")
            .output()
            .expect("run ignatius activity --locks");

        assert_eq!(
            code(&locks_output),
            0,
            "activity --locks exits zero: {}",
            stderr(&locks_output)
        );

        // 3. Run ignatius activity <uri> --format json --output <path>
        let temp_file =
            temp_config_dir().join(format!("test_activity_{}.json", std::process::id()));
        let _ = std::fs::remove_file(&temp_file);

        let json_output = binary()
            .arg("activity")
            .arg(&uri)
            .arg("--format")
            .arg("json")
            .arg("--output")
            .arg(&temp_file)
            .output()
            .expect("run ignatius activity --format json --output");

        assert_eq!(
            code(&json_output),
            0,
            "activity json output exits zero: {}",
            stderr(&json_output)
        );
        assert!(temp_file.exists(), "activity JSON output file created");
        let json_content = std::fs::read_to_string(&temp_file).expect("read activity JSON file");
        assert!(
            json_content.contains("\"active_connections\""),
            "JSON contains active_connections: {json_content}"
        );
        assert!(
            json_content.contains("\"backends\""),
            "JSON contains backends: {json_content}"
        );

        // 4. Verify running ignatius activity <uri> --output <same_path> without --force exits non-zero and preserves file
        let conflict_output = binary()
            .arg("activity")
            .arg(&uri)
            .arg("--output")
            .arg(&temp_file)
            .output()
            .expect("run ignatius activity conflict");

        assert_ne!(
            code(&conflict_output),
            0,
            "activity without --force must exit non-zero when output file exists"
        );
        let preserved_content = std::fs::read_to_string(&temp_file).expect("read preserved file");
        assert_eq!(
            preserved_content, json_content,
            "existing file content must be preserved untouched"
        );

        let _ = std::fs::remove_file(&temp_file);
    }

    #[test]
    fn audit_subcommand_exports_roles_rls_and_grants_and_protects_existing_files() {
        let uri = uri_or_skip!();

        // 1. Run ignatius audit <uri> (default all-sections table format)
        let output = binary()
            .arg("audit")
            .arg(&uri)
            .output()
            .expect("run ignatius audit");

        assert_eq!(code(&output), 0, "audit exits zero: {}", stderr(&output));
        let out = stdout(&output);
        assert!(out.contains("Roles:"), "stdout contains Roles: {out}");
        assert!(
            out.contains("RLS POSTURE"),
            "stdout contains RLS POSTURE: {out}"
        );
        assert!(
            out.contains("PRIVILEGES"),
            "stdout contains PRIVILEGES: {out}"
        );

        // 2. Run ignatius audit <uri> --section rls --schema public
        let rls_output = binary()
            .arg("audit")
            .arg(&uri)
            .arg("--section")
            .arg("rls")
            .arg("--schema")
            .arg("public")
            .output()
            .expect("run ignatius audit --section rls --schema public");

        assert_eq!(
            code(&rls_output),
            0,
            "audit --section rls exits zero: {}",
            stderr(&rls_output)
        );

        // 3. Run ignatius audit <uri> --format json --output <path>
        let temp_file = temp_config_dir().join(format!("test_audit_{}.json", std::process::id()));
        let _ = std::fs::remove_file(&temp_file);

        let json_output = binary()
            .arg("audit")
            .arg(&uri)
            .arg("--format")
            .arg("json")
            .arg("--output")
            .arg(&temp_file)
            .output()
            .expect("run ignatius audit --format json --output");

        assert_eq!(
            code(&json_output),
            0,
            "audit json output exits zero: {}",
            stderr(&json_output)
        );
        assert!(temp_file.exists(), "audit JSON output file created");
        let json_content = std::fs::read_to_string(&temp_file).expect("read audit JSON file");
        assert!(
            json_content.contains("\"roles\""),
            "JSON contains roles: {json_content}"
        );
        assert!(
            json_content.contains("\"tables\""),
            "JSON contains tables: {json_content}"
        );
        assert!(
            json_content.contains("\"grants\""),
            "JSON contains grants: {json_content}"
        );

        // 4. Verify running ignatius audit <uri> --output <same_path> without --force exits non-zero and preserves file
        let conflict_output = binary()
            .arg("audit")
            .arg(&uri)
            .arg("--output")
            .arg(&temp_file)
            .output()
            .expect("run ignatius audit conflict");

        assert_ne!(
            code(&conflict_output),
            0,
            "audit without --force must exit non-zero when output file exists"
        );
        let preserved_content = std::fs::read_to_string(&temp_file).expect("read preserved file");
        assert_eq!(
            preserved_content, json_content,
            "existing file content must be preserved untouched"
        );

        let _ = std::fs::remove_file(&temp_file);
    }

    #[test]
    fn diff_subcommand_compares_schemas_generates_sql_and_protects_existing_files() {
        let uri = uri_or_skip!();

        // 1. Run ignatius diff <uri> --source-schema public --target-schema public (identical schemas)
        let output = binary()
            .arg("diff")
            .arg(&uri)
            .arg("--source-schema")
            .arg("public")
            .arg("--target-schema")
            .arg("public")
            .output()
            .expect("run ignatius diff identical");

        assert_eq!(code(&output), 0, "diff exits zero: {}", stderr(&output));
        let out = stdout(&output);
        assert!(
            out.contains("no migration necessary"),
            "identical schemas report no migration necessary: {out}"
        );

        // 2. Run ignatius diff <uri> --source-schema public --target-schema reporting --format tree
        let tree_output = binary()
            .arg("diff")
            .arg(&uri)
            .arg("--source-schema")
            .arg("public")
            .arg("--target-schema")
            .arg("reporting")
            .arg("--format")
            .arg("tree")
            .output()
            .expect("run ignatius diff --format tree");

        assert_eq!(
            code(&tree_output),
            0,
            "diff --format tree exits zero: {}",
            stderr(&tree_output)
        );
        let tree_out = stdout(&tree_output);
        assert!(
            tree_out.contains("Schema Diff: public -> reporting"),
            "stdout contains tree header: {tree_out}"
        );

        // 3. Run ignatius diff <uri> --source-schema public --target-schema reporting --format json --output <path>
        let temp_file = temp_config_dir().join(format!("test_diff_{}.json", std::process::id()));
        let _ = std::fs::remove_file(&temp_file);

        let json_output = binary()
            .arg("diff")
            .arg(&uri)
            .arg("--source-schema")
            .arg("public")
            .arg("--target-schema")
            .arg("reporting")
            .arg("--format")
            .arg("json")
            .arg("--output")
            .arg(&temp_file)
            .output()
            .expect("run ignatius diff --format json --output");

        assert_eq!(
            code(&json_output),
            0,
            "diff json output exits zero: {}",
            stderr(&json_output)
        );
        assert!(temp_file.exists(), "diff JSON output file created");
        let json_content = std::fs::read_to_string(&temp_file).expect("read diff JSON file");
        assert!(
            json_content.contains("\"schema_version\": 1"),
            "JSON contains schema_version: {json_content}"
        );
        assert!(
            json_content.contains("\"source_schema\": \"public\""),
            "JSON contains source_schema: {json_content}"
        );

        // 4. Verify running ignatius diff <uri> without --force exits non-zero and preserves file
        let conflict_output = binary()
            .arg("diff")
            .arg(&uri)
            .arg("--source-schema")
            .arg("public")
            .arg("--target-schema")
            .arg("reporting")
            .arg("--output")
            .arg(&temp_file)
            .output()
            .expect("run ignatius diff conflict");

        assert_ne!(
            code(&conflict_output),
            0,
            "diff without --force must exit non-zero when output file exists"
        );
        let preserved_content = std::fs::read_to_string(&temp_file).expect("read preserved file");
        assert_eq!(
            preserved_content, json_content,
            "existing file content must be preserved untouched"
        );

        let _ = std::fs::remove_file(&temp_file);

        // 5. Verify error when neither target2 nor target_schema is provided
        let missing_args_output = binary()
            .arg("diff")
            .arg(&uri)
            .output()
            .expect("run ignatius diff without target2 or target_schema");

        assert_ne!(
            code(&missing_args_output),
            0,
            "diff without target schema or target2 must fail"
        );
        assert!(
            stderr(&missing_args_output).contains("missing target schema"),
            "stderr explains missing target schema: {}",
            stderr(&missing_args_output)
        );
    }

    #[test]
    fn import_export_backup_restore_subcommands_contract() {
        let uri = uri_or_skip!();

        // 1. Export query to CSV on stdout
        let export_output = binary()
            .arg("export")
            .arg(&uri)
            .arg("SELECT 1 AS num, 'hello' AS greeting")
            .arg("--format")
            .arg("csv")
            .output()
            .expect("run ignatius export");

        assert_eq!(
            code(&export_output),
            0,
            "export exits zero: {}",
            stderr(&export_output)
        );
        let export_out = stdout(&export_output);
        assert!(
            export_out.contains("num,greeting"),
            "export stdout contains CSV header: {export_out}"
        );
        assert!(
            export_out.contains("1,hello"),
            "export stdout contains CSV row: {export_out}"
        );

        // 2. Export query to file with output overwrite protection
        let temp_file = temp_config_dir().join(format!("test_export_{}.csv", std::process::id()));
        let _ = std::fs::remove_file(&temp_file);

        let export_file_output = binary()
            .arg("export")
            .arg(&uri)
            .arg("SELECT 42 AS answer")
            .arg("--output")
            .arg(&temp_file)
            .output()
            .expect("run ignatius export --output");

        assert_eq!(
            code(&export_file_output),
            0,
            "export to file exits zero: {}",
            stderr(&export_file_output)
        );
        assert!(temp_file.exists(), "export file created");
        let content = std::fs::read_to_string(&temp_file).expect("read exported file");
        assert!(
            content.contains("answer"),
            "file content contains answer: {content}"
        );
        assert!(
            content.contains("42"),
            "file content contains 42: {content}"
        );

        // Export without --force refuses to overwrite
        let conflict_output = binary()
            .arg("export")
            .arg(&uri)
            .arg("SELECT 99 AS answer")
            .arg("--output")
            .arg(&temp_file)
            .output()
            .expect("run ignatius export conflict");

        assert_ne!(
            code(&conflict_output),
            0,
            "export without --force must fail on existing file"
        );
        let preserved = std::fs::read_to_string(&temp_file).expect("read preserved file");
        assert_eq!(preserved, content, "file preserved untouched");
        let _ = std::fs::remove_file(&temp_file);

        // 3. Backup with --dry-run prints command and redacted env
        let dump_file = temp_config_dir().join(format!("test_backup_{}.dump", std::process::id()));
        let backup_output = binary()
            .arg("backup")
            .arg(&uri)
            .arg("--output")
            .arg(&dump_file)
            .arg("--dry-run")
            .output()
            .expect("run ignatius backup --dry-run");

        assert_eq!(
            code(&backup_output),
            0,
            "backup --dry-run exits zero: {}",
            stderr(&backup_output)
        );
        let backup_out = stdout(&backup_output);
        assert!(
            backup_out.contains("Command: pg_dump"),
            "stdout contains pg_dump: {backup_out}"
        );
        assert!(
            backup_out.contains("-F c"),
            "stdout contains format custom: {backup_out}"
        );

        // 4. Restore with --dry-run prints command and redacted env
        let restore_output = binary()
            .arg("restore")
            .arg(&uri)
            .arg("--input")
            .arg(&dump_file)
            .arg("--dry-run")
            .output()
            .expect("run ignatius restore --dry-run");

        assert_eq!(
            code(&restore_output),
            0,
            "restore --dry-run exits zero: {}",
            stderr(&restore_output)
        );
        let restore_out = stdout(&restore_output);
        assert!(
            restore_out.contains("Command: pg_restore"),
            "stdout contains pg_restore: {restore_out}"
        );
    }

    #[test]
    fn routine_runner_and_profiler_cli_subcommands() {
        // 1. Validate help outputs without requiring live database
        let run_help = binary()
            .args(["run-routine", "--help"])
            .output()
            .expect("run ignatius run-routine --help");
        assert_eq!(code(&run_help), 0);
        let run_help_str = stdout(&run_help);
        assert!(run_help_str.contains("--param"));
        assert!(run_help_str.contains("--rollback"));
        assert!(run_help_str.contains("--confirm-production"));
        assert!(run_help_str.contains("--format"));

        let profile_help = binary()
            .args(["profile-routine", "--help"])
            .output()
            .expect("run ignatius profile-routine --help");
        assert_eq!(code(&profile_help), 0);
        let profile_help_str = stdout(&profile_help);
        assert!(profile_help_str.contains("--format"));
        assert!(profile_help_str.contains("--output"));
        assert!(profile_help_str.contains("--force"));

        // 2. Test live routine execution and profiling against PostgreSQL container
        let uri = uri_or_skip!();

        // Setup test function
        let setup = binary()
            .args([
                "query",
                &uri,
                "-c",
                "CREATE OR REPLACE FUNCTION public.ignatius_test_add(a integer, b integer) \
                 RETURNS integer AS $$ BEGIN RETURN a + b; END; $$ LANGUAGE plpgsql;",
            ])
            .output()
            .expect("create test function");
        assert_eq!(
            code(&setup),
            0,
            "create function failed: {}",
            stderr(&setup)
        );

        // Execute via run-routine with JSON format
        let run_json = binary()
            .args([
                "run-routine",
                &uri,
                "public.ignatius_test_add",
                "-a",
                "a=17",
                "-a",
                "b=25",
                "--format",
                "json",
            ])
            .output()
            .expect("run-routine json");
        assert_eq!(
            code(&run_json),
            0,
            "run-routine json failed: {}",
            stderr(&run_json)
        );
        let run_json_str = stdout(&run_json);
        assert!(run_json_str.contains("\"success\": true"));
        assert!(run_json_str.contains("\"42\""));

        // Execute via run-routine with tabular format
        let run_table = binary()
            .args([
                "run-routine",
                &uri,
                "public.ignatius_test_add",
                "-a",
                "a=5",
                "-a",
                "b=7",
                "--format",
                "table",
            ])
            .output()
            .expect("run-routine table");
        assert_eq!(
            code(&run_table),
            0,
            "run-routine table failed: {}",
            stderr(&run_table)
        );
        let run_table_str = stdout(&run_table);
        assert!(run_table_str.contains("ignatius_test_add"));
        assert!(run_table_str.contains("12"));

        // Profile routine
        let profile_table = binary()
            .args([
                "profile-routine",
                &uri,
                "public.ignatius_test_add",
                "--format",
                "table",
            ])
            .output()
            .expect("profile-routine table");
        assert_eq!(
            code(&profile_table),
            0,
            "profile-routine table failed: {}",
            stderr(&profile_table)
        );
        let profile_table_str = stdout(&profile_table);
        assert!(profile_table_str.contains("ignatius_test_add"));

        // Cleanup test function
        let cleanup = binary()
            .args([
                "query",
                &uri,
                "-c",
                "DROP FUNCTION public.ignatius_test_add(integer, integer);",
            ])
            .output()
            .expect("drop test function");
        assert_eq!(
            code(&cleanup),
            0,
            "drop function failed: {}",
            stderr(&cleanup)
        );
    }

    #[test]
    fn listen_notify_and_maintain_cli_subcommands() {
        // 1. Validate help outputs without requiring live database
        let listen_help = binary()
            .args(["listen", "--help"])
            .output()
            .expect("run ignatius listen --help");
        assert_eq!(code(&listen_help), 0);
        let listen_help_str = stdout(&listen_help);
        assert!(listen_help_str.contains("--timeout"));
        assert!(listen_help_str.contains("--format"));
        assert!(listen_help_str.contains("--output"));
        assert!(listen_help_str.contains("--force"));

        let notify_help = binary()
            .args(["notify", "--help"])
            .output()
            .expect("run ignatius notify --help");
        assert_eq!(code(&notify_help), 0);
        let notify_help_str = stdout(&notify_help);
        assert!(notify_help_str.contains("[CHANNEL]"));

        let maintain_help = binary()
            .args(["maintain", "--help"])
            .output()
            .expect("run ignatius maintain --help");
        assert_eq!(code(&maintain_help), 0);
        let maintain_help_str = stdout(&maintain_help);
        assert!(maintain_help_str.contains("vacuum"));
        assert!(maintain_help_str.contains("analyze"));
        assert!(maintain_help_str.contains("reindex"));
        assert!(maintain_help_str.contains("progress"));

        let vacuum_help = binary()
            .args(["maintain", "vacuum", "--help"])
            .output()
            .expect("run ignatius maintain vacuum --help");
        assert_eq!(code(&vacuum_help), 0);
        let vacuum_help_str = stdout(&vacuum_help);
        assert!(vacuum_help_str.contains("--full"));
        assert!(vacuum_help_str.contains("--freeze"));
        assert!(vacuum_help_str.contains("--verbose"));
        assert!(vacuum_help_str.contains("--analyze"));

        // 2. Test live maintenance and notify against PostgreSQL container
        let uri = uri_or_skip!();

        // Dispatch a pub/sub notification
        let notify_out = binary()
            .args([
                "notify",
                &uri,
                "ignatius_alerts",
                "{\"alert\": \"test_event\"}",
            ])
            .output()
            .expect("run ignatius notify");
        assert_eq!(
            code(&notify_out),
            0,
            "notify failed: {}",
            stderr(&notify_out)
        );
        let notify_str = stdout(&notify_out);
        assert!(notify_str.contains("Dispatched notification to channel \"ignatius_alerts\""));

        // Run vacuum analyze
        let maintain_vacuum = binary()
            .args(["maintain", &uri, "vacuum", "--analyze", "--verbose"])
            .output()
            .expect("run ignatius maintain vacuum");
        assert_eq!(
            code(&maintain_vacuum),
            0,
            "maintain vacuum failed: {}",
            stderr(&maintain_vacuum)
        );
        let vacuum_str = stdout(&maintain_vacuum);
        assert!(vacuum_str.contains("Maintenance completed successfully"));

        // Run analyze
        let maintain_analyze = binary()
            .args(["maintain", &uri, "analyze", "--verbose"])
            .output()
            .expect("run ignatius maintain analyze");
        assert_eq!(
            code(&maintain_analyze),
            0,
            "maintain analyze failed: {}",
            stderr(&maintain_analyze)
        );
        let analyze_str = stdout(&maintain_analyze);
        assert!(analyze_str.contains("Maintenance completed successfully"));

        // Query maintenance progress report in JSON
        let progress_out = binary()
            .args(["maintain", &uri, "--format", "json", "progress"])
            .output()
            .expect("run ignatius maintain progress");
        assert_eq!(
            code(&progress_out),
            0,
            "maintain progress failed: {}",
            stderr(&progress_out)
        );
        let progress_str = stdout(&progress_out);
        assert!(progress_str.starts_with('[') || progress_str.contains("[]"));
    }
}
