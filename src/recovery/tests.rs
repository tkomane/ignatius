//! Sidecar unit tests for session recovery domain engine.

use super::*;
use crate::buffer::{BufferManager, EditorSplitMode};
use std::path::PathBuf;

#[test]
fn recovery_journal_roundtrip_serialization() {
    let mut manager = BufferManager::new();
    manager.active_buffer_mut().editor.set_text("SELECT 1;");

    manager.new_buffer(
        Some("migration.sql".into()),
        Some("ALTER TABLE users ADD COLUMN age int;".into()),
    );
    manager.split_mode = EditorSplitMode::Vertical;

    let journal = RecoveryJournal::from_buffers(&manager, "session-test-1", false);
    assert_eq!(journal.version, RECOVERY_SCHEMA_VERSION);
    assert_eq!(journal.session_id, "session-test-1");
    assert!(!journal.clean_exit);
    assert_eq!(journal.buffers.len(), 2);
    assert_eq!(journal.split_mode, "vertical");
    assert!(journal.has_recoverable_content());
    assert_eq!(journal.recoverable_buffer_count(), 2);
    assert_eq!(journal.truncated_buffer_count(), 0);

    let json = serde_json::to_string(&journal).expect("serialize");
    let deserialized: RecoveryJournal = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(journal, deserialized);
}

#[test]
fn recovery_content_detection_guards_clean_exits_and_empty_buffers() {
    let mut manager = BufferManager::new();
    assert_eq!(manager.count(), 1);

    // Empty buffer + un-clean exit: not recoverable
    let journal = RecoveryJournal::from_buffers(&manager, "s-empty", false);
    assert!(!journal.has_recoverable_content());
    assert_eq!(journal.recoverable_buffer_count(), 0);

    // Non-empty edited buffer + clean exit: not recoverable
    manager.active_buffer_mut().editor.set_text("SELECT 42;");
    manager.active_buffer_mut().editor.insert(' ');
    let clean_journal = RecoveryJournal::from_buffers(&manager, "s-clean", true);
    assert!(!clean_journal.has_recoverable_content());

    // Non-empty edited buffer + un-clean exit: recoverable!
    let crash_journal = RecoveryJournal::from_buffers(&manager, "s-crash", false);
    assert!(crash_journal.has_recoverable_content());
    assert_eq!(crash_journal.recoverable_buffer_count(), 1);
    assert_eq!(crash_journal.unsaved_buffer_count(), 1);
}

#[test]
fn an_untouched_starter_buffer_is_not_a_crash_to_report() {
    let mut manager = BufferManager::new();
    manager
        .active_buffer_mut()
        .editor
        .set_text("-- starter\nSELECT 1;");

    let journal = RecoveryJournal::from_buffers(&manager, "s-starter", false);
    assert!(
        !journal.has_recoverable_content(),
        "loading text is not editing; a single untouched tab is not recovery-worthy"
    );
    assert_eq!(journal.unsaved_buffer_count(), 0);
}

#[test]
fn several_empty_tabs_are_not_a_crash_to_report() {
    let mut manager = BufferManager::new();
    manager.new_buffer(Some("two.sql".into()), None);
    manager.new_buffer(Some("three.sql".into()), None);

    let journal = RecoveryJournal::from_buffers(&manager, "s-empty-tabs", false);
    assert!(
        !journal.has_recoverable_content(),
        "tab count alone is not recoverable content"
    );
    assert_eq!(journal.recoverable_buffer_count(), 0);
}

