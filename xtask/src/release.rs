//! Semantic checks for the source-controlled release catalogue.
//!
//! This module deliberately does not build, sign, upload or publish an
//! artefact. It checks that a release record is internally consistent and that
//! a record claiming readiness has the evidence required by Feature 008.

use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};

const RELEASE_EVIDENCE_PREFIX: &str = "release-evidence/";
const RELEASE_TARGETS: [&str; 3] = [
    "aarch64-apple-darwin",
    "x86_64-pc-windows-msvc",
    "x86_64-unknown-linux-gnu",
];

const JSON_NUMBER_TOKEN: &str = "$serde_json::private::Number";

struct UniqueJson(Value);

impl<'de> Deserialize<'de> for UniqueJson {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct UniqueJsonVisitor;

        impl<'de> Visitor<'de> for UniqueJsonVisitor {
            type Value = Value;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a JSON value without duplicate object keys")
            }

            fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(Value::Bool(value))
            }

            fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(Value::Number(Number::from(value)))
            }

            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(Value::Number(Number::from(value)))
            }

            fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Number::from_f64(value)
                    .map(Value::Number)
                    .ok_or_else(|| E::custom("JSON number is not finite"))
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(Value::String(value.to_owned()))
            }

            fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(Value::String(value))
            }

            fn visit_none<E>(self) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(Value::Null)
            }

            fn visit_unit<E>(self) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(Value::Null)
            }

            fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
            where
                D: Deserializer<'de>,
            {
                UniqueJson::deserialize(deserializer).map(|value| value.0)
            }

            fn visit_seq<A>(self, mut access: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let mut values = Vec::new();
                while let Some(value) = access.next_element::<UniqueJson>()? {
                    values.push(value.0);
                }
                Ok(Value::Array(values))
            }

            fn visit_map<A>(self, mut access: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut object = Map::new();
                while let Some(key) = access.next_key::<String>()? {
                    if key == JSON_NUMBER_TOKEN {
                        let raw = access.next_value::<String>()?;
                        let number = serde_json::from_str::<Number>(&raw)
                            .map_err(|error| de::Error::custom(error.to_string()))?;
                        if access.next_key::<String>()?.is_some() {
                            return Err(de::Error::custom(
                                "arbitrary-precision JSON number carried extra fields",
                            ));
                        }
                        return Ok(Value::Number(number));
                    }
                    let value = access.next_value::<UniqueJson>()?;
                    if object.insert(key.clone(), value.0).is_some() {
                        return Err(de::Error::custom(format!(
                            "duplicate JSON object key: {key}"
                        )));
                    }
                }
                Ok(Value::Object(object))
            }
        }

        deserializer
            .deserialize_any(UniqueJsonVisitor)
            .map(UniqueJson)
    }
}

fn parse_unique_json(text: &str) -> Result<Value, String> {
    let mut deserializer = serde_json::Deserializer::from_str(text);
    let value = UniqueJson::deserialize(&mut deserializer)
        .map_err(|error| error.to_string())?
        .0;
    deserializer.end().map_err(|error| error.to_string())?;
    Ok(value)
}

fn parse_unique_json_bytes(bytes: &[u8]) -> Result<Value, String> {
    let text = std::str::from_utf8(bytes).map_err(|error| error.to_string())?;
    parse_unique_json(text)
}

fn json_u64(value: &Value) -> Option<u64> {
    let number = value.as_number()?;
    number
        .as_u64()
        .or_else(|| parse_json_integer(number.as_str()))
}

fn parse_json_integer(text: &str) -> Option<u64> {
    let exponent_index = text.find("e").or_else(|| text.find("E"));
    let exponent_text = exponent_index.map(|index| &text[index + 1..]);
    let significand = exponent_index.map_or(text, |index| &text[..index]);
    if significand.starts_with("-") || significand.is_empty() {
        return None;
    }
    let (whole, fraction) = significand
        .split_once(".")
        .map_or((significand, ""), |parts| parts);
    if whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    let mut digits = String::with_capacity(whole.len() + fraction.len());
    digits.push_str(whole);
    digits.push_str(fraction);
    let Some(first_nonzero) = digits.bytes().position(|byte| byte != 48) else {
        return Some(0);
    };
    let digits = &digits[first_nonzero..];
    let exponent = exponent_text.map_or(Some(0), |value| value.parse::<i64>().ok())?;
    let fraction_length = i64::try_from(fraction.len()).ok()?;
    let shift = exponent.checked_sub(fraction_length)?;
    if shift >= 0 {
        let zero_count = usize::try_from(shift).ok()?;
        if digits.len().checked_add(zero_count)? > 20 {
            return None;
        }
        let mut value = parse_json_digits(digits)?;
        for _ in 0..zero_count {
            value = value.checked_mul(10)?;
        }
        return Some(value);
    }
    let remove = usize::try_from(shift.checked_neg()?).ok()?;
    if remove > digits.len() {
        return None;
    }
    let split = digits.len() - remove;
    if digits[split..].bytes().any(|byte| byte != 48) {
        return None;
    }
    parse_json_digits(&digits[..split])
}

fn parse_json_digits(text: &str) -> Option<u64> {
    text.bytes().try_fold(0_u64, |value, byte| {
        let digit = byte.checked_sub(48)?;
        if digit > 9 {
            return None;
        }
        value.checked_mul(10)?.checked_add(u64::from(digit))
    })
}

#[derive(Debug)]
struct ArtefactFacts {
    name: String,
    target: String,
    format: String,
    size_bytes: u64,
    checksum_algorithm: String,
    checksum_status: String,
    checksum: Option<String>,
    source_revision: String,
    build_identity: String,
    signature_status: String,
    provenance_status: String,
}

/// Checks a non-publishing release catalogue and returns a failure when it is
/// not ready for the requested release action.
pub fn check(path: &Path) -> Result<(), String> {
    let (value, evidence_root) = read_catalogue(path)?;
    let mut blockers = Vec::new();
    validate_catalogue(&value, &evidence_root, &mut blockers, true);

    if blockers.is_empty() {
        println!("Release readiness: ready");
        Ok(())
    } else {
        println!("Release readiness: blocked");
        for blocker in &blockers {
            println!("  - {blocker}");
        }
        Err(format!(
            "release readiness blocked by {} issue(s)",
            blockers.len()
        ))
    }
}

/// Validates the release-record contract without requiring a publishable state.
/// Candidate and blocked records remain visibly non-ready; they are not treated
/// as published merely because their JSON is internally consistent.
pub fn validate(path: &Path) -> Result<(), String> {
    let (value, evidence_root) = read_catalogue(path)?;
    let mut blockers = Vec::new();
    validate_catalogue(&value, &evidence_root, &mut blockers, false);

    let status = value
        .get("records")
        .and_then(Value::as_array)
        .and_then(|records| records.first())
        .and_then(Value::as_object)
        .and_then(|record| record.get("status"))
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let statuses = value
        .get("records")
        .and_then(Value::as_array)
        .map(|records| {
            records
                .iter()
                .map(|record| {
                    record
                        .as_object()
                        .and_then(|record| record.get("status"))
                        .and_then(Value::as_str)
                        .unwrap_or("unknown")
                        .to_owned()
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let unexpected = blockers
        .iter()
        .filter(|blocker| !is_expected_non_ready_blocker(&statuses, blocker))
        .collect::<Vec<_>>();

    if unexpected.is_empty() && matches!(status, "candidate" | "blocked") {
        println!("Release record: valid ({status})");
        Ok(())
    } else {
        println!("Release record: invalid");
        for blocker in &unexpected {
            println!("  - {blocker}");
        }
        Err(format!(
            "release record validation found {} issue(s)",
            unexpected.len()
        ))
    }
}

/// Checks that a candidate staging root contains exactly the explicitly
/// allowlisted regular files. This is a local preparation contract for the
/// later CI evidence-scope gate; it does not upload, scan or publish anything.
pub fn evidence_scope(args: &[&str]) -> Result<(), String> {
    let options = EvidenceScopeOptions::parse(args)?;
    let root = resolve_path(&options.root);
    let allowlist = resolve_path(&options.allowlist);

    ensure_scope_root(&root)?;
    if allowlist.starts_with(&root) {
        return Err("evidence scope allowlist must be outside the staging root".to_owned());
    }
    if path_has_symlink_in_path(&allowlist)? {
        return Err("evidence scope allowlist must not use a symlink".to_owned());
    }
    let expected = read_scope_allowlist(&allowlist)?;
    let mut observed = BTreeSet::new();
    collect_scope_files(&root, &root, &mut observed)?;

    let mut differences = Vec::new();
    for path in expected.difference(&observed) {
        differences.push(format!("allowlisted file is missing: {path}"));
    }
    for path in observed.difference(&expected) {
        differences.push(format!("file is not allowlisted: {path}"));
    }
    if !differences.is_empty() {
        return Err(format!(
            "evidence scope blocked:\n  - {}",
            differences.join("\n  - ")
        ));
    }

    println!("Evidence scope: verified {} file(s)", observed.len());
    Ok(())
}

#[derive(Debug)]
struct EvidenceScopeOptions {
    root: PathBuf,
    allowlist: PathBuf,
}

impl EvidenceScopeOptions {
    fn parse(args: &[&str]) -> Result<Self, String> {
        let values =
            parse_named_options(args, &["--root", "--allowlist"], "release evidence-scope")?;
        Ok(Self {
            root: required_option(&values, "--root")?,
            allowlist: required_option(&values, "--allowlist")?,
        })
    }
}

fn read_scope_allowlist(path: &Path) -> Result<BTreeSet<String>, String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("could not read evidence scope allowlist: {error}"))?;
    let value = parse_unique_json(&text)
        .map_err(|error| format!("evidence scope allowlist is not valid JSON: {error}"))?;
    let object = value
        .as_object()
        .ok_or_else(|| "evidence scope allowlist must be an object".to_owned())?;
    for key in object.keys() {
        if !matches!(key.as_str(), "schema_version" | "files") {
            return Err(format!(
                "evidence scope allowlist field is not recognised: {key}"
            ));
        }
    }
    if object.get("schema_version").and_then(Value::as_u64) != Some(1) {
        return Err("evidence scope allowlist schema_version must be 1".to_owned());
    }
    let values = object
        .get("files")
        .and_then(Value::as_array)
        .ok_or_else(|| "evidence scope allowlist files must be an array".to_owned())?;
    if values.is_empty() {
        return Err("evidence scope allowlist files must not be empty".to_owned());
    }
    let mut files = BTreeSet::new();
    for (index, value) in values.iter().enumerate() {
        let file = value
            .as_str()
            .ok_or_else(|| format!("evidence scope allowlist files[{index}] must be text"))?;
        let file = normalise_scope_path(file, &format!("files[{index}]"))?;
        if !files.insert(file.clone()) {
            return Err(format!(
                "evidence scope allowlist contains a duplicate path: {file}"
            ));
        }
    }
    Ok(files)
}

fn ensure_scope_root(root: &Path) -> Result<(), String> {
    if root
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err("evidence scope root must not contain path traversal".to_owned());
    }
    if path_has_symlink_in_path(root)? {
        return Err("evidence scope root must not use a symlink".to_owned());
    }
    let metadata = fs::symlink_metadata(root)
        .map_err(|error| format!("could not inspect evidence scope root: {error}"))?;
    if !metadata.is_dir() {
        return Err("evidence scope root must be a directory".to_owned());
    }
    Ok(())
}

fn collect_scope_files(
    root: &Path,
    directory: &Path,
    files: &mut BTreeSet<String>,
) -> Result<(), String> {
    let entries = fs::read_dir(directory)
        .map_err(|error| format!("could not read evidence scope directory: {error}"))?;
    for entry in entries {
        let entry =
            entry.map_err(|error| format!("could not inspect evidence scope entry: {error}"))?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| format!("could not inspect {}: {error}", path.display()))?;
        let relative = path
            .strip_prefix(root)
            .map_err(|error| format!("evidence scope path escaped its root: {error}"))?;
        let relative = scope_relative_name(relative)?;
        if metadata.file_type().is_symlink() {
            return Err(format!("evidence scope contains a symlink: {relative}"));
        }
        if metadata.is_dir() {
            collect_scope_files(root, &path, files)?;
        } else if metadata.is_file() {
            files.insert(relative);
        } else {
            return Err(format!(
                "evidence scope contains a non-regular file: {relative}"
            ));
        }
    }
    Ok(())
}

