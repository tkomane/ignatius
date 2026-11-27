# Implementation Plan: SQL Editor Power Tools

**Spec**: [spec.md](spec.md)

## Delivery Slices

Feature 070 (Milestone M24) is delivered across two pull requests:

### PR 2A - Editor Line Operations & Jump to Line (`feat/m24-editor-edit-ops`)

1. **`src/app/editor.rs` & `src/app/editor/tests.rs`**:
   - Implement `toggle_line_comment`, `duplicate_line`, `move_line_up`, `move_line_down`, and `jump_to_line` on `Editor`.
   - Ensure UTF-8 char-boundary safety, `end_undo_run()` isolation, single-snapshot undo/redo for edits, no-op boundary handling for `move_line_up`/`move_line_down`, and zero buffer/history mutation for `jump_to_line`.
2. **`src/app/message.rs`, `src/ui/keymap.rs`, `src/app/discovery.rs`, `docs/design/keymap.md`**:
   - Add `ToggleLineComment` (`"toggle-comment"`, `Ctrl+/`), `DuplicateLine` (`"duplicate-line"`, `Ctrl+Shift+D`), `MoveLineUp` (`"move-line-up"`, `Alt+Up`), `MoveLineDown` (`"move-line-down"`, `Alt+Down`), and `JumpToLine` (`"jump-to-line"`, `Ctrl+L`).
   - Expose all five in `Action::all()`, `CONFIGURABLE`, `discovery::palette_actions` (when `Focus::Editor`), and `docs/design/keymap.md`.
3. **`src/app/model.rs`, `src/app/update.rs`, `src/ui/widgets/overlays.rs`, `src/ui/widgets/footer.rs`**:
   - Add `NamePurpose::JumpToLine` and `NamePrompt::for_jump_to_line` for pure in-TUI line jumping.
   - Keep `BufferManager` active buffer synced via `edit_editor` and `sync_active_buffer_from_editor()`, protect covered editor state while Help is open, and surface invalid input notices via `model.buffer_notice`.

### PR 2B - In-Buffer Find & Replace (`feat/m24-editor-find-replace`)

1. **`src/app/editor.rs` & `src/app/model.rs`**:
   - Add find query, case-sensitivity toggle, match indexing, and single/batch replacement primitives.
2. **`src/app/update.rs`, `src/ui/keymap.rs`, `src/app/discovery.rs`, `src/ui/widgets/*`**:
   - Wire Find & Replace actions, bar/overlay rendering via `Theme`/`Token`, and tests.

## Verification Gates

- `cargo --locked xtask verify` (formatting, clippy, unit tests, UI/layout/doc/CLI contracts, and integration suite).
