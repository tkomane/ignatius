//! Session state persistence and automatic buffer crash recovery.
//!
//! The journal is an ordinary JSON file under the data directory. It is
//! written atomically, bounded in size, and only ever read to offer a recovery
//! decision. It is not a database and it is not a substitute for saving a
//! query the operator meant to keep.

use crate::buffer::{BufferManager, EditorBuffer, EditorSplitMode};
use crate::diagnostics::{Diagnostic, DiagnosticKind};
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

pub mod session;

pub use session::RecoverySession;

/// Current schema version for session recovery serialization.
pub const RECOVERY_SCHEMA_VERSION: u32 = 1;

/// The canonical directory name under the data directory.
pub const RECOVERY_DIR_NAME: &str = "recovery";

/// The canonical file name for session recovery journals.
pub const RECOVERY_FILE_NAME: &str = "session_journal.json";

/// Largest single buffer worth checkpointing. A buffer above this bound is
/// stored truncated and reported as such rather than silently cut.
pub const MAX_RECOVERED_BUFFER_BYTES: usize = 1024 * 1024;

/// Largest journal accepted from disk. A larger file is refused, not parsed.
pub const MAX_JOURNAL_BYTES: u64 = 24 * 1024 * 1024;

/// State of an individual recovered SQL editor buffer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveredBuffer {
    /// Unique identifier for this buffer session.
    pub id: u64,
    /// Human-readable buffer title.
    pub title: String,
    /// Optional underlying file path.
    pub file_path: Option<PathBuf>,
    /// The full SQL content in the buffer.
    pub sql: String,
    /// Cursor offset position within the buffer.
    pub cursor_offset: usize,
    /// Whether the buffer contained unsaved edits.
    pub dirty: bool,
    /// Whether `sql` was cut at [`MAX_RECOVERED_BUFFER_BYTES`].
    #[serde(default)]
    pub truncated: bool,
}

impl RecoveredBuffer {
    /// Captures one editor buffer, bounding its stored text.
    #[must_use]
    pub fn capture(buffer: &EditorBuffer) -> Self {
        let text = buffer.editor.text();
        let (sql, truncated) = if text.len() > MAX_RECOVERED_BUFFER_BYTES {
            let mut end = MAX_RECOVERED_BUFFER_BYTES;
            while end > 0 && !text.is_char_boundary(end) {
                end -= 1;
            }
            (text[..end].to_owned(), true)
        } else {
            (text.to_owned(), false)
        };
        Self {
            id: buffer.id,
            title: buffer.title.clone(),
            file_path: buffer.file_path.clone(),
            sql,
            cursor_offset: buffer.editor.cursor(),
            dirty: buffer.editor.is_modified(),
            truncated,
        }
    }
}

/// Persistent journal documenting active multi-tab editor state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryJournal {
    /// Schema format version.
    pub version: u32,
    /// Unique session identifier.
    pub session_id: String,
    /// Timestamp when session started (RFC3339).
    pub started_at: String,
    /// Timestamp when session was last checkpointed (RFC3339).
    pub saved_at: String,
    /// Whether this session was terminated cleanly.
    pub clean_exit: bool,
    /// The zero-based index of the active buffer.
    pub active_buffer_idx: usize,
    /// The active split view mode ("none", "vertical", "horizontal").
    pub split_mode: String,
    /// All saved buffer snapshots.
    pub buffers: Vec<RecoveredBuffer>,
}

impl RecoveryJournal {
    /// Constructs a recovery journal snapshot from an active `BufferManager`.
    #[must_use]
    pub fn from_buffers(buffers: &BufferManager, session_id: &str, clean_exit: bool) -> Self {
        let now = chrono::Local::now().to_rfc3339();
        Self::from_buffers_at(buffers, session_id, &now, clean_exit)
    }