#[test]
fn restore_into_rebuilds_exact_buffer_state() {
    let mut source = BufferManager::new();
    source
        .active_buffer_mut()
        .editor
        .set_text("SELECT 'buffer1';");
    source.active_buffer_mut().file_path = Some(PathBuf::from("/tmp/b1.sql"));

    source.new_buffer(Some("b2.sql".into()), Some("SELECT 'buffer2';".into()));
    source.active_buffer_mut().editor.insert(' ');
    source.active_buffer_mut().editor.set_cursor(7);
    source.split_mode = EditorSplitMode::Horizontal;

    let journal = RecoveryJournal::from_buffers(&source, "s-restore", false);

    let mut target = BufferManager::new();
    assert_eq!(target.count(), 1);

    let restored_count = journal.restore_into(&mut target);
    assert_eq!(restored_count, 2);
    assert_eq!(target.count(), 2);
    assert_eq!(target.split_mode, EditorSplitMode::Horizontal);
    assert_eq!(target.active_idx, 1);
    assert_eq!(target.buffers[0].editor.text(), "SELECT 'buffer1';");
    assert_eq!(
        target.buffers[0].file_path,
        Some(PathBuf::from("/tmp/b1.sql"))
    );
    assert_eq!(target.buffers[1].editor.text(), "SELECT 'buffer2'; ");
    assert_eq!(target.buffers[1].editor.cursor(), 7);
    assert!(
        target.buffers[1].editor.is_modified(),
        "the dirty marker recorded in the journal is restored"
    );
    assert_eq!(
        target.secondary_idx,
        Some(0),
        "a restored split needs a distinct second pane"
    );
    assert_eq!(
        target.next_id, 3,
        "new tabs after a restore must not reuse a restored id"
    );
}

#[cfg(unix)]
#[test]
fn the_journal_and_its_directory_are_owner_only_on_unix() {
    use std::os::unix::fs::PermissionsExt as _;

    let temp_dir = tempfile::tempdir().expect("tempdir");
    let recovery_dir = temp_dir.path().join("recovery");
    let mut manager = BufferManager::new();
    manager
        .active_buffer_mut()
        .editor
        .set_text("SELECT 'private';");

    let journal = RecoveryJournal::from_buffers(&manager, "s-mode", false);
    let path = journal.save_to_disk(&recovery_dir).expect("save");

    let file_mode = std::fs::metadata(&path)
        .expect("metadata")
        .permissions()
        .mode()
        & 0o777;
    let dir_mode = std::fs::metadata(&recovery_dir)
        .expect("dir metadata")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(file_mode, 0o600, "journal must not be group/world readable");
    assert_eq!(dir_mode, 0o700, "recovery directory must be owner-only");
}

#[cfg(unix)]
#[test]
fn saving_replaces_a_planted_symlink_instead_of_writing_through_it() {
    use std::os::unix::fs::symlink;

    let temp_dir = tempfile::tempdir().expect("tempdir");
    let recovery_dir = temp_dir.path().join("recovery");
    std::fs::create_dir_all(&recovery_dir).expect("create");
    let victim = temp_dir.path().join("victim.txt");
    std::fs::write(&victim, "must not change").expect("write victim");
    symlink(&victim, recovery_dir.join(RECOVERY_FILE_NAME)).expect("plant symlink");

    let mut manager = BufferManager::new();
    manager
        .active_buffer_mut()
        .editor
        .set_text("SELECT 'safe';");
    let journal = RecoveryJournal::from_buffers(&manager, "s-symlink", false);
    journal.save_to_disk(&recovery_dir).expect("save");

    assert_eq!(
        std::fs::read_to_string(&victim).expect("read victim"),
        "must not change",
        "the journal write must not follow the planted symlink"
    );
    let metadata =
        std::fs::symlink_metadata(recovery_dir.join(RECOVERY_FILE_NAME)).expect("journal metadata");
    assert!(
        metadata.file_type().is_file(),
        "the journal replaces the link as a directory entry"
    );
}

#[test]
fn a_single_buffer_journal_never_restores_an_empty_split_pane() {
    let mut source = BufferManager::new();
    source.active_buffer_mut().editor.set_text("SELECT 1;");
    source.active_buffer_mut().editor.insert(' ');
    source.split_mode = EditorSplitMode::Vertical;

    let journal = RecoveryJournal::from_buffers(&source, "s-one-split", false);
    let mut target = BufferManager::new();
    assert_eq!(journal.restore_into(&mut target), 1);
    assert_eq!(target.split_mode, EditorSplitMode::None);
    assert_eq!(target.secondary_idx, None);
}

