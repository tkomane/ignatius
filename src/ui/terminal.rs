//! Terminal acquisition and restoration.
//!
//! Every mode this module turns on has a matching restore attempt on every exit path:
//! normal quit, error, cancellation, panic and supported signals. Acquisition is
//! guarded by RAII so an early return cannot skip the restore, and a panic hook
//! attempts restoration before the panic message is printed, so successful
//! cleanup leaves the message on a usable terminal rather than the alternate screen.
//!
//! The escape sequences are emitted through an injected writer, which is what
//! makes restoration testable without a pseudo-terminal. `tests/terminal_restore.rs`
//! proves normal-quit drawing and restoration order at process level. This
//! module's Unix tests exercise the actual guard and panic hook in owned PTYs.

use crate::diagnostics::doctor::TerminalFacts;
use crossterm::cursor::{Hide, Show};
use crossterm::event::{
    DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
};
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{execute, terminal};
use std::io::{self, IsTerminal, Write};
use std::sync::atomic::{AtomicU8, Ordering};

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
/// Every step runs even if an earlier one fails, so one broken sequence does not
/// prevent the remaining cleanup attempts.
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

const HELD: u8 = 1;
const MOUSE: u8 = 1 << 1;
const ALTERNATE_SCREEN: u8 = 1 << 2;
const BRACKETED_PASTE: u8 = 1 << 3;

/// Retains the acquired options atomically, so the panic hook restores those
/// modes without taking a lock while the process is unwinding. Zero is idle.
static TERMINAL_STATE: AtomicU8 = AtomicU8::new(0);

fn hold_terminal(options: TerminalOptions) {
    let state = HELD
        | if options.mouse { MOUSE } else { 0 }
        | if options.alternate_screen {
            ALTERNATE_SCREEN
        } else {
            0
        }
        | if options.bracketed_paste {
            BRACKETED_PASTE
        } else {
            0
        };
    TERMINAL_STATE.store(state, Ordering::SeqCst);
}

fn take_terminal_options() -> Option<TerminalOptions> {
    let state = TERMINAL_STATE.swap(0, Ordering::SeqCst);
    (state & HELD != 0).then_some(TerminalOptions {
        mouse: state & MOUSE != 0,
        alternate_screen: state & ALTERNATE_SCREEN != 0,
        bracketed_paste: state & BRACKETED_PASTE != 0,
    })
}

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
            let _ = restore_modes(&mut stdout, options, terminal::disable_raw_mode);
            return Err(err);
        }
        hold_terminal(options);
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
        TERMINAL_STATE.store(0, Ordering::SeqCst);
        let mut stdout = io::stdout();
        restore_modes(&mut stdout, self.options, terminal::disable_raw_mode)
    }
}

/// Attempts every output-mode cleanup before disabling raw mode.
///
/// `enter` can fail after it has already emitted one or more mode changes. The
/// normal guard does not exist on that path, so the attempted cleanup must run
/// before raw mode is disabled and must still try every requested mode.
fn restore_modes<W: Write>(
    writer: &mut W,
    options: TerminalOptions,
    disable_raw_mode: impl FnOnce() -> io::Result<()>,
) -> io::Result<()> {
    leave(writer, options).and(disable_raw_mode())
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        // A failure here cannot be reported anywhere useful, but it must not
        // panic during unwinding.
        let _ = self.restore();
    }
}

/// Installs a panic hook that attempts restoration before the previous hook runs.
///
/// Without this, a panic prints its message into the alternate screen, which then
/// disappears, leaving the user with a broken terminal and no explanation.
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let restored = take_terminal_options().map(|options| {
            let mut stdout = io::stdout();
            restore_modes(&mut stdout, options, terminal::disable_raw_mode).is_ok()
        });
        let mut stderr = io::stderr();
        let _ = write_panic_notice(&mut stderr, restored);
        previous(info);
    }));
}

fn write_panic_notice<W: Write>(writer: &mut W, restored: Option<bool>) -> io::Result<()> {
    let restoration = match restored {
        Some(true) => " Your terminal has been restored.",
        Some(false) => " Terminal restoration was attempted but did not complete.",
        None => "",
    };
    writeln!(
        writer,
        "\n{} stopped unexpectedly.{restoration}",
        crate::branding::PRODUCT_NAME
    )?;
    writeln!(
        writer,
        "Details follow; they may contain sensitive information. Enabling {} adds a local log file.",
        crate::diagnostics::logging::LOG_ENV
    )
}

/// Reads what can be honestly determined about the terminal.
///
/// Detection is deliberately shallow: over-detection produces confident wrong
/// answers. Anything uncertain is reported as such and can be overridden through
/// configuration.
#[must_use]
pub fn capabilities() -> TerminalFacts {
    use crate::config::ColorDepthChoice;
    let is_terminal = io::stdout().is_terminal();
    let mut facts = TerminalFacts {
        is_terminal,
        term: std::env::var("TERM").ok().filter(|v| !v.is_empty()),
        term_program: std::env::var("TERM_PROGRAM").ok().filter(|v| !v.is_empty()),
        no_color: std::env::var_os("NO_COLOR").is_some(),
        colorterm: std::env::var("COLORTERM").ok().filter(|v| !v.is_empty()),
        size: if is_terminal {
            terminal::size().ok()
        } else {
            None
        },
        unicode: detect_unicode(),
        color_depth: crate::ui::theme::ColorDepth::TrueColor,
        color_depth_source: crate::ui::theme::DepthSource::Default,
    };
    // The environment alone decides here; the runtime overwrites these two
    // fields with the effective flag and configuration resolution before
    // `doctor` runs.
    let (depth, source) = resolve_color_depth(ColorDepthChoice::Auto, None, true, &facts);
    facts.color_depth = depth;
    facts.color_depth_source = source;
    facts
}

/// Resolves the active colour depth and where it came from.
///
/// Precedence is pinned by `contracts/presentation.md`: a forced-off route
/// wins over everything; an explicit choice wins over detection; `COLORTERM`
/// wins over `TERM`; and the fallback is the 16 basic colours.
#[must_use]
pub fn resolve_color_depth(
    choice: crate::config::ColorDepthChoice,
    choice_source: Option<crate::ui::theme::DepthSource>,
    color_enabled: bool,
    facts: &TerminalFacts,
) -> (crate::ui::theme::ColorDepth, crate::ui::theme::DepthSource) {
    use crate::config::ColorDepthChoice;
    use crate::ui::theme::{ColorDepth, DepthSource};
    if !color_enabled || facts.no_color || facts.term.as_deref() == Some("dumb") {
        return (ColorDepth::None, DepthSource::ForcedOff);
    }
    let source = choice_source.unwrap_or(DepthSource::Configuration);
    match choice {
        ColorDepthChoice::TrueColor => (ColorDepth::TrueColor, source),
        ColorDepthChoice::Indexed256 => (ColorDepth::Indexed256, source),
        ColorDepthChoice::Basic16 => (ColorDepth::Basic16, source),
        ColorDepthChoice::Auto => {
            if matches!(facts.colorterm.as_deref(), Some("truecolor" | "24bit")) {
                (ColorDepth::TrueColor, DepthSource::Colorterm)
            } else if facts
                .term
                .as_deref()
                .is_some_and(|term| term.contains("256color"))
            {
                (ColorDepth::Indexed256, DepthSource::Term)
            } else {
                (ColorDepth::Basic16, DepthSource::Default)
            }
        }
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
mod tests;
