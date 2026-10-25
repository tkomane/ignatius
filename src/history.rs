//! What was run, and the rules about keeping it.
//!
//! A history is the difference between retyping a query and pressing a key, and
//! it is also a file of SQL sitting on a disk. Both facts are taken seriously:
//!
//! - **It stays on this machine.** One file, in the local data directory, owned
//!   by the user and readable only by them on Unix. Nothing is ever sent
//!   anywhere.
//! - **A statement that mentions a credential is never written.** The check is a
//!   word list matched as substrings, and it is a heuristic rather than a
//!   guarantee, which is exactly why it is not the only control: history can be
//!   turned off in configuration, paused for a session, and cleared.
//! - **Nothing is recorded silently that the user cannot see.** `history list`
//!   prints the file, `history clear` empties it, and `config paths` says where
//!   it is.
//! - **The connection target is redacted before it is written**, using the same
//!   single implementation every other output path uses.
//!
//! The file is JSON Lines: one object per line, appended. That survives a
//! partial write of the last line, which a single JSON document would not, and
//! it can be read by anything.

use crate::config::paths::Paths;
use crate::config::schema::HistoryConfig;
use crate::diagnostics::{Diagnostic, DiagnosticKind, redact_text};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};

/// How many past statements a session keeps in memory for its search.
///
/// The file may be far longer. This is a real bound, enforced when the history
/// is read and again as statements are added, not a limit on what is drawn.
pub const IN_MEMORY_LIMIT: usize = 200;

/// How a recorded statement ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Outcome {
    /// It succeeded.
    Succeeded,
    /// It failed.
    Failed,
    /// It was cancelled.
    Cancelled,
    /// The connection dropped, so what happened on the server is unknown.
    Unknown,
}

impl Outcome {
    /// The word shown in a listing.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Succeeded => "ok",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Unknown => "unknown",
        }
    }

    /// Reads an outcome from an execution status.
    #[must_use]
    pub const fn from_status(status: &crate::query::ExecutionStatus) -> Self {
        use crate::query::ExecutionStatus as S;
        match status {
            S::Succeeded => Self::Succeeded,
            S::Failed => Self::Failed,
            S::Cancelled => Self::Cancelled,
            S::ConnectionLost => Self::Unknown,
        }
    }
}

/// One statement, as it is written to the file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// When it ran, in RFC 3339 with an offset, so the file is unambiguous.
    pub at: String,
    /// The connection it ran on, already redacted.
    pub target: String,
    /// The database name.
    pub database: String,
    /// How that connection is classified, for example `production`.
    pub environment: String,
    /// The statement itself.
    pub sql: String,
    /// How it ended.
    pub outcome: Outcome,
    /// How long it took, measured client-side.
    pub elapsed_ms: u64,
}

impl Entry {
    /// Builds an entry, stamping it with the current local time.
    ///
    /// The clock is read here, at the edge, rather than anywhere a decision is
    /// made, so nothing that has to be tested depends on the time of day.
    #[must_use]
    pub fn now(
        target: &str,
        database: &str,
        environment: &str,
        sql: &str,
        outcome: Outcome,
        elapsed: std::time::Duration,
    ) -> Self {
        Self {
            at: chrono::Local::now().to_rfc3339(),
            target: redact_text(target),
            database: database.to_owned(),
            environment: environment.to_owned(),
            sql: sql.trim().to_owned(),
            outcome,
            elapsed_ms: u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX),
        }
    }

    /// The date and time as a person reads it, without the offset noise.
    #[must_use]
    pub fn when(&self) -> String {
        chrono::DateTime::parse_from_rfc3339(&self.at).map_or_else(
            |_| self.at.clone(),
            |parsed| parsed.format("%Y-%m-%d %H:%M:%S").to_string(),
        )
    }

    /// The statement on one line, for a listing.
    #[must_use]
    pub fn one_line(&self) -> String {
        let collapsed = self.sql.split_whitespace().collect::<Vec<_>>().join(" ");
        crate::query::sanitize_for_display(&collapsed)
    }
}

/// What happened to a statement offered for recording.
///
/// Every reason is named, because "it did not appear in my history" is a
/// question the user is entitled to an answer to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recorded {
    /// It was written.
    Written,
    /// History is off in configuration.
    Disabled,
    /// History is paused for this session.
    Paused,
    /// It mentioned a credential, so it was not written.
    LooksLikeACredential,
    /// There was nothing to write.
    Empty,
}

impl Recorded {
    /// Whether the statement reached the file.
    #[must_use]
    pub const fn was_written(self) -> bool {
        matches!(self, Self::Written)
    }

    /// A sentence for the interface, when there is something worth saying.
    #[must_use]
    pub const fn note(self) -> Option<&'static str> {
        match self {
            Self::LooksLikeACredential => {
                Some("Not added to history: this statement mentions a credential.")
            }
            Self::Written | Self::Disabled | Self::Paused | Self::Empty => None,
        }
    }
}

/// The statement history, and the rules about writing to it.
#[derive(Debug, Clone)]
pub struct History {
    path: PathBuf,
    max_entries: usize,
    enabled: bool,
    paused: bool,
}

impl History {
    /// Opens the history described by the configuration.
    ///
    /// `paused` comes from the session rather than the file: `--no-history` and
    /// the in-client toggle both stop recording without changing what the user
    /// has configured.
    #[must_use]
    pub fn open(paths: &Paths, config: &HistoryConfig, paused: bool) -> Self {
        Self {
            path: paths.history_file.clone(),
            max_entries: config.max_entries,
            enabled: config.enabled,
            paused,
        }
    }

