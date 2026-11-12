//! Acceptance contract for Feature 043: session state and crash recovery.
//!
//! The headless routes are exercised through the real binary against an
//! explicit data directory. The interactive banner and reducer are covered by
//! sidecar tests; a PTY is not required to prove that a journal round-trips.

#![allow(clippy::print_stderr)]

use ignatius::buffer::{BufferManager, EditorSplitMode};
use ignatius::recovery::{RECOVERY_DIR_NAME, RECOVERY_FILE_NAME, RecoveryJournal};
#[cfg(unix)]
use std::io::Write as _;
use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::process::{Child, Stdio};
use std::process::{Command, Output};

fn binary(root: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ignatius"));
    command.env("IGNATIUS_CONFIG_DIR", root);
    command.env("IGNATIUS_DATA_DIR", root.join("data"));
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

fn temp_root() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let unique = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "ignatius-recovery-test-{}-{}-{unique}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("create temp root");
    dir
}

fn recovery_dir(root: &Path) -> PathBuf {
    root.join("data").join(RECOVERY_DIR_NAME)
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

/// Seeds the same shape a crashed interactive session would leave behind.
fn seed_crashed_journal(root: &Path) -> RecoveryJournal {
    let mut manager = BufferManager::new();
    manager
        .active_buffer_mut()
        .editor
        .set_text("SELECT 'alpha';");
    manager.active_buffer_mut().editor.insert(' ');
    manager.active_buffer_mut().editor.set_cursor(7);

    manager.new_buffer(
        Some("migration.sql".into()),
        Some("ALTER TABLE users ADD COLUMN age int;".into()),
    );
    manager.active_buffer_mut().editor.insert(' ');
    manager.split_mode = EditorSplitMode::Vertical;

    let journal = RecoveryJournal::from_buffers(&manager, "contract-session", false);
    journal
        .save_to_disk(&recovery_dir(root))
        .expect("seed journal");
    journal
}

#[test]
fn status_reports_absent_journal_without_failing() {
    let root = temp_root();
    let output = binary(&root)
        .args(["recovery", "status"])
        .output()
        .expect("run recovery status");
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    assert!(stdout(&output).contains("No session recovery journal."));

    let output = binary(&root)
        .args(["recovery", "status", "--format", "json"])
        .output()
        .expect("run recovery status json");
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    let value: serde_json::Value =
        serde_json::from_str(&stdout(&output)).expect("valid json output");
    assert_eq!(value["present"], false);
}

#[test]
fn status_reports_a_crashed_session_in_table_and_json() {
    let root = temp_root();
    seed_crashed_journal(&root);

    let output = binary(&root)
        .args(["recovery", "status"])
        .output()
        .expect("run recovery status");
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("Unrecovered session"), "{text}");
    assert!(text.contains("2 buffer(s)"), "{text}");
    assert!(text.contains("2 unsaved"), "{text}");
    assert!(text.contains("0 truncated"), "{text}");

    let output = binary(&root)
        .args(["recovery", "status", "--format", "json"])
        .output()
        .expect("run recovery status json");
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    let value: serde_json::Value =
        serde_json::from_str(&stdout(&output)).expect("valid json output");
    assert_eq!(value["present"], true);
    assert_eq!(value["clean_exit"], false);
    assert_eq!(value["session_id"], "contract-session");
    assert_eq!(value["recoverable_buffers"], 2);
    assert_eq!(value["unsaved_buffers"], 2);
    assert_eq!(value["truncated_buffers"], 0);
}

#[test]
fn restore_emits_sql_and_json_without_destroying_the_journal() {
    let root = temp_root();
    seed_crashed_journal(&root);
    let journal_path = recovery_dir(&root).join(RECOVERY_FILE_NAME);

    let output = binary(&root)
        .args(["recovery", "restore"])
        .output()
        .expect("run recovery restore");
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(
        text.contains("-- ---- buffer 1: buffer-1.sql ----"),
        "{text}"
    );
    assert!(text.contains("SELECT 'alpha';"), "{text}");
    assert!(text.contains("migration.sql"), "{text}");
    assert!(
        text.contains("ALTER TABLE users ADD COLUMN age int;"),
        "{text}"
    );
    assert!(
        stderr(&output).contains("journal is still on disk"),
        "restore must say it did not consume the journal: {}",
        stderr(&output)
    );
    assert!(journal_path.exists(), "restore must not delete the journal");

    let output = binary(&root)
        .args(["recovery", "restore", "--format", "json"])
        .output()
        .expect("run recovery restore json");
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    let value: serde_json::Value =
        serde_json::from_str(&stdout(&output)).expect("valid json output");
    assert_eq!(value["session_id"], "contract-session");
    assert_eq!(value["buffers"].as_array().unwrap().len(), 2);
    assert_eq!(value["split_mode"], "vertical");
}

