//! Contract checks for the cross-platform hosted release runtime rehearsal.
//!
//! The executable smoke test runs on the target runners.  These assertions
//! keep its privacy, extraction and rollback boundaries visible in the source
//! tree before a workflow dispatch is made.

#[test]
fn runtime_smoke_script_keeps_the_target_boundary_fail_closed() {
    let script = include_str!("../scripts/release-runtime-smoke.py");

    for expected in [
        "hashlib.sha256",
        "release archive checksum does not match SHA256SUMS",
        "release archive checksum does not match release-manifest.json",
        "release archive checksum does not match release-record.json",
        "tar archive does not contain exactly one expected root binary",
        "zip archive does not contain exactly one expected root binary",
        "hashlib.sha256(candidate_binary.read_bytes()).hexdigest()",
        "\"binary_sha256\": binary_sha256",
        "IGNATIUS_CONFIG_DIR",
        "IGNATIUS_DATA_DIR",
        "controlled invalid configuration",
        "malformed configuration doctor refusal",
        "verify_doctor(json.loads(doctor.stdout), product_version)",
        "verify_privacy(\"doctor --json\", doctor)",
        "history_file = data_dir / \"history.jsonl\"",
        "known-good rollback validation",
        "endpoint.bind((\"127.0.0.1\", 0))",
        "[\"--plain\", \"connect\", address]",
        "\"plain loopback refusal and privacy\", result, 4",
        "input_text=f\"SELECT",
        "verify_privacy(\"plain loopback refusal\", result, logs, data_dir / \"history.jsonl\")",
        "no database or publication action",
    ] {
        assert!(
            script.contains(expected),
            "missing runtime contract: {expected}"
        );
    }

    assert!(
        !script.contains("shell=True"),
        "runtime checks must not invoke a shell"
    );
    assert!(
        !script.contains(".listen("),
        "the owned loopback port must refuse connections"
    );
}

#[test]
fn runtime_smoke_script_retains_only_sanitised_outcomes() {
    let script = include_str!("../scripts/release-runtime-smoke.py");

    assert!(
        script.contains("\"checks\": checks"),
        "runtime evidence should retain check outcomes rather than command output"
    );
    assert!(
        script.contains("capture_output=True"),
        "runtime command output must remain in-memory for review"
    );
    assert!(
        script.contains("\"scope\": \"hosted runtime smoke and side-by-side rollback"),
        "runtime evidence must declare its bounded scope"
    );
}
