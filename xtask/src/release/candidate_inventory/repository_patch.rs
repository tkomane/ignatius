//! The single repository-local dependency admitted to candidate inventories.

use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

pub(super) const NAME: &str = "tokio-postgres";
pub(super) const VERSION: &str = "0.7.18";
pub(super) const MANIFEST: &str = "vendor/tokio-postgres/Cargo.toml";

const MAX_ENTRIES: usize = 1024;
const MAX_TREE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_PATH_BYTES: usize = 240;

pub(super) fn matches_manifest(root: &Path, supplied: &Path) -> bool {
    let expected = root.join(MANIFEST);
    supplied.is_absolute()
        && (supplied == expected
            || expected
                .canonicalize()
                .is_ok_and(|canonical| supplied == canonical))
}

pub(super) fn tree_sha256(root: &Path) -> Result<String, String> {
    let vendor = root.join("vendor");
    let package = vendor.join(NAME);
    if !source_metadata(&vendor)?.is_dir() || !source_metadata(&package)?.is_dir() {
        return Err("candidate repository patch requires regular source directories".into());
    }
    if !source_metadata(&package.join("Cargo.toml"))?.is_file() {
        return Err("candidate repository patch requires a regular package manifest".into());
    }

    let mut pending = vec![(package, String::new())];
    let mut files = BTreeMap::new();
    let mut entry_count = 0;
    let mut total_size = 0;
    while let Some((directory, relative)) = pending.pop() {
        if !source_metadata(&directory)?.is_dir() {
            return Err("candidate repository patch contains a changed source directory".into());
        }
        let entries =
            fs::read_dir(directory).map_err(|_| "cannot enumerate candidate repository patch")?;
        for entry in entries {
            entry_count += 1;
            if entry_count > MAX_ENTRIES {
                return Err("candidate repository patch exceeds the source entry limit".into());
            }
            let entry = entry.map_err(|_| "cannot enumerate candidate repository patch")?;
            let name = entry.file_name();
            let name = name
                .to_str()
                .filter(|name| super::bounded_token(name) && !name.ends_with('.'))
                .ok_or("candidate repository patch has a nonportable filename")?;
            let name = if relative.is_empty() {
                name.to_owned()
            } else {
                format!("{relative}/{name}")
            };
            if name.len() > MAX_PATH_BYTES {
                return Err("candidate repository patch exceeds the relative path limit".into());
            }
            let path = entry.path();
            let metadata = source_metadata(&path)?;
            if metadata.is_dir() {
                pending.push((path, name));
                continue;
            }
            if !metadata.is_file() || metadata.len() > MAX_TREE_BYTES - total_size {
                return Err("candidate repository patch requires bounded regular files".into());
            }
            let digest = source_file_sha256(&path, &metadata)?;
            total_size += metadata.len();
            files.insert(name, digest);
        }
    }

    // Hash the sorted SHA-256 list without retaining source paths or bytes in
    // the evidence. Paths are ASCII, package-relative and slash-separated.
    let mut tree = Sha256::new();
    for (name, digest) in files {
        tree.update(format!("{digest}  {name}\n").as_bytes());
    }
    Ok(hex(&tree.finalize()))
}

fn source_metadata(path: &Path) -> Result<fs::Metadata, String> {
    let metadata =
        fs::symlink_metadata(path).map_err(|_| "cannot read candidate repository patch")?;
    let redirected = metadata.file_type().is_symlink();
    #[cfg(windows)]
    let redirected = {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        redirected || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    };
    if redirected {
        return Err("candidate repository patch must not contain links or reparse points".into());
    }
    Ok(metadata)
}

fn source_file_sha256(path: &Path, metadata: &fs::Metadata) -> Result<String, String> {
    let mut bytes = Vec::new();
    File::open(path)
        .and_then(|file| file.take(metadata.len() + 1).read_to_end(&mut bytes))
        .map_err(|_| "cannot read candidate repository patch file")?;
    let current = source_metadata(path)?;
    if !current.is_file()
        || bytes.len() as u64 != metadata.len()
        || current.len() != metadata.len()
        || current.modified().ok() != metadata.modified().ok()
    {
        return Err("candidate repository patch file changed while it was read".into());
    }
    Ok(hex(&Sha256::digest(bytes)))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
