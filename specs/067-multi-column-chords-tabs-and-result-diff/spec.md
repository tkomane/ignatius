# Feature Specification: Adaptive Multi-Column Chords, Tab Pinning/Renaming, and Result Snapshot Diffing

**Feature Branch**: `feat/m21-chords-and-buffer-tabs` (PR 2A) and `feat/m21-result-snapshot-diff` (PR 2B)
**Created**: 2026-11-26
**Status**: Complete (PR 2B #195 `e28dfbb` and PR 2A #196 `7c4cd15` merged on `main`)
**Milestone**: M21 - Adaptive Chord Layout, Buffer Tab Management, and Result Snapshot Diffing

## Problem Statement

Three workbench usability and comparison gaps remain as Ignatius's chord catalog and multi-buffer workflows grow:

1. **Two-column chord menu overflows standard 120x30 terminals once `CHORDS` exceeds 56 entries.** `render_chords` previously switched only between 1 and 2 columns, which requires 29+ inner rows once `CHORDS` grows to 57+ entries and clips items on 30-row terminals.
2. **Multi-tab editor buffers lack protection against accidental close and custom session labels.** Users cannot pin important scratch/query tabs (`close_active()` unconditionally resets or removes the tab) or assign custom in-session tab titles without writing a `.sql` file to disk.
3. **Retained query results cannot be pinned in memory and compared against subsequent query runs or local result files.** Operators iterating on queries or comparing pre/post-change rows must export manually and diff externally.

## Principles This Feature Must Keep

- **Principle I (Delight must remain truthful).** Refusing to close a pinned buffer explains why clearly in the status footer; result diffs report explicit alignment mode (`Keyed` vs `Positional`) and truncation posture without hiding fallback assumptions.
- **Principle III (Local-first and private by default).** Buffer tab renames and pinned result snapshots stay strictly in-memory for the session (zero unrequested disk writes or network calls).
- **Principle V (Keyboard-first, never keyboard-only).** Every chord remains visible on `Ctrl+K` without silent clipping, pin/rename actions are reachable via `CHORDS`, configurable `[keys]`, and Command Palette, and tab clicks remain exact.
- **Principle VI (Accessible meaning survives styling loss).** Adaptive chords, pinned tab markers, and result-diff status badges (`[+]`, `[-]`, `[~]`, `[=]`) carry explicit text and layout meaning in ASCII, `Basic16`, and `NO_COLOR` modes.

## User Scenarios & Testing

### User Story 1 - Adaptive 1/2/3-Column Chord Guide (Priority: P1, Slice 1 / PR 2A)

When an operator presses `Ctrl+K`, `render_chords` chooses 1, 2, or 3 balanced columns based on `CHORDS.len()`, `area.width`, and `area.height`. When 2 columns exceed the available inner height and `area.width >= 114`, 3 balanced columns render with `Theme`/`Token` dividers and compact per-column width budgeting that truncates only overlong descriptions safely with an ellipsis, ensuring every key and description stays legible without clipping or overlap in both Unicode and ASCII modes.

### User Story 2 - Buffer Tab Pinning and Session Renaming (Priority: P1, Slice 1 / PR 2A)

When working across multiple SQL editor buffers, an operator can pin/unpin the active buffer (`Ctrl+K 9` / `pin-buffer`) and rename the active tab (`Ctrl+K 0` / `rename-buffer`) through a single-line in-TUI prompt. Closing a pinned buffer (`Ctrl+K -` / `close-buffer`) leaves the buffer intact and shows `"Buffer is pinned; unpin it before closing."` in the footer. Pinned tabs render a clear marker in both Unicode and ASCII modes while keeping `tab_item_width` and mouse click hit-testing exact.

### User Story 3 - Pinned Result-Set Snapshot Diffing (Priority: P1, Slice 2 / PR 2B)

An operator can pin an ephemeral in-memory `PinnedResult` snapshot from the active retained result (`PinResultSnapshot` / `pin-result`), run another query or edit filters, and open a scrollable `ResultDiff` overlay (`CompareResultSnapshot` / `compare-result`) showing schema deltas, key-column or explicit positional row alignment, and per-row `[+]`/`[-]`/`[~]`/`[=]` changes, plus offline CLI parity via `ignatius result-diff`.

## Functional Requirements

### Slice 1 - Adaptive Multi-Column Chords and Buffer Tab Pinning/Renaming (PR 2A)

- **FR-001**: Refactor `render_chords` in `src/ui/widgets/overlays.rs` to choose `1`, `2`, or `3` balanced columns dynamically from `chords.len()`, `area.width`, and `area.height`, using 3 columns when 2 columns exceed available inner height and `area.width >= 114`, with `Token::Border` dividers, safe ellipsis truncation only for overlong descriptions, and strict adherence to `area.height` in Unicode and ASCII modes.
- **FR-002**: Add `pub pinned: bool` (default `false`) to `EditorBuffer` (`src/buffer/mod.rs`). Update `BufferManager::close_active()` to return `false` without mutating buffers or `active_idx` when the active buffer is pinned.
- **FR-003**: Add `BufferManager::toggle_pin_active(&mut self) -> bool` and `BufferManager::rename_active(&mut self, title: &str) -> bool` (stripping control/bidi characters, trimming whitespace, sanitizing for display, capping at 48 characters, and refusing empty results).
- **FR-004**: Add `Action::TogglePinEditorBuffer` (`"pin-buffer"`, `Ctrl+K 9`) and `Action::RenameEditorBuffer` (`"rename-buffer"`, `Ctrl+K 0`) across `src/app/message.rs`, `src/ui/keymap.rs`, `src/app/discovery.rs`, `docs/design/keymap.md`, and `README.md`.
- **FR-005**: Wire `RenameEditorBuffer` through `NamePrompt` (`NamePurpose::RenameBuffer`) in `src/app/model.rs`, `src/app/update.rs`, and `src/ui/widgets/overlays.rs` (Enter applies, Esc cancels, pure reducer, zero disk I/O), display `"Buffer is pinned; unpin it before closing."` on refused `CloseEditorBuffer`, trigger `model.motion.begin_tab_glide()` on pin/rename when `allows_motion()`, render pinned markers on tabs in `src/ui/widgets/editor.rs`, and keep `tab_item_width` + mouse click hit-testing (`Region::EditorTab` / `Region::NewEditorTab`) exact.

### Slice 2 - Pinned Result-Set Diffing and Offline CLI (PR 2B)

- **FR-006**: Add `src/result_diff/mod.rs` (`PinnedResult`, `ResultDiff`, `diff_results`, deterministic text/Markdown/JSON formatters) with bounded memory caps, explicit `Keyed` vs `Positional` alignment reporting, and automatic clearing on connection/profile switch or disconnect.
- **FR-007**: Add `Action::PinResultSnapshot` (`"pin-result"`) and `Action::CompareResultSnapshot` (`"compare-result"`) plus the interactive `ResultDiff` overlay (`src/ui/widgets/overlays.rs`) using only `Theme`/`Token` colors.
- **FR-008**: Add offline CLI `ignatius result-diff <baseline> <current> [--key <col>] [--format text|json|markdown] [--output <path>] [--force]` rejecting symlinks, non-regular files, and oversized inputs cleanly with zero database/network calls.

## Non-Functional Requirements

- **NFR-001 (UI Token Discipline)**: Zero raw `Color::Rgb(` or `Color::Indexed(` literals in `src/ui/widgets/`.
- **NFR-002 (Repository Hygiene)**: Sidecar `tests.rs` files only, `AGENTS.md == CLAUDE.md`, zero Unicode em/en dashes (`\u{2014}` / `\u{2013}`) in prose.
- **NFR-003 (Verification)**: Passes `cargo --locked xtask verify` and `cargo --locked xtask security`.
