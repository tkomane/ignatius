//! Runtime checkpoints for the crash recovery journal.
//!
//! The reducer stays pure. This type owns the wall clock, the session identity
//! and the decision to write, so the interactive runtime only has to say "look
//! at the current buffers" and "the session is ending".

use super::{RecoveryJournal, content_signature, structure_signature};
use crate::buffer::BufferManager;
use crate::diagnostics::Diagnostic;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Ordinary typing is checkpointed at most this often. Structural changes
/// (new tab, close tab, split, save path) bypass the throttle.
pub const CHECKPOINT_INTERVAL: Duration = Duration::from_secs(2);

/// Owns recovery persistence for one interactive session.
#[derive(Debug)]
pub struct RecoverySession {
    dir: PathBuf,
    session_id: String,
    started_at: String,
    enabled: bool,
    /// Set when an unrecovered journal needs a user decision. Nothing may
    /// overwrite it before that decision.
    pending: bool,
    /// Set when the journal on disk could not be read. Preserve it.
    blocked: bool,
    last_write: Option<Instant>,
    last_structure: Option<u64>,
    last_content: Option<u64>,
}

impl RecoverySession {
    /// Builds a session rooted at the given recovery directory.
    #[must_use]
    pub fn new(dir: PathBuf, enabled: bool) -> Self {
        let now = chrono::Local::now();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos());
        Self {
            dir,
            session_id: format!("{}-{nanos}", std::process::id()),
            started_at: now.to_rfc3339(),
            enabled,
            pending: false,
            blocked: false,
            last_write: None,
            last_structure: None,
            last_content: None,
        }
    }

    /// Builds a session from resolved paths.
    #[must_use]
    pub fn from_paths(paths: &crate::config::Paths, enabled: bool) -> Self {
        Self::new(paths.data_dir.join(super::RECOVERY_DIR_NAME), enabled)
    }

    /// Recovery directory this session writes to.
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Whether persistence is on for this session.
    #[must_use]
    pub const fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Whether an undecided journal is being preserved.
    #[must_use]
    pub const fn is_pending(&self) -> bool {
        self.pending
    }

    /// Loads the journal left by a previous session.
    ///
    /// A clean journal is discarded. A journal with content is returned and
    /// marks this session pending, so checkpoints cannot overwrite it until
    /// [`Self::decide`] is called. A damaged journal returns an error and
    /// blocks checkpoints rather than destroying evidence.
    pub fn take_startup(&mut self) -> Result<Option<RecoveryJournal>, Diagnostic> {
        if !self.enabled {
            return Ok(None);
        }
        match RecoveryJournal::load_from_disk(&self.dir) {
            Ok(Some(journal)) if journal.has_recoverable_content() => {
                self.pending = true;
                Ok(Some(journal))
            }
            Ok(Some(_)) => {
                // A clean exit, or nothing worth recovering. Start fresh.
                RecoveryJournal::discard(&self.dir)?;
                Ok(None)
            }
            Ok(None) => Ok(None),
            Err(error) => {
                self.blocked = true;
                Err(error)
            }
        }
    }

    /// Acknowledge that the operator decided what to do with a pending journal.
    pub fn decide(&mut self) {
        self.pending = false;
        self.last_write = None;
        self.last_structure = None;
        self.last_content = None;
    }

    /// Writes a checkpoint when one is due.
    ///
    /// Returns `true` when bytes were written. Never writes while a recovery
    /// decision is pending, while blocked, or when disabled.
    pub fn checkpoint(&mut self, buffers: &BufferManager, force: bool) -> Result<bool, Diagnostic> {
        if !self.enabled || self.pending || self.blocked {
            return Ok(false);
        }
        let has_content = self.dir.join(super::RECOVERY_FILE_NAME).exists()
            || buffers
                .buffers
                .iter()
                .any(|buffer| !buffer.editor.text().trim().is_empty());
        if !has_content {
            return Ok(false);
        }

        let structure = structure_signature(buffers);
        let structural_change = self.last_structure != Some(structure);
        let due = self
            .last_write
            .is_none_or(|at| at.elapsed() >= CHECKPOINT_INTERVAL);

        if !force && !structural_change && !due {
            return Ok(false);
        }

        let content = content_signature(buffers);
        if !force && !structural_change && self.last_content == Some(content) {
            return Ok(false);
        }

        let journal =
            RecoveryJournal::from_buffers_at(buffers, &self.session_id, &self.started_at, false);
        // An oversized journal is refused with the actionable diagnostic in
        // save_to_disk, and the runtime surfaces it once rather than silently
        // dropping crash protection.
        journal.save_to_disk(&self.dir)?;
        self.last_write = Some(Instant::now());
        self.last_structure = Some(structure);
        self.last_content = Some(content);
        Ok(true)
    }

    /// Marks the journal as a clean exit.
    ///
    /// This only writes when the session is enabled and no recovery decision
    /// is outstanding: quitting without deciding must not replace the crashed
    /// journal with the empty workspace it landed on.
    pub fn finish(&mut self, buffers: &BufferManager) -> Result<(), Diagnostic> {
        if !self.enabled || self.pending || self.blocked {
            return Ok(());
        }
        let journal =
            RecoveryJournal::from_buffers_at(buffers, &self.session_id, &self.started_at, true);
        journal.save_to_disk(&self.dir)?;
        Ok(())
    }
}