fn scope_relative_name(path: &Path) -> Result<String, String> {
    let mut components = Vec::new();
    for component in path.components() {
        let Component::Normal(component) = component else {
            return Err("evidence scope path contains an invalid component".to_owned());
        };
        let component = component
            .to_str()
            .ok_or_else(|| "evidence scope path is not valid UTF-8".to_owned())?;
        components.push(component);
    }
    normalise_scope_path(&components.join("/"), "observed path")
}

fn normalise_scope_path(value: &str, label: &str) -> Result<String, String> {
    if value.is_empty()
        || value.starts_with('/')
        || value.contains('\\')
        || value.contains("://")
        || value.contains(':')
        || value.chars().any(char::is_control)
    {
        return Err(format!(
            "evidence scope {label} must be a relative portable path"
        ));
    }
    let components = value.split('/').collect::<Vec<_>>();
    if components
        .iter()
        .any(|component| component.is_empty() || matches!(*component, "." | ".."))
    {
        return Err(format!(
            "evidence scope {label} must not contain empty, dot or parent components"
        ));
    }
    if !value
        .chars()
        .next()
        .is_some_and(|character| character.is_ascii_alphanumeric())
    {
        return Err(format!(
            "evidence scope {label} must be a relative portable path"
        ));
    }
    if !value.chars().all(|character| {
        character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '+' | '-' | '/')
    }) {
        return Err(format!(
            "evidence scope {label} contains an unsupported path character"
        ));
    }
    Ok(value.to_owned())
}

fn path_has_symlink_in_path(path: &Path) -> Result<bool, String> {
    let mut current = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => current.push(prefix.as_os_str()),
            Component::RootDir => current.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => return Ok(true),
            Component::Normal(name) => {
                current.push(name);
                match fs::symlink_metadata(&current) {
                    Ok(metadata) if metadata.file_type().is_symlink() => return Ok(true),
                    Ok(_) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
                    Err(error) => {
                        return Err(format!("could not inspect {}: {error}", current.display()));
                    }
                }
            }
        }
    }
    Ok(false)
}

fn is_expected_non_ready_blocker(statuses: &[String], blocker: &str) -> bool {
    let Some(record_index) = blocker
        .strip_prefix("records[")
        .and_then(|value| value.split_once(']'))
        .and_then(|(index, _)| index.parse::<usize>().ok())
    else {
        return false;
    };
    let Some(status) = statuses.get(record_index).map(String::as_str) else {
        return false;
    };
    matches!(status, "candidate" | "blocked")
        && (blocker.ends_with(" has no checksum")
            || blocker.contains(".evidence_reference.path is not yet materialised"))
}

/// Generates one non-publishing schema-v1 candidate record from explicit
/// target and artefact facts plus identity captured from this checkout.
pub fn generate(args: &[&str]) -> Result<(), String> {
    let options = GenerateOptions::parse(args)?;
    let root = super::repo_root();
    let product_version = read_product_version(&root)?;
    let (source_revision, source_state) = source_identity(&root)?;
    let release_tag = exact_release_tag(&root);

    if !is_safe_identity(&options.build_identity) {
        return Err("--build-identity must use non-empty release identity text".to_owned());
    }
    if !is_safe_target(&options.target) {
        return Err(
            "--target must be a non-empty target triple without whitespace or path separators"
                .to_owned(),
        );
    }
    if !is_evidence_reference(&options.evidence_path) {
        return Err("--evidence-path must stay under release-evidence/".to_owned());
    }
    if options.notes_entry.trim().is_empty() {
        return Err("--notes-entry must not be empty".to_owned());
    }
    let expected_name = artefact_name(&product_version, &options.target, &options.format)
        .ok_or_else(|| "--artefact-format must be tar.gz, zip or binary".to_owned())?;
    if options.artefact_name != expected_name {
        return Err(format!(
            "--artefact-name must be {expected_name} for this version, target and format"
        ));
    }
    if options.size_bytes == 0 {
        return Err("--artefact-size must be greater than zero".to_owned());
    }
    if let Some(checksum) = &options.checksum
        && !is_digest(checksum)
    {
        return Err("--checksum must be a lower-case SHA-256 digest".to_owned());
    }
    if options.checksum.is_some() && options.artefact_path.is_none() {
        return Err("--checksum requires --artefact-path so bytes can be observed".to_owned());
    }
    let observed_checksum = if let Some(path) = &options.artefact_path {
        let path = resolve_path(path);
        let actual_name = path.file_name().and_then(|name| name.to_str());
        if actual_name != Some(options.artefact_name.as_str()) {
            return Err(format!(
                "--artefact-path must name {}",
                options.artefact_name
            ));
        }
        let (actual_size, checksum) = hash_archive(&path)?;
        if actual_size != options.size_bytes {
            return Err(format!(
                "--artefact-size is {}, but the exact archive is {actual_size} bytes",
                options.size_bytes
            ));
        }
        if let Some(declared) = &options.checksum
            && declared != &checksum
        {
            return Err("--checksum does not match the exact archive bytes".to_owned());
        }
        Some(checksum)
    } else {
        None
    };
    let platform = platform_for_target(&options.target)
        .ok_or_else(|| "--target must identify macos, windows or linux".to_owned())?;
    let architecture = architecture_for_target(&options.target);

    let mut blockers = Vec::new();
    if source_state != "clean" {
        blockers.push(format!("source state is {source_state}"));
    }
    if !release_tag
        .as_deref()
        .is_some_and(|tag| tag_matches_version(tag, &product_version))
    {
        blockers.push("source revision is not tagged for this product version".to_owned());
    }
    if observed_checksum.is_none() {
        blockers.push("checksum has not been observed from archive bytes".to_owned());
    }
    let status = if blockers.is_empty() {
        "candidate"
    } else {
        "blocked"
    };
    let mut record = json!({
        "schema_version": 1,
        "product_version": product_version,
        "source_revision": source_revision,
        "source_state": source_state,
        "build_identity": options.build_identity,
        "target_rows": [{
            "target": options.target,
            "platform": platform,
            "architecture": architecture,
            "coverage": "covered",
            "artefact_name": options.artefact_name
        }],
        "artefacts": [{
            "name": options.artefact_name,
            "target": options.target,
            "format": options.format,
            "size_bytes": options.size_bytes,
            "checksum_algorithm": "sha256",
            "checksum_status": if observed_checksum.is_some() { "present" } else { "missing" },
            "checksum": observed_checksum,
            "source_revision": source_revision,
            "build_identity": options.build_identity,
            "signature_status": "not-configured",
            "provenance_status": "not-configured"
        }],
        "notes_reference": {
            "path": "CHANGELOG.md",
            "entry": options.notes_entry
        },
        "evidence_reference": {
            "path": options.evidence_path,
            "product_version": product_version,
            "source_revision": source_revision,
            "status": "incomplete"
        },
        "status": status,
        "state_history": if status == "blocked" {
            json!([
                {"from": "draft", "to": "candidate"},
                {"from": "candidate", "to": "blocked"}
            ])
        } else {
            json!([{"from": "draft", "to": "candidate"}])
        }
    });
    if !blockers.is_empty() {
        record["blockers"] = json!(blockers);
    }

    let document = json!({
        "schema_version": 1,
        "records": [record]
    });
    let output = resolve_path(&options.output);
    let parent = output
        .parent()
        .ok_or_else(|| "--output must name a file".to_owned())?;
    if !parent.is_dir() {
        return Err(format!(
            "output directory does not exist: {}",
            parent.display()
        ));
    }
    if path_is_symlink(parent)? {
        return Err(format!(
            "output file parent must not use a symlink: {}",
            parent.display()
        ));
    }
    let bytes = serde_json::to_vec_pretty(&document)
        .map_err(|error| format!("could not serialize release record: {error}"))?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output)
        .map_err(|error| {
            format!(
                "could not create {} without overwriting: {error}",
                output.display()
            )
        })?;
    if let Err(error) = file.write_all(&bytes) {
        drop(file);
        let _ = fs::remove_file(&output);
        return Err(format!("could not write {}: {error}", output.display()));
    }
    if let Err(error) = file.write_all(b"\n") {
        drop(file);
        let _ = fs::remove_file(&output);
        return Err(format!("could not finish {}: {error}", output.display()));
    }
    if let Err(error) = file.sync_all() {
        drop(file);
        let _ = fs::remove_file(&output);
        return Err(format!("could not sync {}: {error}", output.display()));
    }

    println!(
        "Release record: generated ({status}) at {}",
        output.display()
    );
    Ok(())
}

