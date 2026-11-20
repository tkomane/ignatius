//! Candidate inventory from the Cargo invocations that supplied the archive
//! and its packaging tool. Raw Cargo output stays on the build runner.

use super::{
    RELEASE_TARGETS, hash_archive, is_digest, looks_like_semver, parse_canonical_manifest,
    parse_lockfile, parse_unique_json, parse_unique_json_bytes, read_product_identity,
    resolve_path, write_new_file,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

mod repository_patch;

const MAX_CARGO_INPUT: u64 = 32 * 1024 * 1024;
const MAX_EVIDENCE: u64 = 8 * 1024 * 1024;
const REGISTRY_SOURCE: &str = "registry+https://github.com/rust-lang/crates.io-index";
const PRODUCT: &str = "ignatius";
const PACKAGING: &str = "xtask";
const UNIT: &str = "ignatius:cargo:compilation";
const RELATION: &str = "ignatius:cargo:locked-dependency";
const BINARY: &str = "ignatius:release:binary-sha256";
const PACKAGING_BINARY: &str = "ignatius:release:packaging-binary-sha256";
const INPUT_FILES: [&str; 5] = [
    ".cargo/config.toml",
    "Cargo.toml",
    "Cargo.lock",
    "rust-toolchain.toml",
    "xtask/Cargo.toml",
];

pub(super) fn run(args: &[&str]) -> Result<(), String> {
    let (mode, flags) = Options::parse(args)?;
    let root = super::super::repo_root();
    let (manifest, archive) = load_manifest(flags["--manifest"], flags["--target"])?;
    let inputs = Inputs::load(
        &root,
        &manifest.source_revision,
        &manifest.product_version,
        mode,
    )?;
    let context = Context {
        inputs: &inputs,
        manifest: &manifest,
        archive,
    };
    let result = match mode {
        Mode::Generate | Mode::VerifyBuild => {
            let facts = BuildFacts::capture(&root, &context, &flags)?;
            let expected = render(&context, &facts)?;
            if mode == Mode::Generate {
                let output = resolve_path(Path::new(flags["--out"]));
                let parent = output
                    .parent()
                    .ok_or("candidate SBOM requires an output parent")?;
                if super::path_is_symlink(parent)? || !parent.is_dir() {
                    return Err("candidate SBOM output parent must be a regular directory".into());
                }
                write_new_file(&output, &expected)
            } else {
                let stored =
                    read_bounded(Path::new(flags["--sbom"]), MAX_EVIDENCE, "candidate SBOM")?;
                compare(&stored, &expected)
            }
        }
        Mode::Bind => {
            let stored = read_bounded(Path::new(flags["--sbom"]), MAX_EVIDENCE, "candidate SBOM")?;
            verify_binding(&context, &stored)
        }
    };
    result?;
    println!(
        "Candidate SBOM: {} {}",
        mode.label(),
        context.archive().target
    );
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Mode {
    Generate,
    VerifyBuild,
    Bind,
}

impl Mode {
    fn label(self) -> &'static str {
        match self {
            Self::Generate => "generated",
            Self::VerifyBuild => "matches captured build",
            Self::Bind => "bound to manifest",
        }
    }
}

struct Options;

impl Options {
    fn parse<'a>(args: &'a [&str]) -> Result<(Mode, BTreeMap<&'a str, &'a str>), String> {
        let Some((mode, options)) = args.split_first() else {
            return Err("release sbom candidate requires generate, verify-build or bind".into());
        };
        let mode = match *mode {
            "generate" => Mode::Generate,
            "verify-build" => Mode::VerifyBuild,
            "bind" => Mode::Bind,
            _ => {
                return Err(
                    "release sbom candidate requires generate, verify-build or bind".into(),
                );
            }
        };
        let mut required = vec!["--manifest", "--target"];
        if mode != Mode::Bind {
            required.extend([
                "--metadata",
                "--product-build",
                "--packaging-build",
                "--binary",
                "--rustflags",
                "--policy",
            ]);
        }
        required.push(if mode == Mode::Generate {
            "--out"
        } else {
            "--sbom"
        });
        let mut flags = BTreeMap::new();
        for option in options {
            let (key, value) = option
                .split_once('=')
                .ok_or("candidate SBOM options require --flag=value")?;
            if !required.contains(&key) || value.is_empty() || flags.insert(key, value).is_some() {
                return Err("candidate SBOM has an unexpected, empty or repeated option".into());
            }
        }
        for key in required {
            if !flags.contains_key(key) {
                return Err(format!("candidate SBOM requires {key}"));
            }
        }
        if !RELEASE_TARGETS.contains(&flags["--target"]) {
            return Err("candidate SBOM target is outside the native release matrix".into());
        }
        if mode != Mode::Bind
            && (flags["--policy"] != "pass"
                || flags["--rustflags"] != product_flags(flags["--target"]))
        {
            return Err(
                "candidate SBOM requires passed policy and the approved target flags".into(),
            );
        }
        Ok((mode, flags))
    }
}

