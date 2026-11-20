use super::*;

const DIGEST: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const REVISION: &str = "9876543210abcdef9876543210abcdef98765432";
const APP: &str = "pkg:cargo/ignatius@0.1.0";
const TOOL: &str = "pkg:cargo/xtask@0.0.0";
const CRATE: &str = "pkg:cargo/shared@2.0.0";
const DEV: &str = "pkg:cargo/dev-only@1.0.0";
const PATCHED: &str = "pkg:cargo/tokio-postgres@0.7.18";
const PATH_ID: &str = "path+file:///private/fake/build/source";
const PATCH_LOCK_ENTRY: &str = "\n[[package]]\nname = \"tokio-postgres\"\nversion = \"0.7.18\"\n";

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
        packages: locked_packages(Path::new("."), &lockfile()).expect("lockfile fixture"),
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
        locked_packages(
            &root,
            &lockfile().replace(REGISTRY_SOURCE, "git+https://bad.example/secret")
        )
        .is_err()
    );
    assert_eq!(
        purl("wasi", "0.11.1+wasi-snapshot-preview1").expect("purl"),
        "pkg:cargo/wasi@0.11.1%2Bwasi-snapshot-preview1"
    );
}

struct RepositoryPatchFixture(PathBuf);

impl RepositoryPatchFixture {
    fn new() -> Self {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "ignatius-sbom-patch-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&root).expect("new unit test directory");
        let fixture = Self(root);
        fs::create_dir(fixture.0.join("xtask")).expect("packaging directory");
        fs::create_dir_all(fixture.0.join("vendor/tokio-postgres/src"))
            .expect("repository patch directory");
        fs::write(fixture.0.join("Cargo.toml"), "fixture").expect("product manifest");
        fs::write(fixture.0.join("xtask/Cargo.toml"), "fixture").expect("packaging manifest");
        fs::write(
            fixture.0.join(repository_patch::MANIFEST),
            "[package]\nname = \"tokio-postgres\"\nversion = \"0.7.18\"\n",
        )
        .expect("repository patch manifest");
        fs::write(
            fixture.0.join("vendor/tokio-postgres/src/lib.rs"),
            "pub fn fixture() {}\n",
        )
        .expect("repository patch source");
        fixture
    }

    fn inputs(&self) -> Inputs {
        Inputs {
            packages: locked_packages(&self.0, &(lockfile() + PATCH_LOCK_ENTRY))
                .expect("approved repository patch in fixture lockfile"),
            ..inputs()
        }
    }
}

impl Drop for RepositoryPatchFixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn repository_patch_binding_uses_source_bytes_without_a_registry_checksum() {
    let fixture = RepositoryPatchFixture::new();
    let inputs = fixture.inputs();
    let mut facts = captured(&self::inputs());
    assert!(facts.validate(&inputs).is_err());
    facts.units.insert(
        PATCHED.into(),
        BTreeSet::from([Unit {
            role: Role::Product,
            kinds: BTreeSet::from(["lib".into()]),
            features: BTreeSet::new(),
            opt_level: Role::Product.opt_level().into(),
        }]),
    );
    facts.relations.insert(Relation {
        from: APP.into(),
        to: PATCHED.into(),
        kind: "normal",
        target: None,
    });

