//! Multi-tab and split editor buffer management.

use crate::app::editor::Editor;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Maximum number of concurrent editor buffers.
pub const MAX_EDITOR_BUFFERS: usize = 16;

/// Layout split mode for editor buffers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum EditorSplitMode {
    /// Single buffer fills the entire editor pane.
    #[default]
    None,
    /// Dual side-by-side vertical split.
    Vertical,
    /// Dual stacked horizontal split.
    Horizontal,
}

impl EditorSplitMode {
    /// Human-readable label for status capsules.
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::None => "OFF",
            Self::Vertical => "VERTICAL",
            Self::Horizontal => "HORIZONTAL",
        }
    }

    /// Cycles to the next split mode (None -> Vertical -> Horizontal -> None).
    #[must_use]
    pub const fn next(&self) -> Self {
        match self {
            Self::None => Self::Vertical,
            Self::Vertical => Self::Horizontal,
            Self::Horizontal => Self::None,
        }
    }
}

/// An individual named SQL editor buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorBuffer {
    /// Unique buffer identity.
    pub id: u64,
    /// Display title (e.g. "buffer-1.sql" or custom/file name).
    pub title: String,
    /// The SQL editor containing buffer text, cursor, and undo/redo history.
    pub editor: Editor,
    /// Optional underlying filesystem path if saved or loaded.
    pub file_path: Option<PathBuf>,
}

impl EditorBuffer {
    /// Creates a new editor buffer with the given id, title, and initial text.
    #[must_use]
    pub fn new(id: u64, title: impl Into<String>, initial_text: Option<String>) -> Self {
        let editor = if let Some(txt) = initial_text {
            Editor::with_text(txt)
        } else {
            Editor::default()
        };
        Self {
            id,
            title: title.into(),
            editor,
            file_path: None,
        }
    }

    /// Display label with modification indicator if modified.
    #[must_use]
    pub fn display_label(&self) -> String {
        if self.editor.is_modified() {
            format!("{} *", self.title)
        } else {
            self.title.clone()
        }
    }
}

/// Manager coordinating multiple SQL editor tabs and split views.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BufferManager {
    /// All currently open editor buffers (1..=MAX_EDITOR_BUFFERS).
    pub buffers: Vec<EditorBuffer>,
    /// Index of the primary active buffer in `buffers`.
    pub active_idx: usize,
    /// Split mode for dual-pane editing.
    pub split_mode: EditorSplitMode,
    /// Index of the secondary buffer shown in split mode, if any.
    pub secondary_idx: Option<usize>,
    /// Next monotonic buffer identity.
    pub next_id: u64,
}

impl Default for BufferManager {
    fn default() -> Self {
        Self::new()
    }
}

impl BufferManager {
    /// Creates a new buffer manager initialized with a single scratch buffer.
    #[must_use]
    pub fn new() -> Self {
        let initial_buffer = EditorBuffer::new(1, "buffer-1.sql", None);
        Self {
            buffers: vec![initial_buffer],
            active_idx: 0,
            split_mode: EditorSplitMode::None,
            secondary_idx: None,
            next_id: 2,
        }
    }

    /// Number of open buffers.
    #[must_use]
    pub fn count(&self) -> usize {
        self.buffers.len()
    }

    /// The active buffer.
    #[must_use]
    pub fn active_buffer(&self) -> &EditorBuffer {
        &self.buffers[self.active_idx.min(self.buffers.len().saturating_sub(1))]
    }

    /// Mutable reference to the active buffer.
    pub fn active_buffer_mut(&mut self) -> &mut EditorBuffer {
        let idx = self.active_idx.min(self.buffers.len().saturating_sub(1));
        &mut self.buffers[idx]
    }

    /// The active editor.
    #[must_use]
    pub fn active_editor(&self) -> &Editor {
        &self.active_buffer().editor
    }

    /// Mutable reference to the active editor.
    pub fn active_editor_mut(&mut self) -> &mut Editor {
        &mut self.active_buffer_mut().editor
    }

    /// The secondary buffer when in split mode.
    #[must_use]
    pub fn secondary_buffer(&self) -> Option<&EditorBuffer> {
        if self.split_mode == EditorSplitMode::None {
            return None;
        }
        let sec_idx = self.secondary_idx?;
        self.buffers.get(sec_idx)
    }