fn product_flags(target: &str) -> &'static str {
    if target == "x86_64-pc-windows-msvc" {
        "-D warnings -C link-arg=/Brepro"
    } else {
        "-D warnings"
    }
}

fn load_manifest(path: &str, target: &str) -> Result<(super::ParsedManifest, usize), String> {
    let bytes = read_bounded(Path::new(path), MAX_EVIDENCE, "release manifest")?;
    let manifest = parse_canonical_manifest(&bytes)?;
    let archive = manifest
        .artefacts
        .iter()
        .position(|artefact| artefact.target == target)
        .ok_or("release manifest is missing the candidate target")?;
    Ok((manifest, archive))
}

fn read_bounded(path: &Path, max: u64, label: &str) -> Result<Vec<u8>, String> {
    let path = resolve_path(path);
    let file = fs::symlink_metadata(&path).map_err(|_| format!("cannot read {label}"))?;
    if !file.is_file() || file.len() == 0 || file.len() > max {
        return Err(format!("{label} must be a bounded, nonempty regular file"));
    }
    let mut contents = Vec::new();
    File::open(&path)
        .and_then(|file| file.take(max + 1).read_to_end(&mut contents))
        .map_err(|_| format!("cannot read {label}"))?;
    if contents.len() as u64 != file.len() {
        return Err(format!("{label} changed while it was read"));
    }
    Ok(contents)
}

fn command_output(root: &Path, program: &str, args: &[&str]) -> Result<String, String> {
    let result = Command::new(program)
        .args(args)
        .current_dir(root)
        .output()
        .map_err(|_| format!("could not execute {program} for candidate inventory"))?;
    if !result.status.success() || result.stdout.len() > 65_536 {
        return Err(format!("{program} did not supply bounded candidate facts"));
    }
    String::from_utf8(result.stdout)
        .map_err(|_| format!("{program} returned invalid text for candidate inventory"))
}

fn string(value: &Value, field: &str) -> Result<String, String> {
    value[field]
        .as_str()
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| format!("candidate inventory expects string field {field}"))
}

fn array<'a>(value: &'a Value, field: &str) -> Result<&'a [Value], String> {
    value[field]
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| format!("candidate inventory expects array field {field}"))
}

fn bounded_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_.+".contains(&byte))
}

fn purl(name: &str, version: &str) -> Result<String, String> {
    if !bounded_token(name) || version.len() > 128 || !looks_like_semver(version) {
        return Err("candidate inventory contains an invalid crate identity".into());
    }
    Ok(format!("pkg:cargo/{name}@{}", version.replace('+', "%2B")))
}

fn workspace_manifest(name: &str) -> Option<&'static str> {
    match name {
        PRODUCT => Some("Cargo.toml"),
        PACKAGING => Some("xtask/Cargo.toml"),
        _ => None,
    }
}

#[derive(Debug)]
struct Package {
    name: String,
    version: String,
    source: PackageSource,
}