    /// Constructs a snapshot with an explicit session start timestamp.
    #[must_use]
    pub fn from_buffers_at(
        buffers: &BufferManager,
        session_id: &str,
        started_at: &str,
        clean_exit: bool,
    ) -> Self {
        let split_mode = match buffers.split_mode {
            EditorSplitMode::None => "none",
            EditorSplitMode::Vertical => "vertical",
            EditorSplitMode::Horizontal => "horizontal",
        }
        .to_owned();

        Self {
            version: RECOVERY_SCHEMA_VERSION,
            session_id: session_id.to_owned(),
            started_at: started_at.to_owned(),
            saved_at: chrono::Local::now().to_rfc3339(),
            clean_exit,
            active_buffer_idx: buffers.active_idx,
            split_mode,
            buffers: buffers
                .buffers
                .iter()
                .map(RecoveredBuffer::capture)
                .collect(),
        }
    }

    /// Formatted human-readable timestamp of when the journal was last checkpointed.
    #[must_use]
    pub fn saved_when(&self) -> String {
        chrono::DateTime::parse_from_rfc3339(&self.saved_at).map_or_else(
            |_| self.saved_at.clone(),
            |parsed| parsed.format("%Y-%m-%d %H:%M:%S").to_string(),
        )
    }

    /// Returns true if this journal represents an unrecovered crash session with unsaved content.
    ///
    /// A lone untouched starter buffer is not a crash worth interrupting the
    /// operator for. Multiple open tabs, a loaded file, or an edited buffer
    /// are.
    #[must_use]
    pub fn has_recoverable_content(&self) -> bool {
        if self.clean_exit {
            return false;
        }
        let has_content = self.buffers.iter().any(|b| !b.sql.trim().is_empty());
        if !has_content {
            return false;
        }
        self.buffers.len() > 1
            || self
                .buffers
                .iter()
                .any(|b| !b.sql.trim().is_empty() && (b.dirty || b.file_path.is_some()))
    }

    /// Number of buffers that held content at capture time.
    #[must_use]
    pub fn recoverable_buffer_count(&self) -> usize {
        self.buffers
            .iter()
            .filter(|b| !b.sql.trim().is_empty())
            .count()
    }

    /// Number of buffers that were edited and never saved to a file.
    #[must_use]
    pub fn unsaved_buffer_count(&self) -> usize {
        self.buffers
            .iter()
            .filter(|b| !b.sql.trim().is_empty() && b.dirty && b.file_path.is_none())
            .count()
    }

    /// Number of buffers whose stored SQL was bounded at capture time.
    #[must_use]
    pub fn truncated_buffer_count(&self) -> usize {
        self.buffers.iter().filter(|b| b.truncated).count()
    }

    /// Restores the journal's snapshots into a target `BufferManager`.
    ///
    /// Buffers that were never written to are not restored: an empty tab is
    /// not worth a recovery decision.
    pub fn restore_into(&self, target: &mut BufferManager) -> usize {
        let valid_buffers: Vec<&RecoveredBuffer> = self
            .buffers
            .iter()
            .filter(|b| !b.sql.trim().is_empty() || b.file_path.is_some())
            .collect();

        if valid_buffers.is_empty() {
            return 0;
        }

        target.buffers.clear();
        for (i, b) in valid_buffers.iter().enumerate() {
            let mut ed_buf =
                EditorBuffer::new((i + 1) as u64, b.title.clone(), Some(b.sql.clone()));
            ed_buf.file_path = b.file_path.clone();
            ed_buf.editor.set_cursor(b.cursor_offset);
            ed_buf.editor.restore_modified(b.dirty);
            target.buffers.push(ed_buf);
        }

        target.active_idx = self
            .active_buffer_idx
            .min(target.buffers.len().saturating_sub(1));
        target.split_mode = match self.split_mode.as_str() {
            "vertical" => EditorSplitMode::Vertical,
            "horizontal" => EditorSplitMode::Horizontal,
            _ => EditorSplitMode::None,
        };
        if target.buffers.len() < 2 {
            // A split needs two panes; a single restored tab cannot fill one.
            target.split_mode = EditorSplitMode::None;
        }
        // A split with no distinct second pane renders a blank half. The
        // journal does not name the secondary tab, so choose one.
        target.secondary_idx = target
            .secondary_idx
            .filter(|idx| *idx < target.buffers.len() && *idx != target.active_idx)
            .or_else(|| {
                (target.split_mode != EditorSplitMode::None && target.buffers.len() > 1)
                    .then(|| (target.active_idx + 1) % target.buffers.len())
            });
        target.next_id = target
            .buffers
            .iter()
            .map(|buffer| buffer.id)
            .max()
            .unwrap_or(0)
            .saturating_add(1);

        valid_buffers.len()
    }

