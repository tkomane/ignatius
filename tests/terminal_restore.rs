//! Terminal restoration, checked against a real pseudo-terminal.
//!
//! The unit tests in `ui::terminal` prove that the escape sequences balance.
//! They cannot prove that the binary actually emits them, in order, when a
//! person quits it. This runs the real client under a pty and reads the bytes.
//!
//! Unix only. The Windows equivalent needs ConPTY and is tracked as open work in
//! `docs/status.md`; skipping is reported rather than passed over.

// Tests report skips to the developer running them; that is what stderr is for.
#![allow(clippy::print_stderr)]
#![cfg(unix)]

use std::io::{Read, Seek, SeekFrom, Write};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

/// A target that is refused immediately, so the client draws and then sits.
const UNREACHABLE: &str = "postgres://someone@127.0.0.1:1/nothing?connect_timeout=2";

/// Ctrl+Q, the quit binding.
const QUIT: &[u8] = b"\x11";

/// Routes read by connection resolution, password files and service files.
const PG_CONNECTION_ENV: &[&str] = &[
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
];

/// The returned directory must outlive the child, including its final output.
fn isolate_terminal_command(command: &mut Command) -> tempfile::TempDir {
    let state = tempfile::tempdir().expect("isolated terminal state");
    let config = state.path().join("config");
    let data = state.path().join("data");
    std::fs::create_dir_all(&config).expect("isolated terminal configuration");
    std::fs::create_dir_all(&data).expect("isolated terminal data");
    command
        .env("IGNATIUS_CONFIG_DIR", config)
        .env("IGNATIUS_DATA_DIR", data)
        .env_remove("IGNATIUS_LOG");
    for key in PG_CONNECTION_ENV {
        command.env_remove(key);
    }
    // Clearing the inherited setting alone would fall back to ~/.pgpass.
    command.env("PGPASSFILE", state.path().join("missing.pgpass"));
    state
}