#[derive(Debug)]
enum PackageSource {
    Registry(String),
    Workspace(&'static str),
    RepositoryPatch(String),
}

#[derive(Debug)]
struct Inputs {
    version: String,
    digests: BTreeMap<&'static str, String>,
    packages: BTreeMap<String, Package>,
}

impl Inputs {
    fn load(
        root: &Path,
        revision: &str,
        product_version: &str,
        mode: Mode,
    ) -> Result<Self, String> {
        let mut status = vec!["--no-optional-locks", "status", "--porcelain"];
        if mode == Mode::Bind {
            status.push("--untracked-files=no");
        }
        if command_output(root, "git", &["rev-parse", "HEAD"])?.trim() != revision
            || !command_output(root, "git", &status)?.trim().is_empty()
        {
            return Err("candidate inventory requires the exact clean source checkout".into());
        }
        let (name, version) = read_product_identity(root)?;
        if name != PRODUCT || version != product_version {
            return Err("release manifest does not match the product manifest".into());
        }
        let mut digests = BTreeMap::new();
        for input in INPUT_FILES {
            let (_, checksum) = hash_archive(&root.join(input))?;
            digests.insert(input, checksum);
        }
        let toolchain = read_bounded(
            &root.join("rust-toolchain.toml"),
            MAX_EVIDENCE,
            "toolchain file",
        )?;
        let toolchain =
            std::str::from_utf8(&toolchain).map_err(|_| "toolchain file is not UTF-8")?;
        let version = toolchain
            .lines()
            .filter_map(|line| line.trim().split_once('='))
            .find_map(|(key, value)| {
                (key.trim() == "channel")
                    .then(|| value.trim().strip_prefix('"')?.strip_suffix('"'))
                    .flatten()
            })
            .filter(|version| looks_like_semver(version))
            .ok_or("candidate inventory requires a pinned Rust version")?
            .to_owned();
        let bytes = read_bounded(&root.join("Cargo.lock"), MAX_EVIDENCE, "Cargo.lock")?;
        let text = std::str::from_utf8(&bytes).map_err(|_| "Cargo.lock is not UTF-8")?;
        let packages = locked_packages(root, text)?;
        Ok(Self {
            version,
            digests,
            packages,
        })
    }

    fn root_ref(&self, name: &str) -> Result<&str, String> {
        self.packages
            .iter()
            .find(|(_, package)| {
                package.name == name && matches!(package.source, PackageSource::Workspace(_))
            })
            .map(|(reference, _)| reference.as_str())
            .ok_or_else(|| format!("lockfile is missing workspace package {name}"))
    }
}

fn locked_packages(root: &Path, text: &str) -> Result<BTreeMap<String, Package>, String> {
    let mut packages = BTreeMap::new();
    for locked in parse_lockfile(text, Path::new("Cargo.lock"))
        .map_err(|_| "Cargo.lock is not a supported locked package inventory")?
    {
        let source = match (locked.source.as_deref(), locked.checksum) {
            (Some(REGISTRY_SOURCE), Some(checksum)) if is_digest(&checksum) => {
                PackageSource::Registry(checksum)
            }
            (None, None)
                if locked.name == repository_patch::NAME
                    && locked.version == repository_patch::VERSION =>
            {
                PackageSource::RepositoryPatch(repository_patch::tree_sha256(root)?)
            }
            (None, None) => PackageSource::Workspace(
                workspace_manifest(&locked.name).ok_or("unexpected local package in Cargo.lock")?,
            ),
            _ => {
                return Err("candidate lockfile contains an unsupported source or checksum".into());
            }
        };
        let reference = purl(&locked.name, &locked.version)?;
        let package = Package {
            name: locked.name,
            version: locked.version,
            source,
        };
        if packages.insert(reference, package).is_some() {
            return Err("candidate lockfile contains an ambiguous component identity".into());
        }
    }
    Ok(packages)
}

struct Context<'a> {
    inputs: &'a Inputs,
    manifest: &'a super::ParsedManifest,
    archive: usize,
}

impl Context<'_> {
    fn archive(&self) -> &super::ManifestArtefact {
        &self.manifest.artefacts[self.archive]
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Relation {
    from: String,
    to: String,
    kind: &'static str,
    target: Option<String>,
}

fn relation_target(raw: &Value) -> Result<Option<String>, String> {
    if raw.is_null() {
        return Ok(None);
    }
    raw.as_str()
        .filter(|value| {
            !value.is_empty()
                && value.len() <= 4096
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"_, -.()=\"".contains(&byte))
        })
        .map(|value| Some(value.to_owned()))
        .ok_or("candidate inventory contains an unsupported dependency target expression".into())
}

#[derive(Debug)]
struct Metadata {
    ids: BTreeMap<String, String>,
    relations: BTreeSet<Relation>,
}

impl Metadata {
    fn resolve(&self, raw_id: &str) -> Result<&str, String> {
        self.ids
            .get(raw_id)
            .map(String::as_str)
            .ok_or("Cargo input references a package absent from locked metadata".into())
    }

