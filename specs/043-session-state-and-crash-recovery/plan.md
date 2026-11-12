# Implementation Plan: Session State Persistence and Automatic Crash Recovery (Feature 043)

## 1. Pure Domain Engine (`src/recovery/`)
- `src/recovery/mod.rs`:
  - `RecoveredBuffer`:
    - `id: u64`
    - `title: String`
    - `file_path: Option<PathBuf>`
    - `sql: String`
    - `cursor_offset: usize`
    - `dirty: bool`
  - `RecoveryJournal`:
    - `version: u32` (schema version = 1)
    - `session_id: String`
    - `started_at: String` (RFC3339)
    - `saved_at: String` (RFC3339)
    - `clean_exit: bool`
    - `active_buffer_idx: usize`
    - `split_mode: String`
    - `buffers: Vec<RecoveredBuffer>`
  - Functions:
    - `from_buffers(buffers: &BufferManager, session_id: &str, clean_exit: bool) -> Self`
    - `save_to_disk(&self, dir: &Path) -> Result<PathBuf, Diagnostic>` (atomic rename write)
    - `load_from_disk(dir: &Path) -> Result<Option<Self>, Diagnostic>`
    - `discard(dir: &Path) -> Result<(), Diagnostic>`
    - `mark_clean_exit(dir: &Path) -> Result<(), Diagnostic>`
- `src/recovery/tests.rs`:
  - Sidecar unit tests for journal creation, serialization/deserialization round-trip, atomic saving, crash recovery detection, discard, and clean exit flagging.
- `src/recovery/session.rs`:
  - Runtime checkpoint policy separated from the serialized domain: session identity, immediate structural writes, a two-second typing throttle, pending-decision protection, and clean-exit marking. Sidecar tests live in `src/recovery/tests.rs`.

## 2. TEA Architecture Integration
- `src/app/model.rs`:
  - `SessionRecoveryBanner`:
    - `journal: RecoveryJournal`
    - `buffer_count: usize`
  - Add `pub recovery_banner: Option<SessionRecoveryBanner>` to `Model`.
- `src/app/message.rs`:
  - Add actions:
    - `RestoreSession`
    - `DiscardRecovery`
- `src/ui/keymap.rs`:
  - Add `("restore-session", Action::RestoreSession)` and `("discard-recovery", Action::DiscardRecovery)` to `CONFIGURABLE`.
  - Add `('r', Action::RestoreSession, "Restore recovered session buffers")` to `CHORDS`.
  - Add `('d', Action::DiscardRecovery, "Discard crash recovery journal")` to `CHORDS`.
- `src/app/discovery.rs`:
  - Add availability and grouping rules for recovery actions.
- `src/app/update.rs`:
  - Handle `Action::RestoreSession`: restores buffers into `model.buffers`, syncs active editor, dismisses banner, and clears journal.
  - Handle `Action::DiscardRecovery`: dismisses banner and discards recovery journal from disk.
  - Hook auto-save logic on changes or clean exit on `Action::Quit`.
- `src/ui/widgets/banner.rs` or `src/ui/widgets/editor.rs`:
  - Render tactile Dieter Rams crash recovery capsule above editor tabs when `model.recovery_banner.is_some()`.

## 3. Headless Scriptable CLI Subcommand
- `src/cli/mod.rs`:
  - Add `Recovery(RecoveryArgs)` to `Command`.
  - Subcommands: `status`, `restore`, `discard`.
  - Output formats: `--format table|json|sql`.

## 4. Acceptance Contracts & Verification
- `tests/session_recovery_contract.rs`:
  - Test CLI `ignatius recovery status` (reports existing or clean recovery status).
  - Test CLI `ignatius recovery restore` (emits recovered SQL).
  - Test CLI `ignatius recovery discard` (cleans orphaned journal).
  - Test TUI reducer integration (`Action::RestoreSession`, `Action::DiscardRecovery`).
- Run `cargo test --test documentation_matches_the_build`.
- Run `cargo fmt --all` and `cargo clippy --all-targets -- -D warnings`.
- Run full verification battery: `cargo xtask verify`.
- Update `docs/status.md`, `CHANGELOG.md`, and `README.md`.
