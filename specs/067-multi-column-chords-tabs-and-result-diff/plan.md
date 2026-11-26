# Implementation Plan: Adaptive Multi-Column Chords, Tab Pinning/Renaming, and Result Snapshot Diffing

**Spec**: [spec.md](spec.md)

## Delivery Slices

Feature 067 (Milestone M21) is delivered across two independent pull requests:

### PR 2A - Adaptive Multi-Column Chords & Buffer Tab Pinning/Renaming (`feat/m21-chords-and-buffer-tabs`)

1. **`src/ui/widgets/overlays.rs` & `src/ui/layout/tests.rs`**:
   - Upgrade `render_chords` to select `1`, `2`, or `3` balanced columns dynamically from `chords.len()`, `area.width`, and `area.height`.
   - When 2 columns exceed available inner height and `area.width >= 114`, lay out 3 balanced columns with `Token::Border` dividers and water-filling width allocation that truncates only overlong descriptions safely with ellipsis (`truncate_to_width`).
   - Update `a_pending_chord_lists_every_key_that_could_follow_it` for 58+ chords at `120x30` and add 3-column Unicode + ASCII layout tests.
2. **`src/buffer/mod.rs` & `src/buffer/tests.rs`**:
   - Add `pub pinned: bool` (default `false`) to `EditorBuffer`, `MAX_BUFFER_TITLE_CHARS` (48), and `sanitize_buffer_title`.
   - Refuse `BufferManager::close_active()` when `active_buffer().pinned` is `true` (returning `false` with buffers and `active_idx` untouched).
   - Add `toggle_pin_active(&mut self) -> bool` and `rename_active(&mut self, title: &str) -> bool`.
3. **`src/app/*`, `src/ui/keymap.rs`, `src/ui/widgets/editor.rs`, `src/ui/widgets/mod.rs`, `src/ui/widgets/footer.rs`, `docs/design/keymap.md`, `README.md`**:
   - Add `TogglePinEditorBuffer` (`"pin-buffer"`, `Ctrl+K 9`) and `RenameEditorBuffer` (`"rename-buffer"`, `Ctrl+K 0`).
   - Reuse `NamePrompt` (`NamePurpose::RenameBuffer`) for pure in-TUI single-line tab renaming (zero disk I/O), add `Model::buffer_notice` for refused pinned closes (`"Buffer is pinned; unpin it before closing."`), and trigger `begin_tab_glide()` on pin/rename when `allows_motion()`.
   - Render pinned tab markers in Unicode and ASCII modes using `Theme`/`Token` styles and share tab bar layout geometry between `render_tab_bar` and `tab_bar_hit` (`Region::EditorTab` / `Region::NewEditorTab`) so `tab_item_width` and mouse click hit-testing remain exact.

### PR 2B - Pinned Result Snapshot & Diffing (`feat/m21-result-snapshot-diff`)

1. **`src/result_diff/mod.rs` & `src/result_diff/tests.rs`**:
   - Implement bounded in-memory `PinnedResult`, `ResultDiff`, `diff_results`, and deterministic text/Markdown/JSON formatters.
2. **`src/app/*`, `src/ui/widgets/overlays.rs`, `src/ui/keymap.rs`**:
   - Add `PinResultSnapshot` (`"pin-result"`), `CompareResultSnapshot` (`"compare-result"`), and the interactive `ResultDiff` overlay.
3. **`src/cli/mod.rs`, `src/cli/output.rs`, `tests/cli_contract.rs`**:
   - Add offline `ignatius result-diff` command with bounded local JSON/NDJSON parsing and atomic `--output`.

## Verification Gates

- `cargo --locked xtask verify` (formatting, clippy, unit tests, UI/layout/doc/CLI contracts, live PostgreSQL 18.7 integration tests).
- `cargo --locked xtask security` (sidecar test hygiene, `AGENTS.md == CLAUDE.md`, zero governed em/en dashes, policy checks).
