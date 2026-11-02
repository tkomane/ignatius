//! Contract tests for the non-publishing release-readiness gate.
//!
//! These tests invoke the portable `xtask` boundary rather than importing
//! implementation details. They use synthetic records and evidence only.

use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::{Mutex, OnceLock};
use tempfile::{TempDir, tempdir};

static CARGO_GATE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn fixture(name: &str) -> Value {
    let path = format!("../release-notes/fixtures/{name}");
    let source = match name {
        "valid-candidate.json" => include_str!("../release-notes/fixtures/valid-candidate.json"),
        "blocked-missing-checksum.json" => {
            include_str!("../release-notes/fixtures/blocked-missing-checksum.json")
        }
        "mismatched-source.json" => {
            include_str!("../release-notes/fixtures/mismatched-source.json")
        }
        _ => panic!("unknown fixture {path}"),
    };
    serde_json::from_str(source).expect("fixture must be valid JSON")
}

fn run_catalogue(catalogue: Value, inventory_status: Option<&str>) -> (TempDir, Output) {
    run_catalogue_with_evidence(catalogue, inventory_status, |_| {})
}

fn run_validate(catalogue: Value) -> (TempDir, Output) {
    let text = serde_json::to_vec_pretty(&catalogue).expect("catalogue JSON");
    run_validate_text(&text)
}

fn run_validate_text(text: &[u8]) -> (TempDir, Output) {
    let temp = tempdir().expect("temporary directory");
    let catalogue_path = temp.path().join("catalog.json");
    fs::write(&catalogue_path, text).expect("catalogue");
    let output = run_xtask(vec![
        "release".to_owned(),
        "validate".to_owned(),
        catalogue_path
            .to_str()
            .expect("UTF-8 catalogue path")
            .to_owned(),
    ]);
    (temp, output)
}

fn catalogue_with_size_spelling(catalogue: &Value, spelling: &str) -> Vec<u8> {
    let expected = catalogue["records"][0]["artefacts"][0]["size_bytes"]
        .as_u64()
        .expect("fixture size");
    let source = serde_json::to_string(catalogue).expect("catalogue JSON");
    let needle = format!("\"size_bytes\":{expected}");
    let replacement = format!("\"size_bytes\":{spelling}");
    assert_eq!(
        source.matches(&needle).count(),
        1,
        "size field must be unique"
    );
    source.replace(&needle, &replacement).into_bytes()
}

fn run_evidence_scope(root: &Path, allowlist: &Path) -> Output {
    run_xtask(vec![
        "release".to_owned(),
        "evidence-scope".to_owned(),
        "--root".to_owned(),
        root.to_str().expect("UTF-8 scope root").to_owned(),
        "--allowlist".to_owned(),
        allowlist
            .to_str()
            .expect("UTF-8 scope allowlist")
            .to_owned(),
    ])
}

fn write_scope_allowlist(path: &Path, files: &[&str]) {
    fs::write(
        path,
        serde_json::to_vec_pretty(&json!({
            "schema_version": 1,
            "files": files,
        }))
        .expect("scope allowlist JSON"),
    )
    .expect("scope allowlist");
}

fn run_catalogue_with_evidence(
    mut catalogue: Value,
    inventory_status: Option<&str>,
    mutate_evidence: impl FnOnce(&mut Value),
) -> (TempDir, Output) {
    let temp = tempdir().expect("temporary directory");
    let record = catalogue
        .get_mut("records")
        .and_then(Value::as_array_mut)
        .and_then(|records| records.first_mut())
        .expect("catalogue has one record");
    let product_version = record["product_version"]
        .as_str()
        .expect("fixture product version")
        .to_owned();
    let source_revision = record["source_revision"]
        .as_str()
        .expect("fixture source revision")
        .to_owned();
    let artefacts = record["artefacts"].clone();

    if let Some(inventory_status) = inventory_status {
        let evidence_dir = temp.path().join("release-evidence");
        fs::create_dir_all(&evidence_dir).expect("evidence directory");
        record["evidence_reference"]["path"] = json!("release-evidence/index.json");
        record["evidence_reference"]["status"] = json!("complete");
        let mut bundle = json!({
            "schema_version": 1,
            "product_version": product_version,
            "source_revision": source_revision,
            "inventory_status": inventory_status,
            "artefacts": artefacts
        });
        mutate_evidence(&mut bundle);
        fs::write(
            evidence_dir.join("index.json"),
            serde_json::to_vec_pretty(&bundle).expect("evidence JSON"),
        )
        .expect("write evidence bundle");
    }

    let catalogue_path = temp.path().join("catalog.json");
    fs::write(
        &catalogue_path,
        serde_json::to_vec_pretty(&catalogue).expect("catalogue JSON"),
    )
    .expect("write catalogue");

    let output = run_xtask(vec![
        "release".to_owned(),
        "check".to_owned(),
        catalogue_path
            .to_str()
            .expect("UTF-8 temporary path")
            .to_owned(),
    ]);
    (temp, output)
}

fn run_xtask(args: Vec<String>) -> Output {
    let _cargo_gate_lock = CARGO_GATE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .expect("release gate subprocess lock");
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    Command::new(cargo)
        .arg("xtask")
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("CARGO_TERM_COLOR", "never")
        .output()
        .expect("run release task")
}

