//! Security and reliability contracts shared by every GitHub Actions workflow.
//!
//! These checks intentionally validate properties rather than one particular
//! action release. Dependabot can update a full, reviewed commit SHA without a
//! second hard-coded copy in a Rust test drifting out of date.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

#[cfg(unix)]
#[path = "support/workflow_socket_readiness.rs"]
mod socket_readiness;

#[derive(Debug)]
struct ActionReference {
    action: String,
    revision: String,
    release_label: String,
    line: usize,
}

fn workflow_paths() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(".github/workflows");
    let mut paths: Vec<PathBuf> = fs::read_dir(&root)
        .expect("read workflow directory")
        .map(|entry| entry.expect("read workflow entry").path())
        .filter(|path| {
            matches!(
                path.extension().and_then(|extension| extension.to_str()),
                Some("yml" | "yaml")
            )
        })
        .collect();
    paths.sort();
    assert!(
        !paths.is_empty(),
        "the repository must define a CI workflow"
    );
    paths
}

fn local_action_paths() -> Vec<PathBuf> {
    fn collect(directory: &Path, paths: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(directory)
            .unwrap_or_else(|error| panic!("read {}: {error}", directory.display()))
        {
            let path = entry
                .unwrap_or_else(|error| {
                    panic!("read action entry in {}: {error}", directory.display())
                })
                .path();
            if path.is_dir() {
                collect(&path, paths);
            } else if matches!(
                path.file_name().and_then(|name| name.to_str()),
                Some("action.yml" | "action.yaml")
            ) {
                paths.push(path);
            }
        }
    }

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(".github/actions");
    let mut paths = Vec::new();
    if root.exists() {
        collect(&root, &mut paths);
    }
    paths.sort();
    paths
}

fn action_definition_paths() -> Vec<PathBuf> {
    let mut paths = workflow_paths();
    paths.extend(local_action_paths());
    paths.sort();
    paths
}

fn workflow_text(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

fn job_block(workflow: &str, name: &str) -> String {
    let lines: Vec<&str> = workflow.lines().collect();
    let marker = format!("  {name}:");
    let start = lines
        .iter()
        .position(|line| *line == marker)
        .unwrap_or_else(|| panic!("workflow has no {name} job"));
    let end = lines[start + 1..]
        .iter()
        .position(|line| {
            line.starts_with("  ")
                && !line.starts_with("    ")
                && line.trim_end().ends_with(':')
                && !line.trim_start().starts_with('#')
        })
        .map_or(lines.len(), |offset| start + 1 + offset);
    lines[start..end].join("\n")
}

fn action_references(path: &Path, workflow: &str) -> Vec<ActionReference> {
    workflow
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let trimmed = line.trim();
            let value = trimmed
                .strip_prefix("- uses: ")
                .or_else(|| trimmed.strip_prefix("uses: "))?;
            if value.starts_with("./") {
                return None;
            }
            let (reference, release_label) = value.split_once(" # ").unwrap_or_else(|| {
                panic!(
                    "{}:{} must label the release represented by its immutable action SHA",
                    path.display(),
                    index + 1
                )
            });
            let (action, revision) = reference.rsplit_once('@').unwrap_or_else(|| {
                panic!(
                    "{}:{} has an external action without a revision",
                    path.display(),
                    index + 1
                )
            });
            Some(ActionReference {
                action: action.to_owned(),
                revision: revision.to_owned(),
                release_label: release_label.to_owned(),
                line: index + 1,
            })
        })
        .collect()
}

