//! Shell-free entry point of the standard Microsoft Azure CLI x64 MSI.

use std::ffi::OsStr;
use std::fs::{self, Metadata};
use std::io;
use std::os::windows::fs::MetadataExt;
use std::path::{Component, Path, PathBuf, Prefix};
use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
use windows_sys::Win32::UI::Shell::FOLDERID_ProgramFilesX64;

pub(super) fn installed_python() -> io::Result<Option<PathBuf>> {
    let Some(root) = dirs_sys::known_folder(FOLDERID_ProgramFilesX64) else {
        return Ok(None);
    };
    let Some(search) = std::env::var_os("PATH") else {
        return Ok(None);
    };
    python_in(&root, &search)
}

fn python_in(root: &Path, search: &OsStr) -> io::Result<Option<PathBuf>> {
    if !root.is_absolute()
        || !matches!(root.components().next(), Some(Component::Prefix(p)) if matches!(p.kind(), Prefix::Disk(_)))
    {
        return Ok(None);
    }
    let sdk = root.join("Microsoft SDKs");
    let azure = sdk.join("Azure");
    let package = azure.join("CLI2");
    let wbin = package.join("wbin");
    // Compare PATH lexically: probing arbitrary entries could access a network
    // share. Only the fixed paths under the OS-known root can be inspected.
    if !std::env::split_paths(search).any(|entry| same_directory(&entry, &wbin)) {
        return Ok(None);
    }
    for directory in [root, &sdk, &azure, &package, &wbin] {
        if !plain_item(directory, Metadata::is_dir)? {
            return Ok(None);
        }
    }
    let python = package.join("python.exe");
    if plain_item(&wbin.join("az.cmd"), Metadata::is_file)?
        && plain_item(&python, Metadata::is_file)?
    {
        Ok(Some(python))
    } else {
        Ok(None)
    }
}

fn same_directory(entry: &Path, expected: &Path) -> bool {
    let mut components = entry.components();
    expected.components().all(|part| {
        components
            .next()
            .is_some_and(|other| part.as_os_str().eq_ignore_ascii_case(other.as_os_str()))
    }) && components.next().is_none()
}

fn plain_item(path: &Path, kind: fn(&Metadata) -> bool) -> io::Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(meta) => Ok(meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0 && kind(&meta)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests;
