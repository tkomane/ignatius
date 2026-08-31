//! Acceptance contract for Feature 005.
//!
//! This deliberately drives the editor through `Message::Action` and the
//! reducer rather than calling `Editor` methods directly. The Unix case also
//! proves the same eight-line journey through the real keymap and a real pty.

#![allow(clippy::print_stderr)]

use ignatius::app::{Action, Direction, Effect, Message, Model, update};
use ignatius::connection::Environment;
use ignatius::postgres::{SessionInfo, TlsState};

const CORRECTED_SQL: &str = r#"SELECT
    o.id,
    o.customer_id,
    o.total,
    o.created_at,
    o.status
FROM orders AS o
ORDER BY o.created_at DESC;"#;

fn connected_model() -> Model {
    let mut model = Model::new(100);
    update(
        &mut model,
        Message::Connected(Box::new(SessionInfo {
            target: "local@localhost:5432/orders".to_owned(),
            database: "orders".to_owned(),
            user: "app".to_owned(),
            server_version: "18.4".to_owned(),
            backend_pid: "t016".to_owned(),
            search_path: "public".to_owned(),
            read_only: false,
            tls: TlsState::Disabled,
            environment: Environment::Local,
        })),
    );
    model
}

fn action(model: &mut Model, action: Action) -> Vec<Effect> {
    update(model, Message::Action(action))
}

fn type_text(model: &mut Model, text: &str) {
    for ch in text.chars() {
        assert!(action(model, Action::Insert(ch)).is_empty());
    }
}

fn type_line(model: &mut Model, text: &str) {
    type_text(model, text);
    assert!(action(model, Action::Activate).is_empty());
}

#[test]
fn an_eight_line_statement_can_be_typed_corrected_navigated_and_run() {
    let mut model = connected_model();

    // Enter after an indented line should carry only that line's leading
    // indentation to the next line. The final line is intentionally left open
    // so the journey can navigate back before running the whole buffer.
    type_line(&mut model, "SELECT");
    type_line(&mut model, "    o.id,");
    type_line(&mut model, "o.custmoer_id,");
    type_line(&mut model, "o.total,");
    type_line(&mut model, "o.created_at,");
    type_line(&mut model, "o.status");

    // The inherited four spaces are removed before the top-level FROM clause.
    // Backspace, not DeleteForward: an indenting Enter leaves the cursor after
    // the spaces it copied, so forward-delete at the end of the buffer removes
    // nothing. This step was written while Enter still inserted a bare newline,
    // when the cursor did sit at the start of an empty line and both keys were
    // no-ops that happened to leave the right text behind.
    for _ in 0..4 {
        action(&mut model, Action::Backspace);
    }
    type_line(&mut model, "FROM orders AS o");
    type_text(&mut model, "ORDER BY o.created_at DESC;");

    assert_eq!(model.editor.line_count(), 8);
    assert_eq!(
        model.editor.text(),
        r#"SELECT
    o.id,
    o.custmoer_id,
    o.total,
    o.created_at,
    o.status
FROM orders AS o
ORDER BY o.created_at DESC;"#,
        "the reducer path must preserve indentation while typing the eight lines"
    );

    // Go to line three, delete the transposed character, and insert it after
    // the following character. Four inherited spaces plus `o.cust` put the
    // cursor immediately before the transposed `m`. No line is retyped.
    action(&mut model, Action::MoveBufferStart);
    action(&mut model, Action::Move(Direction::Down));
    action(&mut model, Action::Move(Direction::Down));
    for _ in 0..10 {
        action(&mut model, Action::Move(Direction::Right));
    }
    action(&mut model, Action::DeleteForward);
    action(&mut model, Action::Move(Direction::Right));
    action(&mut model, Action::Insert('m'));
    assert_eq!(model.editor.text(), CORRECTED_SQL);

    action(&mut model, Action::MoveBufferEnd);
    assert_eq!(model.editor.cursor(), CORRECTED_SQL.len());
    assert_eq!(model.editor.position().0, 8);

    let effects = action(&mut model, Action::RunBuffer);
    let [Effect::Execute { job, sql }] = effects.as_slice() else {
        panic!("running the corrected buffer must emit one Execute effect: {effects:?}");
    };
    assert_eq!(sql, CORRECTED_SQL);
    assert_eq!(model.phase.job(), Some(*job));
}

