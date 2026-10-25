//! Private, same-directory replacement for local state files.

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
        let mut file = match create_private_temporary(&temporary) {
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

fn create_private_temporary(path: &Path) -> io::Result<fs::File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

#[cfg(test)]
mod tests;