fn output_text(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn workflow_job_block(workflow: &str, name: &str) -> String {
    let lines: Vec<&str> = workflow.lines().collect();
    let marker = format!("  {name}:");
    let start = lines
        .iter()
        .position(|line| *line == marker)
        .unwrap_or_else(|| panic!("release workflow has no {name} job"));
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

fn make_ready(mut catalogue: Value) -> Value {
    let record = catalogue["records"][0]
        .as_object_mut()
        .expect("record object");
    record.insert("status".to_owned(), json!("ready"));
    record.insert(
        "authorization_reference".to_owned(),
        json!("approval:fixture-001"),
    );
    record.insert(
        "state_history".to_owned(),
        json!([
            {"from": "draft", "to": "candidate"},
            {"from": "candidate", "to": "ready"}
        ]),
    );
    record["target_rows"]
        .as_array_mut()
        .expect("target rows")
        .extend([
            json!({
                "target": "x86_64-pc-windows-msvc",
                "platform": "windows",
                "architecture": "x86_64",
                "coverage": "blocked",
                "blocked_reason": "fixture target is not built"
            }),
            json!({
                "target": "x86_64-unknown-linux-gnu",
                "platform": "linux",
                "architecture": "x86_64",
                "coverage": "blocked",
                "blocked_reason": "fixture target is not built"
            }),
        ]);
    let artefact = record["artefacts"][0]
        .as_object_mut()
        .expect("artefact object");
    artefact.insert("signature_status".to_owned(), json!("verified"));
    artefact.insert("provenance_status".to_owned(), json!("verified"));
    catalogue
}

#[test]
fn a_schema_valid_candidate_is_not_ready_by_omission() {
    let (_temp, output) = run_catalogue(fixture("valid-candidate.json"), Some("verified"));
    let text = output_text(&output);
    assert!(!output.status.success(), "unexpected success: {text}");
    assert!(
        text.contains("not in a publishable readiness state"),
        "{text}"
    );
}

#[test]
fn a_non_ready_candidate_names_its_missing_readiness_gates() {
    let (_temp, output) = run_catalogue(fixture("valid-candidate.json"), None);
    let text = output_text(&output);
    assert!(!output.status.success(), "unexpected success: {text}");
    assert!(text.contains("requires complete evidence"), "{text}");
    assert!(
        text.contains("without verified signature evidence"),
        "{text}"
    );
    assert!(
        text.contains("without verified provenance evidence"),
        "{text}"
    );
}

#[test]
fn an_evidence_mismatch_is_named_as_a_readiness_blocker() {
    let mut catalogue = fixture("valid-candidate.json");
    catalogue["records"][0]["evidence_reference"]["status"] = json!("mismatch");
    let (_temp, output) = run_catalogue(catalogue, None);
    let text = output_text(&output);
    assert!(!output.status.success(), "unexpected success: {text}");
    assert!(
        text.contains("evidence_reference.status is mismatch"),
        "{text}"
    );
}

#[test]
fn missing_checksum_blocks_readiness() {
    let (_temp, output) = run_catalogue(fixture("blocked-missing-checksum.json"), Some("verified"));
    let text = output_text(&output);
    assert!(!output.status.success(), "unexpected success: {text}");
    assert!(text.contains("no checksum"), "{text}");
}

#[test]
fn mismatched_source_revision_blocks_readiness() {
    let (_temp, output) = run_catalogue(fixture("mismatched-source.json"), Some("verified"));
    let text = output_text(&output);
    assert!(!output.status.success(), "unexpected success: {text}");
    assert!(text.contains("source_revision does not match"), "{text}");
}

#[test]
fn target_and_artefact_name_mismatch_blocks_readiness() {
    let mut catalogue = fixture("valid-candidate.json");
    catalogue["records"][0]["target_rows"][0]["artefact_name"] =
        json!("ignatius-other-target.tar.gz");
    let (_temp, output) = run_catalogue(catalogue, Some("verified"));
    let text = output_text(&output);
    assert!(!output.status.success(), "unexpected success: {text}");
    assert!(text.contains("target row and artefact name"), "{text}");
}

#[test]
fn duplicate_product_versions_block_readiness() {
    let mut catalogue = fixture("valid-candidate.json");
    let duplicate = catalogue["records"][0].clone();
    catalogue["records"]
        .as_array_mut()
        .expect("records array")
        .push(duplicate);
    let (_temp, output) = run_catalogue(catalogue, Some("verified"));
    let text = output_text(&output);
    assert!(!output.status.success(), "unexpected success: {text}");
    assert!(text.contains("duplicate product versions"), "{text}");
}

#[test]
fn dirty_source_state_is_named_as_a_readiness_blocker() {
    let mut catalogue = fixture("valid-candidate.json");
    catalogue["records"][0]["source_state"] = json!("modified");
    let (_temp, output) = run_catalogue(catalogue, Some("verified"));
    let text = output_text(&output);
    assert!(!output.status.success(), "unexpected success: {text}");
    assert!(text.contains("requires a clean source state"), "{text}");
}

#[test]
fn missing_target_is_named_as_a_readiness_blocker() {
    let mut catalogue = fixture("valid-candidate.json");
    catalogue["records"][0]["target_rows"][0]
        .as_object_mut()
        .expect("target row object")
        .remove("target");
    let (_temp, output) = run_catalogue(catalogue, Some("verified"));
    let text = output_text(&output);
    assert!(!output.status.success(), "unexpected success: {text}");
    assert!(text.contains("target_rows[0].target is required"), "{text}");
}

#[test]
fn declared_candidate_blocker_is_reported_by_readiness_check() {
    let catalogue = fixture("blocked-missing-checksum.json");
    let (_temp, output) = run_catalogue(catalogue, Some("verified"));
    let text = output_text(&output);
    assert!(!output.status.success(), "unexpected success: {text}");
    assert!(
        text.contains("blockers[0]: checksum is missing for"),
        "{text}"
    );
}

#[test]
fn evidence_path_cannot_escape_the_controlled_root() {
    let mut catalogue = fixture("valid-candidate.json");
    catalogue["records"][0]["evidence_reference"]["path"] = json!("../outside.json");
    let (_temp, output) = run_catalogue(catalogue, None);
    let text = output_text(&output);
    assert!(!output.status.success(), "unexpected success: {text}");
    assert!(text.contains("outside release-evidence"), "{text}");
}

#[cfg(unix)]
#[test]
fn evidence_path_rejects_a_symlinked_bundle() {
    use std::os::unix::fs::symlink;

    let temp = tempdir().expect("temporary directory");
    let evidence_dir = temp.path().join("release-evidence");
    let outside_dir = temp.path().join("outside");
    fs::create_dir_all(&evidence_dir).expect("evidence directory");
    fs::create_dir_all(&outside_dir).expect("outside directory");
    fs::write(outside_dir.join("index.json"), b"{}\n").expect("outside evidence");
    symlink(
        outside_dir.join("index.json"),
        evidence_dir.join("index.json"),
    )
    .expect("symlink evidence");

    let mut catalogue = fixture("valid-candidate.json");
    catalogue["records"][0]["evidence_reference"]["path"] = json!("release-evidence/index.json");
    catalogue["records"][0]["evidence_reference"]["status"] = json!("complete");
    let catalogue_path = temp.path().join("catalog.json");
    fs::write(
        &catalogue_path,
        serde_json::to_vec_pretty(&catalogue).expect("catalogue JSON"),
    )
    .expect("catalogue");

    let output = run_xtask(vec![
        "release".to_owned(),
        "check".to_owned(),
        catalogue_path
            .to_str()
            .expect("UTF-8 catalogue path")
            .to_owned(),
    ]);
    let text = output_text(&output);
    assert!(!output.status.success(), "unexpected success: {text}");
    assert!(text.contains("path must not use a symlink"), "{text}");
}

#[cfg(unix)]
#[test]
fn evidence_path_rejects_a_symlinked_evidence_root() {
    use std::os::unix::fs::symlink;

    let temp = tempdir().expect("temporary directory");
    let real_root = temp.path().join("real-root");
    let linked_root = temp.path().join("linked-root");
    let evidence_dir = real_root.join("release-evidence");
    fs::create_dir_all(&evidence_dir).expect("evidence directory");
    fs::write(evidence_dir.join("index.json"), b"{}\n").expect("evidence bundle");
    symlink(&real_root, &linked_root).expect("symlink root");

    let mut catalogue = fixture("valid-candidate.json");
    catalogue["records"][0]["evidence_reference"]["path"] = json!("release-evidence/index.json");
    catalogue["records"][0]["evidence_reference"]["status"] = json!("complete");
    let catalogue_path = linked_root.join("catalog.json");
    fs::write(
        &catalogue_path,
        serde_json::to_vec_pretty(&catalogue).expect("catalogue JSON"),
    )
    .expect("catalogue");

    let output = run_xtask(vec![
        "release".to_owned(),
        "check".to_owned(),
        catalogue_path.to_str().expect("catalogue path").to_owned(),
    ]);
    let text = output_text(&output);
    assert!(!output.status.success(), "unexpected success: {text}");
    assert!(text.contains("path must not use a symlink"), "{text}");
}

#[test]
fn validate_rejects_a_non_array_blocker_field() {
    let mut catalogue = fixture("valid-candidate.json");
    catalogue["records"][0]["blockers"] = json!("not-an-array");
    let (_temp, output) = run_validate(catalogue);
    let text = output_text(&output);
    assert!(!output.status.success(), "unexpected success: {text}");
    assert!(text.contains("blockers must be an array"), "{text}");
}

#[test]
fn validate_rejects_duplicate_blockers() {
    let mut catalogue = fixture("valid-candidate.json");
    catalogue["records"][0]["blockers"] = json!(["same", "same"]);
    let (_temp, output) = run_validate(catalogue);
    let text = output_text(&output);
    assert!(!output.status.success(), "unexpected success: {text}");
    assert!(text.contains("duplicates an earlier blocker"), "{text}");
}

#[test]
fn validate_rejects_empty_and_non_text_blockers() {
    let mut catalogue = fixture("valid-candidate.json");
    catalogue["records"][0]["blockers"] = json!(["", 7]);
    let (_temp, output) = run_validate(catalogue);
    let text = output_text(&output);
    assert!(!output.status.success(), "unexpected success: {text}");
    assert!(
        text.contains("blockers[0] must be non-empty text"),
        "{text}"
    );
    assert!(
        text.contains("blockers[1] must be non-empty text"),
        "{text}"
    );
}

#[test]
fn validate_rejects_an_unsafe_build_identity() {
    let mut catalogue = fixture("valid-candidate.json");
    catalogue["records"][0]["build_identity"] = json!("/outside");
    catalogue["records"][0]["artefacts"][0]["build_identity"] = json!("/outside");
    let (_temp, output) = run_validate(catalogue);
    let text = output_text(&output);
    assert!(!output.status.success(), "unexpected success: {text}");
    assert!(
        text.contains("build_identity is not a safe release identity"),
        "{text}"
    );
}

#[test]
fn validate_rejects_a_target_outside_the_release_matrix() {
    let mut catalogue = fixture("valid-candidate.json");
    catalogue["records"][0]["target_rows"][0]["target"] = json!("x86_64-unknown-linux-musl");
    catalogue["records"][0]["target_rows"][0]["artefact_name"] =
        json!("ignatius-0.1.0-preview.1-x86_64-unknown-linux-musl.tar.gz");
    catalogue["records"][0]["artefacts"][0]["target"] = json!("x86_64-unknown-linux-musl");
    catalogue["records"][0]["artefacts"][0]["name"] =
        json!("ignatius-0.1.0-preview.1-x86_64-unknown-linux-musl.tar.gz");
    let (_temp, output) = run_validate(catalogue);
    let text = output_text(&output);
    assert!(!output.status.success(), "unexpected success: {text}");
    assert!(
        text.contains("target is not a supported platform target"),
        "{text}"
    );
}

#[test]
fn validate_rejects_parent_like_target_and_archive_tokens() {
    let mut catalogue = fixture("valid-candidate.json");
    let target = "aarch64-apple..darwin";
    let name = "ignatius-0.1.0-preview.1-aarch64-apple..darwin.tar.gz";
    catalogue["records"][0]["target_rows"][0]["target"] = json!(target);
    catalogue["records"][0]["target_rows"][0]["artefact_name"] = json!(name);
    catalogue["records"][0]["artefacts"][0]["target"] = json!(target);
    catalogue["records"][0]["artefacts"][0]["name"] = json!(name);
    let (_temp, output) = run_validate(catalogue);
    let text = output_text(&output);
    assert!(!output.status.success(), "unexpected success: {text}");
    assert!(text.contains("not a safe target token"), "{text}");
    assert!(text.contains("not a safe archive basename"), "{text}");
}

#[test]
fn validate_rejects_nonportable_archive_basenames() {
    let mut catalogue = fixture("valid-candidate.json");
    let name = "_ignatius-0.1.0-preview.1-aarch64-apple-darwin.tar.gz";
    catalogue["records"][0]["target_rows"][0]["artefact_name"] = json!(name);
    catalogue["records"][0]["artefacts"][0]["name"] = json!(name);
    let (_temp, output) = run_validate(catalogue);
    let text = output_text(&output);
    assert!(!output.status.success(), "unexpected success: {text}");
    assert!(text.contains("not a safe archive basename"), "{text}");
}

#[test]
fn validate_rejects_unsafe_noncovered_target_row_archive_names() {
    let mut catalogue = fixture("valid-candidate.json");
    catalogue["records"][0]["target_rows"]
        .as_array_mut()
        .expect("target rows")
        .push(json!({
            "target": "x86_64-pc-windows-msvc",
            "platform": "windows",
            "architecture": "x86_64",
            "coverage": "blocked",
            "artefact_name": "release-manifest.json",
            "blocked_reason": "fixture target is not built"
        }));
    let (_temp, output) = run_validate(catalogue);
    let text = output_text(&output);
    assert!(!output.status.success(), "unexpected success: {text}");
    assert!(
        text.contains("target_rows[1].artefact_name is not a safe archive basename"),
        "{text}"
    );
}

#[test]
fn validate_rejects_an_artefact_for_a_noncovered_target() {
    let mut catalogue = fixture("valid-candidate.json");
    let record = catalogue["records"][0]
        .as_object_mut()
        .expect("record object");
    let target = "x86_64-pc-windows-msvc";
    let name = "ignatius-0.1.0-preview.1-x86_64-pc-windows-msvc.zip";
    record["target_rows"]
        .as_array_mut()
        .expect("target rows")
        .push(json!({
            "target": target,
            "platform": "windows",
            "architecture": "x86_64",
            "coverage": "blocked",
            "artefact_name": name,
            "blocked_reason": "fixture target is not built"
        }));
    let mut artefact = record["artefacts"][0].clone();
    artefact["name"] = json!(name);
    artefact["target"] = json!(target);
    artefact["format"] = json!("zip");
    artefact["checksum"] =
        json!("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");
    record["artefacts"]
        .as_array_mut()
        .expect("artefacts")
        .push(artefact);

    let (_temp, output) = run_validate(catalogue);
    let text = output_text(&output);
    assert!(!output.status.success(), "unexpected success: {text}");
    assert!(
        text.contains("non-covered target must not have an artefact"),
        "{text}"
    );
}

#[test]
fn validate_rejects_numeric_prerelease_identifiers_with_leading_zeroes() {
    let mut catalogue = fixture("valid-candidate.json");
    let record = catalogue["records"][0]
        .as_object_mut()
        .expect("record object");
    let version = "1.2.3-01";
    let artefact_name = "ignatius-1.2.3-01-aarch64-apple-darwin.tar.gz";
    record["product_version"] = json!(version);
    record["notes_reference"]["entry"] = json!("[1.2.3-01]");
    record["evidence_reference"]["product_version"] = json!(version);
    record["target_rows"][0]["artefact_name"] = json!(artefact_name);
    record["artefacts"][0]["name"] = json!(artefact_name);

    let (_temp, output) = run_validate(catalogue);
    let text = output_text(&output);
    assert!(!output.status.success(), "unexpected success: {text}");
    assert!(
        text.contains("product_version is not semantic version text"),
        "{text}"
    );
}

#[test]
fn validate_accepts_schema_integer_spellings_and_semver_build_metadata() {
    let catalogue = fixture("valid-candidate.json");
    for spelling in ["23.0", "23e0"] {
        let (_temp, output) =
            run_validate_text(&catalogue_with_size_spelling(&catalogue, spelling));
        let text = output_text(&output);
        assert!(
            output.status.success(),
            "unexpected validation failure: {text}"
        );
    }

    let mut build_metadata_catalogue = catalogue;
    let record = build_metadata_catalogue["records"][0]
        .as_object_mut()
        .expect("record object");
    let version = "1.2.3+build.7";
    let artefact_name = "ignatius-1.2.3+build.7-aarch64-apple-darwin.tar.gz";
    record["product_version"] = json!(version);
    record["notes_reference"]["entry"] = json!(format!("[{version}]"));
    record["evidence_reference"]["path"] = json!(format!("release-evidence/{version}/index.json"));
    record["evidence_reference"]["product_version"] = json!(version);
    record["target_rows"][0]["artefact_name"] = json!(artefact_name);
    record["artefacts"][0]["name"] = json!(artefact_name);

    let (_temp, output) = run_validate(build_metadata_catalogue);
    let text = output_text(&output);
    assert!(
        output.status.success(),
        "unexpected build-metadata failure: {text}"
    );
}

#[test]
fn validate_rejects_non_canonical_evidence_paths() {
    for invalid_path in [
        "release-evidence/0.1.0..preview.1/index.json",
        "release-evidence/../outside.json",
        "release-evidence//index.json",
        "release-evidence/./index.json",
        "release-evidence/index.json/",
    ] {
        let mut catalogue = fixture("valid-candidate.json");
        catalogue["records"][0]["evidence_reference"]["path"] = json!(invalid_path);
        let (_temp, output) = run_validate(catalogue);
        let text = output_text(&output);
        assert!(
            !output.status.success(),
            "unexpected success for {invalid_path}: {text}"
        );
        assert!(
            text.contains("outside release-evidence"),
            "{invalid_path}: {text}"
        );
    }
}

#[test]
fn validate_rejects_parent_like_publication_references() {
    let mut catalogue = make_ready(fixture("valid-candidate.json"));
    let record = catalogue["records"][0]
        .as_object_mut()
        .expect("record object");
    record.insert("status".to_owned(), json!("published"));
    record.insert(
        "state_history".to_owned(),
        json!([
            {"from": "draft", "to": "candidate"},
            {"from": "candidate", "to": "ready"},
            {"from": "ready", "to": "published"}
        ]),
    );
    let product_version = record["product_version"].clone();
    let source_revision = record["source_revision"].clone();
    record.insert(
        "publication_evidence".to_owned(),
        json!({
            "reference": "publication:fixture..001",
            "product_version": product_version,
            "source_revision": source_revision,
            "status": "verified",
            "observed_at": "2026-08-16T02:00:00Z"
        }),
    );
    let (_temp, output) = run_catalogue(catalogue, Some("verified"));
    let text = output_text(&output);
    assert!(!output.status.success(), "unexpected success: {text}");
    assert!(
        text.contains("publication_evidence.reference is not controlled"),
        "{text}"
    );
}

#[test]
fn validate_rejects_reserved_sidecar_names_as_artefacts() {
    for reserved_name in ["release-manifest.json", "SHA256SUMS"] {
        let mut catalogue = fixture("valid-candidate.json");
        catalogue["records"][0]["target_rows"][0]["artefact_name"] = json!(reserved_name);
        catalogue["records"][0]["artefacts"][0]["name"] = json!(reserved_name);
        let (_temp, output) = run_validate(catalogue);
        let text = output_text(&output);
        assert!(
            !output.status.success(),
            "unexpected success for {reserved_name}: {text}"
        );
        assert!(
            text.contains("not a safe archive basename"),
            "reserved name {reserved_name} was not rejected: {text}"
        );
    }
}

#[test]
fn validate_does_not_mask_a_ready_record_error_with_a_candidate_record() {
    let mut catalogue = fixture("valid-candidate.json");
    let mut ready = catalogue["records"][0].clone();
    ready["status"] = json!("ready");
    ready["state_history"] = json!([
        {"from": "draft", "to": "candidate"},
        {"from": "candidate", "to": "ready"}
    ]);
    ready["artefacts"][0]["checksum_status"] = json!("missing");
    ready["artefacts"][0]["checksum"] = Value::Null;
    catalogue["records"] = json!([catalogue["records"][0].clone(), ready]);
    let (_temp, output) = run_validate(catalogue);
    let text = output_text(&output);
    assert!(!output.status.success(), "unexpected success: {text}");
    assert!(
        text.contains("records[1].artefacts[0] has no checksum"),
        "{text}"
    );
}

#[test]
fn validate_rejects_nullable_target_row_fields() {
    let mut catalogue = fixture("valid-candidate.json");
    catalogue["records"][0]["target_rows"][0]["artefact_name"] = Value::Null;
    catalogue["records"][0]["target_rows"][0]["blocked_reason"] = Value::Null;
    let (_temp, output) = run_validate(catalogue);
    let text = output_text(&output);
    assert!(!output.status.success(), "unexpected success: {text}");
    assert!(
        text.contains("artefact_name must be non-empty text when present"),
        "{text}"
    );
    assert!(
        text.contains("blocked_reason must be non-empty text when present"),
        "{text}"
    );
}

#[test]
fn validate_rejects_duplicate_json_members_at_each_object_depth() {
    for text in [
        r#"{"schema_version":1,"schema_version":1,"records":[]}"#,
        r#"{"schema_version":1,"records":[{"schema_version":1,"schema_version":1}]}"#,
    ] {
        let (_temp, output) = run_validate_text(text.as_bytes());
        let output_text = output_text(&output);
        assert!(
            !output.status.success(),
            "unexpected success: {output_text}"
        );
        assert!(
            output_text.contains("duplicate JSON object key"),
            "{output_text}"
        );
    }
}

#[test]
fn validate_rejects_invalid_publication_reference_and_timestamp() {
    let mut catalogue = make_ready(fixture("valid-candidate.json"));
    let record = catalogue["records"][0]
        .as_object_mut()
        .expect("record object");
    record["status"] = json!("published");
    record["state_history"] = json!([
        {"from": "draft", "to": "candidate"},
        {"from": "candidate", "to": "ready"},
        {"from": "ready", "to": "published"}
    ]);
    let product_version = record["product_version"].clone();
    let source_revision = record["source_revision"].clone();
    record.insert(
        "publication_evidence".to_owned(),
        json!({
            "reference": "publication:bad?value",
            "product_version": product_version,
            "source_revision": source_revision,
            "status": "verified",
            "observed_at": "not-a-date"
        }),
    );
    let (_temp, output) = run_validate(catalogue);
    let text = output_text(&output);
    assert!(!output.status.success(), "unexpected success: {text}");
    assert!(
        text.contains("publication_evidence.reference is not controlled"),
        "{text}"
    );
    assert!(
        text.contains("observed_at must be an RFC 3339 date-time"),
        "{text}"
    );
}

#[test]
fn check_rejects_a_malformed_blocker_field_on_a_ready_record() {
    let mut catalogue = make_ready(fixture("valid-candidate.json"));
    catalogue["records"][0]["blockers"] = json!("not-an-array");
    let (_temp, output) = run_catalogue(catalogue, Some("verified"));
    let text = output_text(&output);
    assert!(!output.status.success(), "unexpected success: {text}");
    assert!(text.contains("blockers must be an array"), "{text}");
}

#[test]
fn evidence_scope_accepts_the_exact_allowlisted_file_set() {
    let temp = tempdir().expect("temporary directory");
    let temp_path = fs::canonicalize(temp.path()).expect("canonical temporary directory");
    let root = temp_path.join("staging");
    let allowlist = temp_path.join("scope.json");
    fs::create_dir_all(root.join("record")).expect("record directory");
    fs::create_dir_all(root.join("evidence")).expect("evidence directory");
    fs::create_dir_all(root.join("artefacts")).expect("artefacts directory");
    fs::write(root.join("record/release-record.json"), b"{}\n").expect("record");
    fs::write(root.join("evidence/SHA256SUMS"), b"digest  archive\n").expect("checksums");
    fs::write(
        root.join("artefacts/ignatius-1.2.3+build.7-aarch64-apple-darwin.tar.gz"),
        b"archive\n",
    )
    .expect("archive");
    write_scope_allowlist(
        &allowlist,
        &[
            "artefacts/ignatius-1.2.3+build.7-aarch64-apple-darwin.tar.gz",
            "evidence/SHA256SUMS",
            "record/release-record.json",
        ],
    );

    let output = run_evidence_scope(&root, &allowlist);
    let text = output_text(&output);
    assert!(output.status.success(), "unexpected failure: {text}");
    assert!(
        text.contains("Evidence scope: verified 3 file(s)"),
        "{text}"
    );
}

#[test]
fn evidence_scope_rejects_unexpected_files() {
    let temp = tempdir().expect("temporary directory");
    let temp_path = fs::canonicalize(temp.path()).expect("canonical temporary directory");
    let root = temp_path.join("staging");
    let allowlist = temp_path.join("scope.json");
    fs::create_dir_all(&root).expect("staging directory");
    fs::write(root.join("release-record.json"), b"{}\n").expect("record");
    fs::write(root.join("user-data.txt"), b"must not be uploaded\n").expect("unexpected");
    write_scope_allowlist(&allowlist, &["release-record.json"]);

    let output = run_evidence_scope(&root, &allowlist);
    let text = output_text(&output);
    assert!(!output.status.success(), "unexpected success: {text}");
    assert!(
        text.contains("file is not allowlisted: user-data.txt"),
        "{text}"
    );
}

#[test]
fn evidence_scope_rejects_missing_allowlisted_files() {
    let temp = tempdir().expect("temporary directory");
    let temp_path = fs::canonicalize(temp.path()).expect("canonical temporary directory");
    let root = temp_path.join("staging");
    let allowlist = temp_path.join("scope.json");
    fs::create_dir_all(&root).expect("staging directory");
    write_scope_allowlist(&allowlist, &["release-record.json"]);

    let output = run_evidence_scope(&root, &allowlist);
    let text = output_text(&output);
    assert!(!output.status.success(), "unexpected success: {text}");
    assert!(
        text.contains("allowlisted file is missing: release-record.json"),
        "{text}"
    );
}

#[test]
fn evidence_scope_rejects_duplicate_or_traversal_allowlist_paths() {
    let temp = tempdir().expect("temporary directory");
    let temp_path = fs::canonicalize(temp.path()).expect("canonical temporary directory");
    let root = temp_path.join("staging");
    let allowlist = temp_path.join("scope.json");
    fs::create_dir_all(&root).expect("staging directory");
    fs::write(
        &allowlist,
        serde_json::to_vec_pretty(&json!({
            "schema_version": 1,
            "files": ["record.json", "record.json"]
        }))
        .expect("scope allowlist JSON"),
    )
    .expect("scope allowlist");
    let duplicate = run_evidence_scope(&root, &allowlist);
    let duplicate_text = output_text(&duplicate);
    assert!(
        !duplicate.status.success(),
        "unexpected success: {duplicate_text}"
    );
    assert!(
        duplicate_text.contains("contains a duplicate path"),
        "{duplicate_text}"
    );

    fs::write(
        &allowlist,
        serde_json::to_vec_pretty(&json!({
            "schema_version": 1,
            "files": ["../outside.json"]
        }))
        .expect("scope allowlist JSON"),
    )
    .expect("scope allowlist");
    let traversal = run_evidence_scope(&root, &allowlist);
    let traversal_text = output_text(&traversal);
    assert!(
        !traversal.status.success(),
        "unexpected success: {traversal_text}"
    );
    assert!(
        traversal_text.contains("must not contain empty, dot or parent components"),
        "{traversal_text}"
    );

    for invalid_path in ["record/../outside.json", "record//outside.json", "record/"] {
        fs::write(
            &allowlist,
            serde_json::to_vec_pretty(&json!({
                "schema_version": 1,
                "files": [invalid_path]
            }))
            .expect("scope allowlist JSON"),
        )
        .expect("scope allowlist");
        let invalid = run_evidence_scope(&root, &allowlist);
        let invalid_text = output_text(&invalid);
        assert!(
            !invalid.status.success(),
            "unexpected success for {invalid_path}: {invalid_text}"
        );
        assert!(
            invalid_text.contains("must not contain empty, dot or parent components"),
            "{invalid_path}: {invalid_text}"
        );
    }
}

#[cfg(unix)]
#[test]
fn evidence_scope_rejects_symlinked_entries() {
    use std::os::unix::fs::symlink;

    let temp = tempdir().expect("temporary directory");
    let temp_path = fs::canonicalize(temp.path()).expect("canonical temporary directory");
    let root = temp_path.join("staging");
    let outside = temp_path.join("outside.txt");
    let allowlist = temp_path.join("scope.json");
    fs::create_dir_all(&root).expect("staging directory");
    fs::write(&outside, b"outside\n").expect("outside file");
    symlink(&outside, root.join("record.json")).expect("symlink entry");
    write_scope_allowlist(&allowlist, &["record.json"]);

    let output = run_evidence_scope(&root, &allowlist);
    let text = output_text(&output);
    assert!(!output.status.success(), "unexpected success: {text}");
    assert!(text.contains("contains a symlink: record.json"), "{text}");
}

#[cfg(unix)]
#[test]
fn evidence_scope_rejects_a_symlinked_allowlist_parent() {
    use std::os::unix::fs::symlink;

    let temp = tempdir().expect("temporary directory");
    let temp_path = fs::canonicalize(temp.path()).expect("canonical temporary directory");
    let root = temp_path.join("staging");
    let real_scope_dir = temp_path.join("real-scope");
    let linked_scope_dir = temp_path.join("linked-scope");
    fs::create_dir_all(&root).expect("staging directory");
    fs::write(root.join("record.json"), b"{}").expect("record");
    fs::create_dir(&real_scope_dir).expect("real scope directory");
    symlink(&real_scope_dir, &linked_scope_dir).expect("scope directory symlink");
    let allowlist = linked_scope_dir.join("scope.json");
    write_scope_allowlist(&allowlist, &["record.json"]);

    let output = run_evidence_scope(&root, &allowlist);
    let text = output_text(&output);
    assert!(!output.status.success(), "unexpected success: {text}");
    assert!(
        text.contains("evidence scope allowlist must not use a symlink"),
        "{text}"
    );
}

#[test]
fn missing_inventory_blocks_an_otherwise_ready_record() {
    let catalogue = make_ready(fixture("valid-candidate.json"));
    let (_temp, output) = run_catalogue(catalogue, Some("missing"));
    let text = output_text(&output);
    assert!(!output.status.success(), "unexpected success: {text}");
    assert!(text.contains("inventory_status is not verified"), "{text}");
}

#[test]
fn a_ready_record_requires_verified_artifact_evidence() {
    let catalogue = make_ready(fixture("valid-candidate.json"));
    let (_temp, output) = run_catalogue(catalogue, Some("verified"));
    let text = output_text(&output);
    assert!(output.status.success(), "unexpected failure: {text}");
    assert!(text.contains("Release readiness: ready"), "{text}");
}

#[test]
fn a_ready_record_requires_every_supported_target_row() {
    let mut catalogue = make_ready(fixture("valid-candidate.json"));
    catalogue["records"][0]["target_rows"]
        .as_array_mut()
        .expect("target rows")
        .retain(|row| row["target"] != json!("x86_64-unknown-linux-gnu"));
    let (_temp, output) = run_catalogue(catalogue, Some("verified"));
    let text = output_text(&output);
    assert!(!output.status.success(), "unexpected success: {text}");
    assert!(
        text.contains("target_rows is missing supported target: x86_64-unknown-linux-gnu"),
        "{text}"
    );
}

#[test]
fn checksum_mismatch_blocks_readiness_even_with_two_digests() {
    let mut catalogue = make_ready(fixture("valid-candidate.json"));
    catalogue["records"][0]["artefacts"][0]["checksum_status"] = json!("mismatch");
    catalogue["records"][0]["artefacts"][0]["observed_checksum"] =
        json!("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");
    let (_temp, output) = run_catalogue(catalogue, Some("verified"));
    let text = output_text(&output);
    assert!(!output.status.success(), "unexpected success: {text}");
    assert!(text.contains("checksum mismatch"), "{text}");
}

#[test]
fn validate_rejects_empty_nullable_checksum_strings() {
    for field in ["checksum", "observed_checksum"] {
        let mut catalogue = fixture("valid-candidate.json");
        catalogue["records"][0]["artefacts"][0][field] = json!("");
        let (_temp, output) = run_validate(catalogue);
        let text = output_text(&output);
        assert!(
            !output.status.success(),
            "unexpected success for {field}: {text}"
        );
        assert!(
            text.contains(&format!("{field} must be non-empty text or null")),
            "empty {field} was accepted: {text}"
        );
    }
}

#[test]
fn published_requires_observed_publication_evidence() {
    let mut catalogue = make_ready(fixture("valid-candidate.json"));
    catalogue["records"][0]["status"] = json!("published");
    catalogue["records"][0]["state_history"] = json!([
        {"from": "draft", "to": "candidate"},
        {"from": "candidate", "to": "ready"},
        {"from": "ready", "to": "published"}
    ]);
    let (_temp, output) = run_catalogue(catalogue, Some("verified"));
    let text = output_text(&output);
    assert!(!output.status.success(), "unexpected success: {text}");
    assert!(text.contains("publication evidence"), "{text}");
}

#[test]
fn published_with_only_observed_publication_evidence_is_not_ready() {
    let mut catalogue = make_ready(fixture("valid-candidate.json"));
    let record = catalogue["records"][0]
        .as_object_mut()
        .expect("record object");
    record.insert("status".to_owned(), json!("published"));
    record.insert(
        "state_history".to_owned(),
        json!([
            {"from": "draft", "to": "candidate"},
            {"from": "candidate", "to": "ready"},
            {"from": "ready", "to": "published"}
        ]),
    );
    let product_version = record["product_version"].clone();
    let source_revision = record["source_revision"].clone();
    record.insert(
        "publication_evidence".to_owned(),
        json!({
            "reference": "publication:fixture-observed",
            "product_version": product_version,
            "source_revision": source_revision,
            "status": "observed",
            "observed_at": "2026-08-16T02:00:00Z"
        }),
    );
    let (_temp, output) = run_catalogue(catalogue, Some("verified"));
    let text = output_text(&output);
    assert!(!output.status.success(), "unexpected success: {text}");
    assert!(
        text.contains("requires verified publication evidence"),
        "{text}"
    );
}

#[test]
fn published_with_matching_observed_evidence_is_accepted() {
    let mut catalogue = make_ready(fixture("valid-candidate.json"));
    let record = catalogue["records"][0]
        .as_object_mut()
        .expect("record object");
    record.insert("status".to_owned(), json!("published"));
    record.insert(
        "state_history".to_owned(),
        json!([
            {"from": "draft", "to": "candidate"},
            {"from": "candidate", "to": "ready"},
            {"from": "ready", "to": "published"}
        ]),
    );
    let product_version = record["product_version"].clone();
    let source_revision = record["source_revision"].clone();
    record.insert(
        "publication_evidence".to_owned(),
        json!({
            "reference": "publication:fixture-001",
            "product_version": product_version,
            "source_revision": source_revision,
            "status": "verified",
            "observed_at": "2026-08-16T02:00:00Z"
        }),
    );
    let (_temp, output) = run_catalogue(catalogue, Some("verified"));
    let text = output_text(&output);
    assert!(output.status.success(), "unexpected failure: {text}");
    assert!(text.contains("Release readiness: ready"), "{text}");
}

#[test]
fn generation_captures_identity_and_keeps_a_dirty_candidate_non_ready() {
    let temp = tempdir().expect("temporary directory");
    let output_path = temp.path().join("catalog.json");
    let output_text_path = output_path.to_str().expect("UTF-8 output path");
    let args = vec![
        "release".to_owned(),
        "generate".to_owned(),
        "--output".to_owned(),
        output_text_path.to_owned(),
        "--target".to_owned(),
        "aarch64-apple-darwin".to_owned(),
        "--artefact-name".to_owned(),
        format!(
            "ignatius-{}-aarch64-apple-darwin.tar.gz",
            env!("CARGO_PKG_VERSION")
        ),
        "--artefact-format".to_owned(),
        "tar.gz".to_owned(),
        "--artefact-size".to_owned(),
        "1234".to_owned(),
        "--notes-entry".to_owned(),
        format!("[{}]", env!("CARGO_PKG_VERSION")),
        "--evidence-path".to_owned(),
        "release-evidence/0.1.0/index.json".to_owned(),
        "--build-identity".to_owned(),
        "contract-test".to_owned(),
    ];

    let generated = run_xtask(args.clone());
    let generated_text = output_text(&generated);
    assert!(
        generated.status.success(),
        "unexpected failure: {generated_text}"
    );
    let catalogue: Value =
        serde_json::from_slice(&fs::read(&output_path).expect("generated catalogue"))
            .expect("generated JSON");
    let record = &catalogue["records"][0];
    assert_eq!(record["schema_version"], json!(1));
    assert_eq!(record["product_version"], json!(env!("CARGO_PKG_VERSION")));
    assert!(
        record["source_revision"]
            .as_str()
            .is_some_and(is_revision_text)
    );
    assert_eq!(record["build_identity"], json!("contract-test"));
    assert_eq!(
        record["target_rows"][0]["target"],
        json!("aarch64-apple-darwin")
    );
    assert_eq!(record["status"], json!("blocked"));
    assert_eq!(record["state_history"][1]["to"], json!("blocked"));
    assert!(
        record["blockers"]
            .as_array()
            .expect("generation blockers")
            .iter()
            .any(|blocker| blocker == "source revision is not tagged for this product version")
    );

    let validated = run_xtask(vec![
        "release".to_owned(),
        "validate".to_owned(),
        output_text_path.to_owned(),
    ]);
    let validated_text = output_text(&validated);
    assert!(
        validated.status.success(),
        "unexpected validation failure: {validated_text}"
    );
    assert!(
        validated_text.contains("Release record: valid (blocked)"),
        "{validated_text}"
    );

    let readiness = run_xtask(vec![
        "release".to_owned(),
        "check".to_owned(),
        output_text_path.to_owned(),
    ]);
    let readiness_text = output_text(&readiness);
    assert!(
        !readiness.status.success(),
        "unexpected readiness: {readiness_text}"
    );
    assert!(
        readiness_text.contains("not in a publishable readiness state"),
        "{readiness_text}"
    );

    let overwritten = run_xtask(args);
    let overwritten_text = output_text(&overwritten);
    assert!(
        !overwritten.status.success(),
        "unexpected overwrite: {overwritten_text}"
    );
    assert!(
        overwritten_text.contains("without overwriting"),
        "{overwritten_text}"
    );
}

#[cfg(unix)]
#[test]
fn generation_rejects_a_symlinked_output_parent() {
    use std::os::unix::fs::symlink;

    let temp = tempdir().expect("temporary directory");
    let real_parent = temp.path().join("real-output-parent");
    let linked_parent = temp.path().join("linked-output-parent");
    fs::create_dir(&real_parent).expect("real output parent");
    symlink(&real_parent, &linked_parent).expect("symlink output parent");
    let output_path = linked_parent.join("catalog.json");
    let output_text_path = output_path.to_str().expect("UTF-8 output path");
    let version = env!("CARGO_PKG_VERSION");
    let args = vec![
        "release".to_owned(),
        "generate".to_owned(),
        "--output".to_owned(),
        output_text_path.to_owned(),
        "--target".to_owned(),
        "aarch64-apple-darwin".to_owned(),
        "--artefact-name".to_owned(),
        format!("ignatius-{version}-aarch64-apple-darwin.tar.gz"),
        "--artefact-format".to_owned(),
        "tar.gz".to_owned(),
        "--artefact-size".to_owned(),
        "1234".to_owned(),
        "--notes-entry".to_owned(),
        format!("[{version}]"),
        "--evidence-path".to_owned(),
        "release-evidence/0.1.0/index.json".to_owned(),
        "--build-identity".to_owned(),
        "contract-test".to_owned(),
    ];

    let output = run_xtask(args);
    let text = output_text(&output);
    assert!(
        !output.status.success(),
        "unexpected symlink success: {text}"
    );
    assert!(
        text.contains("output file parent must not use a symlink"),
        "{text}"
    );
    assert!(
        !real_parent.join("catalog.json").exists(),
        "symlinked output parent must not receive a generated record"
    );
}

fn is_revision_text(value: &str) -> bool {
    (40..=64).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn prepare_manifest_fixture() -> (TempDir, PathBuf, PathBuf, PathBuf, PathBuf, PathBuf, String) {
    let temp = tempdir().expect("temporary directory");
    let artefact_dir = temp.path().join("artefacts");
    let output_dir = temp.path().join("evidence");
    fs::create_dir_all(&artefact_dir).expect("artefact directory");

    let name = format!(
        "ignatius-{}-aarch64-apple-darwin.tar.gz",
        env!("CARGO_PKG_VERSION")
    );
    let archive = artefact_dir.join(&name);
    let archive_bytes = b"synthetic archive bytes\n";
    fs::write(&archive, archive_bytes).expect("archive");

    let mut catalogue = fixture("valid-candidate.json");
    {
        let record = catalogue["records"][0]
            .as_object_mut()
            .expect("record object");
        record["product_version"] = json!(env!("CARGO_PKG_VERSION"));
        record["target_rows"][0]["artefact_name"] = json!(name);
        record["artefacts"][0]["name"] = json!(name);
        record["artefacts"][0]["size_bytes"] = json!(archive_bytes.len());
        record["artefacts"][0]["checksum_status"] = json!("missing");
        record["artefacts"][0]["checksum"] = Value::Null;
        record["notes_reference"]["entry"] = json!(format!("[{}]", env!("CARGO_PKG_VERSION")));
        record["evidence_reference"]["product_version"] = json!(env!("CARGO_PKG_VERSION"));
    }
    let record_path = temp.path().join("catalog.json");
    fs::write(
        &record_path,
        serde_json::to_vec_pretty(&catalogue).expect("catalogue JSON"),
    )
    .expect("record");

    let generated = run_xtask(vec![
        "release".to_owned(),
        "manifest".to_owned(),
        "generate".to_owned(),
        "--record".to_owned(),
        record_path.to_str().expect("record path").to_owned(),
        "--artefact-dir".to_owned(),
        artefact_dir.to_str().expect("artefact path").to_owned(),
        "--output-dir".to_owned(),
        output_dir.to_str().expect("output path").to_owned(),
    ]);
    let generated_text = output_text(&generated);
    assert!(
        generated.status.success(),
        "unexpected manifest generation failure: {generated_text}"
    );
    let manifest_path = output_dir.join("release-manifest.json");
    let checksums_path = output_dir.join("SHA256SUMS");
    let manifest: Value = serde_json::from_slice(&fs::read(&manifest_path).expect("manifest"))
        .expect("manifest JSON");
    let checksum = manifest["artefacts"][0]["checksum"].clone();
    {
        let record = catalogue["records"][0]
            .as_object_mut()
            .expect("record object");
        record["artefacts"][0]["checksum_status"] = json!("present");
        record["artefacts"][0]["checksum"] = checksum;
    }
    fs::write(
        &record_path,
        serde_json::to_vec_pretty(&catalogue).expect("catalogue JSON"),
    )
    .expect("record with checksum");

    (
        temp,
        record_path,
        artefact_dir,
        output_dir,
        manifest_path,
        checksums_path,
        name,
    )
}

#[test]
fn manifest_generation_and_verification_bind_exact_archive_bytes() {
    let (_temp, record_path, artefact_dir, _output_dir, manifest_path, checksums_path, name) =
        prepare_manifest_fixture();
    let manifest_text = fs::read_to_string(&manifest_path).expect("manifest text");
    assert!(manifest_text.starts_with("{\"artefacts\":["));
    assert!(manifest_text.ends_with("}\n"));
    assert!(!manifest_text.contains("\n\n"));
    let checksums_text = fs::read_to_string(&checksums_path).expect("checksums text");
    assert_eq!(checksums_text.lines().count(), 1);
    assert!(checksums_text.ends_with(&format!("  {name}\n")));

    let verified = run_xtask(vec![
        "release".to_owned(),
        "manifest".to_owned(),
        "verify".to_owned(),
        "--record".to_owned(),
        record_path.to_str().expect("record path").to_owned(),
        "--artefact-dir".to_owned(),
        artefact_dir.to_str().expect("artefact path").to_owned(),
        "--manifest".to_owned(),
        manifest_path.to_str().expect("manifest path").to_owned(),
        "--checksums".to_owned(),
        checksums_path.to_str().expect("checksums path").to_owned(),
    ]);
    let text = output_text(&verified);
    assert!(
        verified.status.success(),
        "unexpected verification failure: {text}"
    );
    assert!(
        text.contains("Release manifest: verified 1 artefact"),
        "{text}"
    );
}

#[test]
fn manifest_operations_accept_schema_integer_number_spellings() {
    let (temp, record_path, artefact_dir, _output_dir, _manifest_path, _checksums_path, _name) =
        prepare_manifest_fixture();
    let original = fs::read_to_string(&record_path).expect("record text");
    let archive_size = fs::read_dir(&artefact_dir)
        .expect("artefact directory")
        .next()
        .expect("archive entry")
        .expect("archive entry result")
        .metadata()
        .expect("archive metadata")
        .len();
    for (label, suffix) in [("decimal", ".0"), ("exponent", "e0")] {
        let source = original.replace(
            &format!("\"size_bytes\": {archive_size}"),
            &format!("\"size_bytes\": {archive_size}{suffix}"),
        );
        assert_ne!(source, original, "size field replacement must succeed");
        fs::write(&record_path, source).expect("spelled record");
        let output_dir = temp.path().join(format!("evidence-{label}"));
        let generated = run_xtask(vec![
            "release".to_owned(),
            "manifest".to_owned(),
            "generate".to_owned(),
            "--record".to_owned(),
            record_path.to_str().expect("record path").to_owned(),
            "--artefact-dir".to_owned(),
            artefact_dir.to_str().expect("artefact path").to_owned(),
            "--output-dir".to_owned(),
            output_dir.to_str().expect("output path").to_owned(),
        ]);
        let generated_text = output_text(&generated);
        assert!(
            generated.status.success(),
            "unexpected {label} manifest generation failure: {generated_text}"
        );
    }
}

#[test]
fn manifest_generation_rejects_a_schema_invalid_ready_record() {
    let (temp, record_path, artefact_dir, _output_dir, _manifest_path, _checksums_path, _name) =
        prepare_manifest_fixture();
    let mut catalogue: Value =
        serde_json::from_slice(&fs::read(&record_path).expect("record")).expect("record JSON");
    catalogue["records"][0]["status"] = json!("ready");
    catalogue["records"][0]["state_history"] = json!([
        {"from": "draft", "to": "candidate"},
        {"from": "candidate", "to": "ready"}
    ]);
    fs::write(
        &record_path,
        serde_json::to_vec_pretty(&catalogue).expect("ready record JSON"),
    )
    .expect("ready record");

    let output_dir = temp.path().join("invalid-ready-evidence");
    let output = run_xtask(vec![
        "release".to_owned(),
        "manifest".to_owned(),
        "generate".to_owned(),
        "--record".to_owned(),
        record_path.to_str().expect("record path").to_owned(),
        "--artefact-dir".to_owned(),
        artefact_dir.to_str().expect("artefact path").to_owned(),
        "--output-dir".to_owned(),
        output_dir.to_str().expect("output path").to_owned(),
    ]);
    let text = output_text(&output);
    assert!(!output.status.success(), "unexpected success: {text}");
    assert!(
        text.contains("release record is not suitable for manifest operation"),
        "{text}"
    );
}

#[test]
fn manifest_verification_rejects_changed_archive_bytes() {
    let (_temp, record_path, artefact_dir, _output_dir, manifest_path, checksums_path, name) =
        prepare_manifest_fixture();
    let archive = artefact_dir.join(name);
    let mut changed = fs::read(&archive).expect("archive bytes");
    changed[0] ^= 1;
    fs::write(&archive, changed).expect("changed archive");
    let verified = run_xtask(vec![
        "release".to_owned(),
        "manifest".to_owned(),
        "verify".to_owned(),
        "--record".to_owned(),
        record_path.to_str().expect("record path").to_owned(),
        "--artefact-dir".to_owned(),
        artefact_dir.to_str().expect("artefact path").to_owned(),
        "--manifest".to_owned(),
        manifest_path.to_str().expect("manifest path").to_owned(),
        "--checksums".to_owned(),
        checksums_path.to_str().expect("checksums path").to_owned(),
    ]);
    let text = output_text(&verified);
    assert!(
        !verified.status.success(),
        "unexpected verification success: {text}"
    );
    assert!(
        text.contains("checksum does not match the exact archive bytes"),
        "{text}"
    );
}

#[test]
fn manifest_verification_rejects_noncanonical_sidecars_and_extra_entries() {
    let (_temp, record_path, artefact_dir, _output_dir, manifest_path, checksums_path, _name) =
        prepare_manifest_fixture();
    let canonical_manifest = fs::read_to_string(&manifest_path).expect("manifest text");
    fs::write(&manifest_path, format!(" {canonical_manifest}")).expect("noncanonical manifest");
    let noncanonical = run_xtask(vec![
        "release".to_owned(),
        "manifest".to_owned(),
        "verify".to_owned(),
        "--record".to_owned(),
        record_path.to_str().expect("record path").to_owned(),
        "--artefact-dir".to_owned(),
        artefact_dir.to_str().expect("artefact path").to_owned(),
        "--manifest".to_owned(),
        manifest_path.to_str().expect("manifest path").to_owned(),
        "--checksums".to_owned(),
        checksums_path.to_str().expect("checksums path").to_owned(),
    ]);
    let noncanonical_text = output_text(&noncanonical);
    assert!(
        !noncanonical.status.success(),
        "unexpected noncanonical success: {noncanonical_text}"
    );
    assert!(
        noncanonical_text.contains("not canonical"),
        "{noncanonical_text}"
    );

    fs::write(&manifest_path, canonical_manifest).expect("restore manifest");
    let checksums = fs::read_to_string(&checksums_path).expect("checksums text");
    fs::write(&checksums_path, format!("{checksums}{checksums}")).expect("duplicate checksum");
    let duplicate = run_xtask(vec![
        "release".to_owned(),
        "manifest".to_owned(),
        "verify".to_owned(),
        "--record".to_owned(),
        record_path.to_str().expect("record path").to_owned(),
        "--artefact-dir".to_owned(),
        artefact_dir.to_str().expect("artefact path").to_owned(),
        "--manifest".to_owned(),
        manifest_path.to_str().expect("manifest path").to_owned(),
        "--checksums".to_owned(),
        checksums_path.to_str().expect("checksums path").to_owned(),
    ]);
    let duplicate_text = output_text(&duplicate);
    assert!(
        !duplicate.status.success(),
        "unexpected duplicate success: {duplicate_text}"
    );
    assert!(
        duplicate_text.contains("SHA256SUMS does not match"),
        "{duplicate_text}"
    );
}

#[cfg(unix)]
#[test]
fn manifest_verification_rejects_symlinked_input_boundaries() {
    use std::os::unix::fs::symlink;

    let (temp, record_path, artefact_dir, _output_dir, manifest_path, checksums_path, name) =
        prepare_manifest_fixture();
    let artefact_link = temp.path().join("artefacts-link");
    symlink(&artefact_dir, &artefact_link).expect("artefact directory symlink");
    let linked_directory = run_xtask(vec![
        "release".to_owned(),
        "manifest".to_owned(),
        "verify".to_owned(),
        "--record".to_owned(),
        record_path.to_str().expect("record path").to_owned(),
        "--artefact-dir".to_owned(),
        artefact_link.to_str().expect("artefact link").to_owned(),
        "--manifest".to_owned(),
        manifest_path.to_str().expect("manifest path").to_owned(),
        "--checksums".to_owned(),
        checksums_path.to_str().expect("checksums path").to_owned(),
    ]);
    let linked_directory_text = output_text(&linked_directory);
    assert!(
        !linked_directory.status.success(),
        "unexpected symlink directory success: {linked_directory_text}"
    );
    assert!(
        linked_directory_text.contains("artefact directory must not be a symlink"),
        "{linked_directory_text}"
    );

    let manifest_link = temp.path().join("manifest-link.json");
    symlink(&manifest_path, &manifest_link).expect("manifest symlink");
    let linked_manifest = run_xtask(vec![
        "release".to_owned(),
        "manifest".to_owned(),
        "verify".to_owned(),
        "--record".to_owned(),
        record_path.to_str().expect("record path").to_owned(),
        "--artefact-dir".to_owned(),
        artefact_dir.to_str().expect("artefact path").to_owned(),
        "--manifest".to_owned(),
        manifest_link.to_str().expect("manifest link").to_owned(),
        "--checksums".to_owned(),
        checksums_path.to_str().expect("checksums path").to_owned(),
    ]);
    let linked_manifest_text = output_text(&linked_manifest);
    assert!(
        !linked_manifest.status.success(),
        "unexpected symlink manifest success: {linked_manifest_text}"
    );
    assert!(
        linked_manifest_text.contains("release manifest must not be a symlink"),
        "{linked_manifest_text}"
    );

    let checksums_link = temp.path().join("checksums-link");
    symlink(&checksums_path, &checksums_link).expect("checksums symlink");
    let linked_checksums = run_xtask(vec![
        "release".to_owned(),
        "manifest".to_owned(),
        "verify".to_owned(),
        "--record".to_owned(),
        record_path.to_str().expect("record path").to_owned(),
        "--artefact-dir".to_owned(),
        artefact_dir.to_str().expect("artefact path").to_owned(),
        "--manifest".to_owned(),
        manifest_path.to_str().expect("manifest path").to_owned(),
        "--checksums".to_owned(),
        checksums_link.to_str().expect("checksums link").to_owned(),
    ]);
    let linked_checksums_text = output_text(&linked_checksums);
    assert!(
        !linked_checksums.status.success(),
        "unexpected symlink checksums success: {linked_checksums_text}"
    );
    assert!(
        linked_checksums_text.contains("checksum sidecar must not be a symlink"),
        "{linked_checksums_text}"
    );

    let archive = artefact_dir.join(&name);
    let archive_target = temp.path().join("archive-real");
    fs::rename(&archive, &archive_target).expect("move archive target");
    symlink(&archive_target, &archive).expect("archive symlink");
    let linked_archive = run_xtask(vec![
        "release".to_owned(),
        "manifest".to_owned(),
        "verify".to_owned(),
        "--record".to_owned(),
        record_path.to_str().expect("record path").to_owned(),
        "--artefact-dir".to_owned(),
        artefact_dir.to_str().expect("artefact path").to_owned(),
        "--manifest".to_owned(),
        manifest_path.to_str().expect("manifest path").to_owned(),
        "--checksums".to_owned(),
        checksums_path.to_str().expect("checksums path").to_owned(),
    ]);
    let linked_archive_text = output_text(&linked_archive);
    assert!(
        !linked_archive.status.success(),
        "unexpected symlink archive success: {linked_archive_text}"
    );
    assert!(
        linked_archive_text.contains("archive must not be a symlink"),
        "{linked_archive_text}"
    );
}

#[cfg(unix)]
#[test]
fn catalogue_validation_rejects_a_symlinked_input_file() {
    use std::os::unix::fs::symlink;

    let temp = tempdir().expect("temporary directory");
    let real_catalogue = temp.path().join("catalog-real.json");
    let catalogue_link = temp.path().join("catalog-link.json");
    fs::write(
        &real_catalogue,
        serde_json::to_vec_pretty(&fixture("valid-candidate.json")).expect("catalogue JSON"),
    )
    .expect("catalogue");
    symlink(&real_catalogue, &catalogue_link).expect("catalogue symlink");

    let output = run_xtask(vec![
        "release".to_owned(),
        "validate".to_owned(),
        catalogue_link
            .to_str()
            .expect("UTF-8 catalogue link")
            .to_owned(),
    ]);
    let text = output_text(&output);
    assert!(
        !output.status.success(),
        "unexpected symlinked catalogue success: {text}"
    );
    assert!(
        text.contains("release catalogue must not be a symlink"),
        "{text}"
    );
}

#[cfg(unix)]
#[test]
fn manifest_generation_rejects_a_dangling_output_directory_symlink() {
    use std::os::unix::fs::symlink;

    let (temp, record_path, artefact_dir, _output_dir, _manifest_path, _checksums_path, _name) =
        prepare_manifest_fixture();
    let missing_target = temp.path().join("missing-evidence");
    let output_link = temp.path().join("evidence-link");
    symlink(&missing_target, &output_link).expect("dangling output symlink");

    let output = run_xtask(vec![
        "release".to_owned(),
        "manifest".to_owned(),
        "generate".to_owned(),
        "--record".to_owned(),
        record_path.to_str().expect("record path").to_owned(),
        "--artefact-dir".to_owned(),
        artefact_dir.to_str().expect("artefact path").to_owned(),
        "--output-dir".to_owned(),
        output_link.to_str().expect("output link").to_owned(),
    ]);
    let text = output_text(&output);
    assert!(
        !output.status.success(),
        "unexpected symlink success: {text}"
    );
    assert!(
        text.contains("output directory already exists without overwriting"),
        "{text}"
    );
    assert_eq!(
        fs::read_link(&output_link).expect("output symlink remains"),
        missing_target,
        "the dangling output symlink must not be replaced"
    );

    let real_parent = temp.path().join("real-output-parent");
    let linked_parent = temp.path().join("linked-output-parent");
    fs::create_dir(&real_parent).expect("real output parent");
    symlink(&real_parent, &linked_parent).expect("symlink output parent");
    let redirected_output = linked_parent.join("evidence");
    let redirected = run_xtask(vec![
        "release".to_owned(),
        "manifest".to_owned(),
        "generate".to_owned(),
        "--record".to_owned(),
        record_path.to_str().expect("record path").to_owned(),
        "--artefact-dir".to_owned(),
        artefact_dir.to_str().expect("artefact path").to_owned(),
        "--output-dir".to_owned(),
        redirected_output
            .to_str()
            .expect("redirected output path")
            .to_owned(),
    ]);
    let redirected_text = output_text(&redirected);
    assert!(
        !redirected.status.success(),
        "unexpected redirected output success: {redirected_text}"
    );
    assert!(
        redirected_text.contains("output directory parent must not use a symlink"),
        "{redirected_text}"
    );
    assert!(
        !real_parent.join("evidence").exists(),
        "redirected output must not be created through a symlinked parent"
    );
}

#[test]
fn unknown_record_fields_are_rejected_before_readiness() {
    let mut catalogue = make_ready(fixture("valid-candidate.json"));
    catalogue["records"][0]["unexpected"] = json!("must not be ignored");
    let (_temp, output) = run_catalogue(catalogue, Some("verified"));
    let text = output_text(&output);
    assert!(!output.status.success(), "unexpected success: {text}");
    assert!(text.contains("unexpected is not recognised"), "{text}");
}

#[test]
fn release_schemas_keep_archive_names_as_portable_basenames() {
    let catalogue = include_str!("../release-notes/catalog.schema.json");
    let manifest = include_str!("../release-notes/release-manifest.schema.json");
    let portable_basename_fragment = "[A-Za-z0-9][A-Za-z0-9._+-]*$";
    let path_capable_fragment = "[A-Za-z0-9][A-Za-z0-9._+/-]*$";

    assert_eq!(
        catalogue.matches(portable_basename_fragment).count(),
        2,
        "catalogue artefact-name schemas must use the basename contract"
    );
    assert_eq!(
        manifest.matches(portable_basename_fragment).count(),
        1,
        "manifest artefact-name schema must use the basename contract"
    );
    assert_eq!(
        catalogue.matches(path_capable_fragment).count(),
        0,
        "catalogue artefact-name schemas must not allow path separators"
    );
    assert_eq!(
        manifest.matches(path_capable_fragment).count(),
        0,
        "manifest artefact-name schema must not allow path separators"
    );
}

#[test]
fn validate_rejects_a_multiline_notes_entry() {
    let mut catalogue = fixture("valid-candidate.json");
    catalogue["records"][0]["notes_reference"]["entry"] = json!("[0.1.0-preview.1]\nleak");
    let (_temp, output) = run_validate(catalogue);
    let text = output_text(&output);
    assert!(
        !output.status.success(),
        "unexpected multiline success: {text}"
    );
    assert!(
        text.contains("notes_reference.entry must be one line"),
        "{text}"
    );
}

#[test]
fn target_format_and_notes_identity_mismatches_are_rejected() {
    let mut catalogue = make_ready(fixture("valid-candidate.json"));
    catalogue["records"][0]["artefacts"][0]["format"] = json!("zip");
    catalogue["records"][0]["notes_reference"]["entry"] = json!("[0.9.0]");
    let (_temp, output) = run_catalogue(catalogue, Some("verified"));
    let text = output_text(&output);
    assert!(!output.status.success(), "unexpected success: {text}");
    assert!(
        text.contains("does not match version, target and format"),
        "{text}"
    );
    assert!(
        text.contains("notes_reference.entry does not name the product version"),
        "{text}"
    );
}

#[test]
fn complete_evidence_must_bind_each_artefact_identity() {
    let catalogue = make_ready(fixture("valid-candidate.json"));
    let (_temp, output) = run_catalogue_with_evidence(catalogue, Some("verified"), |bundle| {
        bundle["artefacts"][0]["target"] = json!("x86_64-unknown-linux-gnu");
    });
    let text = output_text(&output);
    assert!(!output.status.success(), "unexpected success: {text}");
    assert!(
        text.contains("target does not match the release artefact"),
        "{text}"
    );
}

#[test]
fn complete_evidence_rejects_a_conflicting_artefact_name_alias() {
    let catalogue = make_ready(fixture("valid-candidate.json"));
    let (_temp, output) = run_catalogue_with_evidence(catalogue, Some("verified"), |bundle| {
        bundle["artefacts"][0]["artefact_name"] = json!("ignatius-unrelated-artefact.tar.gz");
    });
    let text = output_text(&output);
    assert!(!output.status.success(), "unexpected success: {text}");
    assert!(
        text.contains("name and artefact_name do not match"),
        "{text}"
    );
}

#[test]
fn complete_evidence_must_bind_artefact_format_when_present() {
    let catalogue = make_ready(fixture("valid-candidate.json"));
    let (_temp, output) = run_catalogue_with_evidence(catalogue, Some("verified"), |bundle| {
        bundle["artefacts"][0]["format"] = json!("zip");
    });
    let text = output_text(&output);
    assert!(!output.status.success(), "unexpected success: {text}");
    assert!(
        text.contains("format does not match the release artefact"),
        "{text}"
    );
}

#[test]
fn complete_evidence_accepts_schema_integer_number_spellings() {
    let expected_size = fixture("valid-candidate.json")["records"][0]["artefacts"][0]["size_bytes"]
        .as_u64()
        .expect("fixture size");
    for suffix in [".0", "e0"] {
        let spelling = format!("{expected_size}{suffix}");
        let (_temp, output) = run_catalogue_with_evidence(
            fixture("valid-candidate.json"),
            Some("verified"),
            |bundle| {
                bundle["artefacts"][0]["size_bytes"] =
                    serde_json::from_str(&spelling).expect("JSON number");
            },
        );
        let text = output_text(&output);
        assert!(
            !text.contains("size_bytes does not match the release artefact"),
            "{text}"
        );
    }
}

fn aggregate_fixture(target: &str) -> Value {
    let (platform, architecture, format) = match target {
        "aarch64-apple-darwin" => ("macos", "aarch64", "tar.gz"),
        "x86_64-pc-windows-msvc" => ("windows", "x86_64", "zip"),
        "x86_64-unknown-linux-gnu" => ("linux", "x86_64", "tar.gz"),
        _ => panic!("unsupported aggregate fixture target: {target}"),
    };
    let mut catalogue = fixture("valid-candidate.json");
    let record = &mut catalogue["records"][0];
    let version = record["product_version"]
        .as_str()
        .expect("fixture product version");
    let name = format!("ignatius-{version}-{target}.{format}");
    record["build_identity"] = json!("ci/fixture-run-1");
    record["target_rows"] = json!([{
        "target": target,
        "platform": platform,
        "architecture": architecture,
        "coverage": "covered",
        "artefact_name": name
    }]);
    record["artefacts"] = json!([{
        "name": name,
        "target": target,
        "format": format,
        "size_bytes": 3,
        "checksum_algorithm": "sha256",
        "checksum_status": "present",
        "checksum": "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        "source_revision": "1111111111111111111111111111111111111111",
        "build_identity": "ci/fixture-run-1",
        "signature_status": "not-configured",
        "provenance_status": "not-configured"
    }]);
    record["evidence_reference"]["path"] = json!(format!(
        "release-evidence/runs/fixture-run-1/{target}/release-record.json"
    ));
    catalogue
}

fn write_aggregate_input(directory: &Path, label: &str, catalogue: &Value) -> PathBuf {
    let path = directory.join(format!("{label}.json"));
    fs::write(
        &path,
        serde_json::to_vec_pretty(catalogue).expect("aggregate input JSON"),
    )
    .expect("aggregate input");
    path
}

fn aggregate_args(output: &Path, inputs: &[PathBuf]) -> Vec<String> {
    let mut args = vec![
        "release".to_owned(),
        "aggregate".to_owned(),
        "--output".to_owned(),
        output.to_str().expect("aggregate output path").to_owned(),
        "--evidence-path".to_owned(),
        "release-evidence/runs/fixture-run-1/aggregate/release-record.json".to_owned(),
    ];
    for input in inputs {
        args.push("--input".to_owned());
        args.push(input.to_str().expect("aggregate input path").to_owned());
    }
    args
}

#[test]
fn aggregation_combines_the_exact_supported_targets_without_claiming_readiness() {
    let temp = tempdir().expect("temporary directory");
    let inputs = [
        ("linux", "x86_64-unknown-linux-gnu"),
        ("macos", "aarch64-apple-darwin"),
        ("windows", "x86_64-pc-windows-msvc"),
    ]
    .map(|(label, target)| write_aggregate_input(temp.path(), label, &aggregate_fixture(target)));
    let output_path = temp.path().join("aggregate.json");
    let args = aggregate_args(&output_path, &inputs);

    let output = run_xtask(args.clone());
    let text = output_text(&output);
    assert!(output.status.success(), "unexpected failure: {text}");
    assert!(
        text.contains("Release aggregate: generated 3 artefacts"),
        "{text}"
    );

    let catalogue: Value =
        serde_json::from_slice(&fs::read(&output_path).expect("aggregate catalogue"))
            .expect("aggregate JSON");
    let record = &catalogue["records"][0];
    assert_eq!(record["build_identity"], json!("ci/fixture-run-1"));
    assert_eq!(record["status"], json!("candidate"));
    assert_eq!(
        record["evidence_reference"]["path"],
        json!("release-evidence/runs/fixture-run-1/aggregate/release-record.json")
    );
    let targets = record["target_rows"]
        .as_array()
        .expect("aggregate target rows")
        .iter()
        .map(|row| row["target"].as_str().expect("target"))
        .collect::<Vec<_>>();
    assert_eq!(
        targets,
        vec![
            "aarch64-apple-darwin",
            "x86_64-pc-windows-msvc",
            "x86_64-unknown-linux-gnu"
        ]
    );
    assert_eq!(
        record["artefacts"]
            .as_array()
            .expect("aggregate artefacts")
            .len(),
        3
    );

    let artefact_dir = temp.path().join("artefacts");
    let evidence_dir = temp.path().join("evidence");
    fs::create_dir(&artefact_dir).expect("aggregate artefact directory");
    for artefact in record["artefacts"].as_array().expect("aggregate artefacts") {
        let name = artefact["name"].as_str().expect("aggregate artefact name");
        fs::write(artefact_dir.join(name), b"abc").expect("aggregate artefact bytes");
    }
    let manifest = run_xtask(vec![
        "release".to_owned(),
        "manifest".to_owned(),
        "generate".to_owned(),
        "--record".to_owned(),
        output_path.to_str().expect("aggregate path").to_owned(),
        "--artefact-dir".to_owned(),
        artefact_dir
            .to_str()
            .expect("aggregate artefact path")
            .to_owned(),
        "--output-dir".to_owned(),
        evidence_dir
            .to_str()
            .expect("aggregate evidence path")
            .to_owned(),
    ]);
    let manifest_text = output_text(&manifest);
    assert!(
        manifest.status.success(),
        "aggregate manifest generation failed: {manifest_text}"
    );
    let verified = run_xtask(vec![
        "release".to_owned(),
        "manifest".to_owned(),
        "verify".to_owned(),
        "--record".to_owned(),
        output_path.to_str().expect("aggregate path").to_owned(),
        "--artefact-dir".to_owned(),
        artefact_dir
            .to_str()
            .expect("aggregate artefact path")
            .to_owned(),
        "--manifest".to_owned(),
        evidence_dir
            .join("release-manifest.json")
            .to_str()
            .expect("aggregate manifest path")
            .to_owned(),
        "--checksums".to_owned(),
        evidence_dir
            .join("SHA256SUMS")
            .to_str()
            .expect("aggregate checksums path")
            .to_owned(),
    ]);
    let verified_text = output_text(&verified);
    assert!(
        verified.status.success(),
        "aggregate manifest verification failed: {verified_text}"
    );

    let validated = run_xtask(vec![
        "release".to_owned(),
        "validate".to_owned(),
        output_path.to_str().expect("aggregate path").to_owned(),
    ]);
    let validated_text = output_text(&validated);
    assert!(
        validated.status.success(),
        "aggregate did not validate: {validated_text}"
    );
    let readiness = run_xtask(vec![
        "release".to_owned(),
        "check".to_owned(),
        output_path.to_str().expect("aggregate path").to_owned(),
    ]);
    let readiness_text = output_text(&readiness);
    assert!(
        !readiness.status.success(),
        "unexpected readiness: {readiness_text}"
    );
    assert!(
        readiness_text.contains("not in a publishable readiness state"),
        "{readiness_text}"
    );

    let overwrite = run_xtask(args);
    let overwrite_text = output_text(&overwrite);
    assert!(
        !overwrite.status.success(),
        "unexpected overwrite: {overwrite_text}"
    );
    assert!(
        overwrite_text.contains("without overwriting"),
        "{overwrite_text}"
    );
}

#[test]
fn aggregation_rejects_cross_target_identity_mismatch() {
    let temp = tempdir().expect("temporary directory");
    let macos = write_aggregate_input(
        temp.path(),
        "macos",
        &aggregate_fixture("aarch64-apple-darwin"),
    );
    let mut windows_record = aggregate_fixture("x86_64-pc-windows-msvc");
    windows_record["records"][0]["build_identity"] = json!("ci/other-run");
    windows_record["records"][0]["artefacts"][0]["build_identity"] = json!("ci/other-run");
    let windows = write_aggregate_input(temp.path(), "windows", &windows_record);
    let linux = write_aggregate_input(
        temp.path(),
        "linux",
        &aggregate_fixture("x86_64-unknown-linux-gnu"),
    );
    let output_path = temp.path().join("aggregate.json");

    let output = run_xtask(aggregate_args(&output_path, &[macos, windows, linux]));
    let text = output_text(&output);
    assert!(!output.status.success(), "unexpected success: {text}");
    assert!(
        text.contains("build_identity does not match across aggregate inputs"),
        "{text}"
    );
    assert!(!output_path.exists(), "mismatched aggregate was written");
}

#[test]
fn aggregation_preserves_blocked_status_and_unions_blockers() {
    let temp = tempdir().expect("temporary directory");
    let mut catalogues = [
        aggregate_fixture("aarch64-apple-darwin"),
        aggregate_fixture("x86_64-pc-windows-msvc"),
        aggregate_fixture("x86_64-unknown-linux-gnu"),
    ];
    for (index, catalogue) in catalogues.iter_mut().enumerate() {
        let record = &mut catalogue["records"][0];
        record["status"] = json!("blocked");
        record["state_history"] = json!([
            {"from": "draft", "to": "candidate"},
            {"from": "candidate", "to": "blocked"}
        ]);
        record["blockers"] = json!([format!("target blocker {index}")]);
    }
    let inputs = catalogues
        .iter()
        .enumerate()
        .map(|(index, catalogue)| {
            write_aggregate_input(temp.path(), &format!("target-{index}"), catalogue)
        })
        .collect::<Vec<_>>();
    let output_path = temp.path().join("aggregate.json");

    let output = run_xtask(aggregate_args(&output_path, &inputs));
    let text = output_text(&output);
    assert!(output.status.success(), "unexpected failure: {text}");
    let catalogue: Value =
        serde_json::from_slice(&fs::read(&output_path).expect("aggregate catalogue"))
            .expect("aggregate JSON");
    let record = &catalogue["records"][0];
    assert_eq!(record["status"], json!("blocked"));
    assert_eq!(
        record["blockers"],
        json!(["target blocker 0", "target blocker 1", "target blocker 2"])
    );
}

#[test]
fn aggregation_requires_each_supported_target_exactly_once() {
    let temp = tempdir().expect("temporary directory");
    let macos = write_aggregate_input(
        temp.path(),
        "macos",
        &aggregate_fixture("aarch64-apple-darwin"),
    );
    let linux = write_aggregate_input(
        temp.path(),
        "linux",
        &aggregate_fixture("x86_64-unknown-linux-gnu"),
    );
    let output_path = temp.path().join("aggregate.json");
    let missing = run_xtask(aggregate_args(
        &output_path,
        &[macos.clone(), linux.clone()],
    ));
    let missing_text = output_text(&missing);
    assert!(
        !missing.status.success(),
        "unexpected success: {missing_text}"
    );
    assert!(
        missing_text.contains("release aggregate requires exactly 3 --input records"),
        "{missing_text}"
    );

    let duplicate_macos = write_aggregate_input(
        temp.path(),
        "macos-again",
        &aggregate_fixture("aarch64-apple-darwin"),
    );
    let duplicate = run_xtask(aggregate_args(
        &output_path,
        &[macos, duplicate_macos, linux],
    ));
    let duplicate_text = output_text(&duplicate);
    assert!(
        !duplicate.status.success(),
        "unexpected success: {duplicate_text}"
    );
    assert!(
        duplicate_text.contains("duplicate supported target in aggregate inputs"),
        "{duplicate_text}"
    );
    assert!(!output_path.exists(), "incomplete aggregate was written");

    let mut unsupported_record = aggregate_fixture("x86_64-unknown-linux-gnu");
    unsupported_record["records"][0]["target_rows"][0]["target"] = json!("x86_64-unknown-freebsd");
    unsupported_record["records"][0]["artefacts"][0]["target"] = json!("x86_64-unknown-freebsd");
    unsupported_record["records"][0]["target_rows"][0]["artefact_name"] =
        json!("ignatius-0.1.0-x86_64-unknown-freebsd.tar.gz");
    unsupported_record["records"][0]["artefacts"][0]["name"] =
        json!("ignatius-0.1.0-x86_64-unknown-freebsd.tar.gz");
    let unsupported = write_aggregate_input(temp.path(), "unsupported", &unsupported_record);
    let macos = write_aggregate_input(
        temp.path(),
        "macos-supported",
        &aggregate_fixture("aarch64-apple-darwin"),
    );
    let windows = write_aggregate_input(
        temp.path(),
        "windows-supported",
        &aggregate_fixture("x86_64-pc-windows-msvc"),
    );
    let unsupported_output =
        run_xtask(aggregate_args(&output_path, &[macos, windows, unsupported]));
    let unsupported_text = output_text(&unsupported_output);
    assert!(
        !unsupported_output.status.success(),
        "unexpected success: {unsupported_text}"
    );
    assert!(
        unsupported_text.contains("not a supported platform target"),
        "{unsupported_text}"
    );
    assert!(!output_path.exists(), "unsupported aggregate was written");
}

#[test]
fn release_workflow_binds_run_attempt_to_candidate_identity() {
    let workflow = include_str!("../.github/workflows/release.yml");
    for expected in [
        "IGNATIUS_BUILD_IDENTITY: ci/${{ github.run_id }}-${{ github.run_attempt }}",
        "run_key=\"${GITHUB_RUN_ID}-${GITHUB_RUN_ATTEMPT}\"",
        "build_identity=\"ci/${run_key}\"",
        "evidence_path=\"release-evidence/runs/${run_key}/${TARGET}/release-record.json\"",
        "release-candidate-${{ matrix.target }}-${{ github.run_id }}-${{ github.run_attempt }}",
    ] {
        assert!(
            workflow.contains(expected),
            "missing workflow contract: {expected}"
        );
    }
    assert!(
        !workflow.contains("IGNATIUS_BUILD_IDENTITY: ci/${{ github.run_id }}-${{ github.run_attempt }}/${{ matrix.target }}"),
        "workflow retained a target-specific build identity that cannot aggregate",
    );
}

#[test]
fn release_workflow_aggregates_three_reverified_target_bundles() {
    let workflow = include_str!("../.github/workflows/release.yml");
    for expected in [
        "aggregate:\n    name: Aggregate candidate evidence\n    needs: [source, archive, runtime]",
        "timeout-minutes: 20",
        "ref: ${{ needs.source.outputs.revision }}",
        "actions/download-artifact@",
        "pattern: release-candidate-*-${{ github.run_id }}-${{ github.run_attempt }}",
        "pattern: release-runtime-*-${{ github.run_id }}-${{ github.run_attempt }}",
        "merge-multiple: false",
        "downloaded_entries=(\"$inputs_root\"/*)",
        "unexpected number of downloaded target bundles",
        "Re-run all jobs or start a fresh full dispatch so every target shares one run attempt.",
        "EXPECTED_REVISION: ${{ needs.source.outputs.revision }}",
        "actual_target",
        "actual_version",
        "actual_revision",
        "actual_build_identity",
        "cargo --locked xtask release manifest verify",
        "cargo --locked xtask release aggregate",
        "runtime-evidence.json",
        "runtime/%s.json",
        "cargo --locked xtask release evidence-scope --root \"$upload\" --allowlist \"$scope\"",
        "--volume \"$upload:/scan:ro\"",
        "release-candidate-aggregate-${{ github.run_id }}-${{ github.run_attempt }}",
        "for archive_name in \"${archive_names[@]}\"; do",
        "verified archive count does not match the supported target count",
    ] {
        assert!(
            workflow.contains(expected),
            "missing aggregate workflow contract: {expected}"
        );
    }
}

#[test]
fn release_workflow_keeps_release_note_identity_stable_across_runs() {
    let workflow = include_str!("../.github/workflows/release.yml");
    let stable_notes_assignment = "notes_entry=\"release candidate ${version}\"";
    assert_eq!(
        workflow.matches(stable_notes_assignment).count(),
        1,
        "workflow must assign one product-version-derived release-note identity"
    );
    let stable_notes_line = workflow
        .lines()
        .find(|line| line.contains(stable_notes_assignment))
        .expect("stable notes assignment");
    assert!(
        workflow.contains("--notes-entry \"$notes_entry\""),
        "workflow must pass the stable release-note identity into the record"
    );
    assert!(
        !stable_notes_line.contains("run_key"),
        "release-note identity must not include the retry-scoped run key"
    );
    assert!(
        !stable_notes_line.contains("GITHUB_RUN_"),
        "release-note identity must not include GitHub run or attempt numbers"
    );
    assert!(
        !workflow.contains(
            "notes_entry=\"release candidate ${version} from GitHub Actions run ${run_key}\""
        ),
        "workflow must not make the release-note identity run-specific"
    );
}

#[test]
fn release_workflow_reads_the_product_version_from_its_authority() {
    let workflow = include_str!("../.github/workflows/release.yml");
    assert!(
        workflow.contains(
            "version=\"$(sed -n 's/^version = \"\\([^\"]*\\)\"$/\\1/p' Cargo.toml | head -n 1)\""
        ),
        "release workflow must read the product version directly from Cargo.toml"
    );
    for expected in [
        "could not read the product version from Cargo.toml",
        "target build did not produce the expected binary",
        "refusing a symlinked target binary",
    ] {
        assert!(
            workflow.contains(expected),
            "release workflow must explain its pre-archive refusal: {expected}"
        );
    }
    assert!(
        !workflow.contains("$(cargo pkgid"),
        "release workflow must not parse Cargo's package-ID display format"
    );
}

#[test]
fn release_workflow_freezes_and_rechecks_one_source_identity() {
    let workflow = include_str!("../.github/workflows/release.yml");
    for expected in [
        "revision=\"$(git rev-parse HEAD)\"",
        "revision: ${{ steps.source.outputs.revision }}",
        "needs: source",
        "needs: [source, preflight]",
        "ref: ${{ needs.source.outputs.revision }}",
        "needs: [source, archive]",
        "actual_revision",
        "expected $EXPECTED_REVISION",
        "actual_version",
        "expected $version",
        "actual_build_identity",
        "expected $expected_build_identity",
        "compression-level: 0",
        "merge-multiple: false",
        "All target bundles were re-verified after artifact transport",
    ] {
        assert!(
            workflow.contains(expected),
            "release workflow is missing its immutable candidate-set contract: {expected}"
        );
    }

    let source = workflow_job_block(workflow, "source");
    assert!(
        source.contains("ref: ${{ inputs.ref || github.sha }}"),
        "event-triggered releases must check out the immutable event SHA"
    );
    assert!(
        !source.contains("github.ref"),
        "event-triggered releases must not resolve a mutable branch or tag ref"
    );
}

#[test]
fn release_workflow_preflights_a_frozen_source_before_retaining_candidates() {
    let workflow = include_str!("../.github/workflows/release.yml");
    let preflight = workflow_job_block(workflow, "preflight");
    for expected in [
        "preflight:\n    name: Validate candidate source",
        "timeout-minutes: 60",
        "Validate release notes against the candidate source",
        "run: release-notes/validate.sh",
        "uses: ./.github/actions/select-postgres-fixture",
        "Run the source supply-chain policy",
        "cargo install --locked --version 0.20.2 --root \"$install_root\" cargo-deny",
        "Scan immutable candidate history for secrets",
        "git --no-banner --redact --log-opts=--all",
        "cargo xtask db up",
        "Start the Unix socket verifier fixture",
        "IGNATIUS_TEST_PG_SOCKET_URI=host='%s' port=5432 user=ignatius_test dbname=ignatius_demo password=not-a-real-password-disposable-container sslmode=disable",
        "cargo xtask verify",
        "Remove verifier fixtures",
    ] {
        assert!(
            preflight.contains(expected),
            "release workflow is missing its frozen-source preflight contract: {expected}"
        );
    }
    assert!(
        workflow.contains("needs: [source, preflight]"),
        "archives must wait for successful source preflight"
    );
    for expected in [
        "fetch-depth: 0",
        "--volume \"$GITHUB_WORKSPACE:/repo:ro\"",
        "--workdir /repo",
        "docker run --rm --network none",
        "} >> \"$GITHUB_ENV\"",
    ] {
        assert!(
            preflight.contains(expected),
            "release source preflight is missing its full-history or live-socket contract: {expected}"
        );
    }
}

#[test]
fn release_workflow_scans_the_actual_retained_candidate_bundles() {
    let workflow = include_str!("../.github/workflows/release.yml");
    let aggregate = workflow_job_block(workflow, "aggregate");
    for expected in [
        "Scan the exact retained target bundles and the final aggregate",
        "--volume \"$bundle:/scan:ro\"",
        "--volume \"$upload:/scan:ro\"",
        "ghcr.io/gitleaks/gitleaks:v8.30.0@sha256:691af3c7c5a48b16f187ce3446d5f194838f91238f27270ed36eef6359a574d9",
        "dir --no-banner --redact /scan",
    ] {
        assert!(
            aggregate.contains(expected),
            "release workflow is missing its retained-bundle secret scan: {expected}"
        );
    }
    assert!(
        aggregate.contains(
            "for target in \"${targets[@]}\"; do\n            bundle=\"$inputs_root/release-candidate-${target}-${run_key}\"\n            docker run --rm --network none \\\n              --volume \"$bundle:/scan:ro\""
        ),
        "every retained target bundle must be scanned through a network-isolated read-only mount"
    );
    assert!(
        aggregate.contains(
            "docker run --rm --network none \\\n            --volume \"$upload:/scan:ro\""
        ),
        "the final aggregate must be scanned through a network-isolated read-only mount"
    );
    assert_eq!(
        aggregate.matches("docker run --rm --network none").count(),
        2,
        "the aggregate must contain exactly the target-loop and final-package secret scans"
    );
}

#[test]
fn release_workflow_declares_the_exact_supported_target_matrix() {
    let workflow = include_str!("../.github/workflows/release.yml");
    for expected in [
        "runner: ubuntu-latest\n            target: x86_64-unknown-linux-gnu\n            format: tar.gz\n            binary: ignatius",
        "runner: macos-latest\n            target: aarch64-apple-darwin\n            format: tar.gz\n            binary: ignatius",
        "runner: windows-latest\n            target: x86_64-pc-windows-msvc\n            format: zip\n            binary: ignatius.exe",
    ] {
        assert!(
            workflow.contains(expected),
            "missing release target matrix row: {expected}"
        );
    }
    let archive = workflow
        .split_once("  runtime:\n")
        .map(|(archive, _)| archive)
        .expect("archive job before runtime job");
    assert_eq!(
        archive.matches("            target: ").count(),
        3,
        "the archive job must not silently add or remove a supported target"
    );
}

#[test]
fn release_workflow_is_read_only_and_non_publishing() {
    let workflow = include_str!("../.github/workflows/release.yml");
    let lines: Vec<&str> = workflow.lines().collect();
    let permission_starts: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| (line.trim() == "permissions:").then_some(index))
        .collect();
    assert_eq!(
        permission_starts.len(),
        1,
        "candidate workflow must have exactly one read-only workflow permission block"
    );
    let permission_start = permission_starts[0];
    let jobs = lines
        .iter()
        .position(|line| *line == "jobs:")
        .expect("candidate workflow jobs");
    assert!(
        permission_start < jobs,
        "candidate workflow permissions must be global rather than a job override"
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
    assert_eq!(
        permission_entries,
        ["contents: read"],
        "candidate workflow must grant only read repository contents"
    );
    assert!(
        workflow.contains("actions/upload-artifact@"),
        "candidate evidence must use the upload action"
    );
    assert!(
        !workflow.contains("continue-on-error: true"),
        "candidate workflow must not make a required gate advisory"
    );

    let lowercase = workflow.to_ascii_lowercase();
    for forbidden in [
        "git push",
        "gh release",
        "action-gh-release",
        "cosign",
        "sigstore",
        "secrets.",
    ] {
        assert!(
            !lowercase.contains(forbidden),
            "non-publishing workflow contains forbidden operation: {forbidden}"
        );
    }
}

#[test]
fn release_workflow_uses_the_required_actions_and_pinned_build_inputs() {
    let workflow = include_str!("../.github/workflows/release.yml");
    // workflow_contract owns full-SHA validation and cross-workflow pin
    // consistency. Require action identities here without duplicating their
    // dependency-managed revisions.
    for expected in [
        "actions/checkout@",
        "dtolnay/rust-toolchain@",
        "Swatinem/rust-cache@",
        "actions/upload-artifact@",
        "actions/download-artifact@",
        "toolchain: \"1.97.1\"",
        "fetch-depth: 0",
        "persist-credentials: false",
        "archive_script=\"$PWD/scripts/create-release-archive.py\"",
        "python --version 2>&1 | grep -Eq '^Python 3(\\.|$)'",
        "if [[ \"${RUNNER_OS:-}\" == \"Windows\" ]]",
        "cygpath is required on Windows for the shared release path",
        "--format \"$FORMAT\"",
        "--source-dir \"$bundle\"",
        "--binary \"$BINARY\"",
        "--output \"$artefacts/$archive_name\"",
        "scripts/release-runtime-smoke.py",
        "bundle=\"$runtime_inputs\"",
        "ci/repro-${source_revision}",
        "rustflags: \"-D warnings -C link-arg=/Brepro\"",
        "cmp -- \"$first\" \"$second\"",
        "release-reproducibility-${{ github.run_id }}-${{ github.run_attempt }}",
    ] {
        assert!(
            workflow.contains(expected),
            "missing pinned workflow input: {expected}"
        );
    }
    assert_eq!(
        workflow.matches("        uses: ").count(),
        25,
        "each external workflow action must remain explicit and pinned"
    );
    assert!(
        !workflow.contains("tar -czf") && !workflow.contains("7z a"),
        "archive metadata must be controlled by the repository-owned helper"
    );
}

#[test]
fn shared_ci_scans_only_the_checked_release_evidence_scope() {
    let workflow = include_str!("../.github/workflows/ci.yml");
    let evidence_scan = workflow_job_block(workflow, "release-evidence");
    for expected in [
        "release-evidence:\n    name: Release evidence scope and secret scan",
        "cargo --locked xtask release evidence-scope --root \"$root\" --allowlist \"$allowlist\"",
        "printf 'root=%s\\n' \"$root\" >> \"$GITHUB_OUTPUT\"",
        "--volume \"${{ steps.release-scope.outputs.root }}:/scan:ro\"",
        "ghcr.io/gitleaks/gitleaks:v8.30.0@sha256:691af3c7c5a48b16f187ce3446d5f194838f91238f27270ed36eef6359a574d9",
        "dir --no-banner --redact /scan",
    ] {
        assert!(
            evidence_scan.contains(expected),
            "shared CI is missing the release-evidence control: {expected}"
        );
    }
    assert!(
        evidence_scan.contains("docker run --rm --network none"),
        "the scanner must not receive network access"
    );
    assert_eq!(
        evidence_scan.matches("--volume ").count(),
        1,
        "the release-evidence scan must mount exactly one checked root"
    );
    for forbidden in ["GITHUB_WORKSPACE", "github.workspace", "$PWD"] {
        assert!(
            !evidence_scan.contains(forbidden),
            "the release-evidence scan must not widen to {forbidden}"
        );
    }
    assert!(
        evidence_scan.contains("--volume \"${{ steps.release-scope.outputs.root }}:/scan:ro\""),
        "the release-evidence scan must mount only the verified output root"
    );
}

#[test]
fn deterministic_archive_creation_repeats_identical_bytes_for_each_format() {
    let temp = tempdir().expect("temporary directory");
    let source = temp.path().join("source");
    fs::create_dir(&source).expect("source directory");
    fs::write(source.join("ignatius"), b"synthetic executable bytes\n").expect("binary");
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/create-release-archive.py");
    let python = ["python3", "python"]
        .into_iter()
        .find(|candidate| Command::new(candidate).arg("--version").output().is_ok())
        .expect("a Python 3 interpreter is required for release archive tests");

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;

        let linked_binary = source.join("linked-ignatius");
        symlink(source.join("ignatius"), &linked_binary).expect("source binary symlink");
        let linked_output = temp.path().join("linked-input.tar.gz");
        let linked_result = Command::new(python)
            .arg(&script)
            .args([
                "--format",
                "tar.gz",
                "--source-dir",
                source.to_str().expect("UTF-8 source path"),
                "--binary",
                "linked-ignatius",
                "--output",
                linked_output.to_str().expect("UTF-8 linked output path"),
            ])
            .output()
            .expect("run archive helper against symlinked source binary");
        assert!(
            !linked_result.status.success(),
            "symlinked source binary was accepted"
        );
        assert!(
            String::from_utf8_lossy(&linked_result.stderr)
                .contains("source binary must be a regular, non-symlink file"),
            "symlinked source refusal was not actionable: {}",
            String::from_utf8_lossy(&linked_result.stderr)
        );
        assert!(
            !linked_output.exists(),
            "symlinked source binary produced an archive"
        );
    }

    for archive_format in ["tar.gz", "zip"] {
        let first = temp.path().join(format!("first.{archive_format}"));
        let second = temp.path().join(format!("second.{archive_format}"));
        for output in [&first, &second] {
            let result = Command::new(python)
                .arg(&script)
                .args([
                    "--format",
                    archive_format,
                    "--source-dir",
                    source.to_str().expect("UTF-8 source path"),
                    "--binary",
                    "ignatius",
                    "--output",
                    output.to_str().expect("UTF-8 archive path"),
                ])
                .output()
                .expect("run deterministic archive helper");
            assert!(
                result.status.success(),
                "archive helper failed for {archive_format}: {}",
                String::from_utf8_lossy(&result.stderr)
            );
        }
        assert_eq!(
            fs::read(&first).expect("first archive"),
            fs::read(&second).expect("second archive"),
            "same input must produce identical {archive_format} bytes"
        );

        let existing = temp.path().join(format!("existing.{archive_format}"));
        fs::write(&existing, b"do not replace this file\n").expect("existing output");
        let refusal = Command::new(python)
            .arg(&script)
            .args([
                "--format",
                archive_format,
                "--source-dir",
                source.to_str().expect("UTF-8 source path"),
                "--binary",
                "ignatius",
                "--output",
                existing.to_str().expect("UTF-8 archive path"),
            ])
            .output()
            .expect("run archive helper against existing output");
        assert!(!refusal.status.success(), "existing output was overwritten");
        assert!(
            String::from_utf8_lossy(&refusal.stderr).contains("refusing to overwrite"),
            "existing output refusal was not actionable: {}",
            String::from_utf8_lossy(&refusal.stderr)
        );
        assert_eq!(
            fs::read(&existing).expect("existing output after refusal"),
            b"do not replace this file\n",
            "existing output changed after refusal"
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;

            let real_output_dir = temp.path().join(format!("real-output-{archive_format}"));
            let linked_output_dir = temp.path().join(format!("linked-output-{archive_format}"));
            fs::create_dir(&real_output_dir).expect("real output directory");
            symlink(&real_output_dir, &linked_output_dir).expect("output directory symlink");
            let redirected = linked_output_dir.join("redirected.archive");
            let redirected_result = Command::new(python)
                .arg(&script)
                .args([
                    "--format",
                    archive_format,
                    "--source-dir",
                    source.to_str().expect("UTF-8 source path"),
                    "--binary",
                    "ignatius",
                    "--output",
                    redirected.to_str().expect("UTF-8 redirected path"),
                ])
                .output()
                .expect("run archive helper against symlinked output parent");
            assert!(
                !redirected_result.status.success(),
                "symlinked output parent was accepted for {archive_format}"
            );
            assert!(
                String::from_utf8_lossy(&redirected_result.stderr)
                    .contains("output parent directory must not be a symlink"),
                "symlinked output parent refusal was not actionable: {}",
                String::from_utf8_lossy(&redirected_result.stderr)
            );
            assert!(
                !real_output_dir.join("redirected.archive").exists(),
                "symlinked output parent received an archive"
            );
        }
    }
}

fn base64_encode_contract(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = chunk.get(1).copied().unwrap_or(0) as usize;
        let b2 = chunk.get(2).copied().unwrap_or(0) as usize;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[(triple >> 18) & 0x3f] as char);
        out.push(TABLE[(triple >> 12) & 0x3f] as char);
        if chunk.len() > 1 {
            out.push(TABLE[(triple >> 6) & 0x3f] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[triple & 0x3f] as char);
        } else {
            out.push('=');
        }
    }
    out
}

