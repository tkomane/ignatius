//! Narrow platform differences.
//!
//! Everything that behaves differently on macOS, Windows and Linux lives here so
//! the rest of the code stays single-path. Where a platform cannot provide a
//! guarantee, this module says so rather than pretending.

use std::io;
use std::path::Path;

/// Restricts a file to its owner where the platform supports it.
///
/// On Unix this sets mode `0600` for files and `0700` for directories. On Windows
/// it is a no-op: files created under the per-user roaming profile inherit an ACL
/// that already excludes other standard users, and this project does not claim a
/// stronger guarantee than that. See `docs/security/data-handling.md`.
pub fn restrict_to_owner(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let meta = std::fs::metadata(path)?;
        let mode = if meta.is_dir() { 0o700 } else { 0o600 };
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(())
    }
}

/// Reports whether owner-only permissions can be enforced on this platform.
///
/// `doctor` shows this so the user knows which guarantee actually applies.
#[must_use]
pub const fn enforces_file_permissions() -> bool {
    cfg!(unix)
}

/// Reports whether PostgreSQL Unix-domain sockets are usable on this platform.
#[must_use]
pub const fn supports_unix_sockets() -> bool {
    cfg!(unix)
}

/// Human-readable platform name used in diagnostics and `doctor`.
#[must_use]
pub const fn name() -> &'static str {
    if cfg!(target_os = "macos") {
        "macOS"
    } else if cfg!(target_os = "windows") {
        "Windows"
    } else if cfg!(target_os = "linux") {
        "Linux"
    } else {
        std::env::consts::OS
    }
}

/// The line ending this platform's text editors expect for generated files.
#[must_use]
pub const fn line_ending() -> &'static str {
    if cfg!(windows) { "\r\n" } else { "\n" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restricting_a_file_is_idempotent_and_reported_honestly() {
        let dir = tempfile::tempdir().expect("temp dir");
        let file = dir.path().join("config.toml");
        std::fs::write(&file, b"schema_version = 1\n").expect("write");

        restrict_to_owner(&file).expect("first call");
        restrict_to_owner(&file).expect("repeat call is safe");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&file)
                .expect("metadata")
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600, "owner-only file permissions");
            assert!(enforces_file_permissions());
        }
        #[cfg(not(unix))]
        assert!(!enforces_file_permissions());
    }

    #[test]
    fn platform_name_is_never_empty() {
        assert!(!name().is_empty());
    }
}
