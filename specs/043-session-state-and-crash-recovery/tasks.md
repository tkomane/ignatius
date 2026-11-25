# Tasks: Session State Persistence and Automatic Crash Recovery (Feature 043)

- [x] T001: Implement pure domain engine in `src/recovery/mod.rs` (`RecoveredBuffer`, `RecoveryJournal`, atomic saving, loading, discarding, clean exit marking).
- [x] T002: Add sidecar unit tests in `src/recovery/tests.rs` (serialization round-trip, atomic writes, loading, discarding).
- [x] T003: Register module `pub mod recovery;` in `src/lib.rs` and integrate `SessionRecoveryBanner` in `src/app/model.rs`.
- [x] T004: Register actions `Action::RestoreSession` and `Action::DiscardRecovery` in `src/app/message.rs`, `src/ui/keymap.rs`, `docs/design/keymap.md`, and `README.md`.
- [x] T005: Wire recovery reducers in `src/app/discovery.rs` and `src/app/update.rs` (restoring buffers, discarding); checkpoint and clean-exit policy live in `RecoverySession` and are driven by `src/cli/interactive.rs`.
- [x] T006: Render Dieter Rams Field Industrial crash recovery capsule in `src/ui/widgets/editor.rs`.
- [x] T007: Add scriptable CLI subcommand `ignatius recovery` in `src/cli/mod.rs` (`status`, `restore`, `discard`).
- [x] T008: Add contract integration tests in `tests/session_recovery_contract.rs`.
- [x] T009: Run documentation contract tests and full verification battery (`cargo xtask verify`).
- [x] T010: Update `docs/status.md` and `CHANGELOG.md` with Wave 35 delivery evidence, closing Milestone 9!

## W10 recovery follow-up (2026-11-25)

- [x] T011 Restore the selected draft from its original journal position after
  filtering empty scratch tabs. Exercise cursor, dirty marker, file-backed
  tabs, fallback selection and the visible editor through the restored workspace.

Both selection regressions failed on the base implementation. The fixed build
passed 27 focused unit checks and eight recovery binary contracts. An owned
macOS PTY confirmed `Ctrl+K r` reopened draft A from `[empty, A, B]` and quit
cleanly. [Wave 73](../../docs/status.md#wave-73---restore-the-active-editor-draft---25-november-2026)
