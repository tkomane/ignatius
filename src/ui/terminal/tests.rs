use super::*;

fn sequences(options: TerminalOptions) -> (String, String) {
    let mut entered = Vec::new();
    enter(&mut entered, options).expect("enter");
    let mut left = Vec::new();
    leave(&mut left, options).expect("leave");
    (
        String::from_utf8(entered).expect("utf8"),
        String::from_utf8(left).expect("utf8"),
    )
}

#[test]
fn defaults_take_the_alternate_screen_and_leave_the_mouse_alone() {
    let options = TerminalOptions::default();
    assert!(options.alternate_screen);
    assert!(options.bracketed_paste);
    assert!(!options.mouse, "mouse capture must be opt-in");

    let (entered, left) = sequences(options);
    assert!(entered.contains("\x1b[?1049h"), "alternate screen entered");
    assert!(entered.contains("\x1b[?2004h"), "bracketed paste enabled");
    assert!(entered.contains("\x1b[?25l"), "cursor hidden");
    assert!(
        !entered.contains("\x1b[?1000h"),
        "mouse not captured by default"
    );

    assert!(left.contains("\x1b[?25h"), "cursor shown again");
    assert!(left.contains("\x1b[?2004l"), "bracketed paste disabled");
    assert!(left.contains("\x1b[?1049l"), "alternate screen left");
}

#[test]
fn every_mode_that_is_enabled_is_disabled_again() {
    let options = TerminalOptions {
        mouse: true,
        alternate_screen: true,
        bracketed_paste: true,
    };
    let (entered, left) = sequences(options);
    let mut modes = vec![
        ("\x1b[?1049h", "\x1b[?1049l", "alternate screen"),
        ("\x1b[?2004h", "\x1b[?2004l", "bracketed paste"),
        ("\x1b[?25l", "\x1b[?25h", "cursor visibility"),
    ];
    // Mouse capture is the one mode Crossterm does not express as an escape
    // sequence on Windows: it goes through the console API instead, so no
    // bytes reach this writer. The call is still made and still undone,
    // which is what matters; only its observability differs.
    if cfg!(unix) {
        modes.push(("\x1b[?1000h", "\x1b[?1000l", "mouse capture"));
    }
    for (on, off, what) in modes {
        assert!(entered.contains(on), "{what} was never enabled");
        assert!(left.contains(off), "{what} was left enabled");
    }
}

#[test]
fn restore_order_is_the_reverse_of_acquisition() {
    let options = TerminalOptions {
        mouse: true,
        ..TerminalOptions::default()
    };
    let (_, left) = sequences(options);
    let cursor = left.find("\x1b[?25h").expect("cursor restored");
    let paste = left.find("\x1b[?2004l").expect("paste restored");
    let screen = left.find("\x1b[?1049l").expect("screen left");

    // Mouse capture is released through the console API on Windows and so
    // has no sequence to order; where it does emit one, it belongs between
    // the cursor and the paste mode.
    #[cfg(unix)]
    {
        let mouse = left.find("\x1b[?1000l").expect("mouse released");
        assert!(cursor < mouse && mouse < paste, "mouse released in order");
    }

    assert!(
        cursor < paste && paste < screen,
        "the alternate screen must be left last so the restored state applies to the user's screen"
    );
}

#[test]
fn disabled_options_emit_nothing_for_those_modes() {
    let options = TerminalOptions {
        mouse: false,
        alternate_screen: false,
        bracketed_paste: false,
    };
    let (entered, left) = sequences(options);
    assert!(!entered.contains("\x1b[?1049h"));
    assert!(!left.contains("\x1b[?1049l"));
    assert!(!entered.contains("\x1b[?2004h"));
    assert_eq!(entered, "\x1b[?25l", "only the cursor is touched");
}