#[test]
fn discard_removes_the_journal_and_a_second_discard_is_harmless() {
    let root = temp_root();
    seed_crashed_journal(&root);

    let output = binary(&root)
        .args(["recovery", "discard"])
        .output()
        .expect("run recovery discard");
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    assert!(stdout(&output).contains("Discarded"));
    assert!(!recovery_dir(&root).join(RECOVERY_FILE_NAME).exists());

    let output = binary(&root)
        .args(["recovery", "discard"])
        .output()
        .expect("run recovery discard again");
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    assert!(stdout(&output).contains("No session recovery journal"));
}

#[test]
fn restore_refuses_a_clean_or_absent_journal() {
    let root = temp_root();
    let output = binary(&root)
        .args(["recovery", "restore"])
        .output()
        .expect("run recovery restore");
    assert_eq!(code(&output), 2, "stderr: {}", stderr(&output));
    assert!(stderr(&output).contains("no session recovery journal"));

    let mut manager = BufferManager::new();
    manager
        .active_buffer_mut()
        .editor
        .set_text("SELECT 'clean';");
    manager.active_buffer_mut().editor.insert(' ');
    let mut journal = RecoveryJournal::from_buffers(&manager, "clean-session", true);
    journal.clean_exit = true;
    journal
        .save_to_disk(&recovery_dir(&root))
        .expect("seed clean journal");

    let output = binary(&root)
        .args(["recovery", "restore"])
        .output()
        .expect("run recovery restore clean");
    assert_eq!(code(&output), 2, "stderr: {}", stderr(&output));
    assert!(stderr(&output).contains("nothing worth restoring"));
}

#[test]
fn the_documented_recovery_routes_exist_and_advertise_no_retired_chords() {
    let root = temp_root();
    let output = binary(&root)
        .args(["recovery", "--help"])
        .output()
        .expect("run recovery help");
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    let help = stdout(&output);
    for route in ["status", "restore", "discard"] {
        assert!(help.contains(route), "recovery help omits {route}: {help}");
    }

    let readme = include_str!("../README.md");
    assert!(readme.contains("ignatius recovery status"));
    assert!(readme.contains("ignatius recovery restore --format sql"));
    assert!(readme.contains("ignatius recovery discard"));
    assert!(
        !readme.contains("| `!` | Restore crash-recovered"),
        "the retired global `!` chord is still advertised"
    );

    let keymap = include_str!("../docs/design/keymap.md");
    assert!(keymap.contains("`Ctrl+K r` restores the recovered buffers"));
    assert!(
        !keymap.contains("| `~` | Discard crash recovery journal"),
        "the retired global `~` chord is still advertised"
    );
}

#[test]
fn a_damaged_journal_is_reported_and_preserved_for_inspection() {
    let root = temp_root();
    let dir = recovery_dir(&root);
    std::fs::create_dir_all(&dir).expect("create recovery dir");
    std::fs::write(dir.join(RECOVERY_FILE_NAME), "{not json").expect("write damaged journal");

    let output = binary(&root)
        .args(["recovery", "status"])
        .output()
        .expect("run recovery status");
    assert_eq!(code(&output), 2, "stderr: {}", stderr(&output));
    assert!(stderr(&output).contains("failed to parse recovery journal"));
    assert!(
        dir.join(RECOVERY_FILE_NAME).exists(),
        "a damaged journal is evidence, not something to delete silently"
    );

    let output = binary(&root)
        .args(["recovery", "discard"])
        .output()
        .expect("run recovery discard");
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    assert!(!dir.join(RECOVERY_FILE_NAME).exists());
}

/// A target refused immediately, so the client draws its workbench and sits.
#[cfg(unix)]
const UNREACHABLE: &str = "postgres://someone@127.0.0.1:1/nothing?connect_timeout=2";

#[cfg(unix)]
fn script_available() -> bool {
    Command::new("script").arg("--version").output().is_ok()
        || Command::new("script").arg("-h").output().is_ok()
}