#[test]
fn terminal_children_receive_unique_isolated_state_without_inherited_connections() {
    let hostile = tempfile::tempdir().expect("synthetic inherited state");
    let hostile_config = hostile.path().join("config");
    let hostile_data = hostile.path().join("data");
    std::fs::create_dir_all(&hostile_config).expect("synthetic config directory");
    std::fs::create_dir_all(&hostile_data).expect("synthetic data directory");
    let mut states = Vec::new();
    let mut child_paths = Vec::new();

    for _ in 0..2 {
        // Every hostile value belongs to this command and this temporary tree.
        // No process-global environment or developer profile is read or changed.
        let mut command = Command::new("sh");
        command
            .env("IGNATIUS_CONFIG_DIR", &hostile_config)
            .env("IGNATIUS_DATA_DIR", &hostile_data)
            .env("IGNATIUS_LOG", "debug");
        for key in PG_CONNECTION_ENV {
            command.env(key, "synthetic-inherited-setting");
        }
        let state = isolate_terminal_command(&mut command);
        let output = command
            .args([
                "-c",
                r#"set -eu
for key do
  if printenv "$key" >/dev/null; then
    printf 'present:%s\n' "$key"
  fi
done
printf 'config:%s\ndata:%s\npassfile:%s\n' "$IGNATIUS_CONFIG_DIR" "$IGNATIUS_DATA_DIR" "${PGPASSFILE-}"
if [ -d "$IGNATIUS_CONFIG_DIR" ] && [ -d "$IGNATIUS_DATA_DIR" ]; then
  printf 'state-directories-alive\n'
fi
if [ -n "${PGPASSFILE-}" ] && [ ! -e "$PGPASSFILE" ]; then
  printf 'personal-password-fallback-blocked\n'
fi
"#,
                "terminal-environment-probe",
                "IGNATIUS_LOG",
            ])
            .args(PG_CONNECTION_ENV)
            .output()
            .expect("run real child environment probe");
        assert!(
            output.status.success(),
            "child probe failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let transcript = String::from_utf8(output.stdout).expect("probe UTF-8");
        let present: Vec<_> = transcript
            .lines()
            .filter_map(|line| line.strip_prefix("present:"))
            .collect();
        assert_eq!(
            present,
            ["PGPASSFILE"],
            "only the harness-owned absent password file may reach the child"
        );
        for (label, expected) in [
            ("config:", state.path().join("config")),
            ("data:", state.path().join("data")),
            ("passfile:", state.path().join("missing.pgpass")),
        ] {
            let actual = transcript
                .lines()
                .find_map(|line| line.strip_prefix(label))
                .expect("child reported its actual state path");
            assert_eq!(std::path::Path::new(actual), expected);
            child_paths.push(expected);
        }
        assert!(transcript.contains("state-directories-alive"));
        assert!(transcript.contains("personal-password-fallback-blocked"));
        states.push(state);
    }

    assert_ne!(states[0].path(), states[1].path());
    assert_ne!(child_paths[0], child_paths[3]);
    assert_ne!(child_paths[1], child_paths[4]);
    assert!(states.iter().all(|state| state.path().is_dir()));
}

/// Waits only while this still-unreaped child is owned by the test.
///
/// There is no detached watchdog holding a numeric PID after normal completion.
/// At the deadline the child is terminated and reaped before this returns.
fn wait_for_owned_child(child: &mut Child, timeout: Duration) -> (ExitStatus, bool) {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait().expect("poll owned terminal child") {
            return (status, false);
        }
        if Instant::now() >= deadline {
            // An exit racing this kill is harmless: the child remains unreaped,
            // so its PID cannot have been assigned to an unrelated process.
            let _ = child.kill();
            return (child.wait().expect("reap timed-out terminal child"), true);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn captured_bytes(file: &mut std::fs::File) -> Vec<u8> {
    file.seek(SeekFrom::Start(0))
        .expect("rewind private capture");
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).expect("read private capture");
    bytes
}

#[test]
fn the_owned_deadline_reaps_a_hung_child_without_a_detached_pid_watchdog() {
    let mut child = Command::new("sleep")
        .arg("30")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn owned sleeping probe");
    let (status, timed_out) = wait_for_owned_child(&mut child, Duration::from_millis(50));
    assert!(
        timed_out,
        "the owned sleeping child must reach the deadline"
    );
    assert!(!status.success());
    assert_eq!(
        child.try_wait().expect("child has already been reaped"),
        Some(status)
    );
}

#[test]
fn a_completed_child_and_large_capture_finish_without_pipe_backpressure() {
    let mut stdout = tempfile::tempfile().expect("private stdout capture");
    let mut stderr = tempfile::tempfile().expect("private stderr capture");
    let mut child = Command::new("sh")
        .args([
            "-c",
            "i=0; while [ \"$i\" -lt 10000 ]; do printf 'owned-output-'; i=$((i + 1)); done; printf 'owned-stderr' >&2",
        ])
        .stdin(Stdio::null())
        .stdout(stdout.try_clone().expect("child stdout capture"))
        .stderr(stderr.try_clone().expect("child stderr capture"))
        .spawn()
        .expect("spawn owned output probe");
    let (status, timed_out) = wait_for_owned_child(&mut child, Duration::from_secs(5));
    assert!(!timed_out, "large output must not block child completion");
    assert!(status.success());
    assert_eq!(captured_bytes(&mut stdout), b"owned-output-".repeat(10000));
    assert_eq!(captured_bytes(&mut stderr), b"owned-stderr");
    assert_eq!(
        child.try_wait().expect("completed owned child"),
        Some(status)
    );
}

/// Runs the client under a pty and returns everything it wrote.
fn run_in_pty(extra_args: &[&str], input: &'static [u8]) -> Option<String> {
    if Command::new("script").arg("--version").output().is_err()
        && Command::new("script").arg("-h").output().is_err()
    {
        eprintln!("skipping: `script` is not available to allocate a pty");
        return None;
    }

    let binary = env!("CARGO_BIN_EXE_ignatius");
    let inner = format!(
        "stty rows 30 cols 100; exec {binary} {} connect '{UNREACHABLE}'",
        extra_args.join(" ")
    );

    // BSD and util-linux disagree about where the command goes.
    let mut command = Command::new("script");
    if cfg!(target_os = "linux") {
        // `-e` makes util-linux `script` return the command's exit code.
        // Without it, it returns zero whatever happened, and any assertion
        // about the exit status would be checking `script` rather than the
        // client. BSD `script` returns the child's status already.
        command.args(["-e", "-q", "-c", &inner, "/dev/null"]);
    } else {
        command.args(["-q", "/dev/null", "sh", "-c", &inner]);
    }

    let _state = isolate_terminal_command(&mut command);
    // Private regular files cannot fill a pipe while the parent polls exit.
    // They also avoid output-reader threads that could outlive a failed test.
    let mut stdout = tempfile::tempfile().expect("private PTY stdout capture");
    let mut stderr = tempfile::tempfile().expect("private PTY stderr capture");
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(stdout.try_clone().expect("PTY stdout capture"))
        .stderr(stderr.try_clone().expect("PTY stderr capture"))
        .spawn()
        .expect("spawn under a pty");

    let mut stdin = child.stdin.take().expect("stdin");
    let writer = std::thread::spawn(move || {
        // Let the client take the terminal and draw at least one frame first.
        std::thread::sleep(Duration::from_secs(3));
        let _ = stdin.write_all(input);
        let _ = stdin.flush();
        std::thread::sleep(Duration::from_secs(1));
    });

    let (status, timed_out) = wait_for_owned_child(&mut child, Duration::from_secs(25));
    writer.join().expect("join bounded PTY input writer");
    let stdout = captured_bytes(&mut stdout);
    let stderr = captured_bytes(&mut stderr);

    assert!(
        !timed_out,
        "the owned PTY child exceeded its deadline: {:?}",
        String::from_utf8_lossy(&stderr)
    );
    assert!(
        status.success(),
        "the client did not exit cleanly: {:?}",
        String::from_utf8_lossy(&stderr)
    );
    Some(String::from_utf8_lossy(&stdout).into_owned())
}

#[test]
fn quitting_restores_every_mode_it_took() {
    let Some(session) = run_in_pty(&[], QUIT) else {
        return;
    };

    for (on, off, what) in [
        ("\x1b[?1049h", "\x1b[?1049l", "alternate screen"),
        ("\x1b[?2004h", "\x1b[?2004l", "bracketed paste"),
        ("\x1b[?25l", "\x1b[?25h", "cursor visibility"),
        ("\x1b[?1000h", "\x1b[?1000l", "mouse capture"),
    ] {
        assert!(session.contains(on), "{what} was never enabled");
        assert!(session.contains(off), "{what} was left enabled");
    }
}

#[test]
fn the_alternate_screen_is_left_last_so_the_restored_state_is_the_users() {
    let Some(session) = run_in_pty(&[], QUIT) else {
        return;
    };

    let show_cursor = session.rfind("\x1b[?25h").expect("cursor restored");
    let leave_screen = session.rfind("\x1b[?1049l").expect("screen left");
    assert!(
        show_cursor < leave_screen,
        "the cursor must be restored before the screen is handed back, \
         or the restore applies to a screen the user never sees"
    );
}

#[test]
fn the_client_draws_something_before_it_is_asked_to_leave() {
    // A restoration test that passes because nothing was ever drawn would be
    // worthless, so this asserts the client really took the terminal and used it.
    let Some(session) = run_in_pty(&[], QUIT) else {
        return;
    };
    assert!(
        session.contains("Ignatius"),
        "the interface never rendered, so restoration proves nothing"
    );
    assert!(
        session.contains("Ctrl+Q"),
        "the quit hint should be on screen: the user has to know how to leave"
    );
}

fn assert_clean_terminal_restoration(session: &str) {
    for (on, off, what) in [
        ("\x1b[?1049h", "\x1b[?1049l", "alternate screen"),
        ("\x1b[?2004h", "\x1b[?2004l", "bracketed paste"),
        ("\x1b[?25l", "\x1b[?25h", "cursor visibility"),
        ("\x1b[?1000h", "\x1b[?1000l", "mouse capture"),
    ] {
        assert!(session.contains(on), "{what} was never enabled");
        assert!(session.contains(off), "{what} was left enabled");
    }

    let show_cursor = session.rfind("\x1b[?25h").expect("cursor restored");
    let leave_screen = session.rfind("\x1b[?1049l").expect("screen left");
    assert!(
        show_cursor < leave_screen,
        "the cursor must be restored before the screen is handed back, \
         or the restore applies to a screen the user never sees"
    );
}

fn resolve_live_uri(env_var: &str) -> Option<String> {
    let uri = std::env::var(env_var).ok()?;
    if uri.is_empty() {
        return None;
    }
    let uri = if uri.contains(":ignatius_test_password_do_not_use@") {
        uri.replace(
            ":ignatius_test_password_do_not_use@",
            ":not-a-real-password-disposable-container@",
        )
    } else {
        uri
    };
    Some(resolve_sslrootcert(&uri))
}

fn resolve_sslrootcert(uri: &str) -> String {
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
        uri.to_owned()
    }
}

fn run_live_pty(
    cols: u16,
    rows: u16,
    uri: &str,
    extra_args: &[&str],
    input_fn: impl FnOnce(&mut std::process::ChildStdin) + Send + 'static,
) -> Option<String> {
    if Command::new("script").arg("--version").output().is_err()
        && Command::new("script").arg("-h").output().is_err()
    {
        eprintln!("skipping: `script` is not available to allocate a pty");
        return None;
    }

    let binary = env!("CARGO_BIN_EXE_ignatius");
    let args_str = extra_args.join(" ");
    let inner = format!("stty rows {rows} cols {cols}; exec {binary} {args_str} connect '{uri}'");

    let mut command = Command::new("script");
    if cfg!(target_os = "linux") {
        command.args(["-e", "-q", "-c", &inner, "/dev/null"]);
    } else {
        command.args(["-q", "/dev/null", "sh", "-c", &inner]);
    }
    command.current_dir(env!("CARGO_MANIFEST_DIR"));

    let _state = isolate_terminal_command(&mut command);
    let mut stdout = tempfile::tempfile().expect("private PTY stdout capture");
    let mut stderr = tempfile::tempfile().expect("private PTY stderr capture");
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(stdout.try_clone().expect("PTY stdout capture"))
        .stderr(stderr.try_clone().expect("PTY stderr capture"))
        .spawn()
        .expect("spawn live client under a pty");

    let mut stdin = child.stdin.take().expect("stdin");
    let writer = std::thread::spawn(move || {
        input_fn(&mut stdin);
    });

    let (status, timed_out) = wait_for_owned_child(&mut child, Duration::from_secs(25));
    writer.join().expect("join live PTY input writer");
    let stdout = captured_bytes(&mut stdout);
    let stderr = captured_bytes(&mut stderr);

    assert!(
        !timed_out,
        "the owned live PTY child exceeded its deadline: {:?}",
        String::from_utf8_lossy(&stderr)
    );
    assert!(
        status.success(),
        "the live client did not exit cleanly: {:?}",
        String::from_utf8_lossy(&stderr)
    );
    Some(String::from_utf8_lossy(&stdout).into_owned())
}

#[test]
fn live_postgres_pty_workbench_connects_queries_and_restores_across_viewport_matrix() {
    let Some(uri) = resolve_live_uri("IGNATIUS_TEST_PG_URI") else {
        eprintln!("skipping: IGNATIUS_TEST_PG_URI is not set");
        return;
    };

    // 1. Full layout (100x30): connects, runs query buffer with Ctrl+R, verifies output, restores
    let full = run_live_pty(100, 30, &uri, &[], |stdin| {
        std::thread::sleep(Duration::from_secs(3));
        let _ = stdin.write_all(b"\x12"); // Ctrl+R: run buffer
        let _ = stdin.flush();
        std::thread::sleep(Duration::from_millis(1500));
        let _ = stdin.write_all(QUIT);
        let _ = stdin.flush();
        std::thread::sleep(Duration::from_millis(500));
    });
    if let Some(session) = full {
        assert!(session.contains("Ignatius"), "interface rendered in 100x30");
        assert!(
            session.contains("1 row") || session.contains("Ready") || session.contains("SELECT"),
            "query executed or status updated: {session}"
        );
        assert_clean_terminal_restoration(&session);
    }

    // 2. Standard VT100 viewport (80x24): connects, renders workbench, restores cleanly
    let vt100 = run_live_pty(80, 24, &uri, &[], |stdin| {
        std::thread::sleep(Duration::from_secs(3));
        let _ = stdin.write_all(QUIT);
        let _ = stdin.flush();
        std::thread::sleep(Duration::from_millis(500));
    });
    if let Some(session) = vt100 {
        assert!(session.contains("Ignatius"), "interface rendered in 80x24");
        assert_clean_terminal_restoration(&session);
    }

    // 3. Compact viewport (60x20): connects, renders compact layout, restores cleanly
    let compact = run_live_pty(60, 20, &uri, &[], |stdin| {
        std::thread::sleep(Duration::from_secs(3));
        let _ = stdin.write_all(QUIT);
        let _ = stdin.flush();
        std::thread::sleep(Duration::from_millis(500));
    });
    if let Some(session) = compact {
        assert!(
            session.contains("ignatius_demo") || session.contains("Editor"),
            "compact interface rendered in 60x20: {session}"
        );
        assert_clean_terminal_restoration(&session);
    }

    // 4. Sub-minimum viewport (30x6): renders too-small notification, restores cleanly
    let too_small = run_live_pty(30, 6, &uri, &[], |stdin| {
        std::thread::sleep(Duration::from_secs(2));
        let _ = stdin.write_all(QUIT);
        let _ = stdin.flush();
        std::thread::sleep(Duration::from_millis(500));
    });
    if let Some(session) = too_small {
        assert!(
            session.contains("30x6") || (session.contains("Terminal") && session.contains("small")),
            "too small notification rendered in 30x6: {session}"
        );
        assert_clean_terminal_restoration(&session);
    }
}

#[test]
fn live_postgres_tls_verify_full_pty_renders_trust_badge_and_restores_cleanly() {
    let Some(tls_uri) = resolve_live_uri("IGNATIUS_TEST_PG_TLS_URI") else {
        eprintln!("skipping: IGNATIUS_TEST_PG_TLS_URI is not set");
        return;
    };

    let session = run_live_pty(100, 30, &tls_uri, &[], |stdin| {
        std::thread::sleep(Duration::from_secs(3));
        // Open command palette (Ctrl+P)
        let _ = stdin.write_all(b"\x10");
        let _ = stdin.flush();
        std::thread::sleep(Duration::from_millis(800));
        // Activate "Connection and auth details" (Enter) to open trust surface
        let _ = stdin.write_all(b"\r");
        let _ = stdin.flush();
        std::thread::sleep(Duration::from_millis(800));
        // Quit cleanly (Ctrl+Q)
        let _ = stdin.write_all(QUIT);
        let _ = stdin.flush();
        std::thread::sleep(Duration::from_millis(500));
    });

    if let Some(transcript) = session {
        assert!(transcript.contains("Ignatius"), "interface rendered");
        assert!(
            transcript.contains("TLS") && transcript.contains("verify-full"),
            "trust surface details rendered in transcript: {transcript}"
        );
        assert_clean_terminal_restoration(&transcript);
    }
}
