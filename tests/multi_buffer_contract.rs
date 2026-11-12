//! Acceptance contract for Feature 041: Multi-Tab and Split Editor Buffers.
//!
//! Drives multi-buffer operations through `Message::Action` and the TEA update reducer.

#![allow(clippy::print_stderr)]

use ignatius::app::{Action, Message, Model, update};
use ignatius::buffer::EditorSplitMode;
use ignatius::connection::Environment;
use ignatius::postgres::{SessionInfo, TlsState};
use std::path::PathBuf;

fn connected_model() -> Model {
    let mut model = Model::new(100);
    update(
        &mut model,
        Message::Connected(Box::new(SessionInfo {
            target: "local@localhost:5432/orders".to_owned(),
            database: "orders".to_owned(),
            user: "app".to_owned(),
            server_version: "18.4".to_owned(),
            backend_pid: "t041".to_owned(),
            search_path: "public".to_owned(),
            read_only: false,
            tls: TlsState::Disabled,
            environment: Environment::Local,
        })),
    );
    model
}

fn type_text(model: &mut Model, text: &str) {
    for ch in text.chars() {
        update(model, Message::Action(Action::Insert(ch)));
    }
}

#[test]
fn multi_buffer_creation_switching_and_isolation() {
    let mut model = connected_model();

    // Initial state: 1 buffer
    assert_eq!(model.buffers.count(), 1);
    assert_eq!(model.buffers.active_idx, 0);

    // Type in buffer 1
    type_text(&mut model, "SELECT 1 FROM first_table;");
    assert_eq!(model.editor.text(), "SELECT 1 FROM first_table;");

    // Create Buffer 2 via Action::NewEditorBuffer
    update(&mut model, Message::Action(Action::NewEditorBuffer));
    assert_eq!(model.buffers.count(), 2);
    assert_eq!(model.buffers.active_idx, 1);
    assert_eq!(model.editor.text(), "");

    // Type in buffer 2
    type_text(&mut model, "SELECT 2 FROM second_table;");
    assert_eq!(model.editor.text(), "SELECT 2 FROM second_table;");

    // Switch back to buffer 1 via Action::PrevEditorBuffer
    update(&mut model, Message::Action(Action::PrevEditorBuffer));
    assert_eq!(model.buffers.active_idx, 0);
    assert_eq!(model.editor.text(), "SELECT 1 FROM first_table;");

    // Switch to buffer 2 via Action::NextEditorBuffer
    update(&mut model, Message::Action(Action::NextEditorBuffer));
    assert_eq!(model.buffers.active_idx, 1);
    assert_eq!(model.editor.text(), "SELECT 2 FROM second_table;");
}

#[test]
fn split_mode_toggle_cycles_correctly() {
    let mut model = connected_model();

    // Split cannot activate with only 1 buffer
    assert_eq!(model.buffers.split_mode, EditorSplitMode::None);
    update(&mut model, Message::Action(Action::ToggleEditorSplit));
    assert_eq!(model.buffers.split_mode, EditorSplitMode::None);

    // Add second buffer
    update(&mut model, Message::Action(Action::NewEditorBuffer));
    assert_eq!(model.buffers.count(), 2);

    // Cycle 1: Vertical split
    update(&mut model, Message::Action(Action::ToggleEditorSplit));
    assert_eq!(model.buffers.split_mode, EditorSplitMode::Vertical);
    assert_eq!(model.buffers.secondary_idx, Some(0));

    // Cycle 2: Horizontal split
    update(&mut model, Message::Action(Action::ToggleEditorSplit));
    assert_eq!(model.buffers.split_mode, EditorSplitMode::Horizontal);
    assert_eq!(model.buffers.secondary_idx, Some(0));

    // Cycle 3: Off (None)
    update(&mut model, Message::Action(Action::ToggleEditorSplit));
    assert_eq!(model.buffers.split_mode, EditorSplitMode::None);
    assert_eq!(model.buffers.secondary_idx, None);
}

#[test]
fn close_buffer_action_workflow() {
    let mut model = connected_model();

    // Type in initial buffer
    type_text(&mut model, "SELECT 42;");
    assert_eq!(model.editor.text(), "SELECT 42;");

    // Closing lone buffer resets text to empty
    update(&mut model, Message::Action(Action::CloseEditorBuffer));
    assert_eq!(model.buffers.count(), 1);
    assert_eq!(model.editor.text(), "");

    // Add buffer 2 and buffer 3
    type_text(&mut model, "SELECT 'first';");
    update(&mut model, Message::Action(Action::NewEditorBuffer));
    type_text(&mut model, "SELECT 'second';");
    update(&mut model, Message::Action(Action::NewEditorBuffer));
    type_text(&mut model, "SELECT 'third';");

    assert_eq!(model.buffers.count(), 3);
    assert_eq!(model.buffers.active_idx, 2);

    // Close buffer 3
    update(&mut model, Message::Action(Action::CloseEditorBuffer));
    assert_eq!(model.buffers.count(), 2);
    assert_eq!(model.buffers.active_idx, 1);
    assert_eq!(model.editor.text(), "SELECT 'second';");
}

#[test]
fn preloaded_files_initialize_editor_buffers() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let file1: PathBuf = temp_dir.path().join("users.sql");
    let file2: PathBuf = temp_dir.path().join("orders.sql");

    std::fs::write(&file1, "SELECT * FROM users;").expect("write file1");
    std::fs::write(&file2, "SELECT * FROM orders;").expect("write file2");

    let mut model = connected_model();
    model.buffers.load_files(&[file1.clone(), file2.clone()]);
    model.sync_editor_from_active_buffer();

    assert_eq!(model.buffers.count(), 2);
    assert_eq!(model.buffers.active_idx, 0);
    assert_eq!(model.buffers.buffers[0].title, "users.sql");
    assert_eq!(model.editor.text(), "SELECT * FROM users;");

    update(&mut model, Message::Action(Action::NextEditorBuffer));
    assert_eq!(model.buffers.active_idx, 1);
    assert_eq!(model.buffers.buffers[1].title, "orders.sql");
    assert_eq!(model.editor.text(), "SELECT * FROM orders;");
}
