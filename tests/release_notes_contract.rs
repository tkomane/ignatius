//! Contract tests for the read-only release catalogue/changelog validator.

#[cfg(unix)]
mod unix {
    use std::fs;
    use std::path::Path;
    use std::process::Command;

    use serde_json::{Value, json};
    use tempfile::tempdir;

    fn run_validator(catalogue: &Path, changelog: &Path) -> std::process::Output {
        let repository = Path::new(env!("CARGO_MANIFEST_DIR"));
        let script = repository.join("release-notes/validate.sh");
        let working_directory = tempdir().expect("temporary working directory");
        Command::new("sh")
            .arg(&script)
            .arg("--catalog")
            .arg(catalogue)
            .arg("--changelog")
            .arg(changelog)
            .current_dir(working_directory.path())
            .output()
            .expect("run release notes validator")
    }

    #[test]
    fn repository_catalogue_has_one_exact_versioned_changelog_entry() {
        let repository = Path::new(env!("CARGO_MANIFEST_DIR"));
        let catalogue = repository.join("release-notes/catalog.json");
        let changelog = repository.join("CHANGELOG.md");
        let output = run_validator(&catalogue, &changelog);
        assert!(
            output.status.success(),
            "repository release notes must validate: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("Release notes: verified 1 record")
        );
    }

    #[test]
    fn validator_accepts_one_matching_entry_and_rejects_a_missing_entry() {
        let repository = Path::new(env!("CARGO_MANIFEST_DIR"));
        let catalogue = repository.join("release-notes/fixtures/valid-candidate.json");
        let matching_changelog =
            repository.join("release-notes/fixtures/changelog-with-preview-entry.md");

        let accepted = run_validator(&catalogue, &matching_changelog);
        assert!(
            accepted.status.success(),
            "matching changelog must pass: {}",
            String::from_utf8_lossy(&accepted.stderr)
        );
        assert!(
            String::from_utf8_lossy(&accepted.stdout).contains("Release notes: verified 1 record")
        );

        let repository_changelog = repository.join("CHANGELOG.md");
        let rejected = run_validator(&catalogue, &repository_changelog);
        assert!(
            !rejected.status.success(),
            "missing changelog entry must fail: {}",
            String::from_utf8_lossy(&rejected.stdout)
        );
        assert!(
            String::from_utf8_lossy(&rejected.stderr).contains("occurs 0 times"),
            "missing-entry diagnostic: {}",
            String::from_utf8_lossy(&rejected.stderr)
        );
    }

    #[test]
    fn validator_requires_one_exact_release_heading() {
        let repository = Path::new(env!("CARGO_MANIFEST_DIR"));
        let catalogue = repository.join("release-notes/fixtures/valid-candidate.json");
        for fixture in [
            "changelog-body-only-entry.md",
            "changelog-near-version-heading.md",
            "changelog-duplicate-entry.md",
        ] {
            let changelog = repository.join("release-notes/fixtures").join(fixture);
            let rejected = run_validator(&catalogue, &changelog);
            let stderr = String::from_utf8_lossy(&rejected.stderr);
            assert!(
                !rejected.status.success(),
                "invalid changelog fixture unexpectedly passed: {fixture}\n{stderr}"
            );
            assert!(
                stderr.contains("heading occurs"),
                "exact-heading diagnostic missing for {fixture}: {stderr}"
            );
        }
    }

