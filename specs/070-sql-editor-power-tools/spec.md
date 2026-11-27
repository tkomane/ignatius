# Feature Specification: SQL Editor Power Tools (Line Operations, Jump to Line, and Find & Replace)

**Feature Branch**: `feat/m24-editor-edit-ops` (PR 2A) and `feat/m24-editor-find-replace` (PR 2B)
**Created**: 2026-11-27
**Status**: Slice 1 (PR 2A) implemented; Slice 2 (PR 2B) planned
**Milestone**: M24 - SQL Editor Power Tools

## Problem Statement

Long multi-statement SQL buffers in Ignatius currently lack everyday editor productivity primitives:

1. **Line-level editing operations and direct line navigation require tedious manual cursor movement.** Operators cannot toggle `--` SQL comments, duplicate a line, move lines up or down, or jump directly to a target line number from the keyboard or Command Palette.
2. **In-buffer Find and Replace is missing.** Operators inspecting or refactoring large SQL scripts cannot search case-insensitively or replace occurrences inside the active editor buffer without external tools.

## Principles This Feature Must Keep

- **Principle I (Delight must remain truthful).** Invalid `Jump to line` inputs (`0`, non-integer, or empty) report a clear notice and leave the editor cursor and buffer untouched; no-op line moves on the first or last line do not fabricate undo entries or dirty flags.
- **Principle III (Local-first and private by default).** All editor line manipulations, line jumps, and find/replace operations run purely in memory with zero disk I/O and zero database calls.
- **Principle V (Keyboard-first, never keyboard-only).** Every new editor action is reachable through configurable `[keys]`, default bindings, and the Command Palette without stealing printable typing keys or requiring Vim knowledge.
- **Principle VI (Accessible meaning survives styling loss).** Prompts and footer notices communicate state in plain text across Unicode, ASCII, `Basic16`, and `NO_COLOR` modes.

## User Scenarios & Testing

### User Story 1 - Line Commenting, Duplication, Reordering, and Jump to Line (Priority: P1, Slice 1 / PR 2A)

While writing or editing SQL in the Editor pane, an operator can:
- Toggle `-- ` comments on the current line (`Ctrl+/` / `toggle-comment`), preserving leading indentation and collapsing both `-- ` and `--` back to uncommented text as one undoable step.
- Duplicate the current line immediately below (`Ctrl+Shift+D` / `duplicate-line`), keeping the caret at the same character column as one undoable step.
- Swap the current line with the line above or below (`Alt+Up` / `move-line-up`, `Alt+Down` / `move-line-down`), preserving character column as one undoable step and returning a clean no-op at buffer boundaries.
- Jump directly to a 1-based line number (`Ctrl+L` / `jump-to-line`) via an in-TUI single-line prompt (`NamePrompt`), clamping `1..=line_count`, placing the caret on the first non-whitespace byte (or line end if whitespace-only), leaving revision/modified/undo history untouched, and reporting a clear notice on invalid input (`0` or non-integer).

### User Story 2 - In-Buffer Find and Replace (Priority: P1, Slice 2 / PR 2B)

An operator can open an in-editor Find & Replace bar/overlay to search forward/backward with match counts, toggle case sensitivity, and replace the current match or all matches as a single undoable edit.

## Functional Requirements

### Slice 1 - Line Manipulation Primitives and Jump to Line (PR 2A)

- **FR-001**: Add `Editor::toggle_line_comment(&mut self) -> bool`, `Editor::duplicate_line(&mut self) -> bool`, `Editor::move_line_up(&mut self) -> bool`, `Editor::move_line_down(&mut self) -> bool`, and `Editor::jump_to_line(&mut self, one_based_line: usize) -> bool` in `src/app/editor.rs` with UTF-8-safe cursor placement, clean `end_undo_run()` boundaries, single-step undo/redo for edits, and zero buffer/history mutation on `jump_to_line` or boundary no-ops.
- **FR-002**: Add `Action::ToggleLineComment` (`"toggle-comment"`), `Action::DuplicateLine` (`"duplicate-line"`), `Action::MoveLineUp` (`"move-line-up"`), `Action::MoveLineDown` (`"move-line-down"`), and `Action::JumpToLine` (`"jump-to-line"`) to `Action`, `Action::all()`, `CONFIGURABLE`, `Keymap::new()`, `short_label`, `discovery.rs`, and `docs/design/keymap.md`.
- **FR-003**: Wire `JumpToLine` through `NamePrompt` (`NamePurpose::JumpToLine`) in `src/app/model.rs`, `src/app/update.rs`, `src/ui/widgets/overlays.rs`, and `src/ui/widgets/footer.rs`, syncing `BufferManager` active buffer state cleanly with zero disk I/O and zero database calls.

### Slice 2 - In-Buffer Find and Replace (PR 2B)

- **FR-004**: Implement pure in-buffer search and replace state, match navigation, and single/all replacement on `Editor` and `Model`.
- **FR-005**: Wire Find & Replace actions, keybindings, Command Palette entries, and TUI widget rendering using only `Theme`/`Token` styles.

## Non-Functional Requirements

- **NFR-001 (UI Token Discipline)**: Zero raw `Color::Rgb(` or `Color::Indexed(` literals in `src/ui/widgets/`.
- **NFR-002 (Repository Hygiene)**: Sidecar `tests.rs` files only, `AGENTS.md == CLAUDE.md`, zero Unicode em/en dashes (`\u{2014}` / `\u{2013}`) in prose.
- **NFR-003 (Verification)**: Passes `cargo --locked xtask verify`.