    fn parse(root: &Path, inputs: &Inputs, bytes: &[u8]) -> Result<Self, String> {
        let value = parse_unique_json_bytes(bytes).map_err(|_| "invalid Cargo metadata JSON")?;
        if value["version"].as_u64() != Some(1) {
            return Err("candidate inventory requires Cargo metadata format 1".into());
        }
        let mut ids = BTreeMap::new();
        let mut seen = BTreeSet::new();
        for item in array(&value, "packages")? {
            let name = string(item, "name")?;
            let reference = purl(&name, &string(item, "version")?)?;
            let package = inputs
                .packages
                .get(&reference)
                .ok_or("Cargo metadata package is absent from the lockfile")?;
            let source_ok = match &package.source {
                PackageSource::Registry(_) => item["source"].as_str() == Some(REGISTRY_SOURCE),
                PackageSource::Workspace(manifest) => {
                    item.get("source").is_some_and(Value::is_null)
                        && same_path(
                            Path::new(&string(item, "manifest_path")?),
                            &root.join(manifest),
                        )
                }
                PackageSource::RepositoryPatch(_) => {
                    item.get("source").is_some_and(Value::is_null)
                        && repository_patch::matches_manifest(
                            root,
                            Path::new(&string(item, "manifest_path")?),
                        )
                }
            };
            if !source_ok
                || !seen.insert(reference.clone())
                || ids.insert(string(item, "id")?, reference).is_some()
            {
                return Err(
                    "Cargo metadata contains a mismatched or duplicate package identity".into(),
                );
            }
        }
        let mut metadata = Self {
            ids,
            relations: BTreeSet::new(),
        };
        let workspace = array(&value, "workspace_members")?
            .iter()
            .map(|id| metadata.resolve(id.as_str().ok_or("invalid Cargo workspace member")?))
            .collect::<Result<Vec<_>, String>>()?;
        if workspace.len() != 2
            || !workspace.contains(&inputs.root_ref(PRODUCT)?)
            || !workspace.contains(&inputs.root_ref(PACKAGING)?)
        {
            return Err("Cargo metadata does not identify the exact release workspace".into());
        }
        let mut sources = BTreeSet::new();
        for node in array(&value["resolve"], "nodes")? {
            let from = metadata.resolve(&string(node, "id")?)?.to_owned();
            if !sources.insert(from.clone()) {
                return Err("Cargo metadata has a duplicate resolution node".into());
            }
            for dep in array(node, "deps")? {
                let to = metadata.resolve(&string(dep, "pkg")?)?.to_owned();
                for kind in array(dep, "dep_kinds")? {
                    let label = match kind.get("kind") {
                        Some(Value::Null) => "normal",
                        Some(Value::String(value)) if value == "build" => "build",
                        Some(Value::String(value)) if value == "dev" => continue,
                        _ => return Err("Cargo metadata has an unknown dependency kind".into()),
                    };
                    let target =
                        relation_target(kind.get("target").ok_or("missing target expression")?)?;
                    metadata.relations.insert(Relation {
                        from: from.clone(),
                        to: to.clone(),
                        kind: label,
                        target,
                    });
                }
            }
        }
        if !sources.contains(inputs.root_ref(PRODUCT)?)
            || !sources.contains(inputs.root_ref(PACKAGING)?)
        {
            return Err("Cargo metadata resolution is missing a release root".into());
        }
        Ok(metadata)
    }
}

fn same_path(left: &Path, right: &Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
enum Role {
    Product,
    Packaging,
}

impl Role {
    const ALL: [Self; 2] = [Self::Product, Self::Packaging];

    fn label(self) -> &'static str {
        match self {
            Self::Product => "product",
            Self::Packaging => "packaging",
        }
    }

    fn package(self) -> &'static str {
        match self {
            Self::Product => PRODUCT,
            Self::Packaging => PACKAGING,
        }
    }

    fn opt_level(self) -> &'static str {
        match self {
            Self::Product => "3",
            Self::Packaging => "0",
        }
    }
}

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd)]
struct Unit {
    role: Role,
    kinds: BTreeSet<String>,
    features: BTreeSet<String>,
    opt_level: String,
}

impl Unit {
    fn parse(value: &Value, role: Role) -> Result<Self, String> {
        let kinds = string_set(value, "kind")?;
        if kinds.is_empty()
            || !kinds.iter().all(|kind| {
                matches!(
                    kind.as_str(),
                    "bin"
                        | "lib"
                        | "rlib"
                        | "dylib"
                        | "cdylib"
                        | "staticlib"
                        | "proc-macro"
                        | "custom-build"
                )
            })
        {
            return Err("candidate inventory contains an unsupported compiled target kind".into());
        }
        let opt_level = string(value, "opt_level")?;
        if !matches!(opt_level.as_str(), "0" | "1" | "2" | "3" | "s" | "z") {
            return Err("candidate inventory contains an unsupported optimization level".into());
        }
        Ok(Self {
            role,
            kinds,
            features: string_set(value, "features")?,
            opt_level,
        })
    }