    #[test]
    fn validator_rejects_a_stable_version_embedded_in_a_preview_entry() {
        let mut catalogue: Value = serde_json::from_str(include_str!(
            "../release-notes/fixtures/valid-candidate.json"
        ))
        .expect("valid catalogue fixture");
        let record = catalogue["records"][0]
            .as_object_mut()
            .expect("record object");
        let stable_version = "0.1.0";
        let archive = "ignatius-0.1.0-aarch64-apple-darwin.tar.gz";
        record["product_version"] = json!(stable_version);
        record["target_rows"][0]["artefact_name"] = json!(archive);
        record["artefacts"][0]["name"] = json!(archive);
        record["notes_reference"]["entry"] = json!("[0.1.0-preview.1]");
        record["evidence_reference"]["product_version"] = json!(stable_version);

        let temp = tempdir().expect("temporary directory");
        let temp_path = fs::canonicalize(temp.path()).expect("physical temporary directory");
        let catalogue_path = temp_path.join("catalog.json");
        let changelog_path = temp_path.join("CHANGELOG.md");
        fs::write(
            &catalogue_path,
            serde_json::to_vec_pretty(&catalogue).expect("catalogue JSON"),
        )
        .expect("catalogue");
        fs::write(&changelog_path, "# Fixture\n\n## [0.1.0-preview.1]\n").expect("changelog");

        let rejected = run_validator(&catalogue_path, &changelog_path);
        let output = format!(
            "{}{}",
            String::from_utf8_lossy(&rejected.stdout),
            String::from_utf8_lossy(&rejected.stderr)
        );
        assert!(
            !rejected.status.success(),
            "stable version embedded in preview entry unexpectedly passed: {output}"
        );
        assert!(
            output.contains("notes_reference.entry does not name the product version"),
            "stable/preview identity diagnostic missing: {output}"
        );
    }

    #[test]
    fn validator_rejects_reused_exact_entry_heading() {
        let mut catalogue: Value = serde_json::from_str(include_str!(
            "../release-notes/fixtures/valid-candidate.json"
        ))
        .expect("valid catalogue fixture");
        let entry = "[0.1.0-preview.1 and 0.1.0-preview.2]";
        catalogue["records"][0]["notes_reference"]["entry"] = json!(entry);

        let mut second = catalogue["records"][0].clone();
        second["product_version"] = json!("0.1.0-preview.2");
        second["build_identity"] = json!("ci-rehearsal-2026-08-16-2");
        second["target_rows"][0]["artefact_name"] =
            json!("ignatius-0.1.0-preview.2-aarch64-apple-darwin.tar.gz");
        second["artefacts"][0]["name"] =
            json!("ignatius-0.1.0-preview.2-aarch64-apple-darwin.tar.gz");
        second["artefacts"][0]["build_identity"] = json!("ci-rehearsal-2026-08-16-2");
        second["notes_reference"]["entry"] = json!(entry);
        second["evidence_reference"]["path"] = json!("release-evidence/0.1.0-preview.2/index.json");
        second["evidence_reference"]["product_version"] = json!("0.1.0-preview.2");
        catalogue["records"] = json!([catalogue["records"][0].clone(), second]);

        let temp = tempdir().expect("temporary directory");
        let temp_path = fs::canonicalize(temp.path()).expect("physical temporary directory");
        let catalogue_path = temp_path.join("catalog.json");
        let changelog_path = temp_path.join("CHANGELOG.md");
        fs::write(
            &catalogue_path,
            serde_json::to_vec_pretty(&catalogue).expect("catalogue JSON"),
        )
        .expect("catalogue");
        fs::write(&changelog_path, format!("# Fixture\n\n## {entry}\n")).expect("changelog");

        let rejected = run_validator(&catalogue_path, &changelog_path);
        let stderr = String::from_utf8_lossy(&rejected.stderr);
        assert!(
            !rejected.status.success(),
            "reused entry heading unexpectedly passed: {stderr}"
        );
        assert!(
            stderr.contains("notes_reference.entry duplicates records[0]"),
            "duplicate-entry diagnostic missing: {stderr}"
        );
    }