fn quoted_toml_assignment(path: &Path, section: &str, key: &str) -> String {
    let text =
        fs::read_to_string(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    let assignment = format!("{key} = ");
    let mut active_section = "";

    for raw_line in text.lines() {
        let line = raw_line.split('#').next().unwrap_or_default().trim();
        if line.starts_with('[') && line.ends_with(']') {
            active_section = line;
            continue;
        }
        if active_section != section {
            continue;
        }
        if let Some(value) = line.strip_prefix(&assignment) {
            let value = value.trim();
            assert!(
                value.starts_with('"') && value.ends_with('"'),
                "{} {section}.{key} must be a quoted string",
                path.display()
            );
            return value.trim_matches('"').to_owned();
        }
    }

    panic!("{} has no {section}.{key}", path.display());
}

fn exact_rust_release(version: &str) -> String {
    match version.split('.').count() {
        2 => format!("{version}.0"),
        3 => version.to_owned(),
        _ => panic!("unsupported Rust version contract: {version}"),
    }
}

#[test]
fn every_external_action_is_an_immutable_reviewable_release() {
    let mut revisions: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut references_seen = 0;

    for path in action_definition_paths() {
        let workflow = workflow_text(&path);
        for reference in action_references(&path, &workflow) {
            references_seen += 1;
            assert!(
                reference.action.contains('/'),
                "{}:{} is not a repository-qualified action: {}",
                path.display(),
                reference.line,
                reference.action
            );
            assert!(
                reference.revision.len() == 40
                    && reference
                        .revision
                        .bytes()
                        .all(|byte| byte.is_ascii_hexdigit()),
                "{}:{} must pin {} to one full 40-character commit SHA, found {}",
                path.display(),
                reference.line,
                reference.action,
                reference.revision
            );
            assert!(
                reference.release_label.starts_with('v') || reference.release_label == "master",
                "{}:{} must identify the reviewed action release beside the SHA",
                path.display(),
                reference.line
            );
            revisions
                .entry(reference.action)
                .or_default()
                .insert(reference.revision);
        }
    }

    assert!(
        references_seen > 0,
        "no external workflow actions were checked"
    );
    for (action, pins) in revisions {
        assert_eq!(
            pins.len(),
            1,
            "{action} uses inconsistent immutable pins across workflows: {pins:?}"
        );
    }
}

#[test]
fn checkouts_do_not_leave_the_workflow_token_in_git() {
    for path in workflow_paths() {
        let workflow = workflow_text(&path);
        let lines: Vec<&str> = workflow.lines().collect();

        for (index, line) in lines.iter().enumerate() {
            if !line.contains("- uses: actions/checkout@") {
                continue;
            }
            let indentation = line.len() - line.trim_start().len();
            let block_end = lines[index + 1..]
                .iter()
                .position(|candidate| {
                    let candidate_indent = candidate.len() - candidate.trim_start().len();
                    candidate_indent == indentation && candidate.trim_start().starts_with("- ")
                })
                .map_or(lines.len(), |offset| index + 1 + offset);
            let block = &lines[index + 1..block_end];
            assert!(
                block
                    .iter()
                    .any(|candidate| candidate.trim() == "persist-credentials: false"),
                "{}:{} checkout must set persist-credentials: false",
                path.display(),
                index + 1
            );
        }
    }
}

#[test]
fn workflows_are_read_only_bounded_and_do_not_use_privileged_pr_code() {
    for path in workflow_paths() {
        let workflow = workflow_text(&path);
        let lines: Vec<&str> = workflow.lines().collect();
        let jobs = lines
            .iter()
            .position(|line| *line == "jobs:")
            .unwrap_or_else(|| panic!("{} has no jobs", path.display()));
        let permission_starts: Vec<usize> = lines
            .iter()
            .enumerate()
            .filter_map(|(index, line)| (line.trim() == "permissions:").then_some(index))
            .collect();
        assert_eq!(
            permission_starts.len(),
            1,
            "{} must have exactly one workflow-level permissions block and no job override",
            path.display()
        );
        let permission_start = permission_starts[0];
        assert!(
            permission_start < jobs,
            "{} permissions must be workflow-level rather than a job override",
            path.display()
        );
        let permission_end = lines[permission_start + 1..]
            .iter()
            .position(|line| !line.is_empty() && !line.starts_with(' ') && !line.starts_with('#'))
            .map_or(lines.len(), |offset| permission_start + 1 + offset);
        let permission_entries: Vec<&str> = lines[permission_start + 1..permission_end]
            .iter()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .collect();
        // CodeQL uploads SARIF, which requires the security-events scope. Every
        // other workflow stays read-only.
        let expected_permissions: &[&str] =
            if path.file_name().and_then(|name| name.to_str()) == Some("codeql.yml") {
                &["contents: read", "security-events: write"]
            } else {
                &["contents: read"]
            };
        assert_eq!(
            permission_entries.as_slice(),
            expected_permissions,
            "{} must grant only its reviewed scopes",
            path.display()
        );
        assert!(
            !workflow.contains("pull_request_target"),
            "{} must not run untrusted pull request code in a privileged context",
            path.display()
        );
        assert!(
            !workflow.contains("contents: write"),
            "{} must remain non-publishing",
            path.display()
        );
        assert!(
            !workflow.contains("continue-on-error: true"),
            "{} must not make a required workflow gate advisory",
            path.display()
        );

        let job_starts: Vec<(usize, &str)> = lines[jobs + 1..]
            .iter()
            .enumerate()
            .filter_map(|(offset, line)| {
                let trimmed = line.trim();
                let is_job = line.starts_with("  ")
                    && !line.starts_with("    ")
                    && trimmed.ends_with(':')
                    && !trimmed.starts_with('#');
                is_job.then_some((jobs + 1 + offset, trimmed.trim_end_matches(':')))
            })
            .collect();

        assert!(!job_starts.is_empty(), "{} has no jobs", path.display());
        for (position, (start, name)) in job_starts.iter().enumerate() {
            let end = job_starts
                .get(position + 1)
                .map_or(lines.len(), |(next, _)| *next);
            assert!(
                lines[*start + 1..end]
                    .iter()
                    .any(|line| line.trim_start().starts_with("timeout-minutes:")),
                "{} job {name} needs an explicit timeout-minutes bound",
                path.display()
            );
        }
    }
}

#[test]
fn shared_ci_runs_the_complete_declared_coverage() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(".github/workflows/ci.yml");
    let workflow = workflow_text(&path);

    for expected in [
        "workflow_dispatch:",
        "schedule:",
        "cancel-in-progress: true",
        "cargo test --locked --workspace --all-targets --all-features",
        "cargo test --locked --workspace --doc --all-features",
        "cargo +1.90.0 check --locked --workspace --all-targets --all-features",
        "--test cloud_identity_integration",
        "cargo test --locked --test postgres_socket_integration",
        "workflow-lint:\n    name: GitHub Actions semantics",
        "Lint workflows and referenced local action metadata",
        "docker.io/rhysd/actionlint@sha256:b1934ee5f1c509618f2508e6eb47ee0d3520686341fec936f3b79331f9315667",
        "--network none",
        "--volume \"$GITHUB_WORKSPACE:/repo:ro\"",
        "--workdir /repo",
        "if: always()",
    ] {
        assert!(
            workflow.contains(expected),
            "shared CI is missing its reliability contract: {expected}"
        );
    }

    for version in ["14", "15", "16", "17", "18"] {
        assert!(
            workflow.contains(&format!("\"{version}\"")),
            "shared CI does not cover supported PostgreSQL {version}"
        );
    }

    let integration = job_block(&workflow, "integration");
    for expected in [
        "IGNATIUS_TEST_PG_URI: postgres://",
        "IGNATIUS_TEST_PG_TLS_URI: postgres://",
        "--test postgres_integration",
        "--test cli_contract",
        "--test password_prompt",
        "--test cloud_identity_integration",
        "--test parameter_semantics_contract",
        "--test runtime_privacy_contract",
        "cargo test --locked --lib postgres::session",
        "sslmode=verify-full&sslrootcert=${{ github.workspace }}/docker/tls/generated/ca.crt",
    ] {
        assert!(
            integration.contains(expected),
            "the TCP/TLS integration job is missing live-endpoint wiring: {expected}"
        );
    }

    let socket = job_block(&workflow, "socket-integration");
    for expected in [
        "IGNATIUS_TEST_PG_SOCKET_URI=\"host='$IGNATIUS_SOCKET_DIR'",
        "cargo test --locked --test postgres_socket_integration",
        "Start the socket fixture",
        "pg_isready -h 127.0.0.1",
    ] {
        assert!(
            socket.contains(expected),
            "the socket integration job is missing its live-endpoint wiring: {expected}"
        );
    }
}

