use super::*;

#[test]
fn initial_buffer_manager_has_one_default_buffer() {
    let mgr = BufferManager::new();
    assert_eq!(mgr.count(), 1);
    assert_eq!(mgr.active_idx, 0);
    assert_eq!(mgr.active_buffer().title, "buffer-1.sql");
    assert_eq!(mgr.active_buffer().editor.text(), "");
    assert_eq!(mgr.split_mode, EditorSplitMode::None);
    assert_eq!(mgr.secondary_buffer(), None);
}

#[test]
fn new_buffer_creates_and_focuses() {
    let mut mgr = BufferManager::new();
    let idx2 = mgr.new_buffer(
        Some("migration.sql".to_string()),
        Some("CREATE TABLE foo();".to_string()),
    );
    assert_eq!(idx2, 1);
    assert_eq!(mgr.count(), 2);
    assert_eq!(mgr.active_idx, 1);
    assert_eq!(mgr.active_buffer().title, "migration.sql");
    assert_eq!(mgr.active_buffer().editor.text(), "CREATE TABLE foo();");
}

#[test]
fn buffer_manager_caps_at_maximum() {
    let mut mgr = BufferManager::new();
    for i in 2..=MAX_EDITOR_BUFFERS {
        mgr.new_buffer(Some(format!("test-{i}.sql")), None);
    }
    assert_eq!(mgr.count(), MAX_EDITOR_BUFFERS);

    // Attempting to exceed max buffers
    let current_idx = mgr.active_idx;
    let overflow_idx = mgr.new_buffer(Some("overflow.sql".to_string()), None);
    assert_eq!(overflow_idx, current_idx);
    assert_eq!(mgr.count(), MAX_EDITOR_BUFFERS);
}

#[test]
fn buffer_navigation_next_and_prev_cycles() {
    let mut mgr = BufferManager::new();
    mgr.new_buffer(Some("buf2.sql".to_string()), None);
    mgr.new_buffer(Some("buf3.sql".to_string()), None);
    assert_eq!(mgr.count(), 3);
    assert_eq!(mgr.active_idx, 2);

    mgr.next_buffer();
    assert_eq!(mgr.active_idx, 0);

    mgr.next_buffer();
    assert_eq!(mgr.active_idx, 1);

    mgr.prev_buffer();
    assert_eq!(mgr.active_idx, 0);

    mgr.prev_buffer();
    assert_eq!(mgr.active_idx, 2);
}

#[test]
fn close_buffer_resets_last_remaining() {
    let mut mgr = BufferManager::new();
    mgr.active_buffer_mut().editor = Editor::with_text("SELECT 1;");
    assert_eq!(mgr.active_buffer().editor.text(), "SELECT 1;");

    let closed = mgr.close_active();
    assert!(closed);
    assert_eq!(mgr.count(), 1);
    assert_eq!(mgr.active_idx, 0);
    assert_eq!(mgr.active_buffer().editor.text(), "");
}

#[test]
fn close_buffer_removes_and_adjusts_active() {
    let mut mgr = BufferManager::new();
    mgr.new_buffer(Some("b2.sql".to_string()), None);
    mgr.new_buffer(Some("b3.sql".to_string()), None);
    assert_eq!(mgr.active_idx, 2);

    mgr.close_active();
    assert_eq!(mgr.count(), 2);
    assert_eq!(mgr.active_idx, 1);
    assert_eq!(mgr.active_buffer().title, "b2.sql");
}

#[test]
fn split_mode_cycling_with_multiple_buffers() {
    let mut mgr = BufferManager::new();
    // Cannot split with only 1 buffer
    mgr.cycle_split_mode();
    assert_eq!(mgr.split_mode, EditorSplitMode::None);

    mgr.new_buffer(Some("b2.sql".to_string()), None);
    assert_eq!(mgr.active_idx, 1);

    mgr.cycle_split_mode();
    assert_eq!(mgr.split_mode, EditorSplitMode::Vertical);
    assert_eq!(mgr.secondary_idx, Some(0));
    assert_eq!(
        mgr.secondary_buffer().map(|b| b.title.as_str()),
        Some("buffer-1.sql")
    );

    mgr.cycle_split_mode();
    assert_eq!(mgr.split_mode, EditorSplitMode::Horizontal);
    assert_eq!(mgr.secondary_idx, Some(0));

    mgr.cycle_split_mode();
    assert_eq!(mgr.split_mode, EditorSplitMode::None);
    assert_eq!(mgr.secondary_buffer(), None);
}

#[test]
fn editor_buffer_display_label() {
    let mut buf = EditorBuffer::new(1, "scratch.sql", None);
    assert_eq!(buf.display_label(), "scratch.sql");

    buf.editor.insert('S');
    assert_eq!(buf.display_label(), "scratch.sql *");
}

#[test]
fn load_files_preloads_buffers() {
    let mut mgr = BufferManager::new();
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let file1 = temp_dir.path().join("first.sql");
    let file2 = temp_dir.path().join("second.sql");

    std::fs::write(&file1, "SELECT 100;").expect("write file1");
    std::fs::write(&file2, "SELECT 200;").expect("write file2");

    mgr.load_files(&[file1.clone(), file2.clone()]);

    assert_eq!(mgr.count(), 2);
    assert_eq!(mgr.active_idx, 0);
    assert_eq!(mgr.buffers[0].title, "first.sql");
    assert_eq!(mgr.buffers[0].editor.text(), "SELECT 100;");
    assert_eq!(mgr.buffers[0].file_path, Some(file1));

    assert_eq!(mgr.buffers[1].title, "second.sql");
    assert_eq!(mgr.buffers[1].editor.text(), "SELECT 200;");
    assert_eq!(mgr.buffers[1].file_path, Some(file2));
}