    /// A history rooted at an explicit file, for tests.
    #[must_use]
    pub fn at(path: &Path, max_entries: usize) -> Self {
        Self {
            path: path.to_path_buf(),
            max_entries,
            enabled: true,
            paused: false,
        }
    }

    /// Where the file is.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Whether anything is being recorded at all.
    #[must_use]
    pub const fn is_recording(&self) -> bool {
        self.enabled && !self.paused
    }

    /// Stops recording for this session without changing the configuration.
    pub const fn pause(&mut self) {
        self.paused = true;
    }

    /// Resumes recording, if configuration allows it at all.
    pub const fn resume(&mut self) {
        self.paused = false;
    }

    /// Whether recording is paused for this session.
    #[must_use]
    pub const fn is_paused(&self) -> bool {
        self.paused
    }

    /// Offers a statement to the history.
    ///
    /// Returns why it was not written when it was not, and never fails the
    /// caller's work: a history that cannot be written is reported, but running
    /// a query is not conditional on keeping a record of it.
    pub fn record(&self, entry: &Entry) -> Result<Recorded, Diagnostic> {
        if !self.enabled {
            return Ok(Recorded::Disabled);
        }
        if self.paused {
            return Ok(Recorded::Paused);
        }
        if entry.sql.trim().is_empty() {
            return Ok(Recorded::Empty);
        }
        if mentions_a_credential(&entry.sql) {
            return Ok(Recorded::LooksLikeACredential);
        }

        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|err| self.failure("creating", &err))?;
        }
        let line = serde_json::to_string(entry).map_err(|err| {
            Diagnostic::new(
                DiagnosticKind::Internal,
                "a history entry could not be encoded",
                "recording a statement in the history",
            )
            .likely_cause(err.to_string())
        })?;

        let mut options = std::fs::OpenOptions::new();
        options.create(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            // The file holds SQL that was run against real databases. It is the
            // owner's, and nobody else's, from the moment it is created.
            options.mode(0o600);
        }
        let mut file = options
            .open(&self.path)
            .map_err(|err| self.failure("opening", &err))?;
        writeln!(file, "{line}").map_err(|err| self.failure("writing to", &err))?;

        self.prune()?;
        Ok(Recorded::Written)
    }

    /// Every entry, oldest first.
    ///
    /// A line that cannot be parsed is skipped rather than failing the read: a
    /// history damaged by a crash or by a hand edit should still give back the
    /// statements that survived.
    pub fn entries(&self) -> Result<Vec<Entry>, Diagnostic> {
        let text = match std::fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(err) => return Err(self.failure("reading", &err)),
        };
        Ok(text
            .lines()
            .filter_map(|line| serde_json::from_str::<Entry>(line).ok())
            .collect())
    }

    /// The most recent entries, newest first.
    pub fn recent(&self, limit: usize) -> Result<Vec<Entry>, Diagnostic> {
        let mut entries = self.entries()?;
        entries.reverse();
        entries.truncate(limit);
        Ok(entries)
    }

    /// Empties the history, removing the file entirely.
    pub fn clear(&self) -> Result<(), Diagnostic> {
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(self.failure("removing", &err)),
        }
    }

    /// Keeps the file to its configured length, newest kept.
    fn prune(&self) -> Result<(), Diagnostic> {
        if self.max_entries == 0 {
            return Ok(());
        }
        let entries = self.entries()?;
        if entries.len() <= self.max_entries {
            return Ok(());
        }
        let kept = &entries[entries.len() - self.max_entries..];
        let mut text = String::new();
        for entry in kept {
            if let Ok(line) = serde_json::to_string(entry) {
                text.push_str(&line);
                text.push('\n');
            }
        }
        // Written through a temporary file and renamed, so an interruption
        // leaves the old history rather than half of a new one.
        crate::platform::write_atomic(&self.path, text.as_bytes())
            .map_err(|err| self.failure("rewriting", &err))?;
        Ok(())
    }

    fn failure(&self, verb: &str, error: &std::io::Error) -> Diagnostic {
        Diagnostic::new(
            DiagnosticKind::Config,
            format!("could not {verb} {}", self.path.display()),
            "using the statement history",
        )
        .likely_cause(error.to_string())
        .next_action("check the directory's permissions, or set history.enabled = false")
    }
}

/// Words that stop a statement being written to disk.
///
/// Short and blunt on purpose. A statement about a `tokens` table is a small
/// loss; a password in a file that outlives the session is not.
const CREDENTIAL_WORDS: &[&str] = &[
    "apikey",
    "api_key",
    "credential",
    "credentials",
    "passwd",
    "password",
    "private_key",
    "secret",
    "token",
];

/// Whether a statement mentions a credential and so must not be recorded.
///
/// The match is on substrings, so `password_hash`, `access_token` and
/// `user_secret` are all caught. That over-refuses: a query about a
/// `tokens_used` column is not recorded either. The trade is deliberate and one
/// way round only - a missing history entry costs a retype, and a password in a
/// file that outlives the session costs considerably more.
///
/// This is a heuristic and is documented as one, which is exactly why it is not
/// the only control: history can be disabled, paused and cleared.
#[must_use]
pub fn mentions_a_credential(sql: &str) -> bool {
    let lowered = sql.to_lowercase();
    CREDENTIAL_WORDS.iter().any(|word| lowered.contains(word))
}

#[cfg(test)]
mod tests;