#[test]
fn output_restore_failure_still_attempts_other_modes_and_raw_restore() {
    #[derive(Default)]
    struct FailFirstWrite {
        failed: bool,
        bytes: Vec<u8>,
    }
    impl Write for FailFirstWrite {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if !self.failed {
                self.failed = true;
                return Err(io::Error::other("synthetic output failure"));
            }
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    let mut writer = FailFirstWrite::default();
    let mut raw_attempted = false;
    let result = restore_modes(
        &mut writer,
        TerminalOptions {
            mouse: true,
            ..TerminalOptions::default()
        },
        || {
            raw_attempted = true;
            Err(io::Error::other("synthetic raw-mode failure"))
        },
    );
    assert_eq!(
        result
            .expect_err("first restore failure is retained")
            .to_string(),
        "synthetic output failure"
    );
    assert!(raw_attempted, "output failure must not skip raw cleanup");
    let attempted = String::from_utf8(writer.bytes).expect("restore sequences");
    assert!(attempted.contains("\x1b[?2004l"));
    assert!(attempted.contains("\x1b[?1049l"));
    #[cfg(unix)]
    assert!(attempted.contains("\x1b[?1000l"));
}

#[test]
fn raw_restore_failure_is_reported_after_successful_output_cleanup() {
    let mut writer = Vec::new();
    let result = restore_modes(&mut writer, TerminalOptions::default(), || {
        Err(io::Error::other("synthetic raw-mode failure"))
    });
    assert_eq!(
        result.expect_err("raw restore failed").to_string(),
        "synthetic raw-mode failure"
    );
    let (_, expected) = sequences(TerminalOptions::default());
    assert_eq!(String::from_utf8(writer).expect("UTF-8"), expected);
}

#[test]
fn panic_notice_only_claims_restoration_when_it_succeeded() {
    for restored in [Some(true), Some(false), None] {
        let mut writer = Vec::new();
        write_panic_notice(&mut writer, restored).expect("panic notice");
        let notice = String::from_utf8(writer).expect("UTF-8");
        assert_eq!(
            notice.contains("Your terminal has been restored."),
            restored == Some(true)
        );
        assert_eq!(
            notice.contains("restoration was attempted but did not complete"),
            restored == Some(false)
        );
        assert!(!notice.contains("no data was written"));
        assert!(!notice.contains("contain no connection secrets"));
        assert!(notice.contains("may contain sensitive information"));
    }
}

#[test]
fn no_color_and_dumb_terminals_win_over_configuration() {
    use crate::config::ColorMode;
    let base = TerminalFacts {
        is_terminal: true,
        term: Some("xterm-256color".into()),
        term_program: None,
        no_color: false,
        colorterm: None,
        size: Some((100, 30)),
        unicode: true,
        color_depth: crate::ui::theme::ColorDepth::TrueColor,
        color_depth_source: crate::ui::theme::DepthSource::Default,
    };

    assert!(should_use_color(ColorMode::Auto, &base));
    assert!(should_use_color(ColorMode::Always, &base));
    assert!(!should_use_color(ColorMode::Never, &base));

    let no_color = TerminalFacts {
        no_color: true,
        ..base.clone()
    };
    assert!(
        !should_use_color(ColorMode::Always, &no_color),
        "NO_COLOR is not negotiable"
    );

    let dumb = TerminalFacts {
        term: Some("dumb".into()),
        ..base.clone()
    };
    assert!(!should_use_color(ColorMode::Always, &dumb));

    let piped = TerminalFacts {
        is_terminal: false,
        ..base
    };
    assert!(
        !should_use_color(ColorMode::Auto, &piped),
        "piped output stays plain"
    );
    assert!(
        should_use_color(ColorMode::Always, &piped),
        "explicit --color=always is honoured"
    );
}

/// A capable truecolor terminal with no explicit depth choice. Detection tests
/// are pure over this fact set and never read the process environment.
fn detection_facts() -> TerminalFacts {
    TerminalFacts {
        is_terminal: true,
        term: Some("xterm-256color".into()),
        term_program: None,
        no_color: false,
        colorterm: None,
        size: Some((100, 30)),
        unicode: true,
        color_depth: crate::ui::theme::ColorDepth::TrueColor,
        color_depth_source: crate::ui::theme::DepthSource::Default,
    }
}

#[test]
fn forced_off_routes_win_over_an_explicit_choice() {
    use crate::config::ColorDepthChoice;
    use crate::ui::theme::{ColorDepth, DepthSource};
    let base = detection_facts();

    let no_color = TerminalFacts {
        no_color: true,
        ..base.clone()
    };
    assert_eq!(
        resolve_color_depth(
            ColorDepthChoice::TrueColor,
            Some(DepthSource::Flag),
            true,
            &no_color
        ),
        (ColorDepth::None, DepthSource::ForcedOff),
        "NO_COLOR defeats an explicit override"
    );

    let dumb = TerminalFacts {
        term: Some("dumb".into()),
        colorterm: Some("truecolor".into()),
        ..base.clone()
    };
    assert_eq!(
        resolve_color_depth(
            ColorDepthChoice::TrueColor,
            Some(DepthSource::Flag),
            true,
            &dumb
        ),
        (ColorDepth::None, DepthSource::ForcedOff),
        "TERM=dumb defeats an explicit override"
    );

    assert_eq!(
        resolve_color_depth(
            ColorDepthChoice::TrueColor,
            Some(DepthSource::Flag),
            false,
            &base
        ),
        (ColorDepth::None, DepthSource::ForcedOff),
        "colour resolved off defeats an explicit override"
    );
}

#[test]
fn an_explicit_choice_wins_over_detection_and_reports_its_source() {
    use crate::config::ColorDepthChoice;
    use crate::ui::theme::{ColorDepth, DepthSource};
    let detected = TerminalFacts {
        colorterm: Some("truecolor".into()),
        term: Some("xterm-256color".into()),
        ..detection_facts()
    };

    assert_eq!(
        resolve_color_depth(
            ColorDepthChoice::Basic16,
            Some(DepthSource::Flag),
            true,
            &detected
        ),
        (ColorDepth::Basic16, DepthSource::Flag)
    );
    assert_eq!(
        resolve_color_depth(
            ColorDepthChoice::Indexed256,
            Some(DepthSource::Configuration),
            true,
            &detected
        ),
        (ColorDepth::Indexed256, DepthSource::Configuration)
    );
    assert_eq!(
        resolve_color_depth(
            ColorDepthChoice::TrueColor,
            Some(DepthSource::Flag),
            true,
            &detected
        ),
        (ColorDepth::TrueColor, DepthSource::Flag)
    );
}

#[test]
fn auto_reads_colorterm_case_sensitively() {
    use crate::config::ColorDepthChoice;
    use crate::ui::theme::{ColorDepth, DepthSource};
    let truecolor = TerminalFacts {
        colorterm: Some("truecolor".into()),
        term: Some("xterm-256color".into()),
        ..detection_facts()
    };
    assert_eq!(
        resolve_color_depth(ColorDepthChoice::Auto, None, true, &truecolor),
        (ColorDepth::TrueColor, DepthSource::Colorterm)
    );

    let twenty_four_bit = TerminalFacts {
        colorterm: Some("24bit".into()),
        ..detection_facts()
    };
    assert_eq!(
        resolve_color_depth(ColorDepthChoice::Auto, None, true, &twenty_four_bit),
        (ColorDepth::TrueColor, DepthSource::Colorterm)
    );

    // The convention is case sensitive: "TRUECOLOR" is not a hint, so the
    // lower-tier TERM fallback decides instead.
    let shouted = TerminalFacts {
        colorterm: Some("TRUECOLOR".into()),
        term: Some("xterm-256color".into()),
        ..detection_facts()
    };
    assert_eq!(
        resolve_color_depth(ColorDepthChoice::Auto, None, true, &shouted),
        (ColorDepth::Indexed256, DepthSource::Term)
    );
}

#[test]
fn auto_reads_term_and_falls_back_to_sixteen() {
    use crate::config::ColorDepthChoice;
    use crate::ui::theme::{ColorDepth, DepthSource};
    let indexed = TerminalFacts {
        colorterm: None,
        term: Some("tmux-256color".into()),
        ..detection_facts()
    };
    assert_eq!(
        resolve_color_depth(ColorDepthChoice::Auto, None, true, &indexed),
        (ColorDepth::Indexed256, DepthSource::Term)
    );

    let basic = TerminalFacts {
        colorterm: None,
        term: Some("xterm".into()),
        ..detection_facts()
    };
    assert_eq!(
        resolve_color_depth(ColorDepthChoice::Auto, None, true, &basic),
        (ColorDepth::Basic16, DepthSource::Default)
    );
}

#[test]
fn colorterm_wins_over_term() {
    use crate::config::ColorDepthChoice;
    use crate::ui::theme::{ColorDepth, DepthSource};
    let both = TerminalFacts {
        colorterm: Some("truecolor".into()),
        term: Some("xterm-256color".into()),
        ..detection_facts()
    };
    assert_eq!(
        resolve_color_depth(ColorDepthChoice::Auto, None, true, &both),
        (ColorDepth::TrueColor, DepthSource::Colorterm)
    );
}

#[cfg(unix)]
const PTY_CHILD_TEST: &str = "ui::terminal::tests::owned_pty_guard_child";

#[cfg(unix)]
fn stty_flags() -> String {
    let output = std::process::Command::new("stty")
        .arg("-a")
        .stdin(std::process::Stdio::inherit())
        .output()
        .expect("read this child's PTY flags");
    assert!(output.status.success(), "stty can read the owned PTY");
    String::from_utf8(output.stdout).expect("stty flags are UTF-8")
}

#[cfg(unix)]
fn assert_cooked_flags(flags: &str) {
    let words: Vec<_> = flags.split([' ', '\n', '\r', '\t', ';']).collect();
    for flag in ["icanon", "echo", "isig"] {
        assert!(words.contains(&flag), "{flag} restored in {flags}");
        assert!(
            !words.contains(&format!("-{flag}").as_str()),
            "{flag} must not remain disabled: {flags}"
        );
    }
}

/// This test is inert in the parent harness. Only an isolated self-exec child
/// takes modes or installs a hook; it never starts the application or a session.
#[cfg(unix)]
#[test]
fn owned_pty_guard_child() {
    let Ok(path) = std::env::var("IGNATIUS_TEST_TERMINAL_PATH") else {
        return;
    };
    let bits: u8 = std::env::var("IGNATIUS_TEST_TERMINAL_OPTIONS")
        .expect("child options")
        .parse()
        .expect("option bits");
    let options = TerminalOptions {
        mouse: bits & 1 != 0,
        alternate_screen: bits & 2 != 0,
        bracketed_paste: bits & 4 != 0,
    };
    assert!(io::stdin().is_terminal());
    assert!(io::stdout().is_terminal());
    assert_cooked_flags(&stty_flags());

    if path == "panic" {
        std::panic::set_hook(Box::new(|info| {
            // This observes actual termios before the previous hook prints any
            // marker/details, rather than relying on the guard's later Drop.
            let flags = stty_flags();
            assert_cooked_flags(&flags);
            assert!(!terminal::is_raw_mode_enabled().expect("raw state"));
            writeln!(io::stderr(), "W07_PREVIOUS_HOOK_MARKER").expect("previous-hook marker");
            writeln!(io::stderr(), "W07_PAYLOAD={info}").expect("previous-hook details");
        }));
        install_panic_hook();
        let payload = std::panic::catch_unwind(|| {
            let _guard = TerminalGuard::acquire(options).expect("acquire owned PTY");
            assert!(terminal::is_raw_mode_enabled().expect("raw state"));
            assert!(
                stty_flags()
                    .split_whitespace()
                    .any(|word| word == "-icanon")
            );
            writeln!(io::stderr(), "W07_ACQUIRED_MARKER").expect("acquisition marker");
            panic!("W07 synthetic panic payload");
        })
        .expect_err("synthetic panic is preserved");
        assert_eq!(
            payload.downcast_ref::<&str>(),
            Some(&"W07 synthetic panic payload"),
            "the hook must preserve the original panic payload"
        );
    } else {
        fn return_error(options: TerminalOptions) -> Result<(), &'static str> {
            let _guard = TerminalGuard::acquire(options).expect("acquire owned PTY");
            assert!(terminal::is_raw_mode_enabled().expect("raw state"));
            assert!(
                stty_flags()
                    .split_whitespace()
                    .any(|word| word == "-icanon")
            );
            writeln!(io::stderr(), "W07_ACQUIRED_MARKER").expect("acquisition marker");
            Err("W07 synthetic error")
        }
        assert_eq!(return_error(options), Err("W07 synthetic error"));
    }
    assert!(!terminal::is_raw_mode_enabled().expect("raw state"));
    assert_cooked_flags(&stty_flags());
    writeln!(
        io::stderr(),
        "W07_AFTER_RESTORE_MARKER raw=false icanon echo isig"
    )
    .expect("restoration marker");
}