    fn value(&self) -> Value {
        json!({
            "features": self.features,
            "kind": self.kinds,
            "opt_level": self.opt_level,
            "role": self.role.label(),
        })
    }

    fn is_binary(&self) -> bool {
        self.kinds.len() == 1 && self.kinds.contains("bin")
    }
}

fn string_set(value: &Value, field: &str) -> Result<BTreeSet<String>, String> {
    let mut set = BTreeSet::new();
    for item in array(value, field)? {
        let token = item
            .as_str()
            .filter(|token| bounded_token(token))
            .ok_or_else(|| format!("candidate inventory has an invalid {field} token"))?;
        if !set.insert(token.to_owned()) {
            return Err(format!("candidate inventory repeats a {field} token"));
        }
    }
    Ok(set)
}

#[derive(Debug, Default)]
struct BuildFacts {
    units: BTreeMap<String, BTreeSet<Unit>>,
    relations: BTreeSet<Relation>,
    binary_sha256: String,
    packaging_sha256: String,
}

impl BuildFacts {
    fn capture(
        root: &Path,
        context: &Context<'_>,
        flags: &BTreeMap<&str, &str>,
    ) -> Result<Self, String> {
        let target = &context.archive().target;
        check_toolchain(root, target, &context.inputs.version)?;
        let bytes = read_bounded(
            Path::new(flags["--metadata"]),
            MAX_CARGO_INPUT,
            "Cargo metadata",
        )?;
        let metadata = Metadata::parse(root, context.inputs, &bytes)?;
        let mut facts = Self::default();
        for (role, flag) in [
            (Role::Product, "--product-build"),
            (Role::Packaging, "--packaging-build"),
        ] {
            let bytes = read_bounded(
                Path::new(flags[flag]),
                MAX_CARGO_INPUT,
                "Cargo build capture",
            )?;
            let executable = facts.read_trace(&metadata, context.inputs, role, &bytes)?;
            let expected_name = if target == "x86_64-pc-windows-msvc" {
                format!("{}.exe", role.package())
            } else {
                role.package().to_owned()
            };
            if executable.file_name().and_then(std::ffi::OsStr::to_str)
                != Some(expected_name.as_str())
            {
                return Err("Cargo named an unexpected release executable".into());
            }
            let (_, digest) = hash_archive(&executable)?;
            match role {
                Role::Product => {
                    if !same_path(&executable, &resolve_path(Path::new(flags["--binary"]))) {
                        return Err(
                            "packaged binary differs from the captured Cargo executable".into()
                        );
                    }
                    check_binary_identity(root, &executable, context)?;
                    facts.binary_sha256 = digest;
                }
                Role::Packaging => facts.packaging_sha256 = digest,
            }
        }
        facts.relations = metadata
            .relations
            .into_iter()
            .filter(|edge| facts.has_shared_role(edge))
            .collect();
        facts.validate(context.inputs)?;
        Ok(facts)
    }

    fn read_trace(
        &mut self,
        metadata: &Metadata,
        inputs: &Inputs,
        role: Role,
        bytes: &[u8],
    ) -> Result<PathBuf, String> {
        let text = std::str::from_utf8(bytes).map_err(|_| "Cargo build capture is not UTF-8")?;
        let root = inputs.root_ref(role.package())?;
        let mut executable = None;
        let mut finished = false;
        for line in text.lines() {
            if finished {
                return Err("Cargo build capture continued after build-finished".into());
            }
            let value = parse_unique_json(line).map_err(|_| "malformed Cargo build message")?;
            let reason = string(&value, "reason")?;
            match reason.as_str() {
                "build-finished" => {
                    if value["success"].as_bool() != Some(true) {
                        return Err("Cargo build did not finish successfully".into());
                    }
                    finished = true;
                }
                "compiler-artifact" => {
                    let reference = metadata.resolve(&string(&value, "package_id")?)?;
                    let profile = &value["profile"];
                    if profile["test"].as_bool() != Some(false)
                        || array(&value, "filenames")?.is_empty()
                    {
                        return Err(
                            "Cargo capture includes a test or an empty compiler artifact".into(),
                        );
                    }
                    let unit = Unit::parse(
                        &json!({
                            "kind": value["target"]["kind"],
                            "features": value["features"],
                            "opt_level": profile["opt_level"],
                        }),
                        role,
                    )?;
                    if unit.is_binary() {
                        let path = PathBuf::from(string(&value, "executable")?);
                        if reference != root
                            || string(&value["target"], "name")? != role.package()
                            || unit.opt_level != role.opt_level()
                            || !path.is_absolute()
                            || executable.replace(path).is_some()
                        {
                            return Err("Cargo capture has an unexpected release executable".into());
                        }
                    }
                    self.units
                        .entry(reference.to_owned())
                        .or_default()
                        .insert(unit);
                }
                "compiler-message" | "build-script-executed" | "future-incompat-report" => {}
                _ => return Err("Cargo build capture contains an unknown message kind".into()),
            }
        }
        if !finished {
            return Err("Cargo build capture is missing build-finished".into());
        }
        executable.ok_or_else(|| {
            format!(
                "Cargo build capture is missing the {} executable",
                role.label()
            )
        })
    }