#[test]
fn shared_ci_uses_reviewed_immutable_postgresql_fixtures() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workflow = workflow_text(&root.join(".github/workflows/ci.yml"));
    let selector =
        fs::read_to_string(root.join(".github/actions/select-postgres-fixture/action.yml"))
            .expect("read PostgreSQL fixture selector");
    let resolver = fs::read_to_string(root.join("docker/postgres-image.sh"))
        .expect("read PostgreSQL fixture resolver");
    let images = fs::read_to_string(root.join("docker/postgres-images.env"))
        .expect("read PostgreSQL fixture digest map");

    assert!(
        workflow.contains("tags: [\"v*\"]"),
        "tagged source must receive the same CI coverage as main"
    );
    for expected in [
        "postgres-fixture-drift:\n    name: Reviewed PostgreSQL fixture drift",
        "if: github.event_name == 'schedule'",
        "docker buildx imagetools inspect --format \"{{.Manifest.Digest}}\"",
        "review and update docker/postgres-images.env",
    ] {
        assert!(
            workflow.contains(expected),
            "shared CI is missing its PostgreSQL fixture drift contract: {expected}"
        );
    }
    assert_eq!(
        workflow
            .matches("uses: ./.github/actions/select-postgres-fixture")
            .count(),
        2,
        "both TCP/TLS and socket fixtures must use the immutable selector"
    );
    assert!(
        !workflow.contains("postgres:18-alpine"),
        "the socket job must not retain a mutable PostgreSQL tag"
    );
    for expected in [
        "case \"$INPUT_MAJOR\" in",
        "14|15|16|17|18)",
        "docker/postgres-image.sh\" \"$INPUT_MAJOR\"",
    ] {
        assert!(
            selector.contains(expected),
            "PostgreSQL fixture selector is missing its safety contract: {expected}"
        );
    }
    for expected in [
        "awk -F= -v key=\"$key\"",
        "expected one fixture image entry",
        "^docker\\.io/library/postgres@sha256:[0-9a-f]{64}$",
    ] {
        assert!(
            resolver.contains(expected),
            "PostgreSQL fixture resolver is missing its safety contract: {expected}"
        );
    }
    assert!(
        !resolver.contains("source "),
        "PostgreSQL fixture map must be parsed as data, not sourced as shell"
    );

    let mut majors = BTreeSet::new();
    for line in images
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
    {
        let (name, image) = line
            .split_once('=')
            .unwrap_or_else(|| panic!("invalid PostgreSQL fixture entry: {line}"));
        let major = name
            .strip_prefix("IGNATIUS_POSTGRES_IMAGE_")
            .unwrap_or_else(|| panic!("unexpected PostgreSQL fixture variable: {name}"));
        assert!(
            ["14", "15", "16", "17", "18"].contains(&major),
            "unsupported PostgreSQL fixture major: {major}"
        );
        assert!(
            image.starts_with("docker.io/library/postgres@sha256:")
                && image.len() == "docker.io/library/postgres@sha256:".len() + 64
                && image
                    .strip_prefix("docker.io/library/postgres@sha256:")
                    .expect("digest prefix")
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit()),
            "PostgreSQL {major} must use one full immutable image digest, found {image}"
        );
        assert!(
            majors.insert(major),
            "duplicate PostgreSQL fixture major: {major}"
        );
    }
    assert_eq!(
        majors,
        BTreeSet::from(["14", "15", "16", "17", "18"]),
        "fixture map must cover exactly the supported PostgreSQL majors"
    );
}

