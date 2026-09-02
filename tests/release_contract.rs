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
        "aggregate:\n    name: Aggregate candidate evidence\n    needs: archive",
        "actions/download-artifact@3e5f45b2cfb9172054b4087a40e8e0b5a5461e7c",
        "pattern: release-candidate-*-${{ github.run_id }}-${{ github.run_attempt }}",
        "merge-multiple: false",
        "downloaded_entries=(\"$inputs_root\"/*)",
        "unexpected number of downloaded target bundles",
        "cargo --locked xtask release manifest verify",
        "cargo --locked xtask release aggregate",
        "cargo --locked xtask release evidence-scope --root \"$upload\" --allowlist \"$scope\"",
        "release-candidate-aggregate-${{ github.run_id }}-${{ github.run_attempt }}",
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
    assert_eq!(
        workflow.matches("            target: ").count(),
        3,
        "the release workflow must not silently add or remove a supported target"
    );
}

#[test]
fn release_workflow_is_read_only_and_non_publishing() {
    let workflow = include_str!("../.github/workflows/release.yml");
    assert!(
        workflow.contains("permissions:\n  contents: read"),
        "candidate workflow must use read-only repository permissions"
    );
    assert!(
        workflow.contains("actions/upload-artifact@ea165f8d65b6e75b540449e92b4886f43607fa02"),
        "candidate evidence must use the pinned upload action"
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
fn release_workflow_pins_checkout_toolchain_cache_and_upload_inputs() {
    let workflow = include_str!("../.github/workflows/release.yml");
    for expected in [
        "actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1",
        "dtolnay/rust-toolchain@6c977a6ca4077a0ceb28ffbe03f59d46e9ac8772",
        "Swatinem/rust-cache@6323deb102c322ba6fcbdcafc7e3dddab59af2b6",
        "actions/upload-artifact@ea165f8d65b6e75b540449e92b4886f43607fa02",
        "actions/download-artifact@3e5f45b2cfb9172054b4087a40e8e0b5a5461e7c",
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
    ] {
        assert!(
            workflow.contains(expected),
            "missing pinned workflow input: {expected}"
        );
    }
    assert_eq!(
        workflow.matches("        uses: ").count(),
        9,
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
    for expected in [
        "release-evidence:\n    name: Release evidence scope and secrets",
        "cargo xtask release evidence-scope --root \"$root\" --allowlist \"$allowlist\"",
        "printf 'root=%s\\n' \"$root\" >> \"$GITHUB_OUTPUT\"",
        "--volume \"${{ steps.release-scope.outputs.root }}:/scan:ro\"",
        "ghcr.io/gitleaks/gitleaks:v8.30.0@sha256:691af3c7c5a48b16f187ce3446d5f194838f91238f27270ed36eef6359a574d9",
        "dir --no-banner --redact /scan",
    ] {
        assert!(
            workflow.contains(expected),
            "shared CI is missing the release-evidence control: {expected}"
        );
    }
    assert!(
        workflow.contains("docker run --rm --network none"),
        "the scanner must not receive network access"
    );
    assert!(
        !workflow.contains("--volume \"${{ github.workspace }}:/scan"),
        "the release-evidence scan must not widen to the workspace"
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
