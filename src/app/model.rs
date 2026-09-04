//! The application model.
//!
//! This is a plain data structure with no I/O. Everything the interface draws is
//! derived from it, and every transition into it happens in
//! [`crate::app::update()`], which is a pure function. That is what makes states
//! like "cancellation requested but not yet confirmed" testable without a
//! database, a terminal, or a clock.

use crate::app::discovery::KeymapSnapshot;
pub use crate::app::editor::Editor;
use crate::app::grid::ResultGridState;
use crate::app::plan::PlanView;
use crate::connection::Environment;
use crate::diagnostics::Diagnostic;
use crate::postgres::SessionInfo;
use crate::query::error_location::{ErrorLocation, StatementSource};
use crate::query::result::{Execution, JobId, Notice};
use secrecy::{ExposeSecret, SecretString};
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

/// A shape for an interactive export of retained rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExportFormat {
    /// Comma-separated values.
    #[default]
    Csv,
    /// Tab-separated values.
    Tsv,
    /// One JSON document containing the visible result set.
    Json,
    /// One JSON object per row.
    Ndjson,
    /// A Markdown pipe table.
    Markdown,
}

impl ExportFormat {
    /// The short name shown in a palette and path prompt.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Csv => "CSV",
            Self::Tsv => "TSV",
            Self::Json => "JSON",
            Self::Ndjson => "NDJSON",
            Self::Markdown => "Markdown",
        }
    }

    /// The safe extension suggestion shown beside the format.
    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Csv => ".csv",
            Self::Tsv => ".tsv",
            Self::Json => ".json",
            Self::Ndjson => ".ndjson",
            Self::Markdown => ".md",
        }
    }

    /// What this shape is useful for.
    #[must_use]
    pub const fn description(self) -> &'static str {
        match self {
            Self::Csv => "comma-separated values",
            Self::Tsv => "tab-separated values",
            Self::Json => "one JSON document",
            Self::Ndjson => "one JSON object per row",
            Self::Markdown => "a Markdown table",
        }
    }
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
    /// The selected shape when this is an export path prompt.
    pub export_format: Option<ExportFormat>,
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
            export_format: None,
        }
    }

    /// Opens a prompt for the file rows will be written to.
    ///
    /// The note is the honest part: this writes what is on screen, which is not
    /// the same as what the query returned when it was truncated or filtered.
    #[must_use]
    pub fn for_export(format: ExportFormat, note: impl Into<String>) -> Self {
        Self {
            typed: String::new(),
            subject: format!("Write the retained rows as {} to", format.label()),
            purpose: NamePurpose::ExportRows,
            note: note.into(),
            export_format: Some(format),
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

/// A masked, one-shot prompt for the distinct names in a SQL template.
///
/// The template and names are safe display data. Answers stay in
/// [`SecretString`] until the final value is handed to the execution effect,
/// and this type deliberately implements `Debug` by hand.
#[derive(Clone)]
pub struct ParameterPrompt {
    /// The unchanged SQL template that will be recorded if it runs.
    pub sql: String,
    /// The original editor span when the user ran one statement.
    pub source: Option<StatementSource>,
    /// Distinct names in first-use order.
    pub names: Vec<String>,
    active: usize,
    typed: SecretString,
    accepted: Vec<SecretString>,
}

impl std::fmt::Debug for ParameterPrompt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ParameterPrompt")
            .field("sql", &self.sql)
            .field("source", &self.source)
            .field("names", &self.names)
            .field("active", &self.active)
            .field("typed", &"<hidden>")
            .field("accepted", &"<hidden>")
            .finish()
    }
}

impl ParameterPrompt {
    /// Opens a prompt for a scanned template.
    #[must_use]
    pub fn new(sql: String, source: Option<StatementSource>, names: Vec<String>) -> Self {
        let accepted = names
            .iter()
            .map(|_| SecretString::from(String::new()))
            .collect();
        Self {
            sql,
            source,
            names,
            active: 0,
            typed: SecretString::from(String::new()),
            accepted,
        }
    }

    /// The zero-based active name index.
    #[must_use]
    pub const fn active_index(&self) -> usize {
        self.active
    }

    /// The number of distinct names in this prompt.
    #[must_use]
    pub fn total(&self) -> usize {
        self.names.len()
    }

    /// The active name without its leading colon.
    #[must_use]
    pub fn active_name(&self) -> Option<&str> {
        self.names.get(self.active).map(String::as_str)
    }

