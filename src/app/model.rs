//! The application model.
//!
//! This is a plain data structure with no I/O. Everything the interface draws is
//! derived from it, and every transition into it happens in
//! [`crate::app::update`], which is a pure function. That is what makes states
//! like "cancellation requested but not yet confirmed" testable without a
//! database, a terminal, or a clock.

use crate::connection::Environment;
use crate::diagnostics::Diagnostic;
use crate::postgres::SessionInfo;
use crate::query::result::{Execution, JobId, Notice};
use std::time::Duration;

/// Which pane has keyboard focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Focus {
    /// The SQL editor.
    #[default]
    Editor,
    /// The result grid.
    Results,
    /// The object tree.
    Objects,
}

impl Focus {
    /// The next pane in the cycle.
    ///
    /// The object tree is only in the cycle when it is on screen, so Tab never
    /// moves focus somewhere invisible.
    #[must_use]
    pub const fn next(self, sidebar_visible: bool) -> Self {
        match self {
            Self::Editor => Self::Results,
            Self::Results => {
                if sidebar_visible {
                    Self::Objects
                } else {
                    Self::Editor
                }
            }
            Self::Objects => Self::Editor,
        }
    }

    /// Text label shown on the focused pane's title.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Editor => "Editor",
            Self::Results => "Results",
            Self::Objects => "Objects",
        }
    }
}

/// State of the database connection.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ConnectionState {
    /// Not connected and not trying.
    #[default]
    Disconnected,
    /// A connection attempt is in flight.
    Connecting,
    /// Connected, with what the server told us about the session.
    Connected(Box<SessionInfo>),
    /// The connection was lost after being established.
    Lost {
        /// What the session was, so the interface can still name the target.
        info: Box<SessionInfo>,
    },
    /// The connection attempt failed.
    Failed(Box<Diagnostic>),
}

impl ConnectionState {
    /// Session facts, when there are any.
    #[must_use]
    pub fn info(&self) -> Option<&SessionInfo> {
        match self {
            Self::Connected(info) | Self::Lost { info } => Some(info),
            _ => None,
        }
    }

    /// Whether statements can be sent right now.
    #[must_use]
    pub const fn is_usable(&self) -> bool {
        matches!(self, Self::Connected(_))
    }

    /// Short text for the status bar. Never colour alone.
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::Disconnected => "Not connected".to_owned(),
            Self::Connecting => "Connecting".to_owned(),
            Self::Connected(info) => info.target.clone(),
            Self::Lost { info } => format!("{} (connection lost)", info.target),
            Self::Failed(_) => "Connection failed".to_owned(),
        }
    }
}

/// What the query engine is doing.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum QueryPhase {
    /// Nothing is running.
    #[default]
    Idle,
    /// A statement is running.
    Running {
        /// Identity of the running job.
        job: JobId,
        /// How many statements were submitted.
        statements: usize,
    },
    /// The user asked to cancel and the server has not answered yet.
    ///
    /// This is a distinct state on purpose: saying "cancelled" before the server
    /// confirms would be a claim the client cannot support.
    CancellationRequested {
        /// Identity of the job being cancelled.
        job: JobId,
    },
}

impl QueryPhase {
    /// The job currently in flight, if any.
    #[must_use]
    pub const fn job(&self) -> Option<JobId> {
        match self {
            Self::Idle => None,
            Self::Running { job, .. } | Self::CancellationRequested { job } => Some(*job),
        }
    }

    /// Status wording that matches the real state.
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Idle => "Ready",
            Self::Running { .. } => "Running",
            Self::CancellationRequested { .. } => "Cancellation requested",
        }
    }

    /// Whether a statement is in flight.
    #[must_use]
    pub const fn is_busy(&self) -> bool {
        !matches!(self, Self::Idle)
    }
}

/// A minimal multi-line SQL buffer.
///
/// Feature 003 replaces this with a full editor. What it must already do is hold
/// text, track a cursor in byte offsets so statement selection is exact, and
/// never panic on multi-byte input.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Editor {
    text: String,
    cursor: usize,
    modified: bool,
}

impl Editor {
    /// Creates an editor holding the given text, cursor at the end.
    #[must_use]
    pub fn with_text(text: impl Into<String>) -> Self {
        let text = text.into();
        let cursor = text.len();
        Self {
            text,
            cursor,
            modified: false,
        }
    }