fn sha256_digest_contract(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    let mut h = [
        0x6a09e667u32,
        0xbb67ae85,
        0x3c6ef372,
        0xa54ff53a,
        0x510e527f,
        0x9b05688c,
        0x1f83d9ab,
        0x5be0cd19,
    ];

    let mut msg = data.to_vec();
    let bit_len = (data.len() as u64) * 8;
    msg.push(0x80);
    while !(msg.len() + 8).is_multiple_of(64) {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in msg.as_chunks::<64>().0 {
        let mut w = [0u32; 64];
        for (i, b) in chunk.as_chunks::<4>().0.iter().enumerate() {
            w[i] = u32::from_be_bytes(*b);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let mut a = h[0];
        let mut b = h[1];
        let mut c = h[2];
        let mut d = h[3];
        let mut e = h[4];
        let mut f = h[5];
        let mut g = h[6];
        let mut h_val = h[7];

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = h_val
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            h_val = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(h_val);
    }

    let mut out = [0u8; 32];
    for (i, val) in h.iter().enumerate() {
        out[i * 4..(i + 1) * 4].copy_from_slice(&val.to_be_bytes());
    }
    out
}

fn fixture_digest_for_contract(bytes: &[u8]) -> (String, String) {
    let digest = sha256_digest_contract(bytes);
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    let b64 = base64_encode_contract(&digest);
    (hex, b64)
}

#[test]
fn w11_sbom_generate_and_verify_cli_roundtrips_and_detects_tampered_lockfile_hash() {
    let temp = tempdir().expect("temporary directory");
    let lockfile_source = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.lock");
    let lockfile_copy = temp.path().join("Cargo.lock");
    fs::copy(&lockfile_source, &lockfile_copy).expect("copy Cargo.lock");

    let sbom_path = temp.path().join("ignatius.cdx.json");

    let generate = run_xtask(vec![
        "release".to_owned(),
        "sbom".to_owned(),
        "generate".to_owned(),
        format!("--lockfile={}", lockfile_copy.display()),
        format!("--out={}", sbom_path.display()),
    ]);
    let generate_text = output_text(&generate);
    assert!(
        generate.status.success(),
        "release sbom generate failed: {generate_text}"
    );
    assert!(
        generate_text.contains("SBOM: recorded"),
        "expected SBOM generation summary: {generate_text}"
    );
    assert!(sbom_path.exists(), "generated SBOM file must exist");

    let verify = run_xtask(vec![
        "release".to_owned(),
        "sbom".to_owned(),
        "verify".to_owned(),
        format!("--lockfile={}", lockfile_copy.display()),
        format!("--sbom={}", sbom_path.display()),
    ]);
    let verify_text = output_text(&verify);
    assert!(
        verify.status.success(),
        "release sbom verify failed on clean roundtrip: {verify_text}"
    );
    assert!(
        verify_text.contains("matches"),
        "expected match confirmation: {verify_text}"
    );

    let original_sbom_bytes = fs::read(&sbom_path).expect("read original sbom");
    let mut tampered_sbom = original_sbom_bytes.clone();
    tampered_sbom[12] ^= 0x01;
    fs::write(&sbom_path, &tampered_sbom).expect("write tampered sbom");

    let verify_tampered_sbom = run_xtask(vec![
        "release".to_owned(),
        "sbom".to_owned(),
        "verify".to_owned(),
        format!("--lockfile={}", lockfile_copy.display()),
        format!("--sbom={}", sbom_path.display()),
    ]);
    let tampered_sbom_text = output_text(&verify_tampered_sbom);
    assert!(
        !verify_tampered_sbom.status.success(),
        "release sbom verify must fail on tampered SBOM"
    );
    assert!(
        tampered_sbom_text.contains("does not match")
            && tampered_sbom_text.contains("first difference at byte"),
        "expected difference diagnostic on tampered SBOM: {tampered_sbom_text}"
    );

    fs::write(&sbom_path, &original_sbom_bytes).expect("restore original sbom");

    let original_lock_text = fs::read_to_string(&lockfile_copy).expect("read original lock");
    let needle = "name = \"ignatius\"";
    assert!(
        original_lock_text.contains(needle),
        "Cargo.lock must contain product name"
    );
    let tampered_lock_text = original_lock_text.replacen(needle, "name = \"ignatiux\"", 1);
    fs::write(&lockfile_copy, &tampered_lock_text).expect("write tampered lock");

    let verify_tampered_lock = run_xtask(vec![
        "release".to_owned(),
        "sbom".to_owned(),
        "verify".to_owned(),
        format!("--lockfile={}", lockfile_copy.display()),
        format!("--sbom={}", sbom_path.display()),
    ]);
    let tampered_lock_text = output_text(&verify_tampered_lock);
    assert!(
        !verify_tampered_lock.status.success(),
        "release sbom verify must fail on tampered lockfile"
    );
    assert!(
        tampered_lock_text.contains("does not match")
            && tampered_lock_text.contains("first difference at byte"),
        "expected difference diagnostic on tampered lockfile: {tampered_lock_text}"
    );
}

#[test]
fn w11_trust_verify_cli_validates_complete_bundle_and_refuses_tampered_sidecars() {
    let temp = tempdir().expect("temporary directory");
    let bundle_dir = temp.path().join("bundle");
    fs::create_dir_all(&bundle_dir).expect("create bundle dir");

    let payload = b"synthetic release archive payload for trust verification\n";
    let (hex, b64) = fixture_digest_for_contract(payload);

    let archive_name = "ignatius-0.1.0-preview.1-aarch64-apple-darwin.tar.gz";
    let archive_path = bundle_dir.join(archive_name);
    fs::write(&archive_path, payload).expect("write archive");

    let sums_path = bundle_dir.join("SHA256SUMS");
    fs::write(&sums_path, format!("{hex}  {archive_name}\n")).expect("write SHA256SUMS");

    let sig_path = bundle_dir.join(format!("{archive_name}.sig"));
    fs::write(&sig_path, b"synthetic raw signature bytes\n").expect("write sig");

    let pem_path = bundle_dir.join(format!("{archive_name}.pem"));
    fs::write(
        &pem_path,
        b"-----BEGIN CERTIFICATE-----\nMIIC...synthetic\n-----END CERTIFICATE-----\n",
    )
    .expect("write pem");

    let cdx_path = bundle_dir.join("ignatius-0.1.0.cdx.json");
    fs::write(
        &cdx_path,
        b"{\"bomFormat\":\"CycloneDX\",\"specVersion\":\"1.5\",\"version\":1,\"components\":[]}\n",
    )
    .expect("write cdx");

    let sigstore_path = bundle_dir.join(format!("{archive_name}.sigstore.json"));
    let sigstore_content = format!(
        "{{\"messageSignature\":{{\"messageDigest\":{{\"algorithm\":\"SHA2_256\",\"digest\":\"{b64}\"}},\"signature\":\"AAA=\"}}}}"
    );
    fs::write(&sigstore_path, &sigstore_content).expect("write sigstore");

    let intoto_path = bundle_dir.join(format!("{archive_name}.intoto.jsonl"));
    let intoto_content = format!(
        "{{\"_type\":\"https://in-toto.io/Statement/v1\",\"subject\":[{{\"name\":\"{archive_name}\",\"digest\":{{\"sha256\":\"{hex}\"}}}}],\"predicateType\":\"https://slsa.dev/provenance/v1\"}}\n"
    );
    fs::write(&intoto_path, &intoto_content).expect("write intoto");

    let manifest_path = bundle_dir.join("release-manifest.json");
    let manifest_json = json!({
        "schema_version": 1,
        "product_version": "0.1.0-preview.1",
        "source_revision": "1111111111111111111111111111111111111111",
        "build_identity": "ci-rehearsal-2026-08-16",
        "artefacts": [{
            "name": archive_name,
            "target": "aarch64-apple-darwin",
            "format": "tar.gz",
            "size_bytes": payload.len(),
            "checksum_algorithm": "sha256",
            "checksum": hex
        }]
    });
    fs::write(
        &manifest_path,
        format!(
            "{}\n",
            serde_json::to_string(&manifest_json).expect("manifest json")
        ),
    )
    .expect("write manifest");

    let mut catalogue = fixture("valid-candidate.json");
    catalogue["records"][0]["artefacts"][0]["size_bytes"] = json!(payload.len());
    catalogue["records"][0]["artefacts"][0]["checksum"] = json!(hex);
    let catalogue_path = temp.path().join("catalog.json");
    fs::write(
        &catalogue_path,
        serde_json::to_string_pretty(&catalogue).expect("catalog json"),
    )
    .expect("write catalog");

    let valid_trust = run_xtask(vec![
        "release".to_owned(),
        "trust".to_owned(),
        "verify".to_owned(),
        format!("--manifest-dir={}", bundle_dir.display()),
        format!("--bundle-dir={}", bundle_dir.display()),
    ]);
    let valid_trust_text = output_text(&valid_trust);
    assert!(
        valid_trust.status.success(),
        "release trust verify failed on valid bundle: {valid_trust_text}"
    );
    assert!(
        valid_trust_text.contains("bound ignatius-0.1.0-preview.1-aarch64-apple-darwin.tar.gz"),
        "expected bound artefact in output: {valid_trust_text}"
    );

    let valid_manifest = run_xtask(vec![
        "release".to_owned(),
        "manifest".to_owned(),
        "verify".to_owned(),
        "--record".to_owned(),
        catalogue_path.display().to_string(),
        "--artefact-dir".to_owned(),
        bundle_dir.display().to_string(),
        "--manifest".to_owned(),
        manifest_path.display().to_string(),
        "--checksums".to_owned(),
        sums_path.display().to_string(),
    ]);
    let valid_manifest_text = output_text(&valid_manifest);
    assert!(
        valid_manifest.status.success(),
        "release manifest verify failed on valid bundle: {valid_manifest_text}"
    );

    // (a) Archive SHA-256 digest does not match SHA256SUMS
    let tampered_payload = b"tampered archive payload bytes with mismatched sha256 checksum\n";
    fs::write(&archive_path, tampered_payload).expect("write tampered archive");

    let verify_tampered_archive = run_xtask(vec![
        "release".to_owned(),
        "manifest".to_owned(),
        "verify".to_owned(),
        "--record".to_owned(),
        catalogue_path.display().to_string(),
        "--artefact-dir".to_owned(),
        bundle_dir.display().to_string(),
        "--manifest".to_owned(),
        manifest_path.display().to_string(),
        "--checksums".to_owned(),
        sums_path.display().to_string(),
    ]);
    let tampered_archive_text = output_text(&verify_tampered_archive);
    assert!(
        !verify_tampered_archive.status.success(),
        "manifest verify must fail when archive SHA-256 digest does not match SHA256SUMS"
    );
    assert!(
        tampered_archive_text.contains("checksum does not match the exact archive bytes")
            || tampered_archive_text.contains("size is"),
        "expected mismatch diagnostic: {tampered_archive_text}"
    );

    fs::write(&archive_path, payload).expect("restore archive");

    // (b) A .sig/.pem (or sigstore.json) sidecar is missing or empty
    fs::remove_file(&sigstore_path).expect("remove sigstore sidecar");
    let missing_sidecar = run_xtask(vec![
        "release".to_owned(),
        "trust".to_owned(),
        "verify".to_owned(),
        format!("--manifest-dir={}", bundle_dir.display()),
        format!("--bundle-dir={}", bundle_dir.display()),
    ]);
    let missing_sidecar_text = output_text(&missing_sidecar);
    assert!(
        !missing_sidecar.status.success(),
        "trust verify must fail when sigstore sidecar is missing"
    );
    assert!(
        missing_sidecar_text.contains("binding missing"),
        "expected missing sidecar diagnostic: {missing_sidecar_text}"
    );

    fs::write(&sigstore_path, b"").expect("write empty sigstore sidecar");
    let empty_sidecar = run_xtask(vec![
        "release".to_owned(),
        "trust".to_owned(),
        "verify".to_owned(),
        format!("--manifest-dir={}", bundle_dir.display()),
        format!("--bundle-dir={}", bundle_dir.display()),
    ]);
    let empty_sidecar_text = output_text(&empty_sidecar);
    assert!(
        !empty_sidecar.status.success(),
        "trust verify must fail when sigstore sidecar is empty"
    );
    assert!(
        empty_sidecar_text.contains("is not a Sigstore bundle"),
        "expected empty/malformed sidecar diagnostic: {empty_sidecar_text}"
    );

    fs::write(&sig_path, b"").expect("empty sig");
    assert_eq!(
        fs::metadata(&sig_path).expect("sig metadata").len(),
        0,
        "sig sidecar is empty"
    );
    fs::write(&pem_path, b"").expect("empty pem");
    assert_eq!(
        fs::metadata(&pem_path).expect("pem metadata").len(),
        0,
        "pem sidecar is empty"
    );

    fs::write(&sigstore_path, &sigstore_content).expect("restore sigstore");
    fs::write(&sig_path, b"synthetic raw signature bytes\n").expect("restore sig");
    fs::write(
        &pem_path,
        b"-----BEGIN CERTIFICATE-----\nMIIC...synthetic\n-----END CERTIFICATE-----\n",
    )
    .expect("restore pem");

    // (c) .intoto.jsonl SLSA provenance subject digest does not match archive digest
    let wrong_digest = "00".repeat(32);
    let tampered_intoto = format!(
        "{{\"_type\":\"https://in-toto.io/Statement/v1\",\"subject\":[{{\"name\":\"{archive_name}\",\"digest\":{{\"sha256\":\"{wrong_digest}\"}}}}],\"predicateType\":\"https://slsa.dev/provenance/v1\"}}\n"
    );
    fs::write(&intoto_path, &tampered_intoto).expect("write tampered intoto");

    let tampered_provenance = run_xtask(vec![
        "release".to_owned(),
        "trust".to_owned(),
        "verify".to_owned(),
        format!("--manifest-dir={}", bundle_dir.display()),
        format!("--bundle-dir={}", bundle_dir.display()),
    ]);
    let tampered_provenance_text = output_text(&tampered_provenance);
    assert!(
        !tampered_provenance.status.success(),
        "trust verify must fail when in-toto provenance subject digest does not match"
    );
    assert!(
        tampered_provenance_text.contains("names no subject with the manifest checksum"),
        "expected foreign subject diagnostic: {tampered_provenance_text}"
    );

    fs::write(&intoto_path, &intoto_content).expect("restore intoto");
    let recovered_trust = run_xtask(vec![
        "release".to_owned(),
        "trust".to_owned(),
        "verify".to_owned(),
        format!("--manifest-dir={}", bundle_dir.display()),
        format!("--bundle-dir={}", bundle_dir.display()),
    ]);
    assert!(
        recovered_trust.status.success(),
        "trust verify should pass after restoring sidecars"
    );
}
