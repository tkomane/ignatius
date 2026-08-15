//! Messages into the reducer and effects out of it.
//!
//! Input is translated into an [`Action`] by the keymap before it reaches the
//! application, so the reducer never sees a key code and the keymap can be
//! reconfigured without touching behaviour. Effects are requests, not work: the
//! reducer returns them and the runtime performs them.

use crate::diagnostics::Diagnostic;
use crate::postgres::SessionInfo;
use crate::query::result::{Execution, JobId, Notice};

/// A user intent, produced by the keymap or the command palette.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Run every statement in the buffer.
    RunBuffer,
    /// Run only the statement under the cursor.
    RunStatement,
    /// Ask the server to cancel the running statement.
    Cancel,
    /// Leave the application.
    Quit,
    /// Show or hide help.
    ToggleHelp,
    /// Move focus to the next pane.
    FocusNext,
    /// Expand or collapse the technical section of an error.
    ToggleErrorDetail,
    /// Dismiss the error or the help overlay.
    Dismiss,
    /// Move the selection or cursor.
    Move(Direction),
    /// Type a character into the editor.
    Insert(char),
    /// Delete backwards in the editor.
    Backspace,
    /// Insert a line break in the editor.
    Newline,
}

/// A movement direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Up.
    Up,
    /// Down.
    Down,
    /// Left.
    Left,
    /// Right.
    Right,
}

/// Everything that can change the model.
#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    /// A user intent.
    Action(Action),
    /// The terminal was resized.
    Resized(u16, u16),
    /// A connection attempt succeeded.
    Connected(Box<SessionInfo>),
    /// A connection attempt failed.
    ConnectionFailed(Box<Diagnostic>),
    /// The connection dropped after being established.
    ConnectionLost,
    /// An execution finished. Carries its job identity so a stale result from a
    /// superseded query can be recognised and discarded.
    ExecutionFinished(Box<Execution>),
    /// The cancellation request was delivered to the server.
    CancellationDelivered(JobId),
    /// The cancellation request could not be delivered.
    CancellationFailed(Box<Diagnostic>),
    /// Server messages arrived outside an execution.
    Notices(Vec<Notice>),
}

/// Work the runtime should perform on the application's behalf.
///
/// Effects carry everything the executor needs. The reducer never performs I/O,
/// which is what lets the whole state machine be tested without a database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// Open the connection.
    Connect,
    /// Run SQL under a job identity.
    Execute {
        /// Identity to report back with the result.
        job: JobId,
        /// The SQL to run.
        sql: String,
    },
    /// Ask the server to cancel a job.
    Cancel {
        /// Identity of the job being cancelled.
        job: JobId,
    },
    /// Leave the application.
    Quit,
}