#[test]
fn disk_atomic_save_load_mark_clean_and_discard() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let recovery_dir = temp_dir.path().join("recovery");

    let mut manager = BufferManager::new();
    manager
        .active_buffer_mut()
        .editor
        .set_text("SELECT 'crash content';");

    let journal = RecoveryJournal::from_buffers(&manager, "session-disk-test", false);
    let saved_path = journal.save_to_disk(&recovery_dir).expect("save journal");
    assert!(saved_path.exists());
    assert_eq!(
        std::fs::read_dir(&recovery_dir)
            .expect("read dir")
            .filter_map(Result::ok)
            .count(),
        1,
        "the atomic write must not leave a temporary sibling behind"
    );

    // Load from disk
    let loaded = RecoveryJournal::load_from_disk(&recovery_dir)
        .expect("load journal")
        .expect("journal exists");
    assert_eq!(loaded.session_id, "session-disk-test");
    assert!(!loaded.clean_exit);
    assert_eq!(loaded.buffers.len(), 1);
    assert_eq!(loaded.buffers[0].sql, "SELECT 'crash content';");

    // Mark clean exit
    RecoveryJournal::mark_clean_exit(&recovery_dir).expect("mark clean");
    let clean_loaded = RecoveryJournal::load_from_disk(&recovery_dir)
        .expect("load")
        .expect("exists");
    assert!(clean_loaded.clean_exit);
    assert!(!clean_loaded.has_recoverable_content());

    // Discard
    RecoveryJournal::discard(&recovery_dir).expect("discard");
    let after_discard = RecoveryJournal::load_from_disk(&recovery_dir).expect("load after discard");
    assert!(after_discard.is_none());
}

#[test]
fn oversized_buffers_are_truncated_on_a_char_boundary_and_reported() {
    let mut manager = BufferManager::new();
    // The byte at the bound lands inside a two-byte character, so a naive cut
    // would panic; capture must walk back to a boundary.
    let text = format!("{}é", "a".repeat(MAX_RECOVERED_BUFFER_BYTES - 1));
    manager.active_buffer_mut().editor.set_text(&text);

    let journal = RecoveryJournal::from_buffers(&manager, "s-large", false);
    let buffer = &journal.buffers[0];
    assert!(buffer.truncated);
    assert_eq!(buffer.sql.len(), MAX_RECOVERED_BUFFER_BYTES - 1);
    assert_eq!(journal.truncated_buffer_count(), 1);
    assert!(buffer.sql.is_char_boundary(buffer.sql.len()));
}

#[test]
fn a_newer_journal_version_is_refused_without_deleting_it() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let recovery_dir = temp_dir.path().join("recovery");
    std::fs::create_dir_all(&recovery_dir).expect("create");
    std::fs::write(
        recovery_dir.join(RECOVERY_FILE_NAME),
        r#"{"version":99,"session_id":"future","started_at":"2026-01-01T00:00:00Z","saved_at":"2026-01-01T00:00:00Z","clean_exit":false,"active_buffer_idx":0,"split_mode":"none","buffers":[]}"#,
    )
    .expect("write");

    let error = RecoveryJournal::load_from_disk(&recovery_dir).expect_err("must refuse");
    assert!(error.headline.contains("newer than this build"));
    assert!(
        recovery_dir.join(RECOVERY_FILE_NAME).exists(),
        "refusing a journal must not delete evidence"
    );
}

#[test]
fn an_oversized_journal_is_refused_by_size_before_parsing() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let recovery_dir = temp_dir.path().join("recovery");
    std::fs::create_dir_all(&recovery_dir).expect("create");
    let file = std::fs::File::create(recovery_dir.join(RECOVERY_FILE_NAME)).expect("create");
    file.set_len(MAX_JOURNAL_BYTES + 1).expect("set length");

    let error = RecoveryJournal::load_from_disk(&recovery_dir).expect_err("must refuse");
    assert!(error.headline.contains("above the"));
}

#[test]
fn session_checkpoints_structure_immediately_and_throttles_typing() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let mut session = RecoverySession::new(temp_dir.path().join("recovery"), true);
    let mut manager = BufferManager::new();

    assert!(
        !session.checkpoint(&manager, false).expect("checkpoint"),
        "an untouched empty workspace writes nothing"
    );

    manager.active_buffer_mut().editor.set_text("SELECT 1;");
    assert!(
        session.checkpoint(&manager, false).expect("checkpoint"),
        "the first content write is immediate"
    );
    assert!(
        temp_dir
            .path()
            .join("recovery")
            .join(RECOVERY_FILE_NAME)
            .exists()
    );

    manager.active_buffer_mut().editor.set_text("SELECT 2;");
    assert!(
        !session.checkpoint(&manager, false).expect("checkpoint"),
        "typing inside the interval is throttled"
    );
    assert!(
        session
            .checkpoint(&manager, true)
            .expect("forced checkpoint"),
        "a forced checkpoint always writes"
    );

    manager.new_buffer(Some("second.sql".into()), None);
    assert!(
        session.checkpoint(&manager, false).expect("checkpoint"),
        "a structural change bypasses the throttle"
    );
}