/// Execute the checked-in shell body, with registry reads replaced by a local
/// fake. The real reviewed-map resolver still runs against an isolated copy.
#[cfg(unix)]
fn fixture_drift_run(scenario: &str) -> (std::process::Output, String, Vec<String>) {
    use std::os::unix::fs::PermissionsExt;

    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workflow = workflow_text(&root.join(".github/workflows/ci.yml"));
    let job = job_block(&workflow, "postgres-fixture-drift");
    let lines: Vec<_> = job.lines().collect();
    let blocks: Vec<_> = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| **line == "        run: |")
        .map(|(index, _)| index)
        .collect();
    assert_eq!(
        blocks.len(),
        1,
        "the drift check has one literal shell body"
    );
    let script = lines[blocks[0] + 1..]
        .iter()
        .take_while(|line| line.trim().is_empty() || line.starts_with("          "))
        .map(|line| line.strip_prefix("          ").unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!script.is_empty(), "the real drift shell must execute");

    let isolated = tempfile::tempdir().expect("isolated fixture drift test");
    let docker_dir = isolated.path().join("docker");
    let bin = isolated.path().join("bin");
    fs::create_dir(&docker_dir).expect("isolated resolver directory");
    fs::create_dir(&bin).expect("fake registry client directory");
    let resolver = docker_dir.join("postgres-image.sh");
    fs::copy(root.join("docker/postgres-image.sh"), &resolver).expect("copy real resolver");
    fs::set_permissions(&resolver, fs::Permissions::from_mode(0o755)).expect("executable resolver");

    let map = fs::read_to_string(root.join("docker/postgres-images.env")).expect("reviewed map");
    let mut command = std::process::Command::new("bash");
    for line in map
        .lines()
        .filter(|line| line.starts_with("IGNATIUS_POSTGRES_IMAGE_"))
    {
        let (name, image) = line.split_once('=').expect("reviewed image assignment");
        let major = name
            .strip_prefix("IGNATIUS_POSTGRES_IMAGE_")
            .expect("major");
        let (_, digest) = image.rsplit_once('@').expect("immutable reviewed digest");
        command.env(format!("EXPECTED_{major}"), digest);
    }
    let map = if scenario == "resolver-failure" {
        map.lines()
            .filter(|line| !line.starts_with("IGNATIUS_POSTGRES_IMAGE_15="))
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        map
    };
    fs::write(docker_dir.join("postgres-images.env"), map).expect("isolated reviewed map");

    let fake = bin.join("docker");
    fs::write(&fake, r#"#!/bin/bash
set -euo pipefail
if [[ "$#" -ne 6 || "$1" != buildx || "$2" != imagetools || "$3" != inspect || "$4" != --format || "$5" != '{{.Manifest.Digest}}' ]]; then
  echo 'unexpected registry-client invocation' >&2
  exit 97
fi
printf '%s\n' "$6" >> "$MOCK_INSPECTIONS"
major="${6#postgres:}"
major="${major%-alpine}"
case "$MOCK_SCENARIO:$major" in
  drift:14|drift:18)
    printf 'sha256:%064d\n' 0
    ;;
  inspection-failure:15)
    echo 'synthetic registry inspection failure' >&2
    exit 1
    ;;
  malformed:16)
    printf 'UNTRUSTED | registry response\n| injected row |\n'
    ;;
  *)
    key="EXPECTED_$major"
    printf '%s\n' "${!key}"
    ;;