/// Runs the real client under a pty with isolated state.
#[cfg(unix)]
fn spawn_interactive(root: &Path) -> (Child, std::fs::File, std::fs::File) {
    use std::os::unix::process::CommandExt as _;

    let binary = env!("CARGO_BIN_EXE_ignatius");
    let inner = format!("stty rows 30 cols 100; exec {binary} connect '{UNREACHABLE}'");
    let mut command = Command::new("script");
    if cfg!(target_os = "linux") {
        command.args(["-e", "-q", "-c", &inner, "/dev/null"]);
    } else {
        command.args(["-q", "/dev/null", "sh", "-c", &inner]);
    }
    command
        .env("IGNATIUS_CONFIG_DIR", root)
        .env("IGNATIUS_DATA_DIR", root.join("data"))
        .env_remove("IGNATIUS_LOG")
        .stdin(Stdio::piped());
    for key in [
        "PGHOST",
        "PGPORT",
        "PGDATABASE",
        "PGUSER",
        "PGPASSWORD",
        "PGSERVICE",
        "PGSSLMODE",
        "PGPASSFILE",
    ] {
        command.env_remove(key);
    }
    command.env("PGPASSFILE", root.join("missing.pgpass"));
    // Own the whole process group so a kill cannot leave a detached client
    // holding the pty or the recovery journal.
    command.process_group(0);

    let stdout = tempfile::tempfile().expect("private pty stdout");
    let stderr = tempfile::tempfile().expect("private pty stderr");
    let child = command
        .stdout(Stdio::from(
            stdout.try_clone().expect("clone private stdout"),
        ))
        .stderr(Stdio::from(
            stderr.try_clone().expect("clone private stderr"),
        ))
        .spawn()
        .expect("spawn under a pty");
    (child, stdout, stderr)
}

#[cfg(unix)]
fn kill_group(child: &mut Child) {
    if child.try_wait().ok().flatten().is_none() {
        let pid = child.id();
        let _ = Command::new("sh")
            .args(["-c", &format!("kill -9 -{pid}")])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let _ = child.kill();
    }
    let _ = child.wait();
}

#[cfg(unix)]
fn pty_text(file: &mut std::fs::File) -> String {
    use std::io::{Read as _, Seek as _, SeekFrom};
    file.seek(SeekFrom::Start(0)).expect("rewind capture");
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).expect("read capture");
    String::from_utf8_lossy(&bytes).into_owned()
}

#[test]
#[cfg(unix)]
fn a_killed_interactive_session_is_recoverable_and_a_quit_without_deciding_preserves_it() {
    if !script_available() {
        eprintln!("skipping: `script` is not available to allocate a pty");
        return;
    }
    let root = temp_root();
    let (mut child, mut screen_file, _stderr) = spawn_interactive(&root);

    // The editor has focus on the first frame. Typing then waiting past the
    // checkpoint interval proves the journal is written without any quit path.
    std::thread::sleep(std::time::Duration::from_millis(900));
    child
        .stdin
        .as_mut()
        .expect("pty stdin")
        .write_all(b"SELECT 'survives a kill';")
        .expect("type into the editor");
    std::thread::sleep(std::time::Duration::from_millis(2600));

    let rendered = {
        // Read a snapshot before killing; the file is still owned by the child.
        use std::io::{Read as _, Seek as _, SeekFrom};
        screen_file.seek(SeekFrom::Start(0)).expect("rewind");
        let mut bytes = Vec::new();
        screen_file
            .read_to_end(&mut bytes)
            .expect("read live capture");
        String::from_utf8_lossy(&bytes).into_owned()
    };
    assert!(
        rendered.contains("Ignatius") || rendered.contains("Editor"),
        "the real client rendered before the crash: {rendered}"
    );

    kill_group(&mut child);

    let status = binary(&root)
        .args(["recovery", "status"])
        .output()
        .expect("run recovery status after a kill");
    assert_eq!(code(&status), 0, "stderr: {}", stderr(&status));
    assert!(
        stdout(&status).contains("Unrecovered session"),
        "a killed client leaves an unrecovered journal: {}",
        stdout(&status)
    );

    let restored = binary(&root)
        .args(["recovery", "restore"])
        .output()
        .expect("run recovery restore after a kill");
    assert_eq!(code(&restored), 0, "stderr: {}", stderr(&restored));
    assert!(
        stdout(&restored).contains("survives a kill"),
        "the typed buffer survived the kill: {}",
        stdout(&restored)
    );

    // Relaunch: the capsule must be visible, and quitting without choosing
    // must not replace the crashed journal.
    let (mut child, mut screen_file, _stderr) = spawn_interactive(&root);
    std::thread::sleep(std::time::Duration::from_millis(1600));
    child
        .stdin
        .as_mut()
        .expect("pty stdin")
        .write_all(b"\x11")
        .expect("quit with Ctrl+Q");
    std::thread::sleep(std::time::Duration::from_millis(800));
    kill_group(&mut child);
    let screen = pty_text(&mut screen_file);
    assert!(
        screen.contains("CRASH RECOVERY"),
        "the startup capsule is visible: {screen}"
    );

    let status = binary(&root)
        .args(["recovery", "status"])
        .output()
        .expect("run recovery status after quitting without deciding");
    assert!(
        stdout(&status).contains("Unrecovered session"),
        "quitting without deciding preserves the crash journal: {}",
        stdout(&status)
    );

    let discard = binary(&root)
        .args(["recovery", "discard"])
        .output()
        .expect("discard after the test");
    assert_eq!(code(&discard), 0, "stderr: {}", stderr(&discard));
}