/// Generates the canonical checksum sidecars for the exact artefacts named by
/// one release record. This is deliberately a local, non-publishing operation.
pub fn manifest_generate(args: &[&str]) -> Result<(), String> {
    let options = ManifestGenerateOptions::parse(args)?;
    let identity = load_release_identity(&options.record)?;
    let artefact_dir = resolve_path(&options.artefact_dir);
    let output_dir = resolve_path(&options.output_dir);
    if path_is_symlink(&artefact_dir)? {
        return Err(format!(
            "artefact directory must not be a symlink: {}",
            artefact_dir.display()
        ));
    }
    if !artefact_dir.is_dir() {
        return Err(format!(
            "artefact directory does not exist: {}",
            artefact_dir.display()
        ));
    }
    let output_parent = output_dir
        .parent()
        .ok_or_else(|| "output directory must have a parent".to_owned())?;
    if !output_parent.is_dir() {
        return Err(format!(
            "output parent directory does not exist: {}",
            output_parent.display()
        ));
    }
    if path_is_symlink(output_parent)? {
        return Err(format!(
            "output directory parent must not use a symlink: {}",
            output_parent.display()
        ));
    }
    match fs::symlink_metadata(&output_dir) {
        Ok(_) => {
            return Err(format!(
                "output directory already exists without overwriting: {}",
                output_dir.display()
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(format!(
                "could not inspect output directory {}: {error}",
                output_dir.display()
            ));
        }
    }

    let mut artefacts = Vec::with_capacity(identity.artefacts.len());
    for expected in &identity.artefacts {
        let archive = checked_child_path(&artefact_dir, &expected.name, "artefact")?;
        let (size_bytes, checksum) = hash_archive(&archive)?;
        if size_bytes != expected.size_bytes {
            return Err(format!(
                "{} size is {size_bytes}, but the release record declares {}",
                expected.name, expected.size_bytes
            ));
        }
        if expected.checksum_status == "present"
            && expected.checksum.as_deref() != Some(checksum.as_str())
        {
            return Err(format!(
                "{} bytes do not match the checksum in the release record",
                expected.name
            ));
        }
        artefacts.push(ManifestArtefact {
            name: expected.name.clone(),
            target: expected.target.clone(),
            format: expected.format.clone(),
            size_bytes,
            checksum,
        });
    }
    sort_manifest_artefacts(&mut artefacts);

    let manifest = canonical_manifest_bytes(
        &identity.product_version,
        &identity.source_revision,
        &identity.build_identity,
        &artefacts,
    );
    let checksums = canonical_checksum_bytes(&artefacts);
    let output_name = output_dir
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty() && *name != "." && *name != "..")
        .ok_or_else(|| "output directory must have a usable final name".to_owned())?;
    let staging_dir = output_parent.join(format!(".{output_name}.tmp.{}", std::process::id()));
    fs::create_dir(&staging_dir).map_err(|error| {
        format!(
            "could not create temporary release evidence directory {}: {error}",
            staging_dir.display()
        )
    })?;
    let result = (|| {
        write_new_file(&staging_dir.join("release-manifest.json"), &manifest)?;
        write_new_file(&staging_dir.join("SHA256SUMS"), &checksums)?;
        if let Ok(directory) = File::open(&staging_dir) {
            let _ = directory.sync_all();
        }
        fs::rename(&staging_dir, &output_dir).map_err(|error| {
            format!(
                "could not publish the new release evidence directory without overwriting: {error}"
            )
        })
    })();
    if let Err(error) = result {
        let _ = fs::remove_dir_all(&staging_dir);
        return Err(error);
    }

    println!(
        "Release manifest: generated {} artefact(s) in {}",
        artefacts.len(),
        output_dir.display()
    );
    Ok(())
}

/// Verifies canonical checksum sidecars, release-record bindings and the
/// exact archive bytes before any later extraction or publication step.
pub fn manifest_verify(args: &[&str]) -> Result<(), String> {
    let options = ManifestVerifyOptions::parse(args)?;
    let identity = load_release_identity(&options.record)?;
    let manifest_path = resolve_path(&options.manifest);
    let checksums_path = resolve_path(&options.checksums);
    if path_is_symlink(&manifest_path)? {
        return Err(format!(
            "release manifest must not be a symlink: {}",
            manifest_path.display()
        ));
    }
    if path_is_symlink(&checksums_path)? {
        return Err(format!(
            "checksum sidecar must not be a symlink: {}",
            checksums_path.display()
        ));
    }
    let manifest_bytes = fs::read(&manifest_path)
        .map_err(|error| format!("could not read {}: {error}", manifest_path.display()))?;
    let manifest = parse_canonical_manifest(&manifest_bytes)?;
    let checksum_bytes = fs::read(&checksums_path)
        .map_err(|error| format!("could not read {}: {error}", checksums_path.display()))?;
    let checksum_lines = parse_checksum_list(&checksum_bytes)?;

    if manifest.product_version != identity.product_version {
        return Err(
            "release-manifest.json product_version does not match the release record".to_owned(),
        );
    }
    if manifest.source_revision != identity.source_revision {
        return Err(
            "release-manifest.json source_revision does not match the release record".to_owned(),
        );
    }
    if manifest.build_identity != identity.build_identity {
        return Err(
            "release-manifest.json build_identity does not match the release record".to_owned(),
        );
    }

    let mut expected = Vec::with_capacity(identity.artefacts.len());
    for artefact in &identity.artefacts {
        if artefact.checksum_status != "present" {
            return Err(format!(
                "{} has no present checksum in the release record",
                artefact.name
            ));
        }
        let checksum = artefact
            .checksum
            .clone()
            .ok_or_else(|| format!("{} has no checksum in the release record", artefact.name))?;
        expected.push(ManifestArtefact {
            name: artefact.name.clone(),
            target: artefact.target.clone(),
            format: artefact.format.clone(),
            size_bytes: artefact.size_bytes,
            checksum,
        });
    }
    sort_manifest_artefacts(&mut expected);
    if manifest.artefacts != expected {
        return Err("release-manifest.json artefacts do not match the release record".to_owned());
    }

    let expected_lines = expected
        .iter()
        .map(|artefact| (artefact.checksum.clone(), artefact.name.clone()))
        .collect::<Vec<_>>();
    if checksum_lines != expected_lines {
        return Err(
            "SHA256SUMS does not match release-manifest.json and the release record".to_owned(),
        );
    }

    let artefact_dir = resolve_path(&options.artefact_dir);
    if path_is_symlink(&artefact_dir)? {
        return Err(format!(
            "artefact directory must not be a symlink: {}",
            artefact_dir.display()
        ));
    }
    if !artefact_dir.is_dir() {
        return Err(format!(
            "artefact directory does not exist: {}",
            artefact_dir.display()
        ));
    }
    for artefact in &manifest.artefacts {
        let archive = checked_child_path(&artefact_dir, &artefact.name, "artefact")?;
        let (size_bytes, checksum) = hash_archive(&archive)?;
        if size_bytes != artefact.size_bytes {
            return Err(format!(
                "{} size is {size_bytes}, but the manifest declares {}",
                artefact.name, artefact.size_bytes
            ));
        }
        if checksum != artefact.checksum {
            return Err(format!(
                "{} checksum does not match the exact archive bytes",
                artefact.name
            ));
        }
    }

    println!(
        "Release manifest: verified {} artefact(s)",
        manifest.artefacts.len()
    );
    Ok(())
}

#[derive(Debug)]
struct ManifestGenerateOptions {
    record: PathBuf,
    artefact_dir: PathBuf,
    output_dir: PathBuf,
}

impl ManifestGenerateOptions {
    fn parse(args: &[&str]) -> Result<Self, String> {
        let values = parse_named_options(
            args,
            &["--record", "--artefact-dir", "--output-dir"],
            "release manifest generate",
        )?;
        Ok(Self {
            record: required_option(&values, "--record")?,
            artefact_dir: required_option(&values, "--artefact-dir")?,
            output_dir: required_option(&values, "--output-dir")?,
        })
    }
}

#[derive(Debug)]
struct ManifestVerifyOptions {
    record: PathBuf,
    artefact_dir: PathBuf,
    manifest: PathBuf,
    checksums: PathBuf,
}

impl ManifestVerifyOptions {
    fn parse(args: &[&str]) -> Result<Self, String> {
        let values = parse_named_options(
            args,
            &["--record", "--artefact-dir", "--manifest", "--checksums"],
            "release manifest verify",
        )?;
        Ok(Self {
            record: required_option(&values, "--record")?,
            artefact_dir: required_option(&values, "--artefact-dir")?,
            manifest: required_option(&values, "--manifest")?,
            checksums: required_option(&values, "--checksums")?,
        })
    }
}

fn parse_named_options(
    args: &[&str],
    allowed: &[&str],
    command: &str,
) -> Result<BTreeMap<String, String>, String> {
    let mut values = BTreeMap::new();
    let mut index = 0;
    while index < args.len() {
        let flag = args[index];
        if !flag.starts_with("--") {
            return Err(format!("unexpected {command} argument: {flag}"));
        }
        if !allowed.contains(&flag) {
            return Err(format!("unknown {command} option: {flag}"));
        }
        let value = args
            .get(index + 1)
            .ok_or_else(|| format!("{flag} needs a value"))?;
        if value.starts_with("--") {
            return Err(format!("{flag} needs a value"));
        }
        if values
            .insert(flag.to_owned(), (*value).to_owned())
            .is_some()
        {
            return Err(format!("{command} option repeated: {flag}"));
        }
        index += 2;
    }
    Ok(values)
}

fn required_option(values: &BTreeMap<String, String>, flag: &str) -> Result<PathBuf, String> {
    values
        .get(flag)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| format!("release manifest command requires {flag}"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ManifestArtefact {
    name: String,
    target: String,
    format: String,
    size_bytes: u64,
    checksum: String,
}

#[derive(Debug)]
struct ParsedManifest {
    product_version: String,
    source_revision: String,
    build_identity: String,
    artefacts: Vec<ManifestArtefact>,
}

#[derive(Debug)]
struct ReleaseIdentity {
    product_version: String,
    source_revision: String,
    build_identity: String,
    artefacts: Vec<ArtefactFacts>,
}

fn load_release_identity(path: &Path) -> Result<ReleaseIdentity, String> {
    let (value, evidence_root) = read_catalogue(path)?;
    let mut blockers = Vec::new();
    validate_catalogue(&value, &evidence_root, &mut blockers, false);
    let status = value
        .get("records")
        .and_then(Value::as_array)
        .and_then(|records| records.first())
        .and_then(Value::as_object)
        .and_then(|record| record.get("status"))
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let statuses = value
        .get("records")
        .and_then(Value::as_array)
        .map(|records| {
            records
                .iter()
                .map(|record| {
                    record
                        .as_object()
                        .and_then(|record| record.get("status"))
                        .and_then(Value::as_str)
                        .unwrap_or("unknown")
                        .to_owned()
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if matches!(status, "ready" | "published") {
        let mut readiness_blockers = Vec::new();
        validate_catalogue(&value, &evidence_root, &mut readiness_blockers, true);
        blockers.extend(readiness_blockers);
    }
    let unexpected = blockers
        .iter()
        .filter(|blocker| !is_expected_non_ready_blocker(&statuses, blocker))
        .cloned()
        .collect::<Vec<_>>();
    if !unexpected.is_empty() {
        return Err(format!(
            "release record is not suitable for manifest operation: {}",
            unexpected.join("; ")
        ));
    }
    if !matches!(status, "candidate" | "blocked" | "ready" | "published") {
        return Err(format!(
            "release record status must be candidate, blocked, ready or published, not {status}"
        ));
    }

    let records = value
        .get("records")
        .and_then(Value::as_array)
        .ok_or_else(|| "release catalogue records must be an array".to_owned())?;
    if records.len() != 1 {
        return Err(
            "manifest operation requires a catalogue with exactly one release record".to_owned(),
        );
    }
    let record = records[0]
        .as_object()
        .ok_or_else(|| "release record must be an object".to_owned())?;
    let product_version = required_text(record, "product_version", "release record")?;
    let source_revision = required_text(record, "source_revision", "release record")?;
    let build_identity = required_text(record, "build_identity", "release record")?;
    let values = record
        .get("artefacts")
        .and_then(Value::as_array)
        .ok_or_else(|| "release record artefacts must be an array".to_owned())?;
    let mut artefacts = Vec::with_capacity(values.len());
    for (index, value) in values.iter().enumerate() {
        let path = format!("release record artefacts[{index}]");
        let object = value
            .as_object()
            .ok_or_else(|| format!("{path} must be an object"))?;
        let name = required_text(object, "name", &path)?;
        let target = required_text(object, "target", &path)?;
        let format = required_text(object, "format", &path)?;
        let size_bytes = object
            .get("size_bytes")
            .and_then(json_u64)
            .filter(|size| *size > 0)
            .ok_or_else(|| format!("{path}.size_bytes must be greater than zero"))?;
        let checksum_algorithm = required_text(object, "checksum_algorithm", &path)?;
        if checksum_algorithm != "sha256" {
            return Err(format!("{path}.checksum_algorithm must be sha256"));
        }
        let checksum_status = required_text(object, "checksum_status", &path)?;
        let checksum = match object.get("checksum") {
            Some(Value::Null) | None => None,
            Some(Value::String(value)) if !value.is_empty() => Some(value.clone()),
            Some(_) => return Err(format!("{path}.checksum must be text or null")),
        };
        let artefact_revision = required_text(object, "source_revision", &path)?;
        let artefact_build_identity = required_text(object, "build_identity", &path)?;
        let signature_status = required_text(object, "signature_status", &path)?;
        let provenance_status = required_text(object, "provenance_status", &path)?;
        artefacts.push(ArtefactFacts {
            name,
            target,
            format,
            size_bytes,
            checksum_algorithm,
            checksum_status,
            checksum,
            source_revision: artefact_revision,
            build_identity: artefact_build_identity,
            signature_status,
            provenance_status,
        });
    }
    Ok(ReleaseIdentity {
        product_version,
        source_revision,
        build_identity,
        artefacts,
    })
}

fn required_text(object: &Map<String, Value>, key: &str, path: &str) -> Result<String, String> {
    object
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| format!("{path}.{key} must be non-empty text"))
}

fn sort_manifest_artefacts(artefacts: &mut [ManifestArtefact]) {
    artefacts.sort_by(|left, right| {
        left.target
            .cmp(&right.target)
            .then_with(|| left.name.cmp(&right.name))
    });
}

fn canonical_manifest_bytes(
    product_version: &str,
    source_revision: &str,
    build_identity: &str,
    artefacts: &[ManifestArtefact],
) -> Vec<u8> {
    let mut text = String::from("{\"artefacts\":[");
    for (index, artefact) in artefacts.iter().enumerate() {
        if index > 0 {
            text.push(',');
        }
        text.push_str("{\"checksum\":");
        text.push_str(&json_string(&artefact.checksum));
        text.push_str(",\"checksum_algorithm\":\"sha256\",\"format\":");
        text.push_str(&json_string(&artefact.format));
        text.push_str(",\"name\":");
        text.push_str(&json_string(&artefact.name));
        text.push_str(",\"size_bytes\":");
        text.push_str(&artefact.size_bytes.to_string());
        text.push_str(",\"target\":");
        text.push_str(&json_string(&artefact.target));
        text.push('}');
    }
    text.push_str("],\"build_identity\":");
    text.push_str(&json_string(build_identity));
    text.push_str(",\"product_version\":");
    text.push_str(&json_string(product_version));
    text.push_str(",\"schema_version\":1,\"source_revision\":");
    text.push_str(&json_string(source_revision));
    text.push_str("}\n");
    text.into_bytes()
}

fn canonical_checksum_bytes(artefacts: &[ManifestArtefact]) -> Vec<u8> {
    let mut text = String::new();
    for artefact in artefacts {
        text.push_str(&artefact.checksum);
        text.push_str("  ");
        text.push_str(&artefact.name);
        text.push('\n');
    }
    text.into_bytes()
}

fn json_string(value: &str) -> String {
    serde_json::to_string(value).expect("serializing a Rust string cannot fail")
}

fn write_new_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| {
            format!(
                "could not create {} without overwriting: {error}",
                path.display()
            )
        })?;
    if let Err(error) = file.write_all(bytes) {
        drop(file);
        let _ = fs::remove_file(path);
        return Err(format!("could not write {}: {error}", path.display()));
    }
    if let Err(error) = file.sync_all() {
        drop(file);
        let _ = fs::remove_file(path);
        return Err(format!("could not sync {}: {error}", path.display()));
    }
    Ok(())
}

fn checked_child_path(directory: &Path, name: &str, kind: &str) -> Result<PathBuf, String> {
    if !is_safe_artefact_name(name) {
        return Err(format!(
            "{kind} name is not a safe archive basename: {name}"
        ));
    }
    Ok(directory.join(name))
}

fn hash_archive(path: &Path) -> Result<(u64, String), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("could not read archive {}: {error}", path.display()))?;
    if metadata.file_type().is_symlink() {
        return Err(format!("archive must not be a symlink: {}", path.display()));
    }
    if !metadata.is_file() {
        return Err(format!("archive is not a regular file: {}", path.display()));
    }
    if metadata.len() == 0 {
        return Err(format!("archive is empty: {}", path.display()));
    }
    let mut file = File::open(path)
        .map_err(|error| format!("could not open archive {}: {error}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut size_bytes = 0_u64;
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("could not read archive {}: {error}", path.display()))?;
        if read == 0 {
            break;
        }
        size_bytes = size_bytes
            .checked_add(read as u64)
            .ok_or_else(|| format!("archive is too large to count: {}", path.display()))?;
        hasher.update(&buffer[..read]);
    }
    if size_bytes != metadata.len() {
        return Err(format!(
            "archive changed while it was read: {}",
            path.display()
        ));
    }
    let digest = hasher.finalize();
    let checksum = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok((size_bytes, checksum))
}

fn path_is_symlink(path: &Path) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => Ok(metadata.file_type().is_symlink()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!("could not inspect {}: {error}", path.display())),
    }
}

