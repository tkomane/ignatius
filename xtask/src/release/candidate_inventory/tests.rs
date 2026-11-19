use super::*;

const DIGEST: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const REVISION: &str = "9876543210abcdef9876543210abcdef98765432";
const APP: &str = "pkg:cargo/ignatius@0.1.0";
const TOOL: &str = "pkg:cargo/xtask@0.0.0";
const CRATE: &str = "pkg:cargo/shared@2.0.0";
const DEV: &str = "pkg:cargo/dev-only@1.0.0";
const PATH_ID: &str = "path+file:///private/fake/build/source";

fn lockfile() -> String {
    format!(
        r#"version = 4

[[package]]
name = "ignatius"
version = "0.1.0"

[[package]]
name = "xtask"
version = "0.0.0"

[[package]]
name = "shared"
version = "2.0.0"
source = "{REGISTRY_SOURCE}"
checksum = "{DIGEST}"

[[package]]
name = "dev-only"
version = "1.0.0"
source = "{REGISTRY_SOURCE}"
checksum = "{DIGEST}"
"#
    )
}

fn inputs() -> Inputs {
    Inputs {
        version: "1.97.1".into(),
        digests: INPUT_FILES
            .iter()
            .map(|&file| (file, DIGEST.into()))
            .collect(),
        packages: locked_packages(&lockfile()).expect("lockfile fixture"),
    }
}

fn manifest(target: &str) -> super::super::ParsedManifest {
    let format = if target == "x86_64-pc-windows-msvc" {
        "zip"
    } else {
        "tar.gz"
    };
    super::super::ParsedManifest {
        product_version: "0.1.0".into(),
        source_revision: REVISION.into(),
        build_identity: "ci/123-1".into(),
        artefacts: vec![super::super::ManifestArtefact {
            name: format!("ignatius-0.1.0-{target}.{format}"),
            target: target.into(),
            format: format.into(),
            size_bytes: 23,
            checksum: DIGEST.into(),
        }],
    }
}

fn metadata(root: &Path) -> Value {
    json!({
        "version": 1,
        "packages": [
            {"id": PATH_ID, "name": "ignatius", "version": "0.1.0", "source": null,
             "manifest_path": root.join("Cargo.toml")},
            {"id": "packaging-id", "name": "xtask", "version": "0.0.0", "source": null,
             "manifest_path": root.join("xtask/Cargo.toml")},
            {"id": "registry-id", "name": "shared", "version": "2.0.0", "source": REGISTRY_SOURCE},
        ],
        "workspace_members": ["packaging-id", PATH_ID],
        "resolve": { "nodes": [
            { "id": PATH_ID, "deps": [{ "pkg": "registry-id",
                "dep_kinds": [{ "kind": null, "target": "cfg(unix)" }] }] },
            { "id": "packaging-id", "deps": [{ "pkg": "registry-id",
                "dep_kinds": [{ "kind": "build", "target": null }] }] },
            { "id": "registry-id", "deps": [] }
        ]}
    })
}

