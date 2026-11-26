//! Private file creation and same-directory publication.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEMPORARY: AtomicU64 = AtomicU64::new(0);

/// Writes a private sibling and replaces the destination only after syncing it.
/// Existing temporary entries are never opened or removed by this operation.
pub(crate) fn write_atomic(path: &Path, contents: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    path.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "the destination needs a file name",
        )
    })?;
    for _ in 0..128 {
        // Keep the temporary basename bounded even when the destination is
        // already at the filesystem's maximum component length.
        let temporary_name = format!(
            ".ignatius-state.tmp.{}.{}",
            std::process::id(),
            NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed)
        );
        let temporary = parent.join(temporary_name);
        let mut file = match create_private_file(&temporary) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        };
        let result = (|| {
            file.write_all(contents)?;
            file.flush()?;
            file.sync_all()
        })();
        drop(file);
        let result = result.and_then(|()| fs::rename(&temporary, path));
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        return result;
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not reserve a new temporary file beside the destination",
    ))
}

/// Exclusively reserves a new file, private before any bytes are written.
/// Existing entries, including dangling symlinks, are never opened.
pub(crate) fn create_private_file(path: &Path) -> io::Result<fs::File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

/// Publishes a closed sibling file atomically, preserving either name on failure.
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(crate) fn rename_noreplace(source: &Path, destination: &Path) -> io::Result<()> {
    use rustix::fs::{self, CWD, RenameFlags};
    use rustix::io::Errno;

    fs::renameat_with(CWD, source, CWD, destination, RenameFlags::NOREPLACE).map_err(|error| {
        // Linux can report EINVAL for filesystems that reject RENAME_NOREPLACE.
        if matches!(error, Errno::INVAL | Errno::NOSYS | Errno::NOTSUP) || error == Errno::OPNOTSUPP
        {
            io::Error::new(io::ErrorKind::Unsupported, io::Error::from(error))
        } else {
            error.into()
        }
    })
}

#[cfg(windows)]
pub(crate) fn rename_noreplace(source: &Path, destination: &Path) -> io::Result<()> {
    // Preserve Win32 name normalization before joining to a verbatim parent.
    let source = std::path::absolute(source)?;
    let destination = std::path::absolute(destination)?;
    let invalid_path = || {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "the source and destination must be sibling files",
        )
    };
    let parent = source.parent().ok_or_else(invalid_path)?;
    let source_name = source.file_name().ok_or_else(invalid_path)?;
    let destination_name = destination.file_name().ok_or_else(invalid_path)?;
    if destination.parent() != Some(parent)
        || Path::new(source_name).file_name() != Some(source_name)
        || Path::new(destination_name).file_name() != Some(destination_name)
    {
        return Err(invalid_path());
    }

    // TempPath passes raw UTF-16 to MoveFileExW. Windows canonicalize gives
    // both siblings the same extended-length parent without opening the final name.
    let parent = parent.canonicalize()?;
    let mut partial = tempfile::TempPath::try_from_path(parent.join(source_name))?;
    // Keep recovery data when dropping the TempPath returned by a failed move.
    partial.disable_cleanup(true);
    partial
        .persist_noclobber(parent.join(destination_name))
        .map_err(|err| err.error)
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
pub(crate) fn rename_noreplace(_source: &Path, _destination: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "this platform has no atomic move without replacement",
    ))
}

#[cfg(test)]
mod tests;