    /// The whole buffer.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Cursor position as a byte offset, always on a character boundary.
    #[must_use]
    pub const fn cursor(&self) -> usize {
        self.cursor
    }

    /// Whether the buffer has unsaved edits.
    #[must_use]
    pub const fn is_modified(&self) -> bool {
        self.modified
    }

    /// Inserts a character at the cursor.
    pub fn insert(&mut self, ch: char) {
        self.text.insert(self.cursor, ch);
        self.cursor += ch.len_utf8();
        self.modified = true;
    }

    /// Deletes the character before the cursor.
    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let mut previous = self.cursor - 1;
        while previous > 0 && !self.text.is_char_boundary(previous) {
            previous -= 1;
        }
        self.text.replace_range(previous..self.cursor, "");
        self.cursor = previous;
        self.modified = true;
    }

    /// Moves the cursor one character left.
    pub fn move_left(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let mut previous = self.cursor - 1;
        while previous > 0 && !self.text.is_char_boundary(previous) {
            previous -= 1;
        }
        self.cursor = previous;
    }

    /// Moves the cursor one character right.
    pub fn move_right(&mut self) {
        if self.cursor >= self.text.len() {
            return;
        }
        let mut next = self.cursor + 1;
        while next < self.text.len() && !self.text.is_char_boundary(next) {
            next += 1;
        }
        self.cursor = next;
    }

    /// Replaces the whole buffer, for example when loading a file.
    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
        self.cursor = self.text.len();
        self.modified = false;
    }

    /// The line and column of the cursor, both one-based, for the status bar.
    #[must_use]
    pub fn position(&self) -> (usize, usize) {
        let before = &self.text[..self.cursor];
        let line = before.matches('\n').count() + 1;
        let column = before
            .rsplit_once('\n')
            .map_or(before.chars().count(), |(_, last)| last.chars().count())
            + 1;
        (line, column)
    }
}

/// The complete application state.
#[derive(Debug, Clone, Default)]
pub struct Model {
    /// Which pane has focus.
    pub focus: Focus,
    /// The SQL buffer.
    pub editor: Editor,
    /// Connection state.
    pub connection: ConnectionState,
    /// What the query engine is doing.
    pub phase: QueryPhase,
    /// The most recent completed execution.
    pub last_execution: Option<Execution>,
    /// Server messages from the most recent execution.
    pub notices: Vec<Notice>,
    /// The error being shown, if any.
    pub error: Option<Diagnostic>,
    /// Whether the help overlay is open.
    pub help_open: bool,
    /// Whether the technical section of the error is expanded.
    pub error_expanded: bool,
    /// Selected row in the result grid.
    pub selected_row: usize,
    /// Selected column in the result grid.
    pub selected_column: usize,
    /// Set when the application should exit.
    pub should_quit: bool,
    /// Counter used to hand out job identities.
    pub next_job: u64,
    /// Last known terminal size.
    pub size: (u16, u16),
    /// How long the last execution took, kept for the status bar.
    pub last_elapsed: Option<Duration>,
    /// Row cap applied to results.
    pub row_cap: usize,
    /// The object tree.
    pub tree: crate::app::tree::ObjectTree,
    /// Whether the object tree is on screen.
    pub sidebar_visible: bool,
    /// The command palette, when it is open.
    pub palette: Option<crate::app::palette::Palette>,
    /// Whether a two-key chord is waiting for its second key.
    pub prefix_pending: bool,
    /// Animation frame, advanced by each tick.
    ///
    /// Nothing derives meaning from it; it only chooses which frame of an
    /// indicator to draw, so a paused animation never hides a real state.
    pub frame: u64,
    /// How long the statement in flight has been running.
    pub running_for: Option<Duration>,
    /// When true, indicators are drawn as static text.
    pub reduced_motion: bool,
}

impl Model {
    /// Builds a model with the configured row cap.
    #[must_use]
    pub fn new(row_cap: usize) -> Self {
        Self {
            row_cap,
            size: (crate::ui::MIN_COLUMNS, crate::ui::MIN_ROWS),
            sidebar_visible: true,
            ..Self::default()
        }
    }

    /// Whether anything is currently worth animating.
    ///
    /// The runtime uses this to stop ticking, so an idle client wakes nothing.
    #[must_use]
    pub const fn is_animating(&self) -> bool {
        self.phase.is_busy() || matches!(self.connection, ConnectionState::Connecting)
    }

