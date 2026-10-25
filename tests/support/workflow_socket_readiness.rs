//! Runs the checked-in socket-start step without a Docker daemon or real waits.

use super::{job_block, workflow_text};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Output;

pub(super) struct Run {
    pub output: Output,
    pub events: Vec<String>,
    pub sleeps: Vec<String>,
    pub published_env: String,
}

pub(super) fn run(scenario: &str, previous_socket_probe: bool) -> Run {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workflow = workflow_text(&root.join(".github/workflows/ci.yml"));
    let job = job_block(&workflow, "socket-integration");
    let lines: Vec<_> = job.lines().collect();
    let step = lines
        .iter()
        .position(|line| *line == "      - name: Start the socket fixture")
        .expect("the socket fixture has a named startup step");
    let body = lines[step + 1..]
        .iter()
        .position(|line| *line == "        run: |")
        .map(|offset| step + 1 + offset)
        .expect("the socket startup step has a literal shell body");
    let script = lines[body + 1..]
        .iter()
        .take_while(|line| line.trim().is_empty() || line.starts_with("          "))
        .map(|line| line.strip_prefix("          ").unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!script.is_empty(), "the actual startup shell must execute");
    let script = if previous_socket_probe {
        // A negative control changes only this isolated copy, never the workflow.
        assert_eq!(script.matches("pg_isready -h 127.0.0.1").count(), 1);
        script.replace(
            "pg_isready -h 127.0.0.1",
            "pg_isready -h /var/run/postgresql",
        )
    } else {
        script
    };

    let isolated = tempfile::tempdir().expect("isolated socket startup test");
    let bin = isolated.path().join("bin");
    fs::create_dir(&bin).expect("fake executable directory");
    let docker = bin.join("docker");
    fs::write(
        &docker,
        r#"#!/bin/bash
set -euo pipefail
container=ignatius-postgres-socket-123-1
case "${1:-}" in
  run)
    [[ "${2:-}" == --detach && "${3:-}" == --name && "${4:-}" == "$container" ]] || exit 97
    printf 'start\n' >> "$MOCK_EVENTS"
    printf 'synthetic-container-id\n'
    ;;
  exec)
    [[ "$#" -eq 9 && "$2" == "$container" && "$3" == pg_isready && "$4" == -h && "$6" == -U && "$7" == ignatius_test && "$8" == -d && "$9" == ignatius_demo ]] || exit 97
    printf 'probe:%s\n' "$5" >> "$MOCK_EVENTS"
    case "$5" in
      /var/run/postgresql)
        # The temporary initialization server accepts local socket connections.
        exit 0
        ;;
      127.0.0.1)
        read -r attempts < "$MOCK_TCP_COUNT"
        attempts=$((attempts + 1))
        printf '%s\n' "$attempts" > "$MOCK_TCP_COUNT"
        [[ "$MOCK_SCENARIO" == eventually-ready && "$attempts" -ge 3 ]]
        ;;
      *) exit 97 ;;
    esac
    ;;
  logs)
    [[ "$#" -eq 2 && "$2" == "$container" ]] || exit 97
    printf 'logs\n' >> "$MOCK_EVENTS"
    printf 'synthetic socket fixture diagnostic\n'
    ;;
  *) exit 97 ;;
esac
"#,
    )
    .expect("fake Docker client");
    fs::set_permissions(&docker, fs::Permissions::from_mode(0o755))
        .expect("executable fake Docker client");
    let sleep = bin.join("sleep");
    fs::write(
        &sleep,
        r#"#!/bin/bash
set -euo pipefail
[[ "$#" -eq 1 && "$1" == 1 ]] || exit 97
printf '%s\n' "$1" >> "$MOCK_SLEEPS"
"#,
    )
    .expect("fake delay");
    fs::set_permissions(&sleep, fs::Permissions::from_mode(0o755)).expect("executable fake delay");

    let events = isolated.path().join("events");
    let sleeps = isolated.path().join("sleeps");
    let count = isolated.path().join("tcp-count");
    let published_env = isolated.path().join("github-env");
    for path in [&events, &sleeps, &published_env] {
        fs::write(path, "").expect("empty event evidence");
    }
    fs::write(&count, "0\n").expect("initial probe count");
    let path = std::env::join_paths(std::iter::once(bin).chain(std::env::split_paths(
        &std::env::var_os("PATH").expect("system executable path"),
    )))
    .expect("fake-client-first executable path");
    let output = std::process::Command::new("bash")
        .current_dir(isolated.path())
        .args(["-c", &script])
        .env("PATH", path)
        .env("RUNNER_TEMP", isolated.path())
        .env("GITHUB_RUN_ID", "123")
        .env("GITHUB_RUN_ATTEMPT", "1")
        .env("GITHUB_ENV", &published_env)
        .env(
            "IGNATIUS_POSTGRES_IMAGE",
            "synthetic-reviewed-postgres-image",
        )
        .env("MOCK_EVENTS", &events)
        .env("MOCK_SLEEPS", &sleeps)
        .env("MOCK_TCP_COUNT", &count)
        .env("MOCK_SCENARIO", scenario)
        .output()
        .expect("run actual startup shell without Docker or real delays");

    Run {
        output,
        events: fs::read_to_string(events)
            .expect("Docker event evidence")
            .lines()
            .map(str::to_owned)
            .collect(),
        sleeps: fs::read_to_string(sleeps)
            .expect("delay evidence")
            .lines()
            .map(str::to_owned)
            .collect(),
        published_env: fs::read_to_string(published_env).expect("published fixture identity"),
    }
}
