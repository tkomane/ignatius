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
        "IGNATIUS_CONFIG_DIR",
        "IGNATIUS_DATA_DIR",
        "controlled invalid configuration",
        "known-good rollback validation",
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