    #[test]
    fn validator_rejects_symlinked_input_parents() {
        use std::os::unix::fs::symlink;

        let temp = tempdir().expect("temporary directory");
        let temp_path = fs::canonicalize(temp.path()).expect("physical temporary directory");
        let real_dir = temp_path.join("real");
        let linked_dir = temp_path.join("linked");
        fs::create_dir(&real_dir).expect("real input directory");
        symlink(&real_dir, &linked_dir).expect("input directory symlink");

        let real_catalogue = real_dir.join("catalog.json");
        let real_changelog = real_dir.join("CHANGELOG.md");
        fs::write(
            &real_catalogue,
            include_bytes!("../release-notes/fixtures/valid-candidate.json"),
        )
        .expect("catalogue");
        fs::write(
            &real_changelog,
            include_bytes!("../release-notes/fixtures/changelog-with-preview-entry.md"),
        )
        .expect("changelog");

        let linked_catalogue = linked_dir.join("catalog.json");
        let rejected_catalogue = run_validator(&linked_catalogue, &real_changelog);
        let catalogue_stderr = String::from_utf8_lossy(&rejected_catalogue.stderr);
        assert!(
            !rejected_catalogue.status.success(),
            "symlinked catalogue parent unexpectedly passed: {catalogue_stderr}"
        );
        assert!(
            catalogue_stderr.contains("catalogue parent must not be a symlink"),
            "catalogue-parent diagnostic missing: {catalogue_stderr}"
        );

        let linked_changelog = linked_dir.join("CHANGELOG.md");
        let rejected_changelog = run_validator(&real_catalogue, &linked_changelog);
        let changelog_stderr = String::from_utf8_lossy(&rejected_changelog.stderr);
        assert!(
            !rejected_changelog.status.success(),
            "symlinked changelog parent unexpectedly passed: {changelog_stderr}"
        );
        assert!(
            changelog_stderr.contains("changelog parent must not be a symlink"),
            "changelog-parent diagnostic missing: {changelog_stderr}"
        );
    }

    #[test]
    fn validator_rejects_symlinked_non_immediate_input_ancestors() {
        use std::os::unix::fs::symlink;

        let temp = tempdir().expect("temporary directory");
        let temp_path = fs::canonicalize(temp.path()).expect("physical temporary directory");
        let real_dir = temp_path.join("real");
        let linked_dir = temp_path.join("linked");
        let nested_dir = real_dir.join("nested");
        fs::create_dir_all(&nested_dir).expect("nested input directory");
        symlink(&real_dir, &linked_dir).expect("input ancestor symlink");

        let real_catalogue = nested_dir.join("catalog.json");
        let real_changelog = nested_dir.join("CHANGELOG.md");
        fs::write(
            &real_catalogue,
            include_bytes!("../release-notes/fixtures/valid-candidate.json"),
        )
        .expect("catalogue");
        fs::write(
            &real_changelog,
            include_bytes!("../release-notes/fixtures/changelog-with-preview-entry.md"),
        )
        .expect("changelog");

        let linked_catalogue = linked_dir.join("nested/catalog.json");
        let rejected_catalogue = run_validator(&linked_catalogue, &real_changelog);
        let catalogue_stderr = String::from_utf8_lossy(&rejected_catalogue.stderr);
        assert!(
            !rejected_catalogue.status.success(),
            "nested symlinked catalogue ancestor unexpectedly passed: {catalogue_stderr}"
        );
        assert!(
            catalogue_stderr.contains("catalogue parent must not be a symlink"),
            "nested catalogue-parent diagnostic missing: {catalogue_stderr}"
        );

        let linked_changelog = linked_dir.join("nested/CHANGELOG.md");
        let rejected_changelog = run_validator(&real_catalogue, &linked_changelog);
        let changelog_stderr = String::from_utf8_lossy(&rejected_changelog.stderr);
        assert!(
            !rejected_changelog.status.success(),
            "nested symlinked changelog ancestor unexpectedly passed: {changelog_stderr}"
        );
        assert!(
            changelog_stderr.contains("changelog parent must not be a symlink"),
            "nested changelog-parent diagnostic missing: {changelog_stderr}"
        );
    }
}