    /// Atomically persists the journal to `<recovery_dir>/session_journal.json`.
    ///
    /// The bytes go to a sibling temporary file, are flushed to the device, and
    /// only then does a rename publish them. A crash mid-write leaves the prior
    /// journal intact.
    pub fn save_to_disk(&self, recovery_dir: &Path) -> Result<PathBuf, Diagnostic> {
        use std::io::Write as _;

        let mut builder = std::fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt as _;
            builder.mode(0o700);
        }
        builder.create(recovery_dir).map_err(|e| {
            Diagnostic::new(
                DiagnosticKind::Usage,
                format!(
                    "failed to create recovery directory {}: {e}",
                    recovery_dir.display()
                ),
                "saving recovery journal",
            )
        })?;

        let file_path = recovery_dir.join(RECOVERY_FILE_NAME);

        let json = serde_json::to_string_pretty(self).map_err(|e| {
            Diagnostic::new(
                DiagnosticKind::Usage,
                format!("failed to serialize recovery journal: {e}"),
                "saving recovery journal",
            )
        })?;

        if json.len() as u64 > MAX_JOURNAL_BYTES {
            return Err(Diagnostic::new(
                DiagnosticKind::Usage,
                format!(
                    "recovery journal would be {} bytes, above the {MAX_JOURNAL_BYTES}-byte bound",
                    json.len()
                ),
                "saving recovery journal",
            )
            .likely_cause("too much editor content is open in one session")
            .next_action("save large buffers to files and close the tabs you do not need"));
        }

        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos());
        let tmp_path = recovery_dir.join(format!(
            "{RECOVERY_FILE_NAME}.tmp.{}.{nonce}",
            std::process::id()
        ));

        let mut file = {
            let mut options = std::fs::OpenOptions::new();
            // Exclusive creation, never reopening a pre-existing entry: a
            // same-user symlink planted at a predictable temp name must not be
            // followed or truncated.
            options.create_new(true).write(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt as _;
                options.mode(0o600);
            }
            options.open(&tmp_path)
        }
        .map_err(|e| {
            Diagnostic::new(
                DiagnosticKind::Usage,
                format!(
                    "failed to create temporary recovery file {}: {e}",
                    tmp_path.display()
                ),
                "saving recovery journal",
            )
        })?;
        if let Err(e) = file
            .write_all(json.as_bytes())
            .and_then(|()| file.sync_all())
        {
            let _ = std::fs::remove_file(&tmp_path);
            return Err(Diagnostic::new(
                DiagnosticKind::Usage,
                format!("failed to write recovery journal: {e}"),
                "saving recovery journal",
            ));
        }
        drop(file);

        if let Err(e) = std::fs::rename(&tmp_path, &file_path) {
            let _ = std::fs::remove_file(&tmp_path);
            return Err(Diagnostic::new(
                DiagnosticKind::Usage,
                format!("failed to atomically publish recovery journal: {e}"),
                "saving recovery journal",
            ));
        }

        // Make the rename itself durable where the platform supports it.
        #[cfg(unix)]
        if let Ok(dir) = std::fs::File::open(recovery_dir) {
            let _ = dir.sync_all();
        }

        Ok(file_path)
    }

    /// Loads the session journal from `<recovery_dir>/session_journal.json` if it exists.
    pub fn load_from_disk(recovery_dir: &Path) -> Result<Option<Self>, Diagnostic> {
        let file_path = recovery_dir.join(RECOVERY_FILE_NAME);
        let metadata = match std::fs::metadata(&file_path) {
            Ok(metadata) => metadata,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => {
                return Err(Diagnostic::new(
                    DiagnosticKind::Usage,
                    format!(
                        "failed to inspect recovery journal {}: {e}",
                        file_path.display()
                    ),
                    "loading recovery journal",
                ));
            }
        };
        if metadata.len() > MAX_JOURNAL_BYTES {
            return Err(Diagnostic::new(
                DiagnosticKind::Usage,
                format!(
                    "recovery journal is {} bytes, above the {MAX_JOURNAL_BYTES}-byte bound",
                    metadata.len()
                ),
                "loading recovery journal",
            )
            .likely_cause("the journal was not written by this build or was damaged")
            .next_action("run `ignatius recovery discard` to remove it"));
        }

        let content = std::fs::read_to_string(&file_path).map_err(|e| {
            Diagnostic::new(
                DiagnosticKind::Usage,
                format!(
                    "failed to read recovery journal {}: {e}",
                    file_path.display()
                ),
                "loading recovery journal",
            )
        })?;

        let journal: Self = serde_json::from_str(&content).map_err(|e| {
            Diagnostic::new(
                DiagnosticKind::Usage,
                format!("failed to parse recovery journal JSON: {e}"),
                "loading recovery journal",
            )
            .likely_cause("the journal was damaged or came from an incompatible build")
            .next_action("run `ignatius recovery discard` to remove it")
        })?;

        if journal.version > RECOVERY_SCHEMA_VERSION {
            return Err(Diagnostic::new(
                DiagnosticKind::Usage,
                format!(
                    "recovery journal version {} is newer than this build understands ({RECOVERY_SCHEMA_VERSION})",
                    journal.version
                ),
                "loading recovery journal",
            )
            .next_action("upgrade Ignatius, or run `ignatius recovery discard` to remove it"));
        }

        Ok(Some(journal))
    }

    /// Discards and deletes the recovery journal file from disk.
    pub fn discard(recovery_dir: &Path) -> Result<(), Diagnostic> {
        let file_path = recovery_dir.join(RECOVERY_FILE_NAME);
        match std::fs::remove_file(&file_path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(Diagnostic::new(
                DiagnosticKind::Usage,
                format!(
                    "failed to remove recovery journal {}: {e}",
                    file_path.display()
                ),
                "discarding recovery journal",
            )),
        }
    }

    /// Updates the journal on disk with `clean_exit: true`.
    pub fn mark_clean_exit(recovery_dir: &Path) -> Result<(), Diagnostic> {
        if let Some(mut journal) = Self::load_from_disk(recovery_dir)? {
            journal.clean_exit = true;
            journal.saved_at = chrono::Local::now().to_rfc3339();
            journal.save_to_disk(recovery_dir)?;
        }
        Ok(())
    }
}

/// A cheap structural identity for a buffer workspace.
///
/// Tab count, order, identities, titles, paths and layout change on the actions
/// a user would be upset to lose, so they trigger an immediate checkpoint.
/// Text itself is deliberately excluded: ordinary typing is throttled by
/// [`RecoverySession`] and would otherwise write on every keystroke.
#[must_use]
pub fn structure_signature(buffers: &BufferManager) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    buffers.buffers.len().hash(&mut hasher);
    buffers.active_idx.hash(&mut hasher);
    buffers.split_mode.label().hash(&mut hasher);
    buffers.secondary_idx.hash(&mut hasher);
    for buffer in &buffers.buffers {
        buffer.id.hash(&mut hasher);
        buffer.title.hash(&mut hasher);
        buffer.file_path.hash(&mut hasher);
    }
    hasher.finish()
}

/// A content identity covering text and cursor position.
#[must_use]
pub fn content_signature(buffers: &BufferManager) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for buffer in &buffers.buffers {
        buffer.editor.text().hash(&mut hasher);
        buffer.editor.cursor().hash(&mut hasher);
    }
    hasher.finish()
}

#[cfg(test)]
mod tests;