fn parse_canonical_manifest(bytes: &[u8]) -> Result<ParsedManifest, String> {
    if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        return Err("release-manifest.json must not contain a UTF-8 BOM".to_owned());
    }
    if !bytes.ends_with(b"\n") {
        return Err("release-manifest.json must end with one LF".to_owned());
    }
    let value = parse_unique_json_bytes(bytes)
        .map_err(|error| format!("release-manifest.json is not valid JSON: {error}"))?;
    let object = value
        .as_object()
        .ok_or_else(|| "release-manifest.json must be a JSON object".to_owned())?;
    let allowed = [
        "artefacts",
        "build_identity",
        "product_version",
        "schema_version",
        "source_revision",
    ];
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(format!(
                "release-manifest.json contains unknown field: {key}"
            ));
        }
    }
    if object.get("schema_version").and_then(Value::as_u64) != Some(1) {
        return Err("release-manifest.json schema_version must be 1".to_owned());
    }
    let product_version = required_text(object, "product_version", "manifest")?;
    if !looks_like_semver(&product_version) {
        return Err(
            "release-manifest.json product_version is not semantic version text".to_owned(),
        );
    }
    let source_revision = required_text(object, "source_revision", "manifest")?;
    if !is_revision(&source_revision) {
        return Err("release-manifest.json source_revision is not a git revision".to_owned());
    }
    let build_identity = required_text(object, "build_identity", "manifest")?;
    if !is_safe_identity(&build_identity) {
        return Err("release-manifest.json build_identity is not safe identity text".to_owned());
    }
    let values = object
        .get("artefacts")
        .and_then(Value::as_array)
        .ok_or_else(|| "release-manifest.json artefacts must be an array".to_owned())?;
    if values.is_empty() {
        return Err("release-manifest.json artefacts must not be empty".to_owned());
    }
    let mut artefacts = Vec::with_capacity(values.len());
    let mut names = BTreeSet::new();
    let mut targets = BTreeSet::new();
    for (index, value) in values.iter().enumerate() {
        let path = format!("release-manifest.json artefacts[{index}]");
        let item = value
            .as_object()
            .ok_or_else(|| format!("{path} must be an object"))?;
        let allowed = [
            "checksum",
            "checksum_algorithm",
            "format",
            "name",
            "size_bytes",
            "target",
        ];
        for key in item.keys() {
            if !allowed.contains(&key.as_str()) {
                return Err(format!("{path} contains unknown field: {key}"));
            }
        }
        let name = required_text(item, "name", &path)?;
        if !is_safe_artefact_name(&name) {
            return Err(format!("{path}.name is not a safe archive basename"));
        }
        let target = required_text(item, "target", &path)?;
        if !is_safe_target(&target) {
            return Err(format!("{path}.target is not a safe target token"));
        }
        let format = required_text(item, "format", &path)?;
        if !matches!(format.as_str(), "tar.gz" | "zip" | "binary") {
            return Err(format!("{path}.format is not recognised"));
        }
        if artefact_name(&product_version, &target, &format).as_deref() != Some(&name) {
            return Err(format!(
                "{path}.name does not match version, target and format"
            ));
        }
        let size_bytes = item
            .get("size_bytes")
            .and_then(Value::as_u64)
            .filter(|size| *size > 0)
            .ok_or_else(|| format!("{path}.size_bytes must be greater than zero"))?;
        if item.get("checksum_algorithm").and_then(Value::as_str) != Some("sha256") {
            return Err(format!("{path}.checksum_algorithm must be sha256"));
        }
        let checksum = required_text(item, "checksum", &path)?;
        if !is_digest(&checksum) {
            return Err(format!(
                "{path}.checksum is not a lower-case SHA-256 digest"
            ));
        }
        if !names.insert(name.clone()) {
            return Err("release-manifest.json contains a duplicate name".to_owned());
        }
        if !targets.insert(target.clone()) {
            return Err("release-manifest.json contains a duplicate target".to_owned());
        }
        artefacts.push(ManifestArtefact {
            name,
            target,
            format,
            size_bytes,
            checksum,
        });
    }
    let mut sorted = artefacts.clone();
    sort_manifest_artefacts(&mut sorted);
    if sorted != artefacts {
        return Err(
            "release-manifest.json artefacts are not sorted by target then basename".to_owned(),
        );
    }
    let parsed = ParsedManifest {
        product_version,
        source_revision,
        build_identity,
        artefacts,
    };
    let canonical = canonical_manifest_bytes(
        &parsed.product_version,
        &parsed.source_revision,
        &parsed.build_identity,
        &parsed.artefacts,
    );
    if canonical != bytes {
        return Err(
            "release-manifest.json is not canonical UTF-8 JSON with sorted keys and one final LF"
                .to_owned(),
        );
    }
    Ok(parsed)
}

fn parse_checksum_list(bytes: &[u8]) -> Result<Vec<(String, String)>, String> {
    if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        return Err("SHA256SUMS must not contain a UTF-8 BOM".to_owned());
    }
    if !bytes.ends_with(b"\n") {
        return Err("SHA256SUMS must end with one LF".to_owned());
    }
    let text =
        std::str::from_utf8(bytes).map_err(|error| format!("SHA256SUMS is not UTF-8: {error}"))?;
    let body = &text[..text.len() - 1];
    if body.is_empty() {
        return Err("SHA256SUMS must contain at least one entry".to_owned());
    }
    let mut entries = Vec::new();
    for (index, line) in body.split('\n').enumerate() {
        if line.is_empty() {
            return Err(format!("SHA256SUMS entry {index} is blank"));
        }
        let mut fields = line.split("  ");
        let checksum = fields.next().unwrap_or_default();
        let name = fields.next().unwrap_or_default();
        if fields.next().is_some() || !is_digest(checksum) || !is_safe_basename(name) {
            return Err(format!("SHA256SUMS entry {index} is malformed"));
        }
        if !is_safe_artefact_name(name) {
            return Err("SHA256SUMS must not list its own sidecars".to_owned());
        }
        entries.push((checksum.to_owned(), name.to_owned()));
    }
    Ok(entries)
}

#[derive(Debug)]
struct GenerateOptions {
    output: PathBuf,
    target: String,
    artefact_name: String,
    format: String,
    size_bytes: u64,
    notes_entry: String,
    evidence_path: String,
    build_identity: String,
    checksum: Option<String>,
    artefact_path: Option<PathBuf>,
}

impl GenerateOptions {
    fn parse(args: &[&str]) -> Result<Self, String> {
        let mut values = BTreeMap::new();
        let mut index = 0;
        while index < args.len() {
            let flag = args[index];
            if !flag.starts_with("--") {
                return Err(format!("unexpected release generate argument: {flag}"));
            }
            let value = args
                .get(index + 1)
                .ok_or_else(|| format!("{flag} needs a value"))?;
            if value.starts_with("--") {
                return Err(format!("{flag} needs a value"));
            }
            if !matches!(
                flag,
                "--output"
                    | "--target"
                    | "--artefact-name"
                    | "--artefact-format"
                    | "--artefact-size"
                    | "--notes-entry"
                    | "--evidence-path"
                    | "--build-identity"
                    | "--checksum"
                    | "--artefact-path"
            ) {
                return Err(format!("unknown release generate option: {flag}"));
            }
            if values.insert(flag, *value).is_some() {
                return Err(format!("release generate option repeated: {flag}"));
            }
            index += 2;
        }

        let required = |flag: &str| {
            values
                .get(flag)
                .copied()
                .filter(|value| !value.is_empty())
                .ok_or_else(|| format!("release generate requires {flag}"))
        };
        let size_text = required("--artefact-size")?;
        let size_bytes = size_text
            .parse::<u64>()
            .map_err(|error| format!("--artefact-size must be an integer: {error}"))?;
        Ok(Self {
            output: PathBuf::from(required("--output")?),
            target: required("--target")?.to_owned(),
            artefact_name: required("--artefact-name")?.to_owned(),
            format: required("--artefact-format")?.to_owned(),
            size_bytes,
            notes_entry: required("--notes-entry")?.to_owned(),
            evidence_path: required("--evidence-path")?.to_owned(),
            build_identity: required("--build-identity")?.to_owned(),
            checksum: values.get("--checksum").map(|value| (*value).to_owned()),
            artefact_path: values.get("--artefact-path").map(PathBuf::from),
        })
    }
}

fn read_product_version(root: &Path) -> Result<String, String> {
    let path = root.join("Cargo.toml");
    let text = fs::read_to_string(&path).map_err(|error| {
        format!(
            "could not read product manifest {}: {error}",
            path.display()
        )
    })?;
    let mut in_package = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_package = line == "[package]";
            continue;
        }
        if in_package
            && let Some((key, value)) = line.split_once('=')
            && key.trim() == "version"
        {
            let version = value.trim().trim_matches('"').trim_matches('\'');
            if looks_like_semver(version) {
                return Ok(version.to_owned());
            }
            return Err(format!(
                "Cargo.toml product version is not semantic: {version}"
            ));
        }
    }
    Err("Cargo.toml has no [package].version".to_owned())
}