    /// Adds one character to the masked current answer.
    pub fn push(&mut self, character: char) {
        let mut typed = self.typed.expose_secret().to_owned();
        typed.push(character);
        self.typed = SecretString::from(typed);
    }

    /// Removes one Unicode scalar value from the current answer.
    pub fn backspace(&mut self) {
        let mut typed = self.typed.expose_secret().to_owned();
        typed.pop();
        self.typed = SecretString::from(typed);
    }

    /// How many characters the mask should draw.
    #[must_use]
    pub fn typed_length(&self) -> usize {
        self.typed.expose_secret().chars().count()
    }

    /// Accepts the current answer, returning final bindings on the last name.
    pub fn accept(
        &mut self,
    ) -> Result<Option<crate::query::ParameterBindings>, crate::query::ParameterError> {
        if self.active >= self.names.len() || self.accepted.len() != self.names.len() {
            return Err(crate::query::ParameterError::InvalidTemplate);
        }
        self.accepted[self.active] =
            std::mem::replace(&mut self.typed, SecretString::from(String::new()));
        if self.active + 1 < self.names.len() {
            self.active += 1;
            return Ok(None);
        }

        let bindings = crate::query::ParameterBindings::from_secrets(
            self.names.clone(),
            self.accepted.clone(),
        )?;
        bindings.validate()?;
        Ok(Some(bindings))
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
    /// Original editor span when this is a Run Statement action.
    pub source: Option<StatementSource>,
}

impl PendingRun {
    /// Whether the confirmation is satisfied.
    #[must_use]
    pub fn is_satisfied(&self) -> bool {
        !self.impact.needs_typed_confirmation() || self.typed == self.required
    }
}

/// An EXPLAIN ANALYZE request waiting for one explicit confirmation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingPlan {
    /// The statement that will be wrapped in EXPLAIN ANALYZE.
    pub sql: String,
    /// Advisory impact classification of the target statement.
    pub impact: crate::query::Impact,
    /// What the user has typed when a database-name confirmation is required.
    pub typed: String,
    /// The database name required for a destructive confirmation.
    pub required: String,
}

impl PendingPlan {
    /// Whether this one-shot confirmation is satisfied.
    #[must_use]
    pub fn is_satisfied(&self) -> bool {
        !self.impact.needs_typed_confirmation() || self.typed == self.required
    }
}

/// A value-free identity held while a clipboard copy awaits confirmation.
///
/// The raw value remains in the retained result. Keeping only its identity and
/// counts makes the prompt safe to debug and lets confirmation reject a result
/// or selection that changed underneath it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingCopy {
    /// The retained execution this candidate came from.
    pub result_job: JobId,
    /// Source-row index before local filtering or sorting.
    pub source_row: usize,
    /// Source-column index in the retained result.
    pub column: usize,
    /// Raw UTF-8 byte count at candidate creation.
    pub bytes: usize,
    /// Unicode scalar-value count at candidate creation.
    pub characters: usize,
}

/// A value-free identity for a selected result cell while update metadata is read.
///
/// The source SQL is retained only so a late result or changed query can be
/// rejected. Its debug representation is hidden because the editor can contain
/// arbitrary user text, including values that should not reach a log.
#[derive(Clone, PartialEq, Eq)]
pub struct UpdateCandidate {
    /// The retained execution this cell came from.
    pub result_job: JobId,
    /// Source-row index before local filtering and sorting.
    pub source_row: usize,
    /// Result-column index selected by the user.
    pub result_column: usize,
    /// The exact SQL source of the retained result.
    pub source_sql: String,
}

impl std::fmt::Debug for UpdateCandidate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UpdateCandidate")
            .field("result_job", &self.result_job)
            .field("source_row", &self.source_row)
            .field("result_column", &self.result_column)
            .field("source_sql", &"<hidden>")
            .finish()
    }
}

/// A source mapping waiting for live relation metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateLookup {
    /// Monotonic request identity.
    pub request: u64,
    /// Result identity to revalidate when the response returns.
    pub candidate: UpdateCandidate,
    /// The value-free source mapping produced by the SQL analyzer.
    pub source: crate::query::UpdateSource,
}

/// A replacement value prompt for one update candidate.
#[derive(Clone)]
pub struct UpdatePrompt {
    /// Result identity to revalidate before the review step.
    pub candidate: UpdateCandidate,
    /// The value-free source mapping.
    pub source: crate::query::UpdateSource,
    /// The live relation facts used by the planner.
    pub relation: crate::postgres::metadata::UpdateRelation,
    typed: SecretString,
}