    fn has_role(&self, reference: &str, role: Role) -> bool {
        self.units
            .get(reference)
            .is_some_and(|units| units.iter().any(|unit| unit.role == role))
    }

    fn has_shared_role(&self, relation: &Relation) -> bool {
        Role::ALL
            .iter()
            .any(|&role| self.has_role(&relation.from, role) && self.has_role(&relation.to, role))
    }

    fn validate(&self, inputs: &Inputs) -> Result<(), String> {
        if !is_digest(&self.binary_sha256) || !is_digest(&self.packaging_sha256) {
            return Err("candidate inventory executable digests are invalid".into());
        }
        for (reference, package) in &inputs.packages {
            if matches!(package.source, PackageSource::RepositoryPatch(_))
                && !self.units.get(reference).is_some_and(|units| {
                    units
                        .iter()
                        .any(|unit| unit.role == Role::Product && unit.kinds.contains("lib"))
                })
            {
                return Err("candidate inventory is missing the compiled repository patch".into());
            }
        }
        for (reference, units) in &self.units {
            if !inputs.packages.contains_key(reference) || units.is_empty() {
                return Err("candidate inventory contains an unrecognised compilation".into());
            }
            for unit in units.iter().filter(|unit| unit.is_binary()) {
                if reference != inputs.root_ref(unit.role.package())?
                    || unit.opt_level != unit.role.opt_level()
                {
                    return Err("candidate inventory contains a misplaced binary".into());
                }
            }
        }
        if self
            .relations
            .iter()
            .any(|relation| !self.has_shared_role(relation) || relation.from == relation.to)
        {
            return Err(
                "candidate inventory contains an inactive or self-referential dependency".into(),
            );
        }
        for role in Role::ALL {
            let root = inputs.root_ref(role.package())?;
            if !self.units.get(root).is_some_and(|units| {
                units
                    .iter()
                    .any(|unit| unit.role == role && unit.is_binary())
            }) {
                return Err("candidate inventory is missing a compiled workspace root".into());
            }
            // Cargo metadata gives a locked package graph, not rustc's unit
            // graph. Still require it to account for every observed input.
            let mut reachable = BTreeSet::from([root]);
            let mut pending = vec![root];
            while let Some(from) = pending.pop() {
                for relation in self
                    .relations
                    .iter()
                    .filter(|relation| relation.from == from)
                {
                    if self.has_role(&relation.to, role) && reachable.insert(&relation.to) {
                        pending.push(&relation.to);
                    }
                }
            }
            if self
                .units
                .keys()
                .any(|key| self.has_role(key, role) && !reachable.contains(key.as_str()))
            {
                return Err(
                    "candidate inventory has a compiled input outside the locked dependency graph"
                        .into(),
                );
            }
        }
        Ok(())
    }
}

fn check_binary_identity(
    root: &Path,
    executable: &Path,
    context: &Context<'_>,
) -> Result<(), String> {
    let version = Command::new(executable)
        .args(["version", "--verbose"])
        .current_dir(root)
        .env("IGNATIUS_LOG", "off")
        .output()
        .map_err(|_| "could not read the captured executable identity")?;
    if !version.status.success() || version.stdout.len() > 65_536 {
        return Err("captured executable did not provide bounded identity output".into());
    }
    let output = std::str::from_utf8(&version.stdout)
        .map_err(|_| "captured executable reported invalid identity text")?;
    let manifest = context.manifest;
    let required = [
        format!("Ignatius {}", manifest.product_version),
        format!("  source revision: {} (clean)", manifest.source_revision),
        format!("  build identity:  {}", manifest.build_identity),
        format!("  target:          {}", context.archive().target),
    ];
    let rustc = format!("  compiler:        rustc {} ", context.inputs.version);
    if !required
        .iter()
        .all(|expected| output.lines().any(|line| line == expected))
        || !output.lines().any(|line| line.starts_with(&rustc))
    {
        return Err("captured executable identity differs from manifest or pinned compiler".into());
    }
    Ok(())
}

