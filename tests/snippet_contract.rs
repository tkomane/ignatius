//! Acceptance contract for Feature 042: Persistent Scratchpad and Snippet Library.
//!
//! Validates both headless CLI execution (`ignatius snippet list|show|add`)
//! and TUI TEA architecture reducer operations.

#![allow(clippy::print_stderr)]

use ignatius::app::{Action, Message, Model, update};
use ignatius::connection::Environment;
use ignatius::postgres::{SessionInfo, TlsState};
use std::process::{Command, Output};

fn binary(temp_dir: &std::path::Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ignatius"));
    command.env("IGNATIUS_CONFIG_DIR", temp_dir);
    command.env("IGNATIUS_DATA_DIR", temp_dir.join("data"));
    command.env_remove("IGNATIUS_LOG");
    for key in [
        "PGHOST",
        "PGPORT",
        "PGDATABASE",
        "PGUSER",
        "PGPASSWORD",
        "PGSERVICE",
        "PGSSLMODE",
    ] {
        command.env_remove(key);
    }
    command
}

fn temp_config_dir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "ignatius-snippet-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("create temp config dir");
    dir
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn code(output: &Output) -> i32 {
    output.status.code().expect("process exited normally")
}

fn connected_model() -> Model {
    let mut model = Model::new(100);
    update(
        &mut model,
        Message::Connected(Box::new(SessionInfo {
            target: "local@localhost:5432/orders".to_owned(),
            database: "orders".to_owned(),
            user: "app".to_owned(),
            server_version: "18.4".to_owned(),
            backend_pid: "t042".to_owned(),
            search_path: "public".to_owned(),
            read_only: false,
            tls: TlsState::Disabled,
            environment: Environment::Local,
        })),
    );
    model
}