#[test]
fn moving_between_edits_starts_a_new_undo_group() {
    let mut model = connected_model();

    type_text(&mut model, "abc");
    action(&mut model, Action::MoveBufferStart);
    type_text(&mut model, "X");

    action(&mut model, Action::Undo);
    assert_eq!(model.editor.text(), "abc");

    action(&mut model, Action::Redo);
    assert_eq!(model.editor.text(), "Xabc");
}

#[cfg(unix)]
fn pty_input() -> Vec<u8> {
    let mut input = b"\x1b[1;5H".to_vec();
    // The interactive runtime opens with its documented starter query. Clear
    // it through the same key path a user has: Ctrl+Home, then Delete.
    for _ in 0..160 {
        input.extend_from_slice(b"\x1b[3~");
    }
    // After five Up keys and Home, four inherited spaces plus `o.cust` put the
    // cursor immediately before the transposed `m` on line three.
    input.extend_from_slice(
        b"SELECT\r\
\x20\x20\x20\x20o.id,\r\
o.custmoer_id,\r\
o.total,\r\
o.created_at,\r\
o.status\r\
\x7f\x7f\x7f\x7f\
FROM orders AS o\r\
ORDER BY o.created_at DESC;\
\x1b[A\x1b[A\x1b[A\x1b[A\x1b[A\x1b[H\
\x1b[C\x1b[C\x1b[C\x1b[C\x1b[C\x1b[C\x1b[C\x1b[C\x1b[C\x1b[C\x1b[3~\x1b[Cm\
\x11",
    );
    input
}