#[cfg(unix)]
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(unix)]
#[allow(
    clippy::print_stdout,
    reason = "libtest captures synthetic PTY transcripts unless --nocapture is requested"
)]
fn observe_owned_pty(path: &str, bits: u8) -> String {
    use std::io::{Read, Seek};
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    let root = tempfile::tempdir().expect("private child state");
    let mut capture = tempfile::tempfile().expect("private output capture");
    let executable = std::env::current_exe().expect("this test executable");
    let inner = format!(
        "stty sane; exec {} --exact {PTY_CHILD_TEST} --nocapture --test-threads=1",
        shell_quote(executable.to_str().expect("UTF-8 executable path"))
    );
    let mut command = Command::new("script");
    if cfg!(target_os = "linux") {
        command.args(["-e", "-q", "-c", &inner, "/dev/null"]);
    } else {
        command.args(["-q", "/dev/null", "sh", "-c", &inner]);
    }
    let mut child = command
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("IGNATIUS_CONFIG_DIR", root.path().join("config"))
        .env("IGNATIUS_DATA_DIR", root.path().join("data"))
        .env("PGPASSFILE", root.path().join("absent.pgpass"))
        .env("TERM", "xterm-256color")
        .env("IGNATIUS_TEST_TERMINAL_PATH", path)
        .env("IGNATIUS_TEST_TERMINAL_OPTIONS", bits.to_string())
        .stdin(Stdio::piped())
        .stdout(capture.try_clone().expect("capture stdout"))
        .stderr(capture.try_clone().expect("capture stderr"))
        .spawn()
        .expect("start owned script PTY");
    let deadline = Instant::now() + Duration::from_secs(20);
    let status = loop {
        if let Some(status) = child.try_wait().expect("poll owned PTY child") {
            break status;
        }
        if Instant::now() >= deadline {
            child.kill().expect("stop only this owned PTY child");
            child.wait().expect("reap owned PTY child");
            panic!("owned PTY child exceeded its deadline");
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    capture.rewind().expect("rewind capture");
    let mut text = String::new();
    capture.read_to_string(&mut text).expect("read PTY output");
    println!("W07 observed path={path} options={bits}: {text}");
    assert!(status.success(), "owned PTY child failed: {text}");
    assert!(text.contains("test result: ok. 1 passed"), "{text}");
    text
}

#[cfg(unix)]
fn assert_owned_pty_restored(path: &str) {
    // First prove the unchanged default path, then the all-modes case that
    // detects the former panic-hook assumption before trying every combination.
    for bits in [6, 7, 0, 1, 2, 3, 4, 5] {
        let text = observe_owned_pty(path, bits);
        let options = TerminalOptions {
            mouse: bits & 1 != 0,
            alternate_screen: bits & 2 != 0,
            bracketed_paste: bits & 4 != 0,
        };
        let acquired = text.find("W07_ACQUIRED_MARKER").expect("acquired marker");
        let restored = if path == "panic" {
            text.find("W07_PREVIOUS_HOOK_MARKER")
                .expect("previous hook preserved")
        } else {
            text.find("W07_AFTER_RESTORE_MARKER")
                .expect("ordinary error restored")
        };
        let restore_output = &text[acquired..restored];
        let (_, expected) = sequences(options);
        assert!(
            restore_output.contains(&expected),
            "{path} options {bits}: all acquired modes must be restored before details: {restore_output:?}"
        );
        for (off, enabled) in [
            ("\x1b[?1000l", options.mouse),
            ("\x1b[?2004l", options.bracketed_paste),
            ("\x1b[?1049l", options.alternate_screen),
        ] {
            assert_eq!(
                restore_output.contains(off),
                enabled,
                "{path} options {bits}: only acquired modes are restored"
            );
        }
        if path == "panic" {
            assert!(text.contains("W07_PAYLOAD="));
            assert!(text.contains("W07 synthetic panic payload"));
            assert!(!text.contains("no data was written"));
            assert!(!text.contains("contain no connection secrets"));
        }
    }
}

#[cfg(unix)]
#[test]
fn owned_pty_error_restores_every_acquired_mode() {
    assert_owned_pty_restored("error");
}

#[cfg(unix)]
#[test]
fn owned_pty_panic_restores_acquired_modes_before_previous_hook() {
    assert_owned_pty_restored("panic");
}
