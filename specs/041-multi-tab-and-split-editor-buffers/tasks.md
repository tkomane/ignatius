# Tasks: Multi-Tab and Split Editor Buffers (Feature 041)

- [x] T001: Implement pure domain models in `src/buffer/mod.rs` (`EditorBuffer`, `BufferManager`, `EditorSplitMode`).
- [x] T002: Add pure unit tests in sidecar `src/buffer/tests.rs` (buffer creation, switching, capping at 16, closing, split mode cycling).
- [x] T003: Register module `pub mod buffer;` in `src/lib.rs` and integrate `BufferManager` into `src/app/model.rs`.
- [x] T004: Register actions and chords (`Ctrl+K =`, `Ctrl+K -`, `Ctrl+K }`, `Ctrl+K {`, `Ctrl+K \`) in `src/app/message.rs`, `src/ui/keymap.rs`, `docs/design/keymap.md`, and `README.md`.
- [x] T005: Wire buffer actions in `src/app/update.rs` and `src/app/discovery.rs`.
- [x] T006: Render Dieter Rams tab bar and split editor layout in `src/ui/widgets/editor.rs`.
- [x] T007: Support `--file` multi-file CLI loading in `src/cli/mod.rs`.
- [x] T008: Add contract integration tests in `tests/multi_buffer_contract.rs`.
- [x] T009: Run documentation contract tests and full verification battery (`cargo xtask verify`).
- [x] T010: Update `docs/status.md` and `CHANGELOG.md` with Wave 33 delivery evidence.