fn check_toolchain(root: &Path, target: &str, expected: &str) -> Result<(), String> {
    let rustc = command_output(root, "rustc", &["-vV"])?;
    let cargo = command_output(root, "cargo", &["--version"])?;
    let mut cargo = cargo.split_whitespace();
    if !rustc
        .lines()
        .any(|line| line.strip_prefix("host: ") == Some(target))
        || !rustc
            .lines()
            .any(|line| line.strip_prefix("release: ") == Some(expected))
        || cargo.next() != Some("cargo")
        || cargo.next() != Some(expected)
    {
        return Err(
            "candidate inventory requires the pinned native Cargo and rustc toolchain".into(),
        );
    }
    Ok(())
}

fn property(name: &str, value: impl ToString) -> Value {
    json!({ "name": name, "value": value.to_string() })
}

fn component(
    reference: &str,
    package: &Package,
    facts: &BuildFacts,
    graph: &BTreeMap<&str, Vec<&Relation>>,
) -> Value {
    let source_property = "ignatius:cargo:source";
    let mut properties = match &package.source {
        PackageSource::Registry(checksum) => vec![
            property(source_property, "crates.io"),
            property("ignatius:cargo:crate-sha256", checksum),
        ],
        PackageSource::Workspace(manifest) => vec![property(source_property, manifest)],
        PackageSource::RepositoryPatch(digest) => vec![
            property(source_property, "repository-patch"),
            property("ignatius:cargo:source-manifest", repository_patch::MANIFEST),
            property("ignatius:cargo:source-tree-sha256", digest),
        ],
    };
    if let Some(units) = facts.units.get(reference) {
        properties.extend(units.iter().map(|unit| property(UNIT, unit.value())));
    } else {
        properties.push(property(
            "ignatius:cargo:exclusion",
            "not observed in either candidate build",
        ));
    }
    if let Some(relations) = graph.get(reference) {
        properties.extend(relations.iter().map(|relation| {
            property(
                RELATION,
                json!({
                    "kind": relation.kind,
                    "target": relation.target,
                    "to": relation.to,
                }),
            )
        }));
    }
    let application = matches!(package.source, PackageSource::Workspace(_));
    json!({
        "bom-ref": reference,
        "name": package.name,
        "properties": properties,
        "purl": reference,
        "scope": if facts.units.contains_key(reference) { "required" } else { "excluded" },
        "type": if application { "application" } else { "library" },
        "version": package.version,
    })
}

fn render(context: &Context<'_>, facts: &BuildFacts) -> Result<Vec<u8>, String> {
    let archive = context.archive();
    let inputs = context.inputs;
    let root_ref = inputs.root_ref(PRODUCT)?;
    let mut properties = vec![
        property(
            "ignatius:release:source-revision",
            &context.manifest.source_revision,
        ),
        property("ignatius:release:source-tree", "clean"),
        property(
            "ignatius:release:build-identity",
            &context.manifest.build_identity,
        ),
        property("ignatius:release:archive-name", &archive.name),
        property("ignatius:release:archive-format", &archive.format),
        property("ignatius:release:archive-size-bytes", archive.size_bytes),
        property("ignatius:release:archive-sha256", &archive.checksum),
        property("ignatius:release:target", &archive.target),
        property(BINARY, &facts.binary_sha256),
        property(PACKAGING_BINARY, &facts.packaging_sha256),
        property("ignatius:rust:host", &archive.target),
        property("ignatius:rust:rustc-version", &inputs.version),
        property("ignatius:rust:cargo-version", &inputs.version),
        property("ignatius:cargo:metadata-format", "1"),
        property("ignatius:cargo:product-profile", "release"),
        property(
            "ignatius:cargo:product-rustflags",
            product_flags(&archive.target),
        ),
        property("ignatius:cargo:packaging-profile", "dev"),
        property("ignatius:cargo:packaging-rustflags", "-D warnings"),
        property(
            "ignatius:cargo:relationship-scope",
            "locked package relations for observed inputs",
        ),
        property("ignatius:policy:lockfile", "verified"),
        property("ignatius:policy:advisories", "pass"),
        property("ignatius:policy:licenses", "pass"),
        property("ignatius:policy:bans", "pass"),
        property("ignatius:policy:sources", "pass"),
    ];
    for (input, digest) in &inputs.digests {
        properties.push(property(
            &format!("ignatius:release:input-sha256:{input}"),
            digest,
        ));
    }
    let mut graph = BTreeMap::<&str, Vec<&Relation>>::new();
    for relation in &facts.relations {
        graph.entry(&relation.from).or_default().push(relation);
    }
    let dependencies = facts
        .units
        .keys()
        .map(|reference| {
            let names = graph
                .get(reference.as_str())
                .into_iter()
                .flatten()
                .map(|relation| &relation.to)
                .collect::<BTreeSet<_>>();
            json!({ "ref": reference, "dependsOn": names })
        })
        .collect::<Vec<_>>();
    let root = component(root_ref, &inputs.packages[root_ref], facts, &graph);
    let components = inputs
        .packages
        .iter()
        .filter(|(reference, _)| reference.as_str() != root_ref)
        .map(|(reference, package)| component(reference, package, facts, &graph))
        .collect::<Vec<_>>();
    let document = json!({
        "bomFormat": "CycloneDX",
        "specVersion": "1.5",
        "version": 1,
        "metadata": {
            "component": root,
            "lifecycles": [{ "phase": "build" }],
            "properties": properties,
        },
        "components": components,
        "dependencies": dependencies,
    });
    let mut bytes =
        serde_json::to_vec_pretty(&document).map_err(|_| "could not serialize candidate SBOM")?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn required_property(value: &Value, name: &str) -> Result<String, String> {
    let matches = array(value, "properties")?
        .iter()
        .filter(|property| property["name"].as_str() == Some(name))
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err("candidate SBOM has a missing or repeated metadata property".into());
    }
    string(matches[0], "value")
}

