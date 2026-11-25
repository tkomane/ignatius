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

## W10 restore the active draft - 2026-11-25

The journal writes the active index before recovery filters empty scratch
tabs. Keep the original index alongside each retained buffer and select the
first retained index at or after the recorded position, falling back to the
last retained buffer. The existing restore action synchronizes the visible
editor from that selection. Use the version 1 journal and existing split,
cursor, dirty marker and file association restoration.

Prove the failure with an empty scratch tab before the active draft and
another retained draft after it. Cover omitted or out-of-range active
positions and a retained empty file-backed tab. Exercise the ordinary
interactive restore action with synthetic content and run the local verifier,
hosted PR checks and post-merge CI and CodeQL.

## W10 recover a cleared file-backed draft

Use the same file-association or nonblank-text decision when the checkpoint
looks for restorable state, startup decides whether to offer recovery, status
counts recoverable buffers and restore selects tabs. Keep the lone untouched
starter rule and the existing checkpoint schedule and version 1 schema.
The SQL export keeps its existing treatment of blank text. On an empty SQL
export, its status guides the operator to the lossless JSON representation.

Clear a preloaded synthetic file-backed tab in an owned PTY, wait for the
normal checkpoint and terminate only that process. Verify the next startup
offers recovery, the restored editor's path and dirty state match the
checkpoint, and the source file still holds its original bytes. Confirm the
headless count and lossless export with the real binary. Run the local gate
and the hosted PR and post-merge gates.

## W10 preserve drafts edited while recovery is pending

Use the existing recovery content rule for modified tabs in the current
workspace. Count the same set that will be appended beside the restored
tabs. Keep the workspace intact when the journal has no restorable tabs or
the combined tabs would exceed capacity. Preserve fresh buffer identities
and the restored active selection.

Prove both empty and whitespace-only file-backed drafts keep their text,
path and dirty marker without writing the source file. Exercise the tab
limit and zero-restorable case in the reducer, then confirm the ordinary
restore shortcut in an owned PTY. Run the local and hosted gates.