fn source_identity(root: &Path) -> Result<(String, String), String> {
    let revision = git_output(root, &["rev-parse", "HEAD"])?;
    if !is_revision(&revision) {
        return Err("git returned an invalid HEAD revision".to_owned());
    }
    let status = git_output(root, &["status", "--porcelain"])?;
    let attached = Command::new("git")
        .args(["symbolic-ref", "--quiet", "--short", "HEAD"])
        .current_dir(root)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .map_err(|error| format!("could not inspect git branch state: {error}"))?;
    let source_state = if !attached {
        "detached"
    } else if status.is_empty() {
        "clean"
    } else {
        "modified"
    };
    Ok((revision, source_state.to_owned()))
}

fn exact_release_tag(root: &Path) -> Option<String> {
    git_output(root, &["describe", "--exact-match", "--tags", "HEAD"]).ok()
}

fn tag_matches_version(tag: &str, version: &str) -> bool {
    tag == version || tag == format!("v{version}")
}

fn git_output(root: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .map_err(|error| format!("could not run git {}: {error}", args.join(" ")))?;
    if !output.status.success() {
        return Err(format!("git {} failed", args.join(" ")));
    }
    String::from_utf8(output.stdout)
        .map(|text| text.trim().to_owned())
        .map_err(|error| format!("git {} returned non-UTF-8 output: {error}", args.join(" ")))
}

fn is_safe_identity(value: &str) -> bool {
    value
        .chars()
        .next()
        .is_some_and(|character| character.is_ascii_alphanumeric())
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'/' | b'+' | b'-')
        })
}

fn is_safe_basename(value: &str) -> bool {
    value
        .bytes()
        .next()
        .is_some_and(|byte| byte.is_ascii_alphanumeric())
        && !value.contains("..")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'+' | b'-'))
}

fn is_safe_artefact_name(value: &str) -> bool {
    is_safe_basename(value) && !matches!(value, "release-manifest.json" | "SHA256SUMS")
}

fn is_safe_target(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_alphanumeric())
        && !value.contains("..")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn artefact_name(version: &str, target: &str, format: &str) -> Option<String> {
    let base = format!("ignatius-{version}-{target}");
    match format {
        "tar.gz" => Some(format!("{base}.tar.gz")),
        "zip" => Some(format!("{base}.zip")),
        "binary" => Some(base),
        _ => None,
    }
}

fn platform_for_target(target: &str) -> Option<&'static str> {
    if !RELEASE_TARGETS.contains(&target) {
        return None;
    }
    match target {
        "aarch64-apple-darwin" => Some("macos"),
        "x86_64-pc-windows-msvc" => Some("windows"),
        "x86_64-unknown-linux-gnu" => Some("linux"),
        _ => None,
    }
}

fn architecture_for_target(target: &str) -> String {
    target.split('-').next().unwrap_or(target).to_owned()
}

fn read_catalogue(path: &Path) -> Result<(Value, PathBuf), String> {
    let resolved = resolve_path(path);
    if path_is_symlink(&resolved)? {
        return Err(format!(
            "release catalogue must not be a symlink: {}",
            resolved.display()
        ));
    }
    let text = fs::read_to_string(&resolved)
        .map_err(|error| format!("could not read release catalogue: {error}"))?;
    let value = parse_unique_json(&text)
        .map_err(|error| format!("release catalogue is not valid JSON: {error}"))?;
    let evidence_root = catalogue_root(&resolved);
    Ok((value, evidence_root))
}

fn resolve_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        super::repo_root().join(path)
    }
}

fn catalogue_root(path: &Path) -> PathBuf {
    let repository = super::repo_root();
    if path.is_relative() || path.starts_with(&repository) {
        repository
    } else {
        path.parent()
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
    }
}

fn path_has_symlink_component(root: &Path, path: &Path) -> Result<bool, String> {
    if path_is_symlink(root)? {
        return Ok(true);
    }
    let relative = path
        .strip_prefix(root)
        .map_err(|error| format!("path is outside its evidence root: {error}"))?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        match component {
            Component::CurDir => continue,
            Component::Normal(name) => current.push(name),
            _ => return Ok(true),
        }
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => return Ok(true),
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => {
                return Err(format!("could not inspect {}: {error}", current.display()));
            }
        }
    }
    Ok(false)
}

fn validate_catalogue(
    value: &Value,
    evidence_root: &Path,
    blockers: &mut Vec<String>,
    require_readiness: bool,
) {
    let Some(catalogue) = value.as_object() else {
        push(blockers, "catalogue must be an object");
        return;
    };
    reject_unknown(
        catalogue,
        "catalogue",
        &["schema_version", "records"],
        blockers,
    );

    if catalogue.get("schema_version").and_then(Value::as_u64) != Some(1) {
        push(blockers, "catalogue schema_version must be 1");
    }

    let Some(records) = catalogue.get("records").and_then(Value::as_array) else {
        push(blockers, "catalogue records must be an array");
        return;
    };
    if records.is_empty() {
        push(
            blockers,
            "catalogue must contain at least one release record",
        );
        return;
    }

    let mut versions = BTreeSet::new();
    for (index, record) in records.iter().enumerate() {
        validate_record(
            record,
            index,
            evidence_root,
            &mut versions,
            blockers,
            require_readiness,
        );
    }
}

fn validate_record(
    value: &Value,
    index: usize,
    evidence_root: &Path,
    versions: &mut BTreeSet<String>,
    blockers: &mut Vec<String>,
    require_readiness: bool,
) {
    let record_path = format!("records[{index}]");
    let Some(record) = value.as_object() else {
        push(blockers, format!("{record_path} must be an object"));
        return;
    };
    reject_unknown(
        record,
        &record_path,
        &[
            "schema_version",
            "product_version",
            "source_revision",
            "source_state",
            "build_identity",
            "target_rows",
            "artefacts",
            "notes_reference",
            "evidence_reference",
            "status",
            "blockers",
            "authorization_reference",
            "publication_evidence",
            "state_history",
        ],
        blockers,
    );

    if record.get("schema_version").and_then(Value::as_u64) != Some(1) {
        push(blockers, format!("{record_path}.schema_version must be 1"));
    }

    let product_version = string_field(record, "product_version", &record_path, blockers);
    if let Some(version) = &product_version {
        if !looks_like_semver(version) {
            push(
                blockers,
                format!("{record_path}.product_version is not semantic version text"),
            );
        }
        if !versions.insert(version.clone()) {
            push(blockers, "catalogue contains duplicate product versions");
        }
    }

    let source_revision = string_field(record, "source_revision", &record_path, blockers);
    if let Some(revision) = &source_revision
        && !is_revision(revision)
    {
        push(
            blockers,
            format!("{record_path}.source_revision is not a git revision"),
        );
    }
    let source_state = string_field(record, "source_state", &record_path, blockers);
    if let Some(state) = &source_state
        && !matches!(
            state.as_str(),
            "clean" | "modified" | "detached" | "unknown"
        )
    {
        push(
            blockers,
            format!("{record_path}.source_state is not recognised"),
        );
    }
    let build_identity = string_field(record, "build_identity", &record_path, blockers);
    if let Some(build_identity) = &build_identity
        && !is_safe_identity(build_identity)
    {
        push(
            blockers,
            format!("{record_path}.build_identity is not a safe release identity"),
        );
    }
    let status = string_field(record, "status", &record_path, blockers);
    if let Some(status) = &status
        && !matches!(
            status.as_str(),
            "draft" | "candidate" | "blocked" | "ready" | "published" | "withdrawn"
        )
    {
        push(blockers, format!("{record_path}.status is not recognised"));
    }

    validate_notes(record, &record_path, product_version.as_deref(), blockers);
    let target_rows = validate_target_rows(record, &record_path, blockers);
    let artefacts = validate_artefacts(
        record,
        &record_path,
        product_version.as_deref(),
        source_revision.as_deref(),
        build_identity.as_deref(),
        blockers,
    );
    let evidence_status = validate_evidence(
        record,
        &record_path,
        evidence_root,
        product_version.as_deref(),
        source_revision.as_deref(),
        &artefacts,
        blockers,
    );
    if require_readiness && matches!(status.as_deref(), Some("ready" | "published")) {
        validate_supported_target_matrix(&target_rows, &record_path, blockers);
    }
    validate_bindings(&target_rows, &artefacts, &record_path, blockers);
    validate_state_history(record, &record_path, status.as_deref(), blockers);

    let authorization =
        optional_string_field(record, "authorization_reference", &record_path, blockers);
    if let Some(reference) = &authorization
        && !is_authorization_reference(reference)
    {
        push(
            blockers,
            format!("{record_path}.authorization_reference is not a controlled reference"),
        );
    }

    let publication_evidence = validate_publication_evidence(
        record,
        &record_path,
        product_version.as_deref(),
        source_revision.as_deref(),
        blockers,
    );

    let record_blockers = validate_record_blockers(record, &record_path, blockers);
    if status.as_deref() == Some("blocked") && record_blockers == 0 {
        push(
            blockers,
            format!("{record_path}.blocked requires at least one blocker"),
        );
    }

    if require_readiness {
        if source_state.as_deref() != Some("clean") {
            push(
                blockers,
                format!("{record_path} requires a clean source state"),
            );
        }
        if let Some(values) = record.get("blockers").and_then(Value::as_array) {
            for (blocker_index, blocker) in values.iter().enumerate() {
                if let Some(blocker) = blocker.as_str().filter(|blocker| !blocker.is_empty()) {
                    push(
                        blockers,
                        format!("{record_path}.blockers[{blocker_index}]: {blocker}"),
                    );
                }
            }
        }
    }

    if !matches!(status.as_deref(), Some("ready" | "published")) {
        if require_readiness {
            push(
                blockers,
                format!("{record_path} is not in a publishable readiness state"),
            );
            if evidence_status.as_deref() != Some("complete") {
                push(
                    blockers,
                    format!("{record_path} requires complete evidence"),
                );
            }
            if evidence_status.as_deref() == Some("mismatch") {
                push(
                    blockers,
                    format!("{record_path}.evidence_reference.status is mismatch"),
                );
            }
            for artefact in &artefacts {
                if artefact.checksum_status != "present"
                    || artefact
                        .checksum
                        .as_deref()
                        .is_none_or(|checksum| !is_digest(checksum))
                {
                    push(
                        blockers,
                        format!("{record_path} has an artefact without a valid checksum"),
                    );
                }
                if artefact.signature_status != "verified" {
                    push(
                        blockers,
                        format!(
                            "{record_path} has an artefact without verified signature evidence"
                        ),
                    );
                }
                if artefact.provenance_status != "verified" {
                    push(
                        blockers,
                        format!(
                            "{record_path} has an artefact without verified provenance evidence"
                        ),
                    );
                }
            }
        }
        return;
    }

    if !require_readiness {
        return;
    }

    if authorization.is_none() {
        push(
            blockers,
            format!("{record_path} requires owner authorization"),
        );
    }
    if evidence_status.as_deref() != Some("complete") {
        push(
            blockers,
            format!("{record_path} requires complete evidence"),
        );
    }
    if artefacts.is_empty() {
        push(
            blockers,
            format!("{record_path} requires at least one artefact"),
        );
    }
    for artefact in &artefacts {
        if artefact.checksum_status != "present"
            || artefact
                .checksum
                .as_deref()
                .is_none_or(|checksum| !is_digest(checksum))
        {
            push(
                blockers,
                format!("{record_path} has an artefact without a valid checksum"),
            );
        }
        if artefact.signature_status != "verified" {
            push(
                blockers,
                format!("{record_path} has an artefact without verified signature evidence"),
            );
        }
        if artefact.provenance_status != "verified" {
            push(
                blockers,
                format!("{record_path} has an artefact without verified provenance evidence"),
            );
        }
    }
    if status.as_deref() == Some("published") && publication_evidence.as_deref() != Some("verified")
    {
        push(
            blockers,
            format!("{record_path} requires verified publication evidence"),
        );
    }
}

