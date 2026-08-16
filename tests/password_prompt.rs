//! The password prompt, driven by hand through a real pseudo-terminal.
//!
//! Everything else about the prompt is proven by tests that never type a
//! password: the reducer's rules, the masking, and the refusal to ask a pipe.
//! None of that proves the one thing that matters most - that typing a password
//! actually opens the connection - and a promise nobody has ever exercised is
//! not evidence.
//!
//! So this runs the real binary under a pty, against the disposable container,
//! with the password removed from the target. The server asks, the test answers,
//! and the session opens. It also reads back everything the terminal received,
//! which is how the "not echoed" claim is checked rather than asserted.
//!
//! Unix only: the Windows equivalent needs ConPTY, and skipping is reported.

// Tests report skips to the developer running them; that is what stderr is for.
#![allow(clippy::print_stderr)]
#![cfg(unix)]

use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Duration;

/// Splits the test URI into a target with no password, and the password.
///
/// The container's credentials are synthetic and defined in `docker/dev.env`;
/// nothing here ever touches a real database.
fn target_and_password() -> Option<(String, String)> {
    let uri = std::env::var("IGNATIUS_TEST_PG_URI")
        .ok()
        .filter(|u| !u.is_empty())?;
    let (scheme, rest) = uri.split_once("://")?;
    let (userinfo, host_and_db) = rest.split_once('@')?;
    let (user, password) = userinfo.split_once(':')?;
    Some((
        format!("{scheme}://{user}@{host_and_db}"),
        password.to_owned(),
    ))
}

/// Runs plain mode under a pty, typing what is given, and returns the transcript.
fn ask_and_answer(target: &str, typed: &[String]) -> Option<(String, bool)> {
    if Command::new("script").arg("--version").output().is_err()
        && Command::new("script").arg("-h").output().is_err()
    {
        eprintln!("skipping: `script` is not available to allocate a pty");
        return None;
    }

    let binary = env!("CARGO_BIN_EXE_ignatius");
    let inner = format!("exec {binary} --plain connect '{target}'");

    // BSD and util-linux disagree about where the command goes.
    let mut command = Command::new("script");
    if cfg!(target_os = "linux") {
        command.args(["-q", "-c", &inner, "/dev/null"]);
    } else {
        command.args(["-q", "/dev/null", "sh", "-c", &inner]);
    }

    let temp = std::env::temp_dir().join("ignatius-password-pty");
    let mut child = command
        .env("IGNATIUS_CONFIG_DIR", &temp)
        .env("IGNATIUS_DATA_DIR", temp.join("data"))
        // The password must come from the prompt and from nowhere else, or the
        // test would pass without ever exercising it.
        .env_remove("PGPASSWORD")
        .env_remove("PGPASSFILE")
        .env("TERM", "dumb")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn under a pty");

    let mut stdin = child.stdin.take().expect("stdin");
    let lines: Vec<String> = typed.to_vec();
    let writer = std::thread::spawn(move || {
        for line in lines {
            // Let the connection be attempted and refused before answering.
            std::thread::sleep(Duration::from_secs(2));
            let _ = stdin.write_all(line.as_bytes());
            let _ = stdin.flush();
        }
        std::thread::sleep(Duration::from_secs(1));
    });

    // A guard, so a build that never prompts fails rather than hanging the suite.
    let id = child.id();
    let guard = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(30));
        let _ = Command::new("kill")
            .args(["-KILL", &id.to_string()])
            .status();
    });

    let output = child.wait_with_output().expect("wait");
    let _ = writer.join();
    drop(guard);

    let transcript = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Some((transcript, output.status.success()))
}

#[test]
fn typing_the_password_opens_the_session_and_the_password_is_never_echoed() {
    let Some((target, password)) = target_and_password() else {
        eprintln!(
            "skipping: IGNATIUS_TEST_PG_URI is not set, or carries no password to remove. \
             Start the container with `docker compose -f docker/compose.yaml up -d`."
        );
        return;
    };

    // Carriage return, not newline: in raw mode a terminal sends `\r` for Enter,
    // and that is exactly what the prompt is reading.
    let Some((transcript, succeeded)) =
        ask_and_answer(&target, &[format!("{password}\r"), "\\q\n".to_owned()])
    else {
        return;
    };

    assert!(
        transcript.contains("Password for"),
        "the server asked and the client did not: {transcript}"
    );
    assert!(
        transcript.contains("Connected to"),
        "the password was typed and the session did not open: {transcript}"
    );
    assert!(succeeded, "the client did not exit cleanly: {transcript}");

    // The claim that matters, read from what the terminal actually received.
    assert!(
        !transcript.contains(&password),
        "the password was echoed into the transcript"
    );
}

#[test]
fn refusing_to_answer_leaves_the_servers_own_refusal() {
    let Some((target, _)) = target_and_password() else {
        eprintln!("skipping: IGNATIUS_TEST_PG_URI is not set");
        return;
    };

    // Enter on an empty prompt is a decision not to answer. What is left is the
    // server's refusal, not a new error about the prompt.
    let Some((transcript, succeeded)) = ask_and_answer(&target, &["\r".to_owned()]) else {
        return;
    };

    assert!(transcript.contains("Password for"), "{transcript}");
    assert!(
        !succeeded,
        "answering nothing cannot open a session: {transcript}"
    );
    assert!(
        transcript.contains("password") || transcript.contains("Authentication"),
        "the reason is still the server's: {transcript}"
    );
}