    let manifest = manifest("aarch64-apple-darwin");
    let context = Context {
        inputs: &inputs,
        manifest: &manifest,
        archive: 0,
    };
    let bytes = render(&context, &facts).expect("repository patch inventory");
    verify_binding(&context, &bytes).expect("binds the exact tree");
    let parsed = parse_unique_json_bytes(&bytes).expect("inventory");
    let component = array(&parsed, "components")
        .expect("components")
        .iter()
        .find(|item| item["bom-ref"] == PATCHED)
        .expect("repository patch");
    assert_eq!(component["type"], "library");
    assert_eq!(component["scope"], "required");
    assert_eq!(
        required_property(component, "ignatius:cargo:source").expect("source type"),
        "repository-patch",
    );
    assert_eq!(
        required_property(component, "ignatius:cargo:source-manifest").expect("manifest"),
        repository_patch::MANIFEST,
    );
    assert!(required_property(component, "ignatius:cargo:crate-sha256").is_err());
    let digest = required_property(component, "ignatius:cargo:source-tree-sha256").expect("digest");
    assert_eq!(
        digest,
        "055e2ac8f1370e2aee8f4b9d791de0c6533993801abc150307095fb45930f8db",
    );
    let second = RepositoryPatchFixture::new();
    assert_eq!(
        digest,
        repository_patch::tree_sha256(&second.0).expect("independent checkout"),
    );
    let source = fixture.0.join("vendor/tokio-postgres/src/lib.rs");
    fs::write(&source, "pub fn changed() {}\n").expect("change fixture input");
    assert!(
        verify_binding(
            &Context {
                inputs: &fixture.inputs(),
                ..context
            },
            &bytes
        )
        .is_err(),
    );
}

#[test]
fn repository_patch_metadata_requires_the_exact_locked_local_identity_and_path() {
    let fixture = RepositoryPatchFixture::new();
    let inputs = fixture.inputs();
    let mut metadata = metadata(&fixture.0);
    let driver = json!({
        "id": "opaque-local-driver-id",
        "name": repository_patch::NAME,
        "version": repository_patch::VERSION,
        "source": null,
        "manifest_path": fixture.0.join(repository_patch::MANIFEST),
    });
    metadata["packages"]
        .as_array_mut()
        .expect("metadata packages")
        .push(driver);
    let parse =
        |metadata: &Value| Metadata::parse(&fixture.0, &inputs, metadata.to_string().as_bytes());
    let accepted = parse(&metadata).expect("exact local metadata");
    assert_eq!(
        accepted.resolve("opaque-local-driver-id").expect("driver"),
        PATCHED,
    );
    for (field, wrong) in [
        ("name", json!("unknown-package")),
        ("version", json!("0.7.19")),
        ("source", json!(REGISTRY_SOURCE)),
        ("manifest_path", json!(fixture.0.join("Cargo.toml"))),
        ("manifest_path", json!(repository_patch::MANIFEST)),
    ] {
        let mut mismatched = metadata.clone();
        mismatched["packages"][3][field] = wrong;
        assert!(parse(&mismatched).is_err(), "wrong {field} passed");
    }
    let mut member = metadata;
    member["workspace_members"] = json!(["packaging-id", PATH_ID, "opaque-local-driver-id"]);
    assert!(parse(&member).is_err());
    for wrong in [
        PATCH_LOCK_ENTRY.replace(repository_patch::NAME, "unknown-package"),
        PATCH_LOCK_ENTRY.replace(repository_patch::VERSION, "0.7.19"),
        format!("{PATCH_LOCK_ENTRY}checksum = \"{DIGEST}\"\n"),
    ] {
        assert!(locked_packages(&fixture.0, &(lockfile() + &wrong)).is_err());
    }
}

#[cfg(unix)]
#[test]
fn repository_patch_rejects_symlinks_to_files_and_source_directories() {
    let fixture = RepositoryPatchFixture::new();
    let source = fixture.0.join("vendor/tokio-postgres/src/lib.rs");
    fs::remove_file(&source).expect("replace fixture source");
    std::os::unix::fs::symlink(fixture.0.join("Cargo.toml"), &source).expect("symlink source");
    assert!(repository_patch::tree_sha256(&fixture.0).is_err());
    fs::remove_file(&source).expect("remove fixture link");
    let vendor = fixture.0.join("vendor");
    let original = fixture.0.join("other");
    fs::rename(&vendor, &original).expect("move fixture vendor tree");
    std::os::unix::fs::symlink(&original, &vendor).expect("symlink vendor directory");
    assert!(repository_patch::tree_sha256(&fixture.0).is_err());
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
