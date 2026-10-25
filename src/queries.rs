//! Saved queries: ordinary `.sql` files in a directory the user owns.
//!
//! `config paths` has advertised this directory since the first release and
//! nothing wrote to it. It does now, and deliberately as plain files: a query
//! saved here can be opened in any editor, kept in a repository, sent to a
//! colleague, or read by `psql`. Nothing about it is this product's format.
//!
//! A name is not a path. Every name is checked before it reaches the
//! filesystem: no separators, no `..`, no control characters, nothing that
//! could write outside the directory it belongs in. That check is a security
//! boundary and is tested as one.

use crate::diagnostics::{Diagnostic, DiagnosticKind};
use std::path::{Path, PathBuf};

/// Names Windows will not give to a file, whatever extension follows them.
const RESERVED_ON_WINDOWS: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// The extension every saved query has.
pub const EXTENSION: &str = "sql";

/// One saved query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedQuery {
    /// The name, without the extension.
    pub name: String,
    /// Where it is.
    pub path: PathBuf,
    /// The first line of it, for the list.
    pub summary: String,
}

/// The directory saved queries live in.
#[derive(Debug, Clone)]
pub struct Library {
    directory: PathBuf,
}

impl Library {
    /// Opens the library at the configured location.
    #[must_use]
    pub fn new(directory: &Path) -> Self {
        Self {
            directory: directory.to_path_buf(),
        }
    }

    /// Where the files are.
    #[must_use]
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// The file a name refers to, refusing anything that is not just a name.
    ///
    /// This is the lexical name boundary: a name may not contain a
    /// separator, may not be `.` or `..`, may not be empty, and may not carry
    /// control characters into a file name. Everything else in this module goes
    /// through here. Reads and writes also refuse linked/non-regular entries.
    pub fn path_for(&self, name: &str) -> Result<PathBuf, Diagnostic> {
        let trimmed = name.trim();
        let refuse = |reason: &str| {
            Err(Diagnostic::new(
                DiagnosticKind::Usage,
                format!("{name:?} is not a name a query can be saved under"),
                "naming a saved query",
            )
            .likely_cause(reason.to_owned())
            .next_action("use a plain name, for example monthly-revenue"))
        };

        if trimmed.is_empty() {
            return refuse("a name cannot be empty");
        }
        if trimmed == "." || trimmed == ".." || trimmed.contains("..") {
            return refuse("a name cannot walk up a directory");
        }
        if trimmed.contains('/') || trimmed.contains('\\') {
            return refuse("a name cannot contain a path separator");
        }
        if trimmed.contains(':') {
            // A Windows drive or stream specifier is a path, not a name.
            return refuse("a name cannot contain a colon");
        }
        if trimmed.chars().any(char::is_control) {
            return refuse("a name cannot contain control characters");
        }
        // Windows reserves these whatever extension follows them: `CON.sql`
        // opens the console, not a file. Refused on every platform, because a
        // saved query is meant to travel between them.
        let stem = trimmed.split('.').next().unwrap_or(trimmed);
        if RESERVED_ON_WINDOWS
            .iter()
            .any(|reserved| stem.eq_ignore_ascii_case(reserved))
        {
            return refuse("Windows reserves this name for a device");
        }

        let file = if trimmed
            .rsplit_once('.')
            .is_some_and(|(_, extension)| extension.eq_ignore_ascii_case(EXTENSION))
        {
            trimmed.to_owned()
        } else {
            format!("{trimmed}.{EXTENSION}")
        };
        Ok(self.directory.join(file))
    }

    /// Saves a query, replacing one of the same name.
    ///
    /// Replacing is the expected behaviour of saving under a name you chose;
    /// the editor is where the previous text was, and it is still there.
    pub fn save(&self, name: &str, sql: &str) -> Result<PathBuf, Diagnostic> {
        let path = self.path_for(name)?;
        std::fs::create_dir_all(&self.directory).map_err(|err| self.failure("creating", &err))?;
        self.check_entry(&path)?;
        let mut text = sql.to_owned();
        if !text.ends_with('\n') {
            text.push('\n');
        }
        crate::platform::write_atomic(&path, text.as_bytes())
            .map_err(|err| self.failure("writing to", &err))?;
        Ok(path)
    }

    /// Reads a saved query.
    pub fn load(&self, name: &str) -> Result<String, Diagnostic> {
        let path = self.path_for(name)?;
        self.check_entry(&path)?;
        std::fs::read_to_string(&path).map_err(|err| {
            Diagnostic::new(
                DiagnosticKind::Usage,
                format!("could not read {}", path.display()),
                "opening a saved query",
            )
            .likely_cause(err.to_string())
            .next_action("check the name, or list what is saved")
        })
    }

    /// Every saved query, by name.
    ///
    /// A file that cannot be read is skipped rather than failing the listing: a
    /// directory of `.sql` files is a place people put things by hand, and one
    /// unreadable file must not hide the rest.
    pub fn list(&self) -> Result<Vec<SavedQuery>, Diagnostic> {
        let entries = match std::fs::read_dir(&self.directory) {
            Ok(entries) => entries,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(err) => return Err(self.failure("reading", &err)),
        };

        let mut found = Vec::new();
        for entry in entries.flatten() {
            if !entry.file_type().is_ok_and(|kind| kind.is_file()) {
                continue;
            }
            let path = entry.path();
            if !path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case(EXTENSION))
            {
                continue;
            }
            let Some(name) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            let summary = std::fs::read_to_string(&path)
                .map_or_else(|_| "could not be read".to_owned(), |text| summarise(&text));
            found.push(SavedQuery {
                name: name.to_owned(),
                path: path.clone(),
                summary,
            });
        }
        found.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(found)
    }

    /// The configured root is trusted, but a named leaf must be a plain file.
    fn check_entry(&self, path: &Path) -> Result<(), Diagnostic> {
        match std::fs::symlink_metadata(path) {
            Ok(metadata) if metadata.is_file() => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(self.failure("reading", &error)),
            Ok(_) => Err(Diagnostic::new(
                DiagnosticKind::Usage,
                "a saved query must be a regular file, not a link or directory",
                "opening or saving a named query",
            )
            .next_action("choose another name, or replace the linked or non-regular entry with a plain .sql file")
            .technical("Path", path.display().to_string())),
        }
    }

    fn failure(&self, verb: &str, error: &std::io::Error) -> Diagnostic {
        Diagnostic::new(
            DiagnosticKind::Usage,
            format!("could not {verb} {}", self.directory.display()),
            "using the saved queries directory",
        )
        .likely_cause(error.to_string())
        .next_action("check the directory's permissions and free space")
    }
}

/// The first line worth showing from a saved query.
///
/// Comments are skipped: a file that starts with a header comment should be
/// listed by what it does, not by who wrote it.
fn summarise(text: &str) -> String {
    let line = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with("--"))
        .unwrap_or("");
    let sanitised = crate::query::sanitize_for_display(line);
    crate::query::truncate_to_width(&sanitised, 60, true)
}

#[cfg(test)]
mod tests;