fn verify_binding(context: &Context<'_>, stored: &[u8]) -> Result<(), String> {
    let value = parse_unique_json_bytes(stored).map_err(|_| "invalid candidate SBOM JSON")?;
    let metadata = &value["metadata"];
    let mut facts = BuildFacts {
        binary_sha256: required_property(metadata, BINARY)?,
        packaging_sha256: required_property(metadata, PACKAGING_BINARY)?,
        ..BuildFacts::default()
    };
    for component in array(&value, "components")?
        .iter()
        .chain(std::iter::once(&metadata["component"]))
    {
        let reference = string(component, "bom-ref")?;
        if !context.inputs.packages.contains_key(&reference) {
            return Err("candidate SBOM refers to a package absent from Cargo.lock".into());
        }
        for property in array(component, "properties")? {
            match property["name"].as_str() {
                Some(UNIT) => {
                    let unit = parse_unique_json(&string(property, "value")?)
                        .map_err(|_| "invalid retained compilation record")?;
                    let role = match unit["role"].as_str() {
                        Some("product") => Role::Product,
                        Some("packaging") => Role::Packaging,
                        _ => return Err("candidate SBOM has an invalid compilation role".into()),
                    };
                    if !facts
                        .units
                        .entry(reference.clone())
                        .or_default()
                        .insert(Unit::parse(&unit, role)?)
                    {
                        return Err("candidate SBOM repeats a compilation".into());
                    }
                }
                Some(RELATION) => {
                    let relation = parse_unique_json(&string(property, "value")?)
                        .map_err(|_| "invalid retained dependency relation")?;
                    let kind = match relation["kind"].as_str() {
                        Some("normal") => "normal",
                        Some("build") => "build",
                        _ => return Err("candidate SBOM has an invalid dependency kind".into()),
                    };
                    let target = relation_target(
                        relation.get("target").ok_or("missing target expression")?,
                    )?;
                    let to = string(&relation, "to")?;
                    if !context.inputs.packages.contains_key(&to)
                        || !facts.relations.insert(Relation {
                            from: reference.clone(),
                            to,
                            kind,
                            target,
                        })
                    {
                        return Err("candidate SBOM has an unknown or repeated dependency".into());
                    }
                }
                _ => {}
            }
        }
    }
    facts.validate(context.inputs)?;
    compare(stored, &render(context, &facts)?)
}

fn compare(stored: &[u8], expected: &[u8]) -> Result<(), String> {
    if stored == expected {
        return Ok(());
    }
    let offset = stored
        .iter()
        .zip(expected)
        .position(|(left, right)| left != right)
        .unwrap_or(stored.len().min(expected.len()));
    Err(format!("candidate SBOM does not match at byte {offset}"))
}

#[cfg(test)]
mod tests;
