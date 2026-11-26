# Tasks: Adaptive Multi-Column Chords, Tab Pinning/Renaming, and Result Snapshot Diffing

**Input**: Design documents from `/specs/067-multi-column-chords-tabs-and-result-diff/`
**Prerequisites**: `spec.md`, `plan.md`

## Phase 1: PR 2A - Adaptive Multi-Column Chords & Buffer Tab Pinning/Renaming (`feat/m21-chords-and-buffer-tabs`)

- [x] T001 Refactor `render_chords` in `src/ui/widgets/overlays.rs` to choose `1`, `2`, or `3` balanced columns dynamically based on `chords.len()`, `area.width`, and `area.height`, with `Token::Border` dividers, safe ellipsis truncation, and Unicode/ASCII parity.
- [x] T002 Add `pinned: bool`, `toggle_pin_active()`, `rename_active()`, and pinned `close_active()` refusal to `EditorBuffer` and `BufferManager` in `src/buffer/mod.rs` with sidecar unit tests in `src/buffer/tests.rs`.
- [x] T003 Add `TogglePinEditorBuffer` (`"pin-buffer"`, `Ctrl+K 9`) and `RenameEditorBuffer` (`"rename-buffer"`, `Ctrl+K 0`) to `Action`, `CONFIGURABLE`, `CHORDS`, `short_label`, `discovery.rs`, `docs/design/keymap.md`, and `README.md`.
- [x] T004 Wire `RenameEditorBuffer` through `NamePrompt` (`NamePurpose::RenameBuffer`), `CloseEditorBuffer` pinned refusal notice (`"Buffer is pinned; unpin it before closing."`), and `begin_tab_glide()` motion in `src/app/model.rs`, `src/app/update.rs`, `src/ui/widgets/overlays.rs`, and `src/ui/widgets/footer.rs`.
- [x] T005 Render pinned tab markers in `src/ui/widgets/editor.rs` using `Theme`/`Token` + Unicode/ASCII glyphs and keep `tab_item_width` and mouse click hit-testing (`Region::EditorTab` / `Region::NewEditorTab`) exact.
- [x] T006 Update `a_pending_chord_lists_every_key_that_could_follow_it` and add 3-column chord, tab-pin/rename, and hit-testing tests across `src/ui/layout/tests.rs`, `src/ui/widgets/editor/tests.rs`, `src/ui/widgets/tests.rs`, and `src/app/update/tests.rs`, then verify with `cargo --locked xtask verify`.

## Phase 2: PR 2B - Pinned Result Snapshot & Diffing (`feat/m21-result-snapshot-diff`)

- [x] T007 Implement pure bounded domain module `src/result_diff/mod.rs` (`PinnedResult`, `ResultDiff`, `diff_results`, text/Markdown/JSON formatters) and sidecar `src/result_diff/tests.rs`.
- [x] T008 Wire `PinResultSnapshot` (`"pin-result"`), `CompareResultSnapshot` (`"compare-result"`), connection/disconnect clearing, and the interactive `ResultDiff` overlay into `src/app/*`, `src/ui/keymap.rs`, `src/ui/widgets/overlays.rs`, and `src/ui/layout.rs`.
- [x] T009 Implement offline CLI `ignatius result-diff` in `src/cli/mod.rs` and `src/cli/output.rs` with bounded JSON/NDJSON ingestion, symlink/non-regular file rejection, and atomic `--output`.
- [x] T010 Add unit, reducer, UI, and CLI contract tests for result diffing and verify with `cargo --locked xtask verify`.
