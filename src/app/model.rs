//! The application model.
//!
//! This is a plain data structure with no I/O. Everything the interface draws is
//! derived from it, and every transition into it happens in
//! [`crate::app::update()`], which is a pure function. That is what makes states
//! like "cancellation requested but not yet confirmed" testable without a
//! database, a terminal, or a clock.

pub use crate::app::editor::Editor;
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

/// Where the object tree reads from.
///
/// A second connection to the same server is a real fact about resource use, so
/// which one is in use is shown rather than assumed. On a server with a
/// connection limit, or behind a pooler, someone is entitled to know that this
/// client opened two.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum MetadataLink {
    /// The tree shares the session's connection, so a load waits behind a long
    /// statement on the server.
    #[default]
    Shared,
    /// A second connection is being opened for it.
    Opening,
    /// The tree has its own connection and cannot be delayed by a query.
    Dedicated,
    /// A second connection could not be opened, and why.
    Unavailable(String),
}

impl MetadataLink {
    /// A short label for the object pane's title. Always words.
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Shared => "shared connection",
            Self::Opening => "opening its own connection",
            Self::Dedicated => "own connection",
            Self::Unavailable(_) => "shared connection, its own was refused",
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

/// An object definition on screen.
///
/// Holds the text and where the view is, and nothing derived: the definition is
/// a snapshot of what the catalogue said when it was asked, and it says when it
/// was asked rather than pretending to be live.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Definition {
    /// The load this is waiting for, when it is still loading.
    pub pending: Option<crate::app::tree::RequestId>,
    /// What is being described, for the title while it loads.
    pub heading: String,
    /// The definition itself, once it arrives.
    pub definition: Option<crate::postgres::metadata::Definition>,
    /// Why it could not be read.
    pub error: Option<String>,
    /// First visible line.
    pub scroll: usize,
}

impl Definition {
    /// The lines of the definition, empty while it is loading.
    #[must_use]
    pub fn lines(&self) -> Vec<&str> {
        self.definition
            .as_ref()
            .map(|definition| definition.text.lines().collect())
            .unwrap_or_default()
    }

    /// Scrolls down, stopping with the last line on screen.
    pub fn scroll_down(&mut self, height: usize) {
        let max = self.lines().len().saturating_sub(height);
        if self.scroll < max {
            self.scroll += 1;
        }
    }

    /// Scrolls up.
    pub const fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }
}

/// What a typed name is for.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum NamePurpose {
    /// Saving the buffer as a query.
    #[default]
    SaveQuery,
    /// Writing the rows on screen to a file.
    ExportRows,
}

/// A name being typed, for the two things in this client that need one.
///
/// Visible, unlike the password prompt: there is nothing here worth hiding, and
/// seeing what a file will be called is the point of asking.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NamePrompt {
    /// What has been typed so far.
    pub typed: String,
    /// What is being named, for the title.
    pub subject: String,
    /// What the name is for.
    pub purpose: NamePurpose,
    /// A sentence about what will happen, shown under the field.
    pub note: String,
}

impl NamePrompt {
    /// Opens a prompt for the name of a saved query.
    #[must_use]
    pub fn for_query(suggestion: impl Into<String>) -> Self {
        Self {
            typed: suggestion.into(),
            subject: "Save the buffer as".to_owned(),
            purpose: NamePurpose::SaveQuery,
            note: "It is saved as an ordinary .sql file you can open in anything.".to_owned(),
        }
    }

    /// Opens a prompt for the file rows will be written to.
    ///
    /// The note is the honest part: this writes what is on screen, which is not
    /// the same as what the query returned when it was truncated or filtered.
    #[must_use]
    pub fn for_export(note: impl Into<String>) -> Self {
        Self {
            typed: String::new(),
            subject: "Write the rows on screen to".to_owned(),
            purpose: NamePurpose::ExportRows,
            note: note.into(),
        }
    }
}

/// A password the user is typing because the server asked for one.
///
/// The characters live here and nowhere else, for as long as it takes to try
/// the connection again. Nothing derived from them is stored: not a length in a
/// log, not a hint, not a copy in the history. `Debug` is written by hand so
/// that a stray `{:?}` on the model - in a test, a panic message, a log line -
/// cannot print what was typed.
#[derive(Clone, Default)]
pub struct PasswordPrompt {
    /// The connection being retried, already safe to display.
    pub target: String,
    /// What the server said, in the words it was reported in.
    pub reason: String,
    typed: String,
}

impl std::fmt::Debug for PasswordPrompt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PasswordPrompt")
            .field("target", &self.target)
            .field("reason", &self.reason)
            .field("typed", &"<hidden>")
            .finish()
    }
}

impl PasswordPrompt {
    /// Opens a prompt for a connection that was refused for want of a password.
    #[must_use]
    pub fn new(target: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            target: target.into(),
            reason: reason.into(),
            typed: String::new(),
        }
    }

    /// Adds a character.
    pub fn push(&mut self, ch: char) {
        self.typed.push(ch);
    }

    /// Removes the last character.
    pub fn backspace(&mut self) {
        self.typed.pop();
    }

    /// How many characters have been typed, for the masked field.
    #[must_use]
    pub fn length(&self) -> usize {
        self.typed.chars().count()
    }

    /// Whether anything has been typed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.typed.is_empty()
    }

    /// Takes the password out, leaving nothing behind.
    #[must_use]
    pub fn take(&mut self) -> String {
        std::mem::take(&mut self.typed)
    }
}

