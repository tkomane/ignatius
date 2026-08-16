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
    /// Show or hide the object tree.
    ToggleSidebar,
    /// Open the command palette.
    OpenPalette,
    /// Begin a two-key chord and show the continuations.
    BeginPrefix,
    /// Filter the object tree.
    StartFilter,
    /// Reload the object tree from the server.
    ReloadObjects,
    /// Enter: a line break, opening a node, or confirming, depending on focus.
    Activate,
    /// Show or hide the expanded view of the selected row.
    ToggleExpandedRow,
    /// Open the inspector on the selected cell, or close it.
    ToggleInspector,
    /// Type a character into the editor.
    Insert(char),
    /// Delete backwards in the editor.
    Backspace,
    /// Delete the character after the cursor.
    DeleteForward,
    /// Delete back to the start of the previous word.
    DeleteWordLeft,
    /// Move by a word.
    MoveWord(Direction),
    /// Move to the start of the line.
    MoveLineStart,
    /// Move past the end of the line.
    MoveLineEnd,
    /// Move to the start of the buffer.
    MoveBufferStart,
    /// Move to the end of the buffer.
    MoveBufferEnd,
    /// Move by a screenful.
    MovePage(Direction),
    /// Save the buffer as a named query.
    SaveQuery,
    /// Open a saved query into the buffer.
    OpenQuery,
    /// Show the definition of the selected object.
    ShowDefinition,
    /// Show what the selected object depends on, and what depends on it.
    ShowDependencies,
    /// Search the statements that have run and reuse one.
    OpenHistory,
    /// Stop or resume recording statements for this session.
    ToggleHistoryRecording,
    /// Take back the last change.
    Undo,
    /// Put back what undo took away.
    Redo,
    /// Insert a line break in the editor.
    Newline,
}

/// A password on its way from the prompt to a connection attempt.
///
/// A newtype rather than a `String` so that `Debug` on an [`Effect`] - in a
/// test failure, a panic, a log - cannot print it.
#[derive(Clone, PartialEq, Eq)]
pub struct TypedPassword(String);

impl TypedPassword {
    /// Wraps what the user typed.
    #[must_use]
    pub const fn new(password: String) -> Self {
        Self(password)
    }

    /// Hands it to the connection attempt, consuming it.
    #[must_use]
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl std::fmt::Debug for TypedPassword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TypedPassword(<hidden>)")
    }
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
    /// The schema list finished loading.
    SchemasLoaded(Box<Result<Vec<crate::postgres::metadata::SchemaSummary>, Diagnostic>>),
    /// A node's children finished loading. Carries the request identity so a
    /// stale answer can be discarded.
    MetadataLoaded {
        /// Which load this answers.
        request: crate::app::tree::RequestId,
        /// The node that asked.
        path: crate::app::tree::NodePath,
        /// What came back.
        payload: Box<Result<crate::app::tree::MetadataPayload, Diagnostic>>,
    },
    /// The saved queries were listed.
    QueriesListed(Vec<crate::queries::SavedQuery>),
    /// A saved query was written, or could not be.
    QuerySaved(Box<Result<std::path::PathBuf, Diagnostic>>),
    /// A saved query was read, or could not be.
    QueryLoaded(Box<Result<String, Diagnostic>>),
    /// An object's dependencies finished loading.
    DependenciesLoaded {
        /// Which request this answers.
        request: crate::app::tree::RequestId,
        /// What was found, or why it could not be.
        result: Box<Result<crate::postgres::metadata::Dependencies, Diagnostic>>,
    },
    /// The tree's own connection is now open, or could not be opened.
    ///
    /// Not opening one is not a failure of the session: the tree falls back to
    /// the connection that is already there, and says which it is using.
    MetadataConnection(crate::app::model::MetadataLink),
    /// An object's definition finished loading.
    DefinitionLoaded {
        /// Which request this answers, so a stale answer is discarded.
        request: crate::app::tree::RequestId,
        /// The definition, or why there is not one.
        result: Box<Result<crate::postgres::metadata::Definition, Diagnostic>>,
    },
    /// The statement history finished loading from disk.
    HistoryLoaded(Vec<crate::history::Entry>),
    /// A statement was offered to the history.
    ///
    /// Carries the entry when one was written, and nothing when it was not, so
    /// the interface never shows a statement as recorded that is not on disk.
    /// It also carries why, because a silently missing entry is a mystery.
    HistoryRecorded {
        /// The entry, when one reached the file.
        entry: Box<Option<crate::history::Entry>>,
        /// What happened to it.
        recorded: crate::history::Recorded,
    },
    /// A frame of elapsed time.
    ///
    /// The reducer reads no clock, so the runtime measures how long the current
    /// statement has been running and passes it in. That keeps every animated
    /// state reproducible in a test.
    Tick {
        /// How long the statement in flight has been running, when one is.
        running_for: Option<std::time::Duration>,
    },
}

/// Work the runtime should perform on the application's behalf.
///
/// Effects carry everything the executor needs. The reducer never performs I/O,
/// which is what lets the whole state machine be tested without a database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// Open the connection.
    Connect,
    /// Try the connection again with a password the user has just typed.
    ///
    /// The password travels no further than the connection attempt: it is not
    /// stored in the model, in configuration, or anywhere on disk.
    Reconnect {
        /// What was typed. Its `Debug` never prints it.
        password: TypedPassword,
    },
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
    /// Load the schema list.
    LoadSchemas,
    /// Read an object's definition.
    LoadDefinition {
        /// Identity to report back with.
        request: crate::app::tree::RequestId,
        /// The object to describe.
        object: Box<crate::postgres::metadata::ObjectSummary>,
    },
    /// List the saved queries.
    ListQueries,
    /// Write the buffer to a named file in the saved queries directory.
    SaveQuery {
        /// The name, which is checked before it reaches the filesystem.
        name: String,
        /// The SQL to write.
        sql: String,
    },
    /// Read a saved query.
    LoadQuery {
        /// The name to read.
        name: String,
    },
    /// Read what an object depends on and what depends on it.
    LoadDependencies {
        /// Identity to report back with.
        request: crate::app::tree::RequestId,
        /// The object to ask about.
        object: Box<crate::postgres::metadata::ObjectSummary>,
    },
    /// Read the statement history from disk.
    LoadHistory,
    /// Offer a statement to the history.
    RecordHistory {
        /// The SQL that ran.
        sql: String,
        /// How it ended.
        outcome: crate::history::Outcome,
        /// How long it took.
        elapsed: std::time::Duration,
    },
    /// Load a node's children.
    LoadMetadata {
        /// Identity to report back with.
        request: crate::app::tree::RequestId,
        /// The node that asked.
        path: crate::app::tree::NodePath,
        /// What to fetch.
        query: crate::app::tree::MetadataQuery,
    },
}