    /// Mutable reference to the secondary buffer.
    pub fn secondary_buffer_mut(&mut self) -> Option<&mut EditorBuffer> {
        if self.split_mode == EditorSplitMode::None {
            return None;
        }
        let sec_idx = self.secondary_idx?;
        self.buffers.get_mut(sec_idx)
    }

    /// Adds a new buffer and focuses it, returning the new active index.
    /// Returns current active index if already at `MAX_EDITOR_BUFFERS`.
    pub fn new_buffer(&mut self, title: Option<String>, initial_text: Option<String>) -> usize {
        if self.buffers.len() >= MAX_EDITOR_BUFFERS {
            return self.active_idx;
        }
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        let default_title = format!("buffer-{id}.sql");
        let title = title.unwrap_or(default_title);
        let new_buf = EditorBuffer::new(id, title, initial_text);
        self.buffers.push(new_buf);
        self.active_idx = self.buffers.len() - 1;
        self.active_idx
    }

    /// Closes the currently active buffer.
    /// If it is the last remaining buffer, resets it to an empty default buffer.
    /// Returns true if a buffer was closed/reset.
    pub fn close_active(&mut self) -> bool {
        if self.buffers.len() <= 1 {
            // Reset lone buffer
            self.buffers[0] = EditorBuffer::new(self.next_id, "buffer-1.sql", None);
            self.next_id = self.next_id.wrapping_add(1);
            self.active_idx = 0;
            self.split_mode = EditorSplitMode::None;
            self.secondary_idx = None;
            return true;
        }

        self.buffers.remove(self.active_idx);
        if self.active_idx >= self.buffers.len() {
            self.active_idx = self.buffers.len().saturating_sub(1);
        }

        // Adjust secondary_idx if affected
        if let Some(sec) = self.secondary_idx {
            if sec >= self.buffers.len() {
                self.secondary_idx = if self.buffers.len() > 1 {
                    Some(0)
                } else {
                    None
                };
            }
            if self.secondary_idx == Some(self.active_idx) {
                // Secondary cannot be identical to active
                self.secondary_idx = self
                    .buffers
                    .iter()
                    .enumerate()
                    .position(|(i, _)| i != self.active_idx);
            }
        }

        true
    }

    /// Switches active buffer to the specified index if within bounds.
    pub fn switch_to(&mut self, idx: usize) {
        if idx < self.buffers.len() {
            self.active_idx = idx;
        }
    }

    /// Cycles active buffer to the next tab.
    pub fn next_buffer(&mut self) {
        if !self.buffers.is_empty() {
            self.active_idx = (self.active_idx + 1) % self.buffers.len();
        }
    }

    /// Cycles active buffer to the previous tab.
    pub fn prev_buffer(&mut self) {
        if !self.buffers.is_empty() {
            self.active_idx = if self.active_idx == 0 {
                self.buffers.len() - 1
            } else {
                self.active_idx - 1
            };
        }
    }

    /// Cycles the editor split mode (None -> Vertical -> Horizontal -> None).
    pub fn cycle_split_mode(&mut self) {
        self.split_mode = self.split_mode.next();
        if self.split_mode != EditorSplitMode::None {
            // Ensure secondary_idx is pointing to a different buffer if available
            if self.buffers.len() > 1 {
                if self.secondary_idx.is_none() || self.secondary_idx == Some(self.active_idx) {
                    self.secondary_idx = self
                        .buffers
                        .iter()
                        .enumerate()
                        .position(|(i, _)| i != self.active_idx);
                }
            } else {
                // Cannot split with only 1 buffer
                self.split_mode = EditorSplitMode::None;
                self.secondary_idx = None;
            }
        } else {
            self.secondary_idx = None;
        }
    }

    /// Loads multiple files into buffers. If files are provided, the first file replaces
    /// the initial empty buffer, and subsequent files are added as new buffers.
    pub fn load_files(&mut self, files: &[PathBuf]) {
        if files.is_empty() {
            return;
        }
        for (i, path) in files.iter().enumerate() {
            if self.buffers.len() >= MAX_EDITOR_BUFFERS && i > 0 {
                break;
            }
            let title = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("buffer.sql")
                .to_string();
            let content = std::fs::read_to_string(path).ok();
            if i == 0 {
                self.buffers[0] = EditorBuffer::new(self.buffers[0].id, title, content);
                self.buffers[0].file_path = Some(path.clone());
            } else {
                let idx = self.new_buffer(Some(title), content);
                self.buffers[idx].file_path = Some(path.clone());
            }
        }
        self.active_idx = 0;
    }
}

#[cfg(test)]
mod tests;
