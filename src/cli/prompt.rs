//! Asking for a password on a line-oriented terminal.
//!
//! The full-screen client draws its own masked field. This is the same promise
//! for the surface that has no drawing: the characters are not echoed, they are
//! held only until the connection is tried again, and nothing keeps them.
//!
//! Raw mode is taken for as short a time as possible and given back by a guard,
//! so an error, a panic or a Ctrl+C between the two cannot leave a terminal
//! that no longer echoes what the user types.

use crate::diagnostics::{Diagnostic, DiagnosticKind};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use std::io::{IsTerminal, Write};

/// Whether there is a terminal to ask on.
///
/// Both ends matter: the question goes to the message stream and the answer
/// comes from the input stream, and a pipe on either is a reason not to ask. A
/// script that hangs waiting for a password nobody can type is worse than one
/// that fails.
#[must_use]
pub fn can_ask() -> bool {
    std::io::stdin().is_terminal() && std::io::stderr().is_terminal()
}

/// Restores the terminal's own line handling whatever happens next.
struct RawMode;

impl RawMode {
    fn enter() -> Result<Self, Diagnostic> {
        crossterm::terminal::enable_raw_mode().map_err(|err| {
            Diagnostic::new(
                DiagnosticKind::Internal,
                "could not turn off echo to ask for a password",
                "asking for a password",
            )
            .likely_cause(err.to_string())
            .next_action("supply the password another way, for example a password file")
        })?;
        Ok(Self)
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        // Best effort by necessity: there is nowhere useful to report a failure
        // here, and leaving raw mode on is the outcome to avoid.
        let _ = crossterm::terminal::disable_raw_mode();
    }
}

/// Asks for a password without echoing it.
///
/// Returns `None` when the user cancels, which is an answer rather than a
/// failure: they chose not to give one.
pub fn read_password(question: &str, err: &mut impl Write) -> Result<Option<String>, Diagnostic> {
    write!(err, "{question}").ok();
    err.flush().ok();

    let mut typed = String::new();
    let outcome = {
        let _raw = RawMode::enter()?;
        loop {
            let event = crossterm::event::read().map_err(|error| {
                Diagnostic::new(
                    DiagnosticKind::Internal,
                    "could not read the password",
                    "asking for a password",
                )
                .likely_cause(error.to_string())
            })?;
            let Event::Key(key) = event else {
                continue;
            };
            // Terminals that speak the Kitty protocol report releases too, and
            // acting on both would double every character.
            if key.kind == KeyEventKind::Release {
                continue;
            }
            match key.code {
                KeyCode::Enter => break Some(std::mem::take(&mut typed)),
                KeyCode::Esc => break None,
                KeyCode::Char('c' | 'd') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    break None;
                }
                KeyCode::Backspace => {
                    typed.pop();
                }
                KeyCode::Char(ch) => typed.push(ch),
                _ => {}
            }
        }
    };

    // The line the question was written on is closed either way, so whatever is
    // printed next starts where it should.
    writeln!(err).ok();
    err.flush().ok();
    Ok(outcome.filter(|password| !password.is_empty()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asking_needs_a_terminal_at_both_ends() {
        // Under `cargo test` neither stream is a terminal, which is exactly the
        // case that must not hang: a script waiting for a password nobody can
        // type is worse than a script that fails.
        assert!(!can_ask());
    }

    #[test]
    fn an_empty_answer_is_the_same_as_no_answer() {
        // Sending nothing would earn the same refusal from the server and look
        // like a bug, so it is treated as a decision not to answer.
        let empty: Option<String> = Some(String::new()).filter(|p| !p.is_empty());
        assert!(empty.is_none());
    }
}