fn validate_record_blockers(
    record: &Map<String, Value>,
    path: &str,
    blockers: &mut Vec<String>,
) -> usize {
    let Some(value) = record.get("blockers") else {
        return 0;
    };
    let Some(values) = value.as_array() else {
        push(blockers, format!("{path}.blockers must be an array"));
        return 0;
    };
    let mut seen = BTreeSet::new();
    for (index, value) in values.iter().enumerate() {
        match value.as_str() {
            Some(blocker) if !blocker.is_empty() => {
                if !seen.insert(blocker) {
                    push(
                        blockers,
                        format!("{path}.blockers[{index}] duplicates an earlier blocker"),
                    );
                }
            }
            _ => push(
                blockers,
                format!("{path}.blockers[{index}] must be non-empty text"),
            ),
        }
    }
    values.len()
}

fn validate_notes(
    record: &Map<String, Value>,
    path: &str,
    product_version: Option<&str>,
    blockers: &mut Vec<String>,
) {
    let Some(notes) = object_field(record, "notes_reference", path, blockers) else {
        return;
    };
    reject_unknown(
        notes,
        &format!("{path}.notes_reference"),
        &["path", "entry"],
        blockers,
    );
    let notes_path = string_field(notes, "path", &format!("{path}.notes_reference"), blockers);
    let entry = string_field(notes, "entry", &format!("{path}.notes_reference"), blockers);
    if notes_path.as_deref() != Some("CHANGELOG.md") {
        push(
            blockers,
            format!("{path}.notes_reference.path must be CHANGELOG.md"),
        );
    }
    if entry.as_deref().is_some_and(|value| {
        value
            .chars()
            .any(|character| matches!(character, '\r' | '\n'))
    }) {
        push(
            blockers,
            format!("{path}.notes_reference.entry must be one line"),
        );
    }
    if entry.is_none() {
        push(
            blockers,
            format!("{path}.notes_reference.entry is required"),
        );
    } else if let (Some(entry), Some(product_version)) = (entry.as_deref(), product_version)
        && !contains_exact_version_token(entry, product_version)
    {
        push(
            blockers,
            format!("{path}.notes_reference.entry does not name the product version"),
        );
    }
}

fn contains_exact_version_token(value: &str, version: &str) -> bool {
    if version.is_empty() {
        return false;
    }
    let mut search_from = 0;
    while let Some(relative_start) = value[search_from..].find(version) {
        let start = search_from + relative_start;
        let end = start + version.len();
        let left_is_boundary = value[..start]
            .chars()
            .next_back()
            .is_none_or(|character| !is_version_token_character(character));
        let right_is_boundary = value[end..]
            .chars()
            .next()
            .is_none_or(|character| !is_version_token_character(character));
        if left_is_boundary && right_is_boundary {
            return true;
        }
        search_from = end;
    }
    false
}

fn is_version_token_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '+')
}

fn validate_evidence(
    record: &Map<String, Value>,
    path: &str,
    evidence_root: &Path,
    product_version: Option<&str>,
    source_revision: Option<&str>,
    artefacts: &[ArtefactFacts],
    blockers: &mut Vec<String>,
) -> Option<String> {
    let evidence = object_field(record, "evidence_reference", path, blockers)?;
    let evidence_path = format!("{path}.evidence_reference");
    reject_unknown(
        evidence,
        &evidence_path,
        &["path", "product_version", "source_revision", "status"],
        blockers,
    );
    let reference = string_field(evidence, "path", &evidence_path, blockers);
    let evidence_version = string_field(evidence, "product_version", &evidence_path, blockers);
    let evidence_revision = string_field(evidence, "source_revision", &evidence_path, blockers);
    let status = string_field(evidence, "status", &evidence_path, blockers);
    let mut evidence_file = None;

    if let Some(reference) = &reference {
        if !is_evidence_reference(reference) {
            push(
                blockers,
                format!("{evidence_path}.path is outside release-evidence"),
            );
        } else {
            let path = evidence_root.join(reference);
            match path_has_symlink_component(evidence_root, &path) {
                Ok(true) => push(
                    blockers,
                    format!("{evidence_path}.path must not use a symlink"),
                ),
                Ok(false) if !path.is_file() => {
                    if status.as_deref() == Some("complete") {
                        push(blockers, format!("{evidence_path}.path does not exist"));
                    } else {
                        push(
                            blockers,
                            format!(
                                "{evidence_path}.path is not yet materialised for incomplete evidence"
                            ),
                        );
                    }
                }
                Ok(false) => evidence_file = Some(path),
                Err(error) => push(
                    blockers,
                    format!("{evidence_path}.path could not be inspected: {error}"),
                ),
            }
        }
    }
    if evidence_version.as_deref() != product_version {
        push(
            blockers,
            format!("{evidence_path}.product_version does not match the record"),
        );
    }
    if evidence_revision.as_deref() != source_revision {
        push(
            blockers,
            format!("{evidence_path}.source_revision does not match the record"),
        );
    }
    if !matches!(
        status.as_deref(),
        Some("complete" | "incomplete" | "mismatch")
    ) {
        push(
            blockers,
            format!("{evidence_path}.status is not recognised"),
        );
    }
    if status.as_deref() == Some("complete")
        && let Some(path) = evidence_file
    {
        validate_evidence_bundle(
            &path,
            &evidence_path,
            product_version,
            source_revision,
            artefacts,
            blockers,
        );
    }
    status
}

fn validate_evidence_bundle(
    path: &Path,
    evidence_path: &str,
    product_version: Option<&str>,
    source_revision: Option<&str>,
    artefacts: &[ArtefactFacts],
    blockers: &mut Vec<String>,
) {
    let Ok(text) = fs::read_to_string(path) else {
        push(blockers, format!("{evidence_path}.path could not be read"));
        return;
    };
    let Ok(value) = parse_unique_json(&text) else {
        push(
            blockers,
            format!("{evidence_path}.path is not a JSON evidence bundle"),
        );
        return;
    };
    let Some(bundle) = value.as_object() else {
        push(
            blockers,
            format!("{evidence_path}.path evidence bundle is not an object"),
        );
        return;
    };
    reject_unknown(
        bundle,
        &format!("{evidence_path}.bundle"),
        &[
            "schema_version",
            "product_version",
            "source_revision",
            "inventory_status",
            "artefacts",
        ],
        blockers,
    );
    if bundle.get("schema_version").and_then(Value::as_u64) != Some(1) {
        push(
            blockers,
            format!("{evidence_path}.bundle schema_version must be 1"),
        );
    }
    let bundle_path = format!("{evidence_path}.bundle");
    let bundle_version = string_field(bundle, "product_version", &bundle_path, blockers);
    let bundle_revision = string_field(bundle, "source_revision", &bundle_path, blockers);
    let inventory_status = string_field(bundle, "inventory_status", &bundle_path, blockers);
    if bundle_version.as_deref() != product_version {
        push(
            blockers,
            format!("{bundle_path}.product_version does not match the record"),
        );
    }
    if bundle_revision.as_deref() != source_revision {
        push(
            blockers,
            format!("{bundle_path}.source_revision does not match the record"),
        );
    }
    if inventory_status.as_deref() != Some("verified") {
        push(
            blockers,
            format!("{bundle_path}.inventory_status is not verified"),
        );
    }
    validate_evidence_artefacts(bundle, &bundle_path, artefacts, blockers);
}

fn validate_evidence_artefacts(
    bundle: &Map<String, Value>,
    path: &str,
    expected: &[ArtefactFacts],
    blockers: &mut Vec<String>,
) {
    let Some(values) = bundle.get("artefacts").and_then(Value::as_array) else {
        push(blockers, format!("{path}.artefacts must be an array"));
        return;
    };
    let mut observed = BTreeMap::new();
    for (index, value) in values.iter().enumerate() {
        let item_path = format!("{path}.artefacts[{index}]");
        let Some(item) = object_field_value(value, &item_path, blockers) else {
            continue;
        };
        reject_unknown(
            item,
            &item_path,
            &[
                "name",
                "artefact_name",
                "target",
                "size_bytes",
                "format",
                "checksum_algorithm",
                "checksum_status",
                "checksum",
                "source_revision",
                "build_identity",
                "signature_status",
                "provenance_status",
            ],
            blockers,
        );
        let name = if item.contains_key("name") {
            string_field(item, "name", &item_path, blockers)
        } else {
            string_field(item, "artefact_name", &item_path, blockers)
        };
        let alias_name = if item.contains_key("name") {
            optional_non_null_string_field(item, "artefact_name", &item_path, blockers)
        } else {
            None
        };
        if let (Some(name), Some(alias_name)) = (name.as_deref(), alias_name.as_deref())
            && name != alias_name
        {
            push(
                blockers,
                format!("{item_path}.name and artefact_name do not match"),
            );
        }
        if let Some(name) = name
            && observed.insert(name, item).is_some()
        {
            push(
                blockers,
                format!("{path}.artefacts contains a duplicate name"),
            );
        }
    }
    if observed.len() != expected.len() {
        push(
            blockers,
            format!("{path}.artefacts does not cover the release artefact set exactly"),
        );
    }
    for expected in expected {
        let Some(item) = observed.get(&expected.name) else {
            push(
                blockers,
                format!("{path}.artefacts is missing {}", expected.name),
            );
            continue;
        };
        let item_path = format!("{path}.artefacts[{}]", expected.name);
        let target = string_field(item, "target", &item_path, blockers);
        let format = if item.contains_key("format") {
            optional_non_null_string_field(item, "format", &item_path, blockers)
        } else {
            None
        };
        let size_bytes = item.get("size_bytes").and_then(json_u64);
        let checksum_algorithm = string_field(item, "checksum_algorithm", &item_path, blockers);
        let checksum_status = string_field(item, "checksum_status", &item_path, blockers);
        let checksum = optional_string_or_null(item, "checksum", &item_path, blockers);
        let source_revision = string_field(item, "source_revision", &item_path, blockers);
        let build_identity = string_field(item, "build_identity", &item_path, blockers);
        let signature_status = string_field(item, "signature_status", &item_path, blockers);
        let provenance_status = string_field(item, "provenance_status", &item_path, blockers);
        if target.as_deref() != Some(expected.target.as_str()) {
            push(
                blockers,
                format!("{item_path}.target does not match the release artefact"),
            );
        }
        if let Some(format) = format.as_deref() {
            if !matches!(format, "tar.gz" | "zip" | "binary") {
                push(blockers, format!("{item_path}.format is not recognised"));
            } else if format != expected.format {
                push(
                    blockers,
                    format!("{item_path}.format does not match the release artefact"),
                );
            }
        }
        if size_bytes != Some(expected.size_bytes) {
            push(
                blockers,
                format!("{item_path}.size_bytes does not match the release artefact"),
            );
        }
        if checksum_algorithm.as_deref() != Some(expected.checksum_algorithm.as_str()) {
            push(
                blockers,
                format!("{item_path}.checksum_algorithm does not match the release artefact"),
            );
        }
        if checksum_status.as_deref() != Some(expected.checksum_status.as_str()) {
            push(
                blockers,
                format!("{item_path}.checksum_status does not match the release artefact"),
            );
        }
        if checksum != expected.checksum {
            push(
                blockers,
                format!("{item_path}.checksum does not match the release artefact"),
            );
        }
        if source_revision.as_deref() != Some(expected.source_revision.as_str()) {
            push(
                blockers,
                format!("{item_path}.source_revision does not match the release artefact"),
            );
        }
        if build_identity.as_deref() != Some(expected.build_identity.as_str()) {
            push(
                blockers,
                format!("{item_path}.build_identity does not match the release artefact"),
            );
        }
        if signature_status.as_deref() != Some(expected.signature_status.as_str()) {
            push(
                blockers,
                format!("{item_path}.signature_status does not match the release artefact"),
            );
        }
        if provenance_status.as_deref() != Some(expected.provenance_status.as_str()) {
            push(
                blockers,
                format!("{item_path}.provenance_status does not match the release artefact"),
            );
        }
    }
}

