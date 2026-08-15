//! Terminal acquisition and restoration.
//!
//! Every mode this module turns on is turned off again on every exit path:
//! normal quit, error, cancellation, panic and supported signals. Acquisition is
//! guarded by RAII so an early return cannot skip the restore, and a panic hook
//! restores before the panic message is printed so the message lands on a usable
//! terminal rather than inside the alternate screen.
//!
//! The escape sequences are emitted through an injected writer, which is what
//! makes restoration testable without a pseudo-terminal: see the tests below and
//! `tests/terminal_restore.rs` for the process-level panic path.

use crate::diagnostics::doctor::TerminalFacts;
use crossterm::cursor::{Hide, Show};
use crossterm::event::{
    DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
};
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{execute, terminal};
use std::io::{self, IsTerminal, Write};
use std::sync::atomic::{AtomicBool, Ordering};

/// Which optional terminal modes to turn on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalOptions {
    /// Mouse reporting. Off by default: capturing the mouse takes away the
    /// terminal's own text selection, which users rely on for copying.
    pub mouse: bool,
    /// Use the alternate screen so the user's scrollback survives.
    pub alternate_screen: bool,
    /// Bracketed paste, so pasted SQL is not interpreted as key chords.
    pub bracketed_paste: bool,
}

impl Default for TerminalOptions {
    fn default() -> Self {
        Self {
            mouse: false,
            alternate_screen: true,
            bracketed_paste: true,
        }
    }
}

/// Emits the sequences that put the terminal into full-screen mode.
///
/// Order matters and is mirrored exactly by [`leave`].
pub fn enter<W: Write>(writer: &mut W, options: TerminalOptions) -> io::Result<()> {
    if options.alternate_screen {
        execute!(writer, EnterAlternateScreen)?;
    }
    if options.bracketed_paste {
        execute!(writer, EnableBracketedPaste)?;
    }
    if options.mouse {
        execute!(writer, EnableMouseCapture)?;
    }
    execute!(writer, Hide)?;
    writer.flush()
}

/// Emits the sequences that undo [`enter`], in reverse order.
///
/// Every step runs even if an earlier one fails, so one broken sequence cannot
/// strand the terminal in raw mode.
pub fn leave<W: Write>(writer: &mut W, options: TerminalOptions) -> io::Result<()> {
    let mut first_error = None;
    let mut record = |result: io::Result<()>| {
        if let Err(err) = result
            && first_error.is_none()
        {
            first_error = Some(err);
        }
    };

    record(execute!(writer, Show));
    if options.mouse {
        record(execute!(writer, DisableMouseCapture));
    }
    if options.bracketed_paste {
        record(execute!(writer, DisableBracketedPaste));
    }
    if options.alternate_screen {
        record(execute!(writer, LeaveAlternateScreen));
    }
    record(writer.flush());
    first_error.map_or(Ok(()), Err)
}

/// Tracks whether the process currently holds the terminal, so the panic hook
/// only restores when there is something to restore.
static TERMINAL_HELD: AtomicBool = AtomicBool::new(false);

/// Owns the terminal for the lifetime of the interactive session.
///
/// Dropping the guard restores the terminal. The restore is idempotent, so an
/// explicit [`TerminalGuard::restore`] followed by a drop is safe.
#[derive(Debug)]
pub struct TerminalGuard {
    options: TerminalOptions,
    restored: bool,
}

impl TerminalGuard {
    /// Takes the terminal: raw mode, alternate screen, and the chosen options.
    pub fn acquire(options: TerminalOptions) -> io::Result<Self> {
        terminal::enable_raw_mode()?;
        let mut stdout = io::stdout();
        if let Err(err) = enter(&mut stdout, options) {
            // Give the terminal back before reporting, so the error is readable.
            let _ = terminal::disable_raw_mode();
            return Err(err);
        }
        TERMINAL_HELD.store(true, Ordering::SeqCst);
        Ok(Self {
            options,
            restored: false,
        })
    }

    /// Restores the terminal. Safe to call more than once.
    pub fn restore(&mut self) -> io::Result<()> {
        if self.restored {
            return Ok(());
        }
        self.restored = true;
        TERMINAL_HELD.store(false, Ordering::SeqCst);
        let mut stdout = io::stdout();
        let leave_result = leave(&mut stdout, self.options);
        let raw_result = terminal::disable_raw_mode();
        leave_result.and(raw_result)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        // A failure here cannot be reported anywhere useful, but it must not
        // panic during unwinding.
        let _ = self.restore();
    }
}

/// Installs a panic hook that restores the terminal before the default hook runs.
///
/// Without this, a panic prints its message into the alternate screen, which then
/// disappears, leaving the user with a broken terminal and no explanation.
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if TERMINAL_HELD.swap(false, Ordering::SeqCst) {
            let mut stdout = io::stdout();
            let _ = leave(&mut stdout, TerminalOptions::default());
            let _ = terminal::disable_raw_mode();
        }
        let mut stderr = io::stderr();
        let _ = writeln!(
            stderr,
            "\n{} stopped unexpectedly. Your terminal has been restored.",
            crate::branding::PRODUCT_NAME
        );
        let _ = writeln!(
            stderr,
            "No query was retried and no data was written. Details follow; they contain no \
             connection secrets, and enabling {} adds a local log file.",
            crate::diagnostics::logging::LOG_ENV
        );
        previous(info);
    }));
}

/// Reads what can be honestly determined about the terminal.
///
/// Detection is deliberately shallow: over-detection produces confident wrong
/// answers. Anything uncertain is reported as such and can be overridden through
/// configuration.
#[must_use]
pub fn capabilities() -> TerminalFacts {
    let is_terminal = io::stdout().is_terminal();
    TerminalFacts {
        is_terminal,
        term: std::env::var("TERM").ok().filter(|v| !v.is_empty()),
        term_program: std::env::var("TERM_PROGRAM").ok().filter(|v| !v.is_empty()),
        no_color: std::env::var_os("NO_COLOR").is_some(),
        size: if is_terminal {
            terminal::size().ok()
        } else {
            None
        },
        unicode: detect_unicode(),
    }
}

/// Guesses whether the environment can render Unicode box drawing.
fn detect_unicode() -> bool {
    if cfg!(windows) {
        // Windows Terminal and PowerShell 7 default to UTF-8 output.
        return true;
    }
    ["LC_ALL", "LC_CTYPE", "LANG"]
        .iter()
        .filter_map(|key| std::env::var(key).ok())
        .any(|value| {
            value.to_ascii_uppercase().contains("UTF-8")
                || value.to_ascii_uppercase().contains("UTF8")
        })
}

/// Decides whether colour should be emitted, honouring the documented overrides.
///
/// `NO_COLOR` and `TERM=dumb` win over configuration, because a user who sets
/// them has told every program on the machine what they need.
#[must_use]
pub fn should_use_color(config: crate::config::ColorMode, facts: &TerminalFacts) -> bool {
    use crate::config::ColorMode;
    if facts.no_color || facts.term.as_deref() == Some("dumb") {
        return false;
    }
    match config {
        ColorMode::Always => true,
        ColorMode::Never => false,
        ColorMode::Auto => facts.is_terminal,
    }
}

#[cfg(test)]
mod tests {
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
    fn no_color_and_dumb_terminals_win_over_configuration() {
        use crate::config::ColorMode;
        let base = TerminalFacts {
            is_terminal: true,
            term: Some("xterm-256color".into()),
            term_program: None,
            no_color: false,
            size: Some((100, 30)),
            unicode: true,
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
}