#[cfg(unix)]
fn run_editor_in_pty() -> Option<String> {
    use std::io::Write;
    use std::process::{Command, Stdio};
    use std::time::Duration;

    if Command::new("script").arg("--version").output().is_err()
        && Command::new("script").arg("-h").output().is_err()
    {
        eprintln!("skipping: `script` is not available to allocate a pty");
        return None;
    }

    const UNREACHABLE: &str = "postgres://someone@127.0.0.1:1/nothing?connect_timeout=2";
    let binary = env!("CARGO_BIN_EXE_ignatius");
    let inner = format!("stty rows 30 cols 100; exec {binary} connect '{UNREACHABLE}'");
    let config_dir =
        std::env::temp_dir().join(format!("ignatius-editor-pty-{}", std::process::id()));

    let mut command = Command::new("script");
    if cfg!(target_os = "linux") {
        command.args(["-e", "-q", "-c", &inner, "/dev/null"]);
    } else {
        command.args(["-q", "/dev/null", "sh", "-c", &inner]);
    }

    let mut child = command
        .env("IGNATIUS_CONFIG_DIR", &config_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn editor under a pty");

    let mut stdin = child.stdin.take().expect("pty stdin");
    let input = pty_input();
    let writer = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(3));
        let _ = stdin.write_all(&input);
        let _ = stdin.flush();
        std::thread::sleep(Duration::from_secs(1));
    });

    let id = child.id();
    let guard = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(25));
        let _ = Command::new("kill")
            .args(["-KILL", &id.to_string()])
            .status();
    });

    let output = child.wait_with_output().expect("wait for editor pty");
    let _ = writer.join();
    drop(guard);
    let _ = std::fs::remove_dir_all(&config_dir);

    assert!(
        output.status.success(),
        "the pty editor run did not exit cleanly: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(unix)]
fn rendered_screen(transcript: &str, width: usize, height: usize) -> Vec<String> {
    if width == 0 || height == 0 {
        return Vec::new();
    }

    let mut screen = vec![vec![' '; width]; height];
    let mut row = 0;
    let mut column = 0;
    let bytes = transcript.as_bytes();
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == 0x1b {
            index += 1;
            if index >= bytes.len() {
                break;
            }
            match bytes[index] {
                b'[' => {
                    index += 1;
                    let parameter_start = index;
                    while index < bytes.len() && !(0x40..=0x7e).contains(&bytes[index]) {
                        index += 1;
                    }
                    if index == bytes.len() {
                        break;
                    }
                    let parameters =
                        std::str::from_utf8(&bytes[parameter_start..index]).unwrap_or_default();
                    let final_byte = bytes[index];
                    let mut values = parameters.split(';');
                    let first = values
                        .next()
                        .and_then(|value| value.trim_start_matches('?').parse::<usize>().ok())
                        .unwrap_or(1);
                    match final_byte {
                        b'H' | b'f' => {
                            let target_row = first;
                            let target_column = values
                                .next()
                                .and_then(|value| value.parse::<usize>().ok())
                                .unwrap_or(1);
                            row = target_row.saturating_sub(1).min(height - 1);
                            column = target_column.saturating_sub(1).min(width - 1);
                        }
                        b'A' => row = row.saturating_sub(first),
                        b'B' => row = row.saturating_add(first).min(height - 1),
                        b'C' => column = column.saturating_add(first).min(width - 1),
                        b'D' => column = column.saturating_sub(first),
                        b'G' => column = first.saturating_sub(1).min(width - 1),
                        b'J' => match first {
                            2 | 3 => {
                                for line in &mut screen {
                                    line.fill(' ');
                                }
                            }
                            1 => {
                                for line in screen.iter_mut().take(row + 1) {
                                    line.fill(' ');
                                }
                            }
                            _ => {
                                screen[row][column..].fill(' ');
                                for line in screen.iter_mut().skip(row + 1) {
                                    line.fill(' ');
                                }
                            }
                        },
                        b'K' => match first {
                            1 => screen[row][..=column].fill(' '),
                            2 => screen[row].fill(' '),
                            _ => screen[row][column..].fill(' '),
                        },
                        _ => {}
                    }
                    index += 1;
                }
                b']' => {
                    index += 1;
                    while index < bytes.len() {
                        if bytes[index] == 0x07 {
                            index += 1;
                            break;
                        }
                        if bytes[index] == 0x1b && bytes.get(index + 1).copied() == Some(b'\\') {
                            index += 2;
                            break;
                        }
                        index += 1;
                    }
                }
                _ => index += 1,
            }
        } else if bytes[index] == b'\r' {
            column = 0;
            index += 1;
        } else if bytes[index] == b'\n' {
            row = row.saturating_add(1).min(height - 1);
            index += 1;
        } else if bytes[index] < 0x20 {
            index += 1;
        } else {
            let Some(character) = transcript[index..].chars().next() else {
                break;
            };
            screen[row][column] = character;
            column = column.saturating_add(1).min(width - 1);
            index += character.len_utf8();
        }
    }

    screen
        .into_iter()
        .map(|line| line.into_iter().collect())
        .collect()
}

#[cfg(unix)]
#[test]
fn a_real_pty_can_type_correct_and_render_the_eight_line_statement() {
    let Some(transcript) = run_editor_in_pty() else {
        return;
    };

    let screen = rendered_screen(&transcript, 100, 30);
    let rendered = screen.join("\n");
    assert!(rendered.contains("SELECT"), "editor did not render SELECT");
    assert!(
        rendered.contains("    o.id,"),
        "editor did not render the explicitly indented second line: {rendered}"
    );
    assert!(
        rendered.contains("    o.customer_id,"),
        "the reducer path did not render inherited indentation after correction: {rendered}"
    );
    assert!(
        rendered.contains("FROM orders AS o"),
        "editor did not render the top-level FROM line"
    );
    // A substring match alone would pass on `    FROM orders AS o`, which is
    // what an unremoved inherited indent looks like. The clause is top level,
    // so say that rather than merely that the words are on the screen.
    assert!(
        !rendered.contains("    FROM orders AS o"),
        "the inherited indentation was not removed before the top-level FROM: {rendered}"
    );
    assert!(
        rendered.contains("ORDER BY o.created_at DESC;"),
        "editor did not render the final line"
    );
}