fn validate_publication_evidence(
    record: &Map<String, Value>,
    path: &str,
    product_version: Option<&str>,
    source_revision: Option<&str>,
    blockers: &mut Vec<String>,
) -> Option<String> {
    let publication = record.get("publication_evidence")?;
    let publication = object_field_value(
        publication,
        &format!("{path}.publication_evidence"),
        blockers,
    )?;
    let publication_path = format!("{path}.publication_evidence");
    reject_unknown(
        publication,
        &publication_path,
        &[
            "reference",
            "product_version",
            "source_revision",
            "status",
            "observed_at",
        ],
        blockers,
    );
    let reference = string_field(publication, "reference", &publication_path, blockers);
    let version = string_field(publication, "product_version", &publication_path, blockers);
    let revision = string_field(publication, "source_revision", &publication_path, blockers);
    let status = string_field(publication, "status", &publication_path, blockers);
    if let Some(reference) = &reference
        && !is_publication_reference(reference)
    {
        push(
            blockers,
            format!("{publication_path}.reference is not controlled"),
        );
    }
    if version.as_deref() != product_version {
        push(
            blockers,
            format!("{publication_path}.product_version does not match the record"),
        );
    }
    if revision.as_deref() != source_revision {
        push(
            blockers,
            format!("{publication_path}.source_revision does not match the record"),
        );
    }
    if !matches!(status.as_deref(), Some("observed" | "verified")) {
        push(
            blockers,
            format!("{publication_path}.status is not recognised"),
        );
    }
    if let Some(observed_at) = string_field(publication, "observed_at", &publication_path, blockers)
        && !is_rfc3339_timestamp(&observed_at)
    {
        push(
            blockers,
            format!("{publication_path}.observed_at must be an RFC 3339 date-time"),
        );
    }
    status
}

fn validate_target_rows(
    record: &Map<String, Value>,
    path: &str,
    blockers: &mut Vec<String>,
) -> BTreeMap<String, (String, Option<String>)> {
    let mut rows = BTreeMap::new();
    let Some(values) = record.get("target_rows").and_then(Value::as_array) else {
        push(
            blockers,
            format!("{path}.target_rows must be a non-empty array"),
        );
        return rows;
    };
    if values.is_empty() {
        push(blockers, format!("{path}.target_rows must not be empty"));
    }
    for (index, value) in values.iter().enumerate() {
        let row_path = format!("{path}.target_rows[{index}]");
        let Some(row) = object_field_value(value, &row_path, blockers) else {
            continue;
        };
        reject_unknown(
            row,
            &row_path,
            &[
                "target",
                "platform",
                "architecture",
                "coverage",
                "artefact_name",
                "blocked_reason",
            ],
            blockers,
        );
        let target = string_field(row, "target", &row_path, blockers);
        let platform = string_field(row, "platform", &row_path, blockers);
        let architecture = string_field(row, "architecture", &row_path, blockers);
        let coverage = string_field(row, "coverage", &row_path, blockers);
        let artefact_name =
            optional_non_null_string_field(row, "artefact_name", &row_path, blockers);
        let blocked_reason =
            optional_non_null_string_field(row, "blocked_reason", &row_path, blockers);
        if artefact_name
            .as_deref()
            .is_some_and(|value| !is_safe_artefact_name(value))
        {
            push(
                blockers,
                format!("{row_path}.artefact_name is not a safe archive basename"),
            );
        }
        if let Some(target) = target {
            if !is_safe_target(&target) {
                push(
                    blockers,
                    format!("{row_path}.target is not a safe target token"),
                );
            }
            let expected_platform = platform_for_target(&target);
            if expected_platform.is_none() {
                push(
                    blockers,
                    format!("{row_path}.target is not a supported platform target"),
                );
            }
            if platform.as_deref() != expected_platform {
                push(
                    blockers,
                    format!("{row_path}.platform does not match the target"),
                );
            }
            if architecture.as_deref() != Some(architecture_for_target(&target).as_str()) {
                push(
                    blockers,
                    format!("{row_path}.architecture does not match the target"),
                );
            }
            if rows
                .insert(
                    target.clone(),
                    (coverage.clone().unwrap_or_default(), artefact_name.clone()),
                )
                .is_some()
            {
                push(
                    blockers,
                    format!("{path}.target_rows contains a duplicate target"),
                );
            }
            match coverage.as_deref() {
                Some("covered") if artefact_name.is_none() => {
                    push(
                        blockers,
                        format!("{row_path}.artefact_name is required for covered targets"),
                    );
                }
                Some("blocked" | "out-of-scope") => {
                    if blocked_reason.is_none() {
                        push(blockers, format!("{row_path}.blocked_reason is required"));
                    }
                }
                Some("covered") => {}
                _ => push(blockers, format!("{row_path}.coverage is not recognised")),
            }
        }
    }
    rows
}

fn validate_supported_target_matrix(
    target_rows: &BTreeMap<String, (String, Option<String>)>,
    path: &str,
    blockers: &mut Vec<String>,
) {
    for target in RELEASE_TARGETS {
        if !target_rows.contains_key(target) {
            push(
                blockers,
                format!("{path}.target_rows is missing supported target: {target}"),
            );
        }
    }
}

fn validate_artefacts(
    record: &Map<String, Value>,
    path: &str,
    product_version: Option<&str>,
    source_revision: Option<&str>,
    build_identity: Option<&str>,
    blockers: &mut Vec<String>,
) -> Vec<ArtefactFacts> {
    let mut artefacts = Vec::new();
    let Some(values) = record.get("artefacts").and_then(Value::as_array) else {
        push(
            blockers,
            format!("{path}.artefacts must be a non-empty array"),
        );
        return artefacts;
    };
    if values.is_empty() {
        push(blockers, format!("{path}.artefacts must not be empty"));
    }
    let mut names = BTreeSet::new();
    let mut targets = BTreeSet::new();
    for (index, value) in values.iter().enumerate() {
        let artefact_path = format!("{path}.artefacts[{index}]");
        let Some(artefact) = object_field_value(value, &artefact_path, blockers) else {
            continue;
        };
        reject_unknown(
            artefact,
            &artefact_path,
            &[
                "name",
                "target",
                "format",
                "size_bytes",
                "checksum_algorithm",
                "checksum_status",
                "checksum",
                "observed_checksum",
                "source_revision",
                "build_identity",
                "signature_status",
                "provenance_status",
            ],
            blockers,
        );
        let name = string_field(artefact, "name", &artefact_path, blockers);
        let target = string_field(artefact, "target", &artefact_path, blockers);
        let format = string_field(artefact, "format", &artefact_path, blockers);
        let size_bytes = artefact.get("size_bytes").and_then(json_u64);
        if size_bytes.is_none_or(|size| size == 0) {
            push(
                blockers,
                format!("{artefact_path}.size_bytes must be greater than zero"),
            );
        }
        let checksum_algorithm =
            string_field(artefact, "checksum_algorithm", &artefact_path, blockers);
        let checksum_status = string_field(artefact, "checksum_status", &artefact_path, blockers);
        let checksum = optional_string_or_null(artefact, "checksum", &artefact_path, blockers);
        let observed_checksum =
            optional_string_or_null(artefact, "observed_checksum", &artefact_path, blockers);
        let artefact_revision = string_field(artefact, "source_revision", &artefact_path, blockers);
        let artefact_build = string_field(artefact, "build_identity", &artefact_path, blockers);
        let signature_status = string_field(artefact, "signature_status", &artefact_path, blockers);
        let provenance_status =
            string_field(artefact, "provenance_status", &artefact_path, blockers);

        if name
            .as_deref()
            .is_some_and(|value| !is_safe_artefact_name(value))
        {
            push(
                blockers,
                format!("{artefact_path}.name is not a safe archive basename"),
            );
        }
        if target
            .as_deref()
            .is_some_and(|value| !is_safe_target(value))
        {
            push(
                blockers,
                format!("{artefact_path}.target is not a safe target token"),
            );
        }

        if let Some(name) = &name
            && !names.insert(name.clone())
        {
            push(
                blockers,
                format!("{path}.artefacts contains a duplicate name"),
            );
        }
        if let Some(target) = &target
            && !targets.insert(target.clone())
        {
            push(
                blockers,
                format!("{path}.artefacts contains a duplicate target"),
            );
        }
        if !matches!(format.as_deref(), Some("tar.gz" | "zip" | "binary")) {
            push(
                blockers,
                format!("{artefact_path}.format is not recognised"),
            );
        }
        if checksum_algorithm.as_deref() != Some("sha256") {
            push(
                blockers,
                format!("{artefact_path}.checksum_algorithm must be sha256"),
            );
        }
        if let (Some(name), Some(target), Some(version), Some(format)) = (
            name.as_deref(),
            target.as_deref(),
            product_version,
            format.as_deref(),
        ) && artefact_name(version, target, format).as_deref() != Some(name)
        {
            push(
                blockers,
                format!("{artefact_path}.name does not match version, target and format"),
            );
        }
        if artefact_revision.as_deref() != source_revision {
            push(
                blockers,
                format!("{artefact_path}.source_revision does not match the record"),
            );
        }
        if artefact_build.as_deref() != build_identity {
            push(
                blockers,
                format!("{artefact_path}.build_identity does not match the record"),
            );
        }

        match checksum_status.as_deref() {
            Some("present") => {
                if checksum.as_deref().is_none_or(|value| !is_digest(value)) {
                    push(
                        blockers,
                        format!("{artefact_path}.checksum is missing or invalid"),
                    );
                }
            }
            Some("missing") => {
                if checksum.is_some() {
                    push(
                        blockers,
                        format!("{artefact_path}.missing checksum must be null"),
                    );
                }
                push(blockers, format!("{artefact_path} has no checksum"));
            }
            Some("mismatch") => {
                let expected_valid = checksum.as_deref().is_some_and(is_digest);
                let observed_valid = observed_checksum.as_deref().is_some_and(is_digest);
                if !expected_valid || !observed_valid {
                    push(
                        blockers,
                        format!("{artefact_path}.checksum mismatch lacks two valid digests"),
                    );
                } else if checksum == observed_checksum {
                    push(
                        blockers,
                        format!("{artefact_path}.checksum mismatch digests are equal"),
                    );
                }
                push(blockers, format!("{artefact_path} has a checksum mismatch"));
            }
            _ => push(
                blockers,
                format!("{artefact_path}.checksum_status is not recognised"),
            ),
        }

        if !matches!(
            signature_status.as_deref(),
            Some("not-configured" | "unsigned" | "verified" | "invalid" | "expired")
        ) {
            push(
                blockers,
                format!("{artefact_path}.signature_status is not recognised"),
            );
        }
        if !matches!(
            provenance_status.as_deref(),
            Some("not-configured" | "missing" | "verified" | "mismatch" | "invalid")
        ) {
            push(
                blockers,
                format!("{artefact_path}.provenance_status is not recognised"),
            );
        }

        if let (
            Some(name),
            Some(target),
            Some(format),
            Some(size_bytes),
            Some(checksum_algorithm),
            Some(checksum_status),
            Some(source_revision),
            Some(build_identity),
            signature_status,
            provenance_status,
        ) = (
            name,
            target,
            format,
            size_bytes,
            checksum_algorithm,
            checksum_status,
            artefact_revision,
            artefact_build,
            signature_status,
            provenance_status,
        ) {
            artefacts.push(ArtefactFacts {
                name,
                target,
                format,
                size_bytes,
                checksum_algorithm,
                checksum_status,
                checksum,
                source_revision,
                build_identity,
                signature_status: signature_status.unwrap_or_default(),
                provenance_status: provenance_status.unwrap_or_default(),
            });
        }
    }
    artefacts
}

