# Architecture & Implementation Plan: Multi-Tab and Split Editor Buffers (Feature 041)

## 1. Domain Modeling: `src/buffer/`
We will introduce a clean, pure module `src/buffer/mod.rs` (and sidecar `tests.rs`):
- `BufferId`: unique incremental identifier (`u64`).
- `EditorSplitMode`:
  ```rust
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
  pub enum EditorSplitMode {
      #[default]
      None,
      Vertical,
      Horizontal,
  }
  ```
- `EditorBuffer`:
  ```rust
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct EditorBuffer {
      pub id: u64,
      pub title: String,
      pub editor: crate::app::editor::Editor,
      pub file_path: Option<PathBuf>,
  }
  ```
- `BufferManager`:
  ```rust
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct BufferManager {
      pub buffers: Vec<EditorBuffer>,
      pub active_idx: usize,
      pub split_mode: EditorSplitMode,
      pub secondary_idx: Option<usize>,
      pub next_id: u64,
  }
  ```
  Methods:
  - `new() -> Self` (initializes with Buffer 1: "buffer-1.sql")
  - `active_buffer(&self) -> &EditorBuffer` / `active_buffer_mut(&mut self) -> &mut EditorBuffer`
  - `active_editor(&self) -> &Editor` / `active_editor_mut(&mut self) -> &mut Editor`
  - `new_buffer(&mut self, title: Option<String>, initial_text: Option<String>) -> usize` (capped at 16)
  - `close_active_buffer(&mut self) -> bool`
  - `next_buffer(&mut self)`
  - `prev_buffer(&mut self)`
  - `switch_to(&mut self, idx: usize)`
  - `cycle_split_mode(&mut self)`
  - `set_split_mode(&mut self, mode: EditorSplitMode)`

## 2. Integration into `Model` & Elm/TEA Architecture
- In `src/app/model.rs`:
  - `Model` holds `pub buffers: BufferManager`.
  - For backward compatibility and seamless refactoring, `model.editor` methods or forwarding can delegate to `model.buffers.active_editor()`, or `model.editor` can remain synchronized with the active buffer's editor.
  - Adding `pub buffers: crate::buffer::BufferManager` to `Model`.
- In `src/app/message.rs`:
  - Add actions:
    - `Action::NewEditorBuffer`
    - `Action::CloseEditorBuffer`
    - `Action::NextEditorBuffer`
    - `Action::PrevEditorBuffer`
    - `Action::ToggleEditorSplit`
- In `src/ui/keymap.rs`:
  - Register configurable action names:
    - `new-buffer`
    - `close-buffer`
    - `next-buffer`
    - `prev-buffer`
    - `split-buffer`
  - Register chords:
    - `('=', Action::NewEditorBuffer, "Create new SQL editor buffer")`
    - `('-', Action::CloseEditorBuffer, "Close current SQL editor buffer")`
    - `('}', Action::NextEditorBuffer, "Switch to next SQL editor buffer")`
    - ('{', Action::PrevEditorBuffer, "Switch to previous SQL editor buffer")`
    - `('\\', Action::ToggleEditorSplit, "Toggle editor buffer split mode")`
- In `src/app/update.rs`:
  - Handle the actions, mutating `model.buffers` and updating active editor text/cursor.
- In `src/ui/widgets/editor.rs`:
  - Render top tab bar with Dieter Rams hardware capsules and tab pills.
  - If `split_mode != None` and secondary buffer exists, split editor area into two sub-panes (horizontal or vertical) and render both buffers.

## 3. CLI Parity
- Add `--file <PATH>` (multiple allowed) to CLI options to preload files into distinct editor tabs on startup.
- Contract test `tests/multi_buffer_contract.rs`.
