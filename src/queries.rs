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
    /// This is the whole of the path safety: a name may not contain a
    /// separator, may not be `.` or `..`, may not be empty, and may not carry
    /// control characters into a file name. Everything else in this module goes
    /// through here.
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
        let mut text = sql.to_owned();
        if !text.ends_with('\n') {
            text.push('\n');
        }
        std::fs::write(&path, text).map_err(|err| self.failure("writing to", &err))?;
        Ok(path)
    }

    /// Reads a saved query.
    pub fn load(&self, name: &str) -> Result<String, Diagnostic> {
        let path = self.path_for(name)?;
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
mod tests {
    use super::*;

    fn library(dir: &tempfile::TempDir) -> Library {
        Library::new(&dir.path().join("queries"))
    }

    #[test]
    fn a_query_is_saved_as_an_ordinary_sql_file_and_read_back() {
        let dir = tempfile::tempdir().expect("temp dir");
        let library = library(&dir);

        let path = library
            .save("monthly-revenue", "SELECT sum(total) FROM orders;")
            .expect("save");
        assert_eq!(path.extension().and_then(|e| e.to_str()), Some("sql"));
        assert_eq!(
            std::fs::read_to_string(&path).expect("read"),
            "SELECT sum(total) FROM orders;\n",
            "it is the SQL, with a final newline, and nothing of ours around it"
        );
        assert_eq!(
            library.load("monthly-revenue").expect("load"),
            "SELECT sum(total) FROM orders;\n"
        );

        // The extension is not doubled when the name already has it.
        let path = library
            .save("with-extension.sql", "SELECT 1")
            .expect("save");
        assert_eq!(
            path.file_name().and_then(|n| n.to_str()),
            Some("with-extension.sql")
        );
    }

    #[test]
    fn a_name_can_never_reach_outside_the_directory() {
        // This is the security boundary of the feature: a name is a name.
        let dir = tempfile::tempdir().expect("temp dir");
        let library = library(&dir);

        for name in [
            "../escape",
            "..",
            ".",
            "sub/dir",
            "sub\\dir",
            "/etc/passwd",
            "C:\\windows\\system32",
            "",
            "   ",
            "with\nnewline",
            "with\u{1b}escape",
        ] {
            let error = library.path_for(name).expect_err(name);
            assert_eq!(error.exit_code(), crate::ExitCode::Usage, "{name}");
            assert!(error.next_action.is_some(), "{name}");
        }

        // And saving refuses for the same reason, before touching anything.
        assert!(library.save("../escape", "SELECT 1").is_err());
        assert!(
            !dir.path().join("escape.sql").exists(),
            "nothing was written outside the directory"
        );
    }

    #[test]
    fn the_listing_is_by_name_and_says_what_each_one_does() {
        let dir = tempfile::tempdir().expect("temp dir");
        let library = library(&dir);
        library
            .save(
                "second",
                "-- a header comment\n-- another\nSELECT 2 FROM orders;",
            )
            .expect("save");
        library.save("first", "SELECT 1;").expect("save");
        // Something that is not a saved query at all.
        std::fs::write(library.directory().join("notes.txt"), "ignore me").expect("write");

        let found = library.list().expect("list");
        assert_eq!(found.len(), 2, "only .sql files: {found:?}");
        assert_eq!(found[0].name, "first");
        assert_eq!(found[1].name, "second");
        assert_eq!(
            found[1].summary, "SELECT 2 FROM orders;",
            "the header comment is skipped: a query is listed by what it does"
        );
    }

    #[test]
    fn a_directory_that_does_not_exist_yet_lists_nothing_rather_than_failing() {
        let dir = tempfile::tempdir().expect("temp dir");
        assert!(library(&dir).list().expect("list").is_empty());
    }

    #[test]
    fn a_hostile_saved_query_cannot_drive_the_terminal_from_the_listing() {
        let dir = tempfile::tempdir().expect("temp dir");
        let library = library(&dir);
        library.save("hostile", "\u{1b}[2JSELECT 1;").expect("save");
        let found = library.list().expect("list");
        assert!(!found[0].summary.contains('\u{1b}'), "{}", found[0].summary);
    }

    #[test]
    fn saving_under_a_name_that_exists_replaces_it() {
        let dir = tempfile::tempdir().expect("temp dir");
        let library = library(&dir);
        library.save("q", "SELECT 1;").expect("save");
        library.save("q", "SELECT 2;").expect("save");
        assert_eq!(library.load("q").expect("load"), "SELECT 2;\n");
        assert_eq!(library.list().expect("list").len(), 1);
    }
}
