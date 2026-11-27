# Tasks: SQL Editor Power Tools

**Input**: Design documents from `/specs/070-sql-editor-power-tools/`
**Prerequisites**: `spec.md`, `plan.md`

## Phase 1: PR 2A - Editor Line Operations & Jump to Line (`feat/m24-editor-edit-ops`)

- [x] T001 Implement `toggle_line_comment`, `duplicate_line`, `move_line_up`, `move_line_down`, and `jump_to_line` on `Editor` in `src/app/editor.rs` with unit tests in `src/app/editor/tests.rs`.
- [x] T002 Add `ToggleLineComment`, `DuplicateLine`, `MoveLineUp`, `MoveLineDown`, and `JumpToLine` to `Action` (`src/app/message.rs`), `CONFIGURABLE` and `Keymap` (`src/ui/keymap.rs`), `discovery.rs`, and `docs/design/keymap.md`.
- [x] T003 Wire all five actions into `src/app/update.rs` and implement `NamePurpose::JumpToLine` in `src/app/model.rs`, `src/app/update.rs`, `src/ui/widgets/overlays.rs`, and `src/ui/widgets/footer.rs` with `BufferManager` sync and zero disk/database I/O.
- [x] T004 Add unit, discovery, keymap, overlay, and reducer tests across `src/app/editor/tests.rs`, `src/ui/keymap/tests.rs`, `src/app/discovery/tests.rs`, `src/ui/widgets/overlays/tests.rs`, and `src/app/update/tests.rs`, and verify with `cargo --locked xtask verify`.

## Phase 2: PR 2B - In-Buffer Find & Replace (`feat/m24-editor-find-replace`)

- [x] T005 Implement in-buffer find match indexing, next/previous navigation, and single/all replace primitives on `Editor` and `Model`.
- [x] T006 Wire Find & Replace actions, keybindings, Command Palette entries, and TUI prompt/bar rendering using `Theme`/`Token` styles.
- [x] T007 Add unit, reducer, UI, and documentation contract tests for Find & Replace and verify with `cargo --locked xtask verify`.
