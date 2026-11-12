# Tasks: Persistent Scratchpad and Snippet Library (Feature 042)

- [x] T001: Implement pure domain models in `src/snippet/mod.rs` (`Snippet`, `SnippetCategory`, `SnippetLibrary`, built-in catalog, disk persistence).
- [x] T002: Add unit tests in sidecar `src/snippet/tests.rs` (loading built-ins, category filtering, search, adding user snippets, disk load/save).
- [x] T003: Register module `pub mod snippet;` in `src/lib.rs` and integrate `SnippetLibraryState` into `src/app/model.rs`.
- [x] T004: Register action `Action::SnippetLibrary` and chord `Ctrl+K ;` in `src/app/message.rs`, `src/ui/keymap.rs`, `docs/design/keymap.md`, and `README.md`.
- [x] T005: Wire snippet library in `src/app/discovery.rs` and `src/app/update.rs` (insertion into editor buffer, open as new tab).
- [x] T006: Render Dieter Rams Field Industrial snippet browser overlay in `src/ui/widgets/overlays.rs`.
- [x] T007: Add scriptable CLI subcommand `ignatius snippet` in `src/cli/mod.rs`.
- [x] T008: Add contract integration tests in `tests/snippet_contract.rs`.
- [x] T009: Run documentation contract tests and full verification battery (`cargo xtask verify`).
- [x] T010: Update `docs/status.md` and `CHANGELOG.md` with Wave 34 delivery evidence.