esac
"#).expect("fake registry client");
    fs::set_permissions(fake, fs::Permissions::from_mode(0o755)).expect("executable fake client");
    let summary_path = isolated.path().join("summary.md");
    let inspections_path = isolated.path().join("inspections.txt");
    let path = std::env::join_paths(std::iter::once(bin).chain(std::env::split_paths(
        &std::env::var_os("PATH").expect("system executable path"),
    )))
    .expect("fake-client-first executable path");
    let output = command
        .current_dir(isolated.path())
        .args(["-c", &script])
        .env("PATH", path)
        .env("GITHUB_STEP_SUMMARY", &summary_path)
        .env("MOCK_INSPECTIONS", &inspections_path)
        .env("MOCK_SCENARIO", scenario)
        .output()
        .expect("run real drift shell without Docker or network");
    let summary = fs::read_to_string(summary_path).expect("workflow summary");
    let inspections = fs::read_to_string(inspections_path)
        .expect("fake registry invocation log")
        .lines()
        .map(str::to_owned)
        .collect();
    (output, summary, inspections)
}

#[cfg(unix)]
fn assert_every_fixture_major_is_reported(summary: &str) {
    for major in 14..=18 {
        assert_eq!(
            summary
                .lines()
                .filter(|line| line.starts_with(&format!("| {major} |")))
                .count(),
            1,
            "PostgreSQL {major} must have exactly one summary row: {summary}"
        );
    }
}

