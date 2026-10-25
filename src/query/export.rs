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
//! - **Say what happened.** An interrupted export distinguishes complete rows
//!   accepted by its writer from a confirmed partial-file flush. It preserves
//!   the partial path without claiming an unconfirmed on-disk row count.
//!
//! Rows are written as they arrive without retaining the complete result.
//! Individual values, rows, driver frames, encoding and file buffers use memory.

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
    /// Complete row encodings accepted by the export writer. Consult `flush`
    /// before claiming that this count reached the partial file.
    pub rows: u64,
    /// Whether the buffered-file flush succeeded; this is not fsync durability.
    pub flush: PartialFlush,
}

/// Confirmation available after attempting to flush an abandoned export.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartialFlush {
    /// The buffer was flushed successfully, without a durability guarantee.
    Flushed,
    /// Flushing failed, so the partial-file row count remains unconfirmed.
    Unconfirmed,
}

impl Abandoned {
    /// The sentence shown to the user. Names the count and the path, because
    /// "the export failed" leaves them wondering whether anything was written.
    #[must_use]
    pub fn message(&self) -> String {
        match self.flush {
            PartialFlush::Flushed => format!(
                "Export incomplete - {} row(s) written to {}",
                self.rows,
                self.partial.display()
            ),
            PartialFlush::Unconfirmed => format!(
                "Export incomplete - {} complete row(s) accepted by the export writer; partial-file row count is unconfirmed because flushing failed: {}",
                self.rows,
                self.partial.display()
            ),
        }
    }
}

impl Export {
    /// Opens an export, refusing to overwrite unless told to.
    ///
    /// The refusal's advice is the caller's, because the way out differs by
    /// surface: a script passes `--force`, and a person in the client has no
    /// flags to pass. Advice that names a flag the reader does not have is the
    /// same failure as telling someone to check a status bar that is not there.
    pub fn create(destination: &Path, force: bool) -> Result<Self, Diagnostic> {
        Self::create_with_advice(
            destination,
            force,
            "choose another path, or pass --force to replace it",
        )
    }

    /// Opens an export, saying what the reader can actually do about a refusal.
    pub fn create_with_advice(
        destination: &Path,
        force: bool,
        advice: &str,
    ) -> Result<Self, Diagnostic> {
        if destination.exists() && !force {
            return Err(Diagnostic::new(
                DiagnosticKind::Usage,
                format!("{} already exists", destination.display()),
                "opening the export destination",
            )
            .likely_cause("an export never replaces a file that is already there")
            .next_action(advice.to_owned()));
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

    /// Records one complete row encoding accepted by the export writer.
    pub const fn count_row(&mut self) {
        self.rows += 1;
    }

    /// Complete row encodings accepted so far, without a durability claim.
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
    pub fn finish(self) -> Result<Finished, Diagnostic> {
        self.finish_with(|writer| writer.flush())
    }

    fn finish_with(
        mut self,
        flush: impl FnOnce(&mut BufWriter<File>) -> std::io::Result<()>,
    ) -> Result<Finished, Diagnostic> {
        let outcome = flush(&mut self.writer)
            .and_then(|()| self.writer.get_ref().sync_all())
            .and_then(|()| std::fs::rename(&self.partial, &self.destination));

        if let Err(err) = outcome {
            let diagnostic = Diagnostic::new(
                DiagnosticKind::ExportInterrupted,
                format!("could not finish writing {}", self.destination.display()),
                "completing the export",
            )
            .likely_cause(err.to_string())
            .next_action(format!(
                "the partial file remains at {}; its on-disk row count is unconfirmed; inspect it before continuing",
                self.partial.display()
            ));
            return Err(diagnostic);
        }

        Ok(Finished {
            path: self.destination.clone(),
            rows: self.rows,
        })
    }

    /// Gives up, leaving the partial file where the user can find it.
    pub fn abandon(self) -> Abandoned {
        self.abandon_with(|writer| writer.flush())
    }

    fn abandon_with(
        mut self,
        flush: impl FnOnce(&mut BufWriter<File>) -> std::io::Result<()>,
    ) -> Abandoned {
        let flush = if flush(&mut self.writer).is_ok() {
            PartialFlush::Flushed
        } else {
            PartialFlush::Unconfirmed
        };
        // Preserve the buffered writer's existing best-effort close. Its result
        // is not reported, so it cannot upgrade explicit flush confirmation.
        Abandoned {
            partial: self.partial.clone(),
            rows: self.rows,
            flush,
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
mod tests;
