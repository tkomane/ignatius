//! Writing a result to a file, safely and without holding it in memory.
//!
//! Three rules, each of which exists because the alternative loses data:
//!
//! - **Never overwrite by accident.** An existing file stops the export unless
//!   the user explicitly said to replace it.
//! - **Write to a partial file and rename at the end.** A file that is present
//!   at the destination is a complete file. An interrupted export leaves its
//!   work behind under a `.partial` name rather than a truncated result that
//!   looks finished.
//! - **Say what happened.** An interrupted export reports how many rows reached
//!   the partial file and where it is, rather than implying the whole thing is
//!   gone.
//!
//! Rows are written as they arrive, so a hundred-million-row export uses the
//! same memory as a one-row export.

use crate::diagnostics::{Diagnostic, DiagnosticKind};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

/// Suffix given to a file that is still being written.
pub const PARTIAL_SUFFIX: &str = "partial";

/// An export in progress.
#[derive(Debug)]
pub struct Export {
    destination: PathBuf,
    partial: PathBuf,
    writer: BufWriter<File>,
    rows: u64,
}

/// How an export ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finished {
    /// Where the completed file is.
    pub path: PathBuf,
    /// How many rows it holds.
    pub rows: u64,
}

/// What was left behind when an export did not finish.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Abandoned {
    /// Where the partial file is.
    pub partial: PathBuf,
    /// How many rows reached it.
    pub rows: u64,
}

impl Abandoned {
    /// The sentence shown to the user. Names the count and the path, because
    /// "the export failed" leaves them wondering whether anything was written.
    #[must_use]
    pub fn message(&self) -> String {
        format!(
            "Export incomplete - {} row(s) written to {}",
            self.rows,
            self.partial.display()
        )
    }
}

impl Export {
    /// Opens an export, refusing to overwrite unless told to.
    pub fn create(destination: &Path, force: bool) -> Result<Self, Diagnostic> {
        if destination.exists() && !force {
            return Err(Diagnostic::new(
                DiagnosticKind::Usage,
                format!("{} already exists", destination.display()),
                "opening the export destination",
            )
            .likely_cause("an export never replaces a file that is already there")
            .next_action("choose another path, or pass --force to replace it"));
        }

        let partial = partial_path(destination);
        if let Some(parent) = destination.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(|err| {
                Diagnostic::new(
                    DiagnosticKind::Usage,
                    format!("could not create {}", parent.display()),
                    "opening the export destination",
                )
                .likely_cause(err.to_string())
                .next_action("check the path and its permissions")
            })?;
        }

        let file = File::create(&partial).map_err(|err| {
            Diagnostic::new(
                DiagnosticKind::Usage,
                format!("could not create {}", partial.display()),
                "opening the export destination",
            )
            .likely_cause(err.to_string())
            .next_action("check the directory's permissions and free space")
        })?;

        Ok(Self {
            destination: destination.to_path_buf(),
            partial,
            writer: BufWriter::new(file),
            rows: 0,
        })
    }

    /// The writer rows are written through.
    pub fn writer(&mut self) -> &mut impl Write {
        &mut self.writer
    }

    /// Records that one more row reached the file.
    pub const fn count_row(&mut self) {
        self.rows += 1;
    }

    /// Rows written so far.
    #[must_use]
    pub const fn rows(&self) -> u64 {
        self.rows
    }

    /// Where the partial file is, for messages.
    #[must_use]
    pub fn partial_path(&self) -> &Path {
        &self.partial
    }

    /// Flushes, syncs, and moves the partial file into place.
    ///
    /// The sync matters: a rename that completes before the data reaches the
    /// disk would leave a complete-looking file with missing rows after a crash.
    pub fn finish(mut self) -> Result<Finished, Diagnostic> {
        let outcome = self
            .writer
            .flush()
            .and_then(|()| self.writer.get_ref().sync_all())
            .and_then(|()| std::fs::rename(&self.partial, &self.destination));

        outcome.map_err(|err| {
            Diagnostic::new(
                DiagnosticKind::ExportInterrupted,
                format!("could not finish writing {}", self.destination.display()),
                "completing the export",
            )
            .likely_cause(err.to_string())
            .next_action(format!(
                "the rows written so far are in {}",
                self.partial.display()
            ))
        })?;

        Ok(Finished {
            path: self.destination.clone(),
            rows: self.rows,
        })
    }

    /// Gives up, leaving the partial file where the user can find it.
    pub fn abandon(mut self) -> Abandoned {
        // Best effort: the point is to keep whatever was written, so a failure
        // to flush here changes nothing that can still be reported.
        let _ = self.writer.flush();
        Abandoned {
            partial: self.partial.clone(),
            rows: self.rows,
        }
    }
}