impl std::fmt::Debug for UpdatePrompt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UpdatePrompt")
            .field("candidate", &self.candidate)
            .field("source", &self.source)
            .field("relation", &self.relation)
            .field("typed", &"<hidden>")
            .finish()
    }
}

impl UpdatePrompt {
    /// Opens an empty replacement prompt.
    #[must_use]
    pub fn new(
        candidate: UpdateCandidate,
        source: crate::query::UpdateSource,
        relation: crate::postgres::metadata::UpdateRelation,
    ) -> Self {
        Self {
            candidate,
            source,
            relation,
            typed: SecretString::from(String::new()),
        }
    }

    /// Adds one Unicode scalar value to the replacement.
    pub fn push(&mut self, character: char) {
        let mut typed = self.typed.expose_secret().to_owned();
        typed.push(character);
        self.typed = SecretString::from(typed);
    }

    /// Removes one Unicode scalar value from the replacement.
    pub fn backspace(&mut self) {
        let mut typed = self.typed.expose_secret().to_owned();
        typed.pop();
        self.typed = SecretString::from(typed);
    }

    /// Number of characters typed, including zero for an intentional empty value.
    #[must_use]
    pub fn typed_length(&self) -> usize {
        self.typed.expose_secret().chars().count()
    }

    /// The replacement for the planner and the value-visible prompt.
    #[must_use]
    pub fn replacement(&self) -> &str {
        self.typed.expose_secret()
    }
}

/// A generated update waiting for one final review confirmation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingUpdate {
    /// Result identity to revalidate before sending.
    pub candidate: UpdateCandidate,
    /// Live relation facts used to produce the plan.
    pub relation: crate::postgres::metadata::UpdateRelation,
    /// Parameterized statement and secret values.
    pub plan: crate::query::UpdatePlan,
}

/// A value-free outcome or refusal shown by the result surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardNotice {
    /// The action was invoked outside a copyable Results cell.
    Unavailable,
    /// The operator has not opted into terminal transport.
    Disabled,
    /// The selected cell is SQL NULL.
    Null,
    /// The selected text exceeded the client-side bound.
    TooLarge { bytes: usize },
    /// The retained result or selection changed while confirmation was open.
    Stale,
    /// The terminal write completed, but acceptance remains unknowable.
    Sent { bytes: usize, characters: usize },
}

impl ClipboardNotice {
    /// Plain-language status for the result surface.
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::Unavailable => "Focus Results on a retained text cell to copy a value.".into(),
            Self::Disabled => {
                "Copy is off; set `[clipboard] osc52 = true` if this terminal path may carry the value."
                    .into()
            }
            Self::Null => "SQL NULL has no text value to copy; nothing was sent.".into(),
            Self::TooLarge { bytes } => format!(
                "Value is {bytes} bytes, over the 1 MiB copy limit; nothing was sent. Use explicit export."
            ),
            Self::Stale => {
                "The selected result changed before confirmation; nothing was sent. Select the current cell and try again."
                    .into()
            }
            Self::Sent { bytes, characters } => format!(
                "Sent {bytes} bytes ({characters} characters) through the terminal; clipboard acceptance is unconfirmed."
            ),
        }
    }
}

/// A value-free outcome from formatting the local SQL buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormatNotice {
    /// The buffer changed as one undoable edit.
    Applied {
        before_lines: usize,
        after_lines: usize,
    },
    /// Formatting produced the same text.
    AlreadyFormatted,
    /// There was no SQL to format.
    Empty,
    /// Formatting refused to guess about the input.
    Refused { message: String },
}

impl FormatNotice {
    /// Plain-language status for the editor footer.
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::Applied {
                before_lines,
                after_lines,
            } => format!(
                "Formatted SQL: {before_lines} lines to {after_lines}. Undo reverses the whole change."
            ),
            Self::AlreadyFormatted => "SQL is already formatted.".to_owned(),
            Self::Empty => "There is no SQL to format.".to_owned(),
            Self::Refused { message } => message.clone(),
        }
    }
}