    /// Hands out the next job identity.
    pub fn allocate_job(&mut self) -> JobId {
        self.next_job += 1;
        JobId(self.next_job)
    }

    /// The environment classification of the current connection.
    #[must_use]
    pub fn environment(&self) -> Environment {
        self.connection
            .info()
            .map_or(Environment::Unclassified, |info| info.environment.clone())
    }

    /// The first result set of the last execution, which the grid displays.
    #[must_use]
    pub fn visible_result(&self) -> Option<&crate::query::result::ResultSet> {
        self.last_execution
            .as_ref()?
            .statements
            .iter()
            .find_map(|s| s.result_set.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_cycles_through_the_panes_that_are_on_screen() {
        assert_eq!(Focus::Editor.next(true), Focus::Results);
        assert_eq!(Focus::Results.next(true), Focus::Objects);
        assert_eq!(Focus::Objects.next(true), Focus::Editor);

        // With the tree hidden, Tab never lands on it.
        assert_eq!(Focus::Results.next(false), Focus::Editor);

        for focus in [Focus::Editor, Focus::Results, Focus::Objects] {
            assert!(!focus.label().is_empty());
        }
    }

    #[test]
    fn cancellation_requested_is_a_state_of_its_own() {
        let requested = QueryPhase::CancellationRequested { job: JobId(1) };
        assert_eq!(requested.label(), "Cancellation requested");
        assert_ne!(requested.label(), QueryPhase::Idle.label());
        assert!(requested.is_busy(), "the statement may still be running");
        assert_eq!(requested.job(), Some(JobId(1)));
    }

    #[test]
    fn a_lost_connection_still_names_its_target() {
        let info = Box::new(SessionInfo {
            target: "app@localhost:5432/orders".into(),
            database: "orders".into(),
            user: "app".into(),
            server_version: "18.6".into(),
            backend_pid: "1".into(),
            search_path: "public".into(),
            read_only: false,
            tls: crate::postgres::TlsState::Disabled,
            environment: Environment::Local,
        });
        let lost = ConnectionState::Lost { info };
        assert!(lost.label().contains("connection lost"));
        assert!(
            !lost.is_usable(),
            "statements must not be sent on a lost connection"
        );
        assert!(lost.info().is_some());
    }

    #[test]
    fn editor_handles_multibyte_text_without_splitting_characters() {
        let mut editor = Editor::default();
        for ch in "SELECT 'héllo 日本';".chars() {
            editor.insert(ch);
        }
        assert_eq!(editor.text(), "SELECT 'héllo 日本';");
        assert!(editor.is_modified());
        assert!(editor.text().is_char_boundary(editor.cursor()));

        for _ in 0..5 {
            editor.backspace();
        }
        // Removed, in order: ';' '\'' '本' '日' ' '
        assert_eq!(editor.text(), "SELECT 'héllo");
        assert!(editor.text().is_char_boundary(editor.cursor()));

        editor.move_left();
        editor.move_left();
        assert!(editor.text().is_char_boundary(editor.cursor()));
        for _ in 0..20 {
            editor.move_right();
        }
        assert_eq!(
            editor.cursor(),
            editor.text().len(),
            "cursor stops at the end"
        );
        for _ in 0..40 {
            editor.move_left();
        }
        assert_eq!(editor.cursor(), 0, "cursor stops at the start");
        editor.backspace();
        assert_eq!(editor.cursor(), 0, "backspace at the start is a no-op");
    }

    #[test]
    fn editor_reports_a_one_based_line_and_column() {
        let mut editor = Editor::with_text("SELECT 1\nFROM t");
        assert_eq!(editor.position(), (2, 7));
        editor.set_text("SELECT 1");
        assert_eq!(editor.position(), (1, 9));
        assert!(!editor.is_modified(), "loading text is not an edit");
    }

    #[test]
    fn job_identities_are_unique_and_increasing() {
        let mut model = Model::new(100);
        let first = model.allocate_job();
        let second = model.allocate_job();
        assert_ne!(first, second);
        assert!(second > first);
    }

    #[test]
    fn an_unclassified_connection_is_not_production() {
        let model = Model::new(100);
        assert_eq!(model.environment(), Environment::Unclassified);
        assert!(!model.environment().is_production());
    }
}