#[cfg(unix)]
#[test]
fn fixture_drift_reports_every_matching_major_and_succeeds() {
    let (output, summary, inspections) = fixture_drift_run("matching");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_every_fixture_major_is_reported(&summary);
    assert_eq!(
        inspections,
        [
            "postgres:14-alpine",
            "postgres:15-alpine",
            "postgres:16-alpine",
            "postgres:17-alpine",
            "postgres:18-alpine"
        ]
    );
}

#[cfg(unix)]
#[test]
fn fixture_drift_reports_simultaneous_14_and_18_changes_before_failing() {
    let (output, summary, inspections) = fixture_drift_run("drift");
    assert!(!output.status.success());
    assert_every_fixture_major_is_reported(&summary);
    assert_eq!(inspections.len(), 5);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("PostgreSQL 14-alpine moved"));
    assert!(stderr.contains("PostgreSQL 18-alpine moved"));
}

#[cfg(unix)]
#[test]
fn fixture_drift_reports_an_inspection_failure_without_hiding_later_majors() {
    let (output, summary, inspections) = fixture_drift_run("inspection-failure");
    assert!(!output.status.success());
    assert_every_fixture_major_is_reported(&summary);
    assert_eq!(
        inspections,
        [
            "postgres:14-alpine",
            "postgres:15-alpine",
            "postgres:16-alpine",
            "postgres:17-alpine",
            "postgres:18-alpine"
        ]
    );
    assert!(summary.contains("inspection failed"));
}

#[cfg(unix)]
#[test]
fn fixture_drift_rejects_malformed_registry_output_without_injecting_summary_rows() {
    let (output, summary, inspections) = fixture_drift_run("malformed");
    assert!(!output.status.success());
    assert_every_fixture_major_is_reported(&summary);
    assert_eq!(inspections.len(), 5);
    assert!(summary.contains("invalid digest"));
    assert!(!summary.contains("UNTRUSTED"));
    assert!(!summary.contains("injected row"));
}

#[cfg(unix)]
#[test]
fn fixture_drift_reports_a_resolver_failure_and_continues_other_majors() {
    let (output, summary, inspections) = fixture_drift_run("resolver-failure");
    assert!(!output.status.success());
    assert_every_fixture_major_is_reported(&summary);
    assert_eq!(
        inspections,
        [
            "postgres:14-alpine",
            "postgres:16-alpine",
            "postgres:17-alpine",
            "postgres:18-alpine"
        ]
    );
    assert!(summary.contains("resolver failed"));
}

#[cfg(unix)]
#[test]
fn socket_readiness_waits_for_final_tcp_server_after_temporary_socket_start() {
    let previous = socket_readiness::run("eventually-ready", true);
    assert!(previous.output.status.success());
    assert_eq!(previous.events, ["start", "probe:/var/run/postgresql"]);
    assert!(previous.sleeps.is_empty());

    let current = socket_readiness::run("eventually-ready", false);
    assert!(
        current.output.status.success(),
        "{}",
        String::from_utf8_lossy(&current.output.stderr)
    );
    assert_eq!(
        current.events,
        [
            "start",
            "probe:127.0.0.1",
            "probe:127.0.0.1",
            "probe:127.0.0.1"
        ],
        "initialization's socket listener must not satisfy final-server readiness"
    );
    assert_eq!(current.sleeps, ["1", "1"]);
    let stdout = String::from_utf8_lossy(&current.output.stdout);
    assert!(stdout.contains("socket fixture ready on attempt 3"));
    assert!(!stdout.contains("synthetic socket fixture diagnostic"));
    assert_eq!(current.published_env.lines().count(), 2);
    assert!(current.published_env.contains("IGNATIUS_SOCKET_DIR="));
    assert!(
        current
            .published_env
            .contains("IGNATIUS_SOCKET_CONTAINER=ignatius-postgres-socket-123-1")
    );
}