fn trace(role: Role, root: &Path) -> String {
    let id = if role == Role::Product {
        PATH_ID
    } else {
        "packaging-id"
    };
    let output = root.join(role.package());
    let messages = [
        json!({"reason": "compiler-message", "message": {"rendered": "synthetic-unretained-message"}}),
        json!({"reason": "build-script-executed", "env": [["PRIVATE_NAME", "synthetic-unretained-value"]]}),
        json!({
            "reason": "compiler-artifact", "package_id": "registry-id",
            "profile": {"test": false, "opt_level": role.opt_level()},
            "target": {"kind": ["lib"], "name": "shared"},
            "features": ["std", "default"], "filenames": ["private-rust-library"],
            "fresh": true, "executable": null
        }),
        json!({
            "reason": "compiler-artifact", "package_id": id,
            "profile": {"test": false, "opt_level": role.opt_level()},
            "target": {"kind": ["bin"], "name": role.package()},
            "features": [], "filenames": [output], "executable": output, "fresh": false
        }),
        json!({"reason": "build-finished", "success": true}),
    ];
    messages
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

fn cargo_fixture(inputs: &Inputs) -> (PathBuf, Metadata) {
    let root = super::super::super::repo_root();
    let metadata = Metadata::parse(&root, inputs, metadata(&root).to_string().as_bytes())
        .expect("locked, filtered metadata");
    (root, metadata)
}

fn captured(inputs: &Inputs) -> BuildFacts {
    let (root, metadata) = cargo_fixture(inputs);
    let mut facts = BuildFacts {
        binary_sha256: DIGEST.into(),
        packaging_sha256: DIGEST.into(),
        ..Default::default()
    };
    for role in Role::ALL {
        let path = facts
            .read_trace(&metadata, inputs, role, trace(role, &root).as_bytes())
            .expect("Cargo emitted its application root successfully");
        assert_eq!(path, root.join(role.package()));
    }
    facts.relations = metadata.relations;
    facts
        .validate(inputs)
        .expect("complete candidate inventory");
    facts
}

#[test]
fn candidate_accounts_for_both_builds_and_excluded_locked_crates() {
    let inputs = inputs();
    let manifest = manifest("x86_64-pc-windows-msvc");
    let context = Context {
        inputs: &inputs,
        manifest: &manifest,
        archive: 0,
    };
    let facts = captured(&inputs);
    let bytes = render(&context, &facts).expect("CycloneDX data");
    verify_binding(&context, &bytes).expect("cross-host transport verification");
    let evidence = std::str::from_utf8(&bytes).expect("UTF-8 CycloneDX data");
    for suppressed in [
        PATH_ID,
        "synthetic-unretained-value",
        "synthetic-unretained-message",
        REGISTRY_SOURCE,
    ] {
        assert!(!evidence.contains(suppressed));
    }
    let parsed = parse_unique_json_bytes(&bytes).expect("JSON inventory");
    assert_eq!(parsed["metadata"]["component"]["bom-ref"], APP);
    assert_eq!(parsed["metadata"]["lifecycles"][0]["phase"], "build");
    let components = array(&parsed, "components").expect("components");
    let component = |reference| {
        components
            .iter()
            .find(|item| item["bom-ref"] == reference)
            .expect("component")
    };
    assert_eq!(component(DEV)["scope"], "excluded");
    assert_eq!(component(TOOL)["scope"], "required");
    assert_eq!(component(CRATE)["scope"], "required");
    assert_eq!(facts.units[CRATE].len(), 2);
    assert_eq!(facts.relations.len(), 2);
    assert_eq!(render(&context, &facts).expect("deterministic"), bytes);
}

#[test]
fn binding_rejects_retargeted_archive_modified_inputs_and_noncanonical_json() {
    let inputs = inputs();
    let win = manifest("x86_64-pc-windows-msvc");
    let linux = manifest("x86_64-unknown-linux-gnu");
    let context = Context {
        inputs: &inputs,
        manifest: &win,
        archive: 0,
    };
    let facts = captured(&inputs);
    let bytes = render(&context, &facts).expect("CycloneDX data");
    assert!(
        verify_binding(
            &Context {
                manifest: &linux,
                ..context
            },
            &bytes
        )
        .is_err()
    );
    let mut changed = self::inputs();
    changed.digests.insert("Cargo.lock", "f".repeat(64));
    assert!(
        verify_binding(
            &Context {
                inputs: &changed,
                ..context
            },
            &bytes
        )
        .is_err()
    );
    let mut altered = parse_unique_json_bytes(&bytes).expect("JSON inventory");
    altered["unexpected"] = json!("synthetic-path-value");
    assert!(verify_binding(&context, altered.to_string().as_bytes()).is_err());
    let mut edited = bytes.clone();
    edited.push(b'\n');
    assert!(verify_binding(&context, &edited).is_err());
    assert!(
        compare(&edited, &bytes)
            .expect_err("byte verification")
            .contains("does not match")
    );
}

#[test]
fn cargo_trace_requires_one_expected_binary_and_successful_completion() {
    let inputs = inputs();
    let (root, metadata) = cargo_fixture(&inputs);
    let trace = trace(Role::Product, &root);
    for malformed in [
        trace.replace("\"success\":true", "\"success\":false"),
        trace.lines().take(4).collect::<Vec<_>>().join("\n"),
        trace.replace("registry-id", "unlocked-artifact"),
        trace.replace(
            "\"features\":[\"std\",\"default\"]",
            "\"features\":[\"/abs/home/private\"]",
        ),
        format!("{trace}\n{{\"reason\":\"compiler-message\"}}"),
    ] {
        assert!(
            BuildFacts::default()
                .read_trace(&metadata, &inputs, Role::Product, malformed.as_bytes())
                .is_err()
        );
    }
    assert!(
        BuildFacts::default()
            .read_trace(&metadata, &inputs, Role::Packaging, trace.as_bytes())
            .is_err()
    );
}

#[test]
fn metadata_identity_and_coverage_fail_closed() {
    let inputs = inputs();
    let root = super::super::super::repo_root();
    let mut mismatch = metadata(&root);
    mismatch["packages"][2]["source"] = json!("registry+https://synthetic@bad.example/registry");
    assert!(Metadata::parse(&root, &inputs, mismatch.to_string().as_bytes()).is_err());
    let mut facts = captured(&inputs);
    facts.relations.retain(|relation| relation.from != TOOL);
    assert!(
        facts
            .validate(&inputs)
            .expect_err("orphan compiled dependency")
            .contains("outside the locked dependency graph")
    );
    assert!(
        locked_packages(&lockfile().replace(REGISTRY_SOURCE, "git+https://bad.example/secret"))
            .is_err()
    );
    assert_eq!(
        purl("wasi", "0.11.1+wasi-snapshot-preview1").expect("purl"),
        "pkg:cargo/wasi@0.11.1%2Bwasi-snapshot-preview1"
    );
}

#[test]
fn options_reject_unapproved_policy_flags_and_unknown_target() {
    let valid = [
        "generate",
        "--target=x86_64-pc-windows-msvc",
        "--manifest=scratch",
        "--metadata=scratch",
        "--product-build=scratch",
        "--packaging-build=scratch",
        "--binary=scratch",
        "--rustflags=-D warnings -C link-arg=/Brepro",
        "--policy=pass",
        "--out=scratch",
    ];
    assert!(Options::parse(&valid).is_ok());
    for (index, replacement) in [
        (1, "--target=unsupported"),
        (7, "--rustflags=-D warnings"),
        (8, "--policy=skipped"),
        (9, "--manifest=duplicate"),
    ] {
        let mut bad = valid;
        bad[index] = replacement;
        assert!(Options::parse(&bad).is_err());
    }
}