#[test]
fn cli_snippet_list_table_and_json() {
    let temp_dir = temp_config_dir();

    // 1. Table output (default)
    let output = binary(&temp_dir)
        .arg("snippet")
        .arg("list")
        .output()
        .expect("run snippet list");
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    let out = stdout(&output);
    assert!(out.contains("NAME"), "table header missing");
    assert!(out.contains("CATEGORY"), "table header missing");
    assert!(out.contains("BUILTIN"), "table header missing");
    assert!(out.contains("cache-hit-ratio"), "built-in missing");
    assert!(out.contains("active-locks"), "built-in missing");
    assert!(out.contains("table-sizes"), "built-in missing");

    // 2. JSON output
    let output = binary(&temp_dir)
        .arg("snippet")
        .arg("list")
        .arg("--format")
        .arg("json")
        .output()
        .expect("run snippet list json");
    assert_eq!(code(&output), 0);
    let json_val: serde_json::Value =
        serde_json::from_str(&stdout(&output)).expect("valid json output");
    assert!(json_val.is_array());
    let arr = json_val.as_array().unwrap();
    assert!(
        arr.len() >= 8,
        "expected at least 8 curated built-in snippets"
    );
    assert!(
        arr.iter()
            .any(|s| s["id"] == "cache-hit-ratio" && s["is_builtin"] == true)
    );

    // 3. Category filter
    let output = binary(&temp_dir)
        .arg("snippet")
        .arg("list")
        .arg("--category")
        .arg("admin")
        .output()
        .expect("run snippet list admin");
    assert_eq!(code(&output), 0);
    let admin_out = stdout(&output);
    assert!(admin_out.contains("kill-idle-tx"));
    assert!(!admin_out.contains("cache-hit-ratio"));

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn cli_snippet_show_sql_and_json() {
    let temp_dir = temp_config_dir();

    // 1. Show SQL (default format)
    let output = binary(&temp_dir)
        .arg("snippet")
        .arg("show")
        .arg("cache-hit-ratio")
        .output()
        .expect("run snippet show");
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    let out = stdout(&output);
    assert!(out.contains("SELECT"));
    assert!(out.contains("pg_statio_user_indexes"));

    // 2. Show JSON
    let output = binary(&temp_dir)
        .arg("snippet")
        .arg("show")
        .arg("cache-hit-ratio")
        .arg("--format")
        .arg("json")
        .output()
        .expect("run snippet show json");
    assert_eq!(code(&output), 0);
    let json_val: serde_json::Value =
        serde_json::from_str(&stdout(&output)).expect("valid json output");
    assert_eq!(json_val["id"], "cache-hit-ratio");
    assert_eq!(json_val["category"], "performance");
    assert_eq!(json_val["is_builtin"], true);

    // 3. Unknown snippet reports usage error (exit code 2)
    let output = binary(&temp_dir)
        .arg("snippet")
        .arg("show")
        .arg("nonexistent-snippet")
        .output()
        .expect("run snippet show nonexistent");
    assert_eq!(code(&output), 2);
    let err = stderr(&output);
    assert!(err.contains("nonexistent-snippet"));

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn cli_snippet_add_user_snippet_and_persist() {
    let temp_dir = temp_config_dir();

    // 1. Add user snippet via CLI
    let output = binary(&temp_dir)
        .arg("snippet")
        .arg("add")
        .arg("my-daily-report")
        .arg("--title")
        .arg("Daily Executive Summary")
        .arg("--description")
        .arg("Aggregates daily sales and orders.")
        .arg("--sql")
        .arg("SELECT date_trunc('day', created_at) AS day, count(*) FROM orders GROUP BY 1;")
        .output()
        .expect("run snippet add");
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    let err = stderr(&output);
    assert!(err.contains("Saved snippet 'my-daily-report'"));

    // Verify file exists on disk
    let snippet_file = temp_dir.join("snippets").join("my-daily-report.sql");
    assert!(snippet_file.exists(), "snippet file was not persisted");

    // 2. Show the newly added snippet
    let output = binary(&temp_dir)
        .arg("snippet")
        .arg("show")
        .arg("my-daily-report")
        .output()
        .expect("run snippet show user snippet");
    assert_eq!(code(&output), 0);
    assert!(stdout(&output).contains("SELECT date_trunc('day', created_at)"));

    // 3. List with user category
    let output = binary(&temp_dir)
        .arg("snippet")
        .arg("list")
        .arg("--category")
        .arg("user")
        .output()
        .expect("run snippet list user");
    assert_eq!(code(&output), 0);
    let out = stdout(&output);
    assert!(out.contains("my-daily-report"));
    assert!(out.contains("Daily Executive Summary"));

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn tui_snippet_overlay_navigation_and_insertion() {
    let mut model = connected_model();
    assert_eq!(model.editor.text(), "");
    assert!(model.snippet_library.is_none());

    // 1. Open snippet overlay via Action::SnippetLibrary
    update(&mut model, Message::Action(Action::SnippetLibrary));
    assert!(model.snippet_library.is_some());

    // 2. Navigate down to second snippet
    update(
        &mut model,
        Message::Action(Action::Move(ignatius::app::Direction::Down)),
    );
    let state = model.snippet_library.as_ref().unwrap();
    assert_eq!(state.library.selected_idx, 1);

    // 3. Press Enter to insert into editor buffer
    update(&mut model, Message::Action(Action::Activate));
    assert!(
        model.snippet_library.is_none(),
        "overlay should close after insertion"
    );
    assert!(
        !model.editor.text().is_empty(),
        "editor text should contain inserted snippet SQL"
    );
    // Multi-buffer active buffer should be synchronized with editor
    assert_eq!(model.buffers.active_editor().text(), model.editor.text());
}

#[test]
fn tui_snippet_overlay_new_tab_action() {
    let mut model = connected_model();
    assert_eq!(model.buffers.count(), 1);

    // Open snippet overlay
    update(&mut model, Message::Action(Action::SnippetLibrary));
    assert!(model.snippet_library.is_some());

    // Press 'n' to open in a new buffer tab
    update(&mut model, Message::Action(Action::Insert('n')));
    assert!(model.snippet_library.is_none());
    assert_eq!(
        model.buffers.count(),
        2,
        "a new buffer tab should be created"
    );
    assert_eq!(model.buffers.active_idx, 1);
    assert!(
        !model.editor.text().is_empty(),
        "new tab should contain snippet SQL"
    );
}