/// A run that is waiting for the user to confirm it.
///
/// There is no session-wide unlock. A mode that quietly stays on is a mode
/// people forget they are in, which is the failure this exists to prevent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingRun {
    /// The SQL that will run if it is confirmed.
    pub sql: String,
    /// What it appears to do.
    pub impact: crate::query::Impact,
    /// What the user has typed, when a typed confirmation is required.
    pub typed: String,
    /// The word that must be typed, which is the database's own name.
    pub required: String,
}

impl PendingRun {
    /// Whether the confirmation is satisfied.
    #[must_use]
    pub fn is_satisfied(&self) -> bool {
        !self.impact.needs_typed_confirmation() || self.typed == self.required
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
    /// Where the object tree reads from.
    pub metadata_link: MetadataLink,
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
    /// A run held back until the user confirms it.
    pub pending_run: Option<PendingRun>,
    /// The transaction state the server last reported.
    pub transaction: crate::query::result::TransactionState,
    /// Animation frame, advanced by each tick.
    ///
    /// Nothing derives meaning from it; it only chooses which frame of an
    /// indicator to draw, so a paused animation never hides a real state.
    pub frame: u64,
    /// How long the statement in flight has been running.
    pub running_for: Option<Duration>,
    /// When true, indicators are drawn as static text.
    pub reduced_motion: bool,
    /// Whether the result pane shows one row down the screen instead of a grid.
    ///
    /// A preference, not a property of a result: it survives a new query,
    /// because someone who turned it on wants it on for the next one too.
    pub expanded_row: bool,
    /// The cell inspector, when it is open.
    pub inspector: Option<crate::app::inspect::Inspector>,
    /// The statements that have run, newest first.
    pub history: Vec<crate::history::Entry>,
    /// Whether recording is paused for this session.
    ///
    /// Session state, never written to configuration: a private session is
    /// something you turn on for now, and it is shown in the header so it is
    /// never a mode someone is in without knowing.
    pub history_paused: bool,
    /// Whether history is off in configuration, which the session cannot undo.
    pub history_disabled: bool,
    /// Why the last statement was not recorded, when there is a reason worth
    /// saying. Cleared by the next statement that is recorded.
    pub history_note: Option<&'static str>,
    /// The SQL of the statement in flight, kept so it can be recorded when it
    /// finishes with an outcome worth recording.
    pub running_sql: Option<String>,
    /// A password being typed because the server asked for one.
    pub password_prompt: Option<PasswordPrompt>,
    /// A name being typed, when a query is being saved.
    pub name_prompt: Option<NamePrompt>,
    /// Where the saved query in the editor came from, if it came from one.
    pub loaded_query: Option<String>,
    /// The dependency lookup in flight, so a late answer can be discarded.
    pub pending_dependencies: Option<crate::app::tree::RequestId>,
    /// The object definition being shown, when one is open.
    pub definition: Option<Definition>,
    /// Text the result grid is filtered by. Empty means every row.
    pub result_filter: String,
    /// Whether the filter is being typed into.
    pub result_filtering: bool,
    /// The cloud identity provider this session's credential came from.
    ///
    /// When there is one, the client never asks for a password: the credential
    /// was obtained from a program, the server accepted or refused it on its
    /// own terms, and nothing anybody types here can help. Asking would describe
    /// the wrong problem and take a real password on the way.
    pub credential_provider: Option<String>,
    /// Safe, display-only facts about the cloud identity provider.
    ///
    /// The provider command itself remains outside the model. This value is a
    /// presentation snapshot, so opening the trust surface never refreshes a
    /// token, executes a program, or performs network I/O.
    pub credential_presentation: Option<crate::connection::cloud::ProviderPresentation>,
    /// Whether the read-only connection trust surface is open.
    pub connection_details: bool,
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

    /// Whether statements are being written to the history right now.
    #[must_use]
    pub const fn records_history(&self) -> bool {
        !self.history_disabled && !self.history_paused
    }

    /// Which retained rows the filter admits, as indices into the result.
    ///
    /// Indices rather than rows, because everything that acts on a selection -
    /// the inspector, the expanded view, an export of what is on screen - needs
    /// to know which row of the result it is looking at, not which row of the
    /// filtered view.
    ///
    /// **The filter runs over retained rows only.** Rows beyond the cap were
    /// never received, so they cannot be searched, and every count that mentions
    /// the filter says so.
    #[must_use]
    pub fn filtered_rows(&self) -> Vec<usize> {
        let Some(set) = self.visible_result() else {
            return Vec::new();
        };
        if self.result_filter.trim().is_empty() {
            return (0..set.rows.len()).collect();
        }
        let needle = self.result_filter.to_lowercase();
        set.rows
            .iter()
            .enumerate()
            .filter(|(_, row)| {
                row.iter()
                    .any(|cell| cell.display().to_lowercase().contains(&needle))
            })
            .map(|(index, _)| index)
            .collect()
    }

    /// The row of the result the selection points at, after filtering.
    #[must_use]
    pub fn selected_source_row(&self) -> Option<usize> {
        self.filtered_rows().get(self.selected_row).copied()
    }

    /// What the result pane says about how much is being shown.
    ///
    /// Always states the filter and the cap together, because "12 rows" when a
    /// filter is on and a result was truncated is three different numbers
    /// collapsed into one, and the wrong one.
    #[must_use]
    pub fn result_window_label(&self) -> String {
        let Some(set) = self.visible_result() else {
            return String::new();
        };
        if self.result_filter.trim().is_empty() {
            return set.window_label();
        }
        let matching = self.filtered_rows().len();
        let retained = set.retained();
        if set.is_truncated() {
            format!(
                "matching {matching} of {retained} retained rows, of {} returned (limit {} reached)",
                set.rows_seen, set.cap
            )
        } else {
            format!("matching {matching} of {retained} rows")
        }
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