fn validate_bindings(
    target_rows: &BTreeMap<String, (String, Option<String>)>,
    artefacts: &[ArtefactFacts],
    path: &str,
    blockers: &mut Vec<String>,
) {
    let by_target: BTreeMap<&str, &ArtefactFacts> = artefacts
        .iter()
        .map(|artefact| (artefact.target.as_str(), artefact))
        .collect();
    for (target, (coverage, expected_name)) in target_rows {
        match coverage.as_str() {
            "covered" => match by_target.get(target.as_str()) {
                Some(artefact) if expected_name.as_deref() == Some(artefact.name.as_str()) => {}
                Some(_) => push(
                    blockers,
                    format!("{path} target row and artefact name do not match"),
                ),
                None => push(blockers, format!("{path} covered target has no artefact")),
            },
            "blocked" | "out-of-scope" if by_target.contains_key(target.as_str()) => push(
                blockers,
                format!("{path} non-covered target must not have an artefact"),
            ),
            "blocked" | "out-of-scope" => {}
            _ => {}
        }
    }
    for artefact in artefacts {
        if !target_rows.contains_key(&artefact.target) {
            push(
                blockers,
                format!("{path} artefact target is absent from target_rows"),
            );
        }
    }
}

fn validate_state_history(
    record: &Map<String, Value>,
    path: &str,
    status: Option<&str>,
    blockers: &mut Vec<String>,
) {
    let Some(history) = record.get("state_history").and_then(Value::as_array) else {
        push(
            blockers,
            format!("{path}.state_history must be a non-empty array"),
        );
        return;
    };
    if history.is_empty() {
        push(blockers, format!("{path}.state_history must not be empty"));
        return;
    }
    let mut first_from = None;
    let mut previous_to = None;
    for (index, transition) in history.iter().enumerate() {
        let transition_path = format!("{path}.state_history[{index}]");
        let Some(transition) = object_field_value(transition, &transition_path, blockers) else {
            continue;
        };
        reject_unknown(transition, &transition_path, &["from", "to"], blockers);
        let from = string_field(transition, "from", &transition_path, blockers);
        let to = string_field(transition, "to", &transition_path, blockers);
        if index == 0 {
            first_from = from.clone();
        } else if from.as_deref() != previous_to.as_deref() {
            push(blockers, format!("{path}.state_history is not contiguous"));
        }
        if let (Some(from), Some(to)) = (from.as_deref(), to.as_deref())
            && !valid_transition(from, to)
        {
            push(
                blockers,
                format!("{transition_path} is not an allowed transition"),
            );
        }
        previous_to = to;
    }
    if first_from.as_deref() != Some("draft") {
        push(
            blockers,
            format!("{path}.state_history must start at draft"),
        );
    }
    if previous_to.as_deref() != status {
        push(
            blockers,
            format!("{path}.state_history does not end at status"),
        );
    }
}

fn valid_transition(from: &str, to: &str) -> bool {
    matches!(
        (from, to),
        ("draft", "candidate")
            | ("candidate", "blocked")
            | ("candidate", "ready")
            | ("candidate", "withdrawn")
            | ("ready", "published")
            | ("ready", "withdrawn")
            | ("published", "withdrawn")
    )
}

fn reject_unknown(
    object: &Map<String, Value>,
    path: &str,
    allowed: &[&str],
    blockers: &mut Vec<String>,
) {
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            push(blockers, format!("{path}.{key} is not recognised"));
        }
    }
}

fn object_field<'a>(
    object: &'a Map<String, Value>,
    key: &str,
    path: &str,
    blockers: &mut Vec<String>,
) -> Option<&'a Map<String, Value>> {
    let Some(value) = object.get(key) else {
        push(blockers, format!("{path}.{key} is required"));
        return None;
    };
    object_field_value(value, &format!("{path}.{key}"), blockers)
}

fn object_field_value<'a>(
    value: &'a Value,
    path: &str,
    blockers: &mut Vec<String>,
) -> Option<&'a Map<String, Value>> {
    match value.as_object() {
        Some(object) => Some(object),
        None => {
            push(blockers, format!("{path} must be an object"));
            None
        }
    }
}

fn string_field(
    object: &Map<String, Value>,
    key: &str,
    path: &str,
    blockers: &mut Vec<String>,
) -> Option<String> {
    let Some(value) = object.get(key) else {
        push(blockers, format!("{path}.{key} is required"));
        return None;
    };
    match value.as_str() {
        Some(value) if !value.is_empty() => Some(value.to_owned()),
        Some(_) => {
            push(blockers, format!("{path}.{key} must not be empty"));
            None
        }
        None => {
            push(blockers, format!("{path}.{key} must be text"));
            None
        }
    }
}

fn optional_string_field(
    object: &Map<String, Value>,
    key: &str,
    path: &str,
    blockers: &mut Vec<String>,
) -> Option<String> {
    match object.get(key) {
        None | Some(Value::Null) => None,
        Some(Value::String(value)) if !value.is_empty() => Some(value.to_owned()),
        Some(Value::String(_)) | Some(_) => {
            push(
                blockers,
                format!("{path}.{key} must be non-empty text or null"),
            );
            None
        }
    }
}

fn optional_non_null_string_field(
    object: &Map<String, Value>,
    key: &str,
    path: &str,
    blockers: &mut Vec<String>,
) -> Option<String> {
    match object.get(key) {
        None => None,
        Some(Value::String(value)) if !value.is_empty() => Some(value.to_owned()),
        Some(Value::String(_)) | Some(_) => {
            push(
                blockers,
                format!("{path}.{key} must be non-empty text when present"),
            );
            None
        }
    }
}

fn optional_string_or_null(
    object: &Map<String, Value>,
    key: &str,
    path: &str,
    blockers: &mut Vec<String>,
) -> Option<String> {
    optional_string_field(object, key, path, blockers)
}

fn push(blockers: &mut Vec<String>, blocker: impl Into<String>) {
    let blocker = blocker.into();
    if !blockers.iter().any(|existing| existing == &blocker) {
        blockers.push(blocker);
    }
}

fn is_revision(value: &str) -> bool {
    (40..=64).contains(&value.len()) && value.bytes().all(is_lower_hex)
}

fn is_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(is_lower_hex)
}

fn is_lower_hex(byte: u8) -> bool {
    byte.is_ascii_digit() || matches!(byte, b'a'..=b'f')
}

fn looks_like_semver(value: &str) -> bool {
    let (without_build, build) = value
        .split_once('+')
        .map_or((value, None), |(core, build)| (core, Some(build)));
    if let Some(build) = build
        && !valid_semver_identifiers(build, false)
    {
        return false;
    }
    let (core, prerelease) = without_build
        .split_once('-')
        .map_or((without_build, None), |(core, prerelease)| {
            (core, Some(prerelease))
        });
    if let Some(prerelease) = prerelease
        && !valid_semver_identifiers(prerelease, true)
    {
        return false;
    }
    let parts: Vec<&str> = core.split('.').collect();
    parts.len() == 3
        && parts.iter().all(|part| {
            !part.is_empty()
                && part.chars().all(|character| character.is_ascii_digit())
                && (*part == "0" || !part.starts_with('0'))
        })
}

fn valid_semver_identifiers(
    value: &str,
    numeric_identifiers_cannot_have_leading_zero: bool,
) -> bool {
    !value.is_empty()
        && value.split('.').all(|identifier| {
            !identifier.is_empty()
                && identifier
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
                && (!numeric_identifiers_cannot_have_leading_zero
                    || !identifier
                        .chars()
                        .all(|character| character.is_ascii_digit())
                    || identifier == "0"
                    || !identifier.starts_with('0'))
        })
}

fn is_evidence_reference(value: &str) -> bool {
    if !value.starts_with(RELEASE_EVIDENCE_PREFIX) || value.len() <= RELEASE_EVIDENCE_PREFIX.len() {
        return false;
    }
    let relative = &value[RELEASE_EVIDENCE_PREFIX.len()..];
    relative
        .split('/')
        .all(|component| !component.is_empty() && !matches!(component, "." | ".."))
        && relative.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '+' | '-' | '/')
        })
        && !value.contains("..")
}

fn is_publication_reference(value: &str) -> bool {
    let prefix = if value.starts_with(RELEASE_EVIDENCE_PREFIX) {
        RELEASE_EVIDENCE_PREFIX
    } else if value.starts_with("publication:") {
        "publication:"
    } else {
        return false;
    };
    value.len() > prefix.len()
        && value[prefix.len()..].chars().all(|character| {
            character.is_ascii_alphanumeric()
                || matches!(character, '.' | '_' | ':' | '+' | '-' | '/')
        })
        && !value.contains("..")
}

fn is_authorization_reference(value: &str) -> bool {
    value.starts_with("approval:")
        && value.len() > "approval:".len()
        && value["approval:".len()..]
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_alphanumeric())
        && value["approval:".len()..]
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
}

fn is_rfc3339_timestamp(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() < 20
        || bytes.get(4) != Some(&b'-')
        || bytes.get(7) != Some(&b'-')
        || bytes.get(10) != Some(&b'T')
        || bytes.get(13) != Some(&b':')
        || bytes.get(16) != Some(&b':')
    {
        return false;
    }
    let Some(year) = four_digits(bytes, 0) else {
        return false;
    };
    let Some(month) = two_digits(bytes, 5) else {
        return false;
    };
    let Some(day) = two_digits(bytes, 8) else {
        return false;
    };
    let Some(hour) = two_digits(bytes, 11) else {
        return false;
    };
    let Some(minute) = two_digits(bytes, 14) else {
        return false;
    };
    let Some(second) = two_digits(bytes, 17) else {
        return false;
    };
    if month == 0
        || month > 12
        || day == 0
        || day > days_in_month(year, month)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return false;
    }
    let mut index = 19;
    if bytes.get(index) == Some(&b'.') {
        index += 1;
        let start = index;
        while bytes.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
        if index == start {
            return false;
        }
    }
    match bytes.get(index) {
        Some(b'Z') => index + 1 == bytes.len(),
        Some(b'+') | Some(b'-') => {
            index + 6 == bytes.len()
                && bytes.get(index + 3) == Some(&b':')
                && two_digits(bytes, index + 1).is_some_and(|hours| hours <= 23)
                && two_digits(bytes, index + 4).is_some_and(|minutes| minutes <= 59)
        }
        _ => false,
    }
}

fn two_digits(bytes: &[u8], index: usize) -> Option<u8> {
    let high = *bytes.get(index)?;
    let low = *bytes.get(index + 1)?;
    if high.is_ascii_digit() && low.is_ascii_digit() {
        Some((high - b'0') * 10 + low - b'0')
    } else {
        None
    }
}

fn four_digits(bytes: &[u8], index: usize) -> Option<u16> {
    let first = two_digits(bytes, index)? as u16;
    let second = two_digits(bytes, index + 2)? as u16;
    Some(first * 100 + second)
}

fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        2 if year.is_multiple_of(400) || (year.is_multiple_of(4) && !year.is_multiple_of(100)) => {
            29
        }
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

#[cfg(test)]
mod tests {
    use super::json_u64;
    use serde_json::Value;

    fn number(text: &str) -> Value {
        serde_json::from_str(text).expect("valid JSON number")
    }

    #[test]
    fn json_u64_accepts_exact_integer_decimal_and_exponent_spellings() {
        assert_eq!(json_u64(&number("23.0")), Some(23));
        assert_eq!(json_u64(&number("23e0")), Some(23));
        assert_eq!(json_u64(&number("18446744073709551615.0")), Some(u64::MAX));
    }

    #[test]
    fn json_u64_rejects_fractional_negative_and_overflow_values() {
        assert_eq!(json_u64(&number("23.1")), None);
        assert_eq!(json_u64(&number("-1.0")), None);
        assert_eq!(json_u64(&number("18446744073709551616")), None);
    }
}