#[cfg(unix)]
#[test]
fn socket_readiness_fails_after_all_60_tcp_attempts_with_diagnostics() {
    let previous = socket_readiness::run("never-ready", true);
    assert!(previous.output.status.success());
    assert_eq!(previous.events, ["start", "probe:/var/run/postgresql"]);

    let current = socket_readiness::run("never-ready", false);
    assert_eq!(
        current.output.status.code(),
        Some(1),
        "a temporary socket must not hide a final server that never starts"
    );
    assert_eq!(current.events.first().map(String::as_str), Some("start"));
    assert_eq!(current.events.last().map(String::as_str), Some("logs"));
    assert_eq!(current.events.len(), 62);
    assert!(
        current.events[1..61]
            .iter()
            .all(|event| event == "probe:127.0.0.1")
    );
    assert_eq!(current.sleeps.len(), 60);
    assert!(current.sleeps.iter().all(|delay| delay == "1"));
    let stdout = String::from_utf8_lossy(&current.output.stdout);
    assert!(stdout.contains("synthetic socket fixture diagnostic"));
    assert!(!stdout.contains("socket fixture ready on attempt"));
    assert!(
        String::from_utf8_lossy(&current.output.stderr)
            .contains("PostgreSQL socket fixture was not ready after 60 attempts")
    );
}

#[test]
fn security_tools_have_a_reproducible_bootstrap_and_no_network_at_scan_time() {
    let workflow =
        workflow_text(&Path::new(env!("CARGO_MANIFEST_DIR")).join(".github/workflows/ci.yml"));

    for forbidden in [
        "EmbarkStudios/cargo-deny-action@",
        "gitleaks/gitleaks-action@",
    ] {
        assert!(
            !workflow.contains(forbidden),
            "shared CI must not delegate scanner installation to an opaque action: {forbidden}"
        );
    }
    for expected in [
        "cargo install --locked --version 0.20.2 --root \"$install_root\" cargo-deny",
        "Cargo verifies the crates.io package checksum",
        "cargo-deny 0.20.2",
        "ghcr.io/gitleaks/gitleaks:v8.30.0@sha256:691af3c7c5a48b16f187ce3446d5f194838f91238f27270ed36eef6359a574d9",
        "git --no-banner --redact --log-opts=--all",
    ] {
        assert!(
            workflow.contains(expected),
            "shared CI is missing its reproducible security-tool contract: {expected}"
        );
    }
    assert!(
        workflow.matches("docker run --rm --network none").count() >= 2,
        "both repository and release-evidence secret scans must run without network access"
    );

    let history_scan = job_block(&workflow, "secret-scan");
    for expected in [
        "fetch-depth: 0",
        "--volume \"$GITHUB_WORKSPACE:/repo:ro\"",
        "--workdir /repo",
        "docker run --rm --network none",
        "git --no-banner --redact --log-opts=--all",
    ] {
        assert!(
            history_scan.contains(expected),
            "the repository history scan is missing its complete-history contract: {expected}"
        );
    }
}

#[test]
fn workflow_toolchains_follow_the_two_project_authorities() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let current =
        quoted_toml_assignment(&root.join("rust-toolchain.toml"), "[toolchain]", "channel");
    let minimum = exact_rust_release(&quoted_toml_assignment(
        &root.join("Cargo.toml"),
        "[package]",
        "rust-version",
    ));
    assert_ne!(
        current, minimum,
        "the current toolchain and MSRV should remain separate contracts"
    );

    let mut current_declarations = 0;
    let mut minimum_declarations = 0;
    for path in workflow_paths() {
        let workflow = workflow_text(&path);
        for (index, line) in workflow.lines().enumerate() {
            let Some(value) = line.trim().strip_prefix("toolchain: ") else {
                continue;
            };
            let value = value.trim_matches('"');
            if value == current {
                current_declarations += 1;
            } else if value == minimum {
                minimum_declarations += 1;
            } else {
                panic!(
                    "{}:{} installs Rust {value}, outside current {current} and MSRV {minimum}",
                    path.display(),
                    index + 1
                );
            }
        }
    }

    assert!(
        current_declarations > 0,
        "no workflow installs the rust-toolchain.toml channel {current}"
    );
    assert_eq!(
        minimum_declarations, 1,
        "CI must have one explicit installation of the Cargo.toml MSRV {minimum}"
    );

    let ci = workflow_text(&root.join(".github/workflows/ci.yml"));
    assert!(
        ci.contains(&format!(
            "cargo +{minimum} check --locked --workspace --all-targets --all-features"
        )),
        "the MSRV job must invoke Cargo with +{minimum} so rust-toolchain.toml cannot override it"
    );
}
