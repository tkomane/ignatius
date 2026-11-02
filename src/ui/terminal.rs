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
    /// On by default; setting false leaves mouse capture off so the terminal owns every pointer gesture.
    pub mouse: bool,
    /// Use the alternate screen so the user's scrollback survives.
    pub alternate_screen: bool,
    /// Bracketed paste, so pasted SQL is not interpreted as key chords.
    pub bracketed_paste: bool,
}

impl Default for TerminalOptions {
    fn default() -> Self {
        Self {
            mouse: true,
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
const FULL_SCREEN: u8 = 1 << 4;

/// Retains the acquired options atomically, so the panic hook restores those
/// modes without taking a lock while the process is unwinding. Zero is idle.
static TERMINAL_STATE: AtomicU8 = AtomicU8::new(0);

fn hold_terminal(options: TerminalOptions) {
    let state = HELD
        | FULL_SCREEN
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

fn take_terminal_state() -> Option<(bool, TerminalOptions)> {
    let state = TERMINAL_STATE.swap(0, Ordering::SeqCst);
    (state & HELD != 0).then_some((
        state & FULL_SCREEN != 0,
        TerminalOptions {
            mouse: state & MOUSE != 0,
            alternate_screen: state & ALTERNATE_SCREEN != 0,
            bracketed_paste: state & BRACKETED_PASTE != 0,
        },
    ))
}

/// Owns the terminal for the lifetime of the interactive session.
///
/// Dropping the guard restores the terminal. The restore is idempotent, so an
/// explicit [`TerminalGuard::restore`] followed by a drop is safe.
#[derive(Debug)]
pub struct TerminalGuard {
    options: TerminalOptions,
    entered_full_screen: bool,
    restored: bool,
}

impl TerminalGuard {
    /// Takes the terminal in raw mode only, without alternate screen or cursor changes.
    pub fn acquire_raw() -> io::Result<Self> {
        terminal::enable_raw_mode()?;
        TERMINAL_STATE.store(HELD, Ordering::SeqCst);
        Ok(Self {
            options: TerminalOptions::default(),
            entered_full_screen: false,
            restored: false,
        })
    }

    /// Transitions a raw-mode terminal into full-screen mode with the given options.
    pub fn enter_full_screen(&mut self, options: TerminalOptions) -> io::Result<()> {
        let mut stdout = io::stdout();
        if let Err(err) = enter(&mut stdout, options) {
            let _ = restore_modes(&mut stdout, options, terminal::disable_raw_mode);
            self.restored = true;
            TERMINAL_STATE.store(0, Ordering::SeqCst);
            return Err(err);
        }
        self.options = options;
        self.entered_full_screen = true;
        hold_terminal(options);
        Ok(())
    }

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
            entered_full_screen: true,
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
        if self.entered_full_screen {
            let mut stdout = io::stdout();
            restore_modes(&mut stdout, self.options, terminal::disable_raw_mode)
        } else {
            terminal::disable_raw_mode()
        }
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
        let restored = take_terminal_state().map(|(entered_full_screen, options)| {
            if entered_full_screen {
                let mut stdout = io::stdout();
                restore_modes(&mut stdout, options, terminal::disable_raw_mode).is_ok()
            } else {
                terminal::disable_raw_mode().is_ok()
            }
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
        background_rgb: None,
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

/// The maximum duration to wait for an OSC 11 terminal background reply.
pub const OSC11_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(100);

/// Queries the terminal for its background colour using OSC 11 with an injected writer and reader.
///
/// Sends `\x1b]11;?\x07`, then reads steps up to `deadline`, parsing the accumulated bytes with
/// [`crate::ui::theme::parse_osc11_reply`]. Buffers at most 256 bytes.
/// Calls `drain` upon completing or reaching the deadline.
pub fn query_osc11_background_with<W, R, D>(
    writer: &mut W,
    deadline: std::time::Instant,
    mut read_step: R,
    mut drain: D,
) -> Option<crate::ui::theme::Rgb>
where
    W: Write,
    R: FnMut(std::time::Duration) -> io::Result<Option<Vec<u8>>>,
    D: FnMut(),
{
    if writer.write_all(crate::ui::theme::OSC11_QUERY).is_err() || writer.flush().is_err() {
        drain();
        return None;
    }

    let mut buffer = Vec::with_capacity(64);
    let mut detected = None;

    while std::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        match read_step(remaining) {
            Ok(Some(bytes)) => {
                if buffer.len() + bytes.len() <= 256 {
                    buffer.extend_from_slice(&bytes);
                } else {
                    let available = 256usize.saturating_sub(buffer.len());
                    buffer.extend_from_slice(&bytes[..available]);
                }
                if let Some(rgb) = crate::ui::theme::parse_osc11_reply(&buffer) {
                    detected = Some(rgb);
                    break;
                }
            }
            Ok(None) => {}
            Err(_) => break,
        }
    }

    drain();
    detected
}

/// Converts a key event to byte(s) for OSC 11 reply parsing.
pub fn key_event_to_bytes(key: &crossterm::event::KeyEvent, out: &mut Vec<u8>) {
    use crossterm::event::{KeyCode, KeyModifiers};
    if key.modifiers.contains(KeyModifiers::ALT) {
        out.push(b'\x1b');
    }
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char(c) if c.is_ascii_alphabetic() => {
                let code = c.to_ascii_lowercase() as u8 - b'a' + 1;
                out.push(code);
            }
            KeyCode::Char(' ') => out.push(0),
            _ => {}
        }
    } else {
        match key.code {
            KeyCode::Esc => out.push(b'\x1b'),
            KeyCode::Char(c) => {
                let mut buf = [0u8; 4];
                out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            }
            KeyCode::Enter => out.push(b'\n'),
            KeyCode::Tab => out.push(b'\t'),
            KeyCode::Backspace => out.push(0x7f),
            _ => {}
        }
    }
}

/// Queries the active terminal for its background RGB colour using an OSC 11 probe.
///
/// Waits up to [`OSC11_TIMEOUT`] (100 ms). Must be called while in raw mode on the main thread
/// before `spawn_input_thread` starts.
pub fn probe_osc11_background() -> Option<crate::ui::theme::Rgb> {
    let mut stdout = io::stdout();
    let deadline = std::time::Instant::now() + OSC11_TIMEOUT;

    let read_step = |timeout: std::time::Duration| -> io::Result<Option<Vec<u8>>> {
        if crossterm::event::poll(timeout)? {
            match crossterm::event::read()? {
                crossterm::event::Event::Key(key) => {
                    let mut bytes = Vec::new();
                    key_event_to_bytes(&key, &mut bytes);
                    Ok(Some(bytes))
                }
                _ => Ok(None),
            }
        } else {
            Ok(None)
        }
    };

    let drain = || {
        while crossterm::event::poll(std::time::Duration::ZERO).unwrap_or(false) {
            if crossterm::event::read().is_err() {
                break;
            }
        }
    };

    query_osc11_background_with(&mut stdout, deadline, read_step, drain)
}

/// Pure state machine filtering out late OSC 11 background replies from user input.
#[derive(Debug, Clone)]
pub struct Osc11LateReplyFilter {
    enabled: bool,
    buffered_keys: Vec<crossterm::event::KeyEvent>,
    buffered_bytes: Vec<u8>,
}

/// Match classification for potential OSC 11 sequences.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Osc11PrefixMatch {
    /// Incomplete sequence matching prefix so far.
    Incomplete,
    /// Complete and valid OSC 11 reply sequence.
    Complete,
    /// Sequence does not match OSC 11 reply.
    Mismatch,
}

/// Validates whether `bytes` is a prefix of or a complete OSC 11 reply.
#[must_use]
pub fn check_osc11_prefix(bytes: &[u8]) -> Osc11PrefixMatch {
    if bytes.is_empty() {
        return Osc11PrefixMatch::Mismatch;
    }
    const PREFIX: &[u8] = b"\x1b]11;rgb:";
    if bytes.len() <= PREFIX.len() {
        if PREFIX.starts_with(bytes) {
            return Osc11PrefixMatch::Incomplete;
        }
        return Osc11PrefixMatch::Mismatch;
    }
    if !bytes.starts_with(PREFIX) {
        return Osc11PrefixMatch::Mismatch;
    }
    let rest = &bytes[PREFIX.len()..];
    let mut parts = Vec::new();
    let mut current_part_start = 0;
    let mut i = 0;
    while i < rest.len() {
        let b = rest[i];
        if b == b'/' {
            let part = &rest[current_part_start..i];
            if !(1..=4).contains(&part.len()) || !part.iter().all(u8::is_ascii_hexdigit) {
                return Osc11PrefixMatch::Mismatch;
            }
            parts.push(part);
            if parts.len() > 2 {
                return Osc11PrefixMatch::Mismatch;
            }
            current_part_start = i + 1;
            i += 1;
            continue;
        }
        if b == b'\x07' {
            if parts.len() != 2 {
                return Osc11PrefixMatch::Mismatch;
            }
            let part = &rest[current_part_start..i];
            if !(1..=4).contains(&part.len()) || !part.iter().all(u8::is_ascii_hexdigit) {
                return Osc11PrefixMatch::Mismatch;
            }
            if i + 1 == rest.len() {
                return Osc11PrefixMatch::Complete;
            }
            return Osc11PrefixMatch::Mismatch;
        }
        if b == b'\x1b' {
            if parts.len() != 2 {
                return Osc11PrefixMatch::Mismatch;
            }
            let part = &rest[current_part_start..i];
            if !(1..=4).contains(&part.len()) || !part.iter().all(u8::is_ascii_hexdigit) {
                return Osc11PrefixMatch::Mismatch;
            }
            if i + 1 == rest.len() {
                return Osc11PrefixMatch::Incomplete;
            }
            if rest[i + 1] == b'\\' && i + 2 == rest.len() {
                return Osc11PrefixMatch::Complete;
            }
            return Osc11PrefixMatch::Mismatch;
        }
        if !b.is_ascii_hexdigit() {
            return Osc11PrefixMatch::Mismatch;
        }
        if i - current_part_start >= 4 {
            return Osc11PrefixMatch::Mismatch;
        }
        i += 1;
    }
    Osc11PrefixMatch::Incomplete
}

impl Osc11LateReplyFilter {
    /// Creates a new filter. When `enabled` is false, all keys pass through immediately.
    #[must_use]
    pub const fn new(enabled: bool) -> Self {
        Self {
            enabled,
            buffered_keys: Vec::new(),
            buffered_bytes: Vec::new(),
        }
    }

    /// Returns whether the filter is actively buffering potential escape keys.
    #[must_use]
    pub fn is_buffering(&self) -> bool {
        !self.buffered_keys.is_empty()
    }

    /// Processes an incoming key event.
    ///
    /// Returns keys to be emitted to the application, or an empty vector while buffering.
    pub fn process_key(
        &mut self,
        key: crossterm::event::KeyEvent,
    ) -> Vec<crossterm::event::KeyEvent> {
        if !self.enabled {
            return vec![key];
        }

        self.buffered_keys.push(key);
        key_event_to_bytes(&key, &mut self.buffered_bytes);

        match check_osc11_prefix(&self.buffered_bytes) {
            Osc11PrefixMatch::Incomplete => Vec::new(),
            Osc11PrefixMatch::Complete => {
                self.buffered_keys.clear();
                self.buffered_bytes.clear();
                self.enabled = false;
                Vec::new()
            }
            Osc11PrefixMatch::Mismatch => {
                self.buffered_bytes.clear();
                std::mem::take(&mut self.buffered_keys)
            }
        }
    }

    /// Flushes any buffered keys immediately.
    pub fn flush(&mut self) -> Vec<crossterm::event::KeyEvent> {
        self.buffered_bytes.clear();
        std::mem::take(&mut self.buffered_keys)
    }
}

#[cfg(test)]
mod tests;