#[test]
fn a_pending_journal_is_never_overwritten_and_finish_leaves_it_alone() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let recovery_dir = temp_dir.path().join("recovery");

    let mut crashed = BufferManager::new();
    crashed
        .active_buffer_mut()
        .editor
        .set_text("SELECT 'crashed';");
    crashed.active_buffer_mut().editor.insert(' ');
    RecoveryJournal::from_buffers(&crashed, "crashed", false)
        .save_to_disk(&recovery_dir)
        .expect("seed journal");

    let mut session = RecoverySession::new(recovery_dir.clone(), true);
    let journal = session
        .take_startup()
        .expect("startup")
        .expect("recoverable journal");
    assert_eq!(journal.session_id, "crashed");
    assert!(session.is_pending());

    let mut fresh = BufferManager::new();
    fresh.active_buffer_mut().editor.set_text("something new");
    assert!(
        !session.checkpoint(&fresh, true).expect("checkpoint"),
        "a forced checkpoint must still not clobber a pending journal"
    );
    session.finish(&fresh).expect("finish");

    let still_there = RecoveryJournal::load_from_disk(&recovery_dir)
        .expect("load")
        .expect("journal survives");
    assert_eq!(still_there.session_id, "crashed");
    assert!(
        still_there.has_recoverable_content(),
        "quitting without deciding preserves the crashed content"
    );

    session.decide();
    assert!(
        session
            .checkpoint(&fresh, true)
            .expect("checkpoint after decide")
    );
    let replaced = RecoveryJournal::load_from_disk(&recovery_dir)
        .expect("load")
        .expect("journal");
    assert_eq!(replaced.buffers[0].sql, "something new");
}

#[test]
fn startup_discards_a_clean_journal_and_reports_nothing() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let recovery_dir = temp_dir.path().join("recovery");

    let mut manager = BufferManager::new();
    manager
        .active_buffer_mut()
        .editor
        .set_text("SELECT 'clean';");
    let mut journal = RecoveryJournal::from_buffers(&manager, "clean", true);
    journal.clean_exit = true;
    journal.save_to_disk(&recovery_dir).expect("seed");

    let mut session = RecoverySession::new(recovery_dir.clone(), true);
    assert!(session.take_startup().expect("startup").is_none());
    assert!(
        !recovery_dir.join(RECOVERY_FILE_NAME).exists(),
        "a clean journal is cleared at startup"
    );
}

#[test]
fn finish_marks_the_journal_clean_for_the_next_launch() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let recovery_dir = temp_dir.path().join("recovery");
    let mut session = RecoverySession::new(recovery_dir.clone(), true);
    let mut manager = BufferManager::new();
    manager
        .active_buffer_mut()
        .editor
        .set_text("SELECT 'work';");

    session.checkpoint(&manager, true).expect("checkpoint");
    session.finish(&manager).expect("finish");

    let journal = RecoveryJournal::load_from_disk(&recovery_dir)
        .expect("load")
        .expect("journal");
    assert!(journal.clean_exit);
    assert!(!journal.has_recoverable_content());

    let mut next = RecoverySession::new(recovery_dir, true);
    assert!(next.take_startup().expect("startup").is_none());
}

#[test]
fn a_disabled_session_never_touches_the_disk() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let recovery_dir = temp_dir.path().join("recovery");

    let mut crashed = BufferManager::new();
    crashed
        .active_buffer_mut()
        .editor
        .set_text("SELECT 'private';");
    RecoveryJournal::from_buffers(&crashed, "private", false)
        .save_to_disk(&recovery_dir)
        .expect("seed journal");

    let mut session = RecoverySession::new(recovery_dir.clone(), false);
    assert!(session.take_startup().expect("startup").is_none());
    assert!(!session.checkpoint(&crashed, true).expect("checkpoint"));
    session.finish(&crashed).expect("finish");

    let untouched = RecoveryJournal::load_from_disk(&recovery_dir)
        .expect("load")
        .expect("journal untouched");
    assert_eq!(untouched.session_id, "private");
}