/// The name of the partial file for a destination.
#[must_use]
pub fn partial_path(destination: &Path) -> PathBuf {
    let mut name = destination.as_os_str().to_os_string();
    name.push(".");
    name.push(PARTIAL_SUFFIX);
    PathBuf::from(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_partial_file_is_named_after_its_destination() {
        assert_eq!(
            partial_path(Path::new("/tmp/report.csv")),
            PathBuf::from("/tmp/report.csv.partial")
        );
        // The extension is added, not replaced, so the intended name is visible.
        assert_eq!(
            partial_path(Path::new("report")),
            PathBuf::from("report.partial")
        );
    }

    #[test]
    fn rows_land_in_the_partial_file_and_only_move_on_completion() {
        let dir = tempfile::tempdir().expect("temp dir");
        let destination = dir.path().join("report.csv");

        let mut export = Export::create(&destination, false).expect("create");
        write!(export.writer(), "id\n1\n").expect("write");
        export.count_row();

        assert!(
            !destination.exists(),
            "nothing appears at the destination until it is complete"
        );
        assert!(
            export.partial_path().exists(),
            "the work is in the partial file"
        );

        let finished = export.finish().expect("finish");
        assert_eq!(finished.rows, 1);
        assert_eq!(finished.path, destination);
        assert!(destination.exists());
        assert!(
            !partial_path(&destination).exists(),
            "the partial file is gone once it is complete"
        );
        assert_eq!(
            std::fs::read_to_string(&destination).expect("read"),
            "id\n1\n"
        );
    }

    #[test]
    fn abandoning_keeps_the_work_and_says_where_it_is() {
        let dir = tempfile::tempdir().expect("temp dir");
        let destination = dir.path().join("report.csv");

        let mut export = Export::create(&destination, false).expect("create");
        for row in 0..3 {
            writeln!(export.writer(), "{row}").expect("write");
            export.count_row();
        }
        let abandoned = export.abandon();

        assert_eq!(abandoned.rows, 3);
        assert!(abandoned.partial.exists(), "the rows written are kept");
        assert!(
            !destination.exists(),
            "an incomplete export never occupies the destination"
        );
        let message = abandoned.message();
        assert!(message.contains("Export incomplete"), "{message}");
        assert!(
            message.contains("3 row(s)"),
            "the count is stated: {message}"
        );
        assert!(
            message.contains("report.csv.partial"),
            "and the path: {message}"
        );
    }

    #[test]
    fn an_existing_file_is_never_replaced_by_accident() {
        let dir = tempfile::tempdir().expect("temp dir");
        let destination = dir.path().join("report.csv");
        std::fs::write(&destination, "precious data").expect("write");

        let error = Export::create(&destination, false).expect_err("must refuse");
        assert_eq!(error.exit_code(), crate::ExitCode::Usage);
        assert!(error.next_action.expect("action").contains("--force"));
        assert_eq!(
            std::fs::read_to_string(&destination).expect("read"),
            "precious data",
            "the existing file is untouched"
        );
    }

    #[test]
    fn force_replaces_the_file_but_only_once_the_new_one_is_complete() {
        let dir = tempfile::tempdir().expect("temp dir");
        let destination = dir.path().join("report.csv");
        std::fs::write(&destination, "old data").expect("write");

        let mut export = Export::create(&destination, true).expect("create with force");
        write!(export.writer(), "new data").expect("write");
        assert_eq!(
            std::fs::read_to_string(&destination).expect("read"),
            "old data",
            "the old file survives until the new one is finished"
        );

        export.finish().expect("finish");
        assert_eq!(
            std::fs::read_to_string(&destination).expect("read"),
            "new data"
        );
    }

    #[test]
    fn a_missing_parent_directory_is_created() {
        let dir = tempfile::tempdir().expect("temp dir");
        let destination = dir.path().join("nested").join("deeper").join("report.csv");
        let export = Export::create(&destination, false).expect("create");
        export.finish().expect("finish");
        assert!(destination.exists());
    }

    #[test]
    fn an_unwritable_destination_fails_with_an_action() {
        let error = Export::create(Path::new("/proc/definitely-not-writable/report.csv"), false)
            .expect_err("must fail");
        assert!(error.next_action.is_some());
        assert_eq!(error.exit_code(), crate::ExitCode::Usage);
    }
}