/// The complete application state.
#[derive(Debug, Clone, Default)]
pub struct Model {
    /// Which pane has focus.
    pub focus: Focus,
    /// The active key labels used when discovery surfaces describe actions.
    ///
    /// This is a configuration snapshot supplied by the interactive runtime,
    /// not a second source of key resolution and not persisted onboarding state.
    pub keymap_snapshot: KeymapSnapshot,
    /// The SQL buffer.
    pub editor: Editor,
    /// Schema-aware completion state and its catalogue snapshot.
    pub completion: crate::app::completion::CompletionState,
    /// Connection state.
    pub connection: ConnectionState,
    /// Safe, ephemeral summaries available to the connection picker.
    ///
    /// Resolved targets and credentials never enter the model. The runtime
    /// keeps those outside this state object and receives only a profile name
    /// when a row is chosen.
    pub connection_profiles: Vec<crate::app::connection_picker::ConnectionProfileSummary>,
    /// Where the object tree reads from.
    pub metadata_link: MetadataLink,
    /// What the query engine is doing.
    pub phase: QueryPhase,
    /// The most recent completed execution.
    pub last_execution: Option<Execution>,
    /// The SQL template that produced the retained execution.
    ///
    /// It is kept separately because `Execution` intentionally contains only
    /// server results, and it is the source the update analyzer must revalidate.
    pub last_sql: Option<String>,
    /// Server messages from the most recent execution.
    pub notices: Vec<Notice>,
    /// The error being shown, if any.
    pub error: Option<Diagnostic>,
    /// Whether the help overlay is open.
    pub help_open: bool,
    /// Whether the technical section of the error is expanded.
    pub error_expanded: bool,
    /// Validated location in the current editor buffer, when one exists.
    pub error_location: Option<ErrorLocation>,
    /// Truthful explanation when a server position cannot be applied.
    pub error_location_note: Option<String>,
    /// Selected row in the result grid.
    pub selected_row: usize,
    /// Selected column in the result grid.
    pub selected_column: usize,
    /// View-only state for the current retained result. Source rows and values
    /// remain owned by `last_execution`; selection stays here because it is a
    /// navigation concern shared by the grid, inspector, and expanded view.
    pub result_grid: ResultGridState,
    /// Set when the application should exit.
    pub should_quit: bool,
    /// Counter used to hand out job identities.
    pub next_job: u64,
    /// Counter used to reject late update-metadata responses.
    pub next_update_request: u64,
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
    /// Named parameter values being collected for one pending run.
    pub parameter_prompt: Option<ParameterPrompt>,
    /// An analyzed plan held back until the user confirms execution.
    pub pending_plan: Option<PendingPlan>,
    /// Whether explicitly confirmed result values may use OSC 52.
    pub clipboard_osc52: bool,
    /// A result-cell copy held back until the user confirms it.
    pub pending_copy: Option<PendingCopy>,
    /// A result-cell update waiting for live relation metadata.
    pub update_lookup: Option<UpdateLookup>,
    /// A replacement value being typed for a result-cell update.
    pub update_prompt: Option<UpdatePrompt>,
    /// A generated update waiting for its final review confirmation.
    pub pending_update: Option<PendingUpdate>,
    /// The last safe status or refusal for the result-cell update workflow.
    pub cell_update_notice: Option<String>,
    /// The last explicit retained-result refresh status or refusal.
    pub refresh_notice: Option<String>,
    /// The last value-free copy outcome shown in Results.
    pub clipboard_notice: Option<ClipboardNotice>,
    /// The last value-free formatting outcome shown in the editor footer.
    pub format_notice: Option<FormatNotice>,
    /// The in-memory plan view over the Results pane.
    pub plan: PlanView,
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
    /// Whether the statement in flight used prompted named parameters.
    pub running_parameterized: bool,
    /// Whether the statement in flight is the one-shot generated cell update.
    pub running_cell_update: bool,
    /// Whether the statement in flight is an explicit retained-result refresh.
    pub running_refresh: bool,
    /// Whether a named-parameter prompt belongs to an explicit refresh.
    pub refresh_pending: bool,
    /// Editor text revision captured when the current execution started.
    pub running_editor_revision: Option<u64>,
    /// Original source span for a Run Statement execution.
    pub running_source: Option<StatementSource>,
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

    /// Hands out the next update-metadata request identity.
    pub fn allocate_update_request(&mut self) -> u64 {
        self.next_update_request += 1;
        self.next_update_request
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

    /// The source row of the result the selection points at after filtering and
    /// local grid sorting.
    #[must_use]
    pub fn selected_source_row(&self) -> Option<usize> {
        self.displayed_rows().get(self.selected_row).copied()
    }

    /// Which retained source rows are displayed, after filtering and local grid
    /// sorting. The result itself is never reordered.
    #[must_use]
    pub fn displayed_rows(&self) -> Vec<usize> {
        let filtered = self.filtered_rows();
        self.visible_result().map_or(filtered.clone(), |set| {
            self.result_grid.sort_rows(&filtered, set)
        })
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
