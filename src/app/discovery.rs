//! Derived guidance for the interactive workbench.
//!
//! Discovery is deliberately a view of the application, not another state
//! machine. The current model and the active keymap are the only facts it uses;
//! opening a hint rail or palette cannot perform work by itself.

use crate::app::message::{Action, Direction};
use crate::app::model::{ConnectionState, Focus, Model};
use crate::query::classify::{Impact, classify};
use crate::query::result::TransactionState;
use crate::query::statements;

/// The maximum number of actions the footer may teach at once.
pub const MAX_CONTEXTUAL_HINTS: usize = crate::app::model::SPACING_TABLE[0].footer_hint_cap;

/// A binding copied from the active keymap into the application boundary.
///
/// The application does not need terminal event types to build palette entries,
/// but it does need the exact key the runtime will resolve. Keeping this small
/// snapshot avoids duplicating defaults in the palette and keeps the app layer
/// independent of crossterm.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyBindingSnapshot {
    /// The action the key invokes.
    pub action: Action,
    /// The human-readable active key.
    pub key: String,
    /// The user-facing description of the action.
    pub description: String,
}

/// A chord continuation copied from the active keymap boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyChordSnapshot {
    /// The second key of the chord.
    pub key: char,
    /// The action the chord invokes.
    pub action: Action,
    /// The user-facing description of the action.
    pub description: String,
}

/// The active keys used by discovery surfaces.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KeymapSnapshot {
    /// Direct bindings in the order the keymap prefers them.
    pub bindings: Vec<KeyBindingSnapshot>,
    /// Chord continuations supported by the runtime.
    pub chords: Vec<KeyChordSnapshot>,
    /// The active first key of a chord, when one is available.
    pub prefix: Option<String>,
}

impl KeymapSnapshot {
    /// Whether the snapshot has not been attached to an interactive model yet.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty() && self.chords.is_empty()
    }
}

/// Connection wording used by discovery decisions and tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionPosture {
    /// No connection attempt is active.
    Disconnected,
    /// A connection attempt is in flight.
    Connecting,
    /// Statements can be sent.
    Connected,
    /// A previously usable connection dropped.
    Lost,
    /// The latest connection attempt failed.
    Failed,
}

impl ConnectionPosture {
    /// Plain-language state text.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Disconnected => "Not connected",
            Self::Connecting => "Connecting",
            Self::Connected => "Connected",
            Self::Lost => "Connection lost",
            Self::Failed => "Connection failed",
        }
    }
}

/// State of the result surface from a discovery perspective.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultPosture {
    /// No completed result set is retained.
    None,
    /// A query is still running or cancellation is not confirmed.
    Running,
    /// The essential description was not retained; this is not an empty answer.
    Unavailable,
    /// A completed result set has no retained rows.
    NoRows,
    /// A completed result set has rows and the current filter admits some.
    Rows,
    /// The filter excludes every retained row.
    FilteredEmpty,
    /// The transaction must be rolled back before another query can run.
    FailedTransaction,
}

/// State of the object tree from a discovery perspective.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectPosture {
    /// The sidebar is hidden.
    Hidden,
    /// The first catalogue read is still in flight.
    Loading,
    /// The tree reported an error.
    Failed,
    /// No object rows are available.
    Empty,
    /// A filter is active but admits no rows.
    FilteredEmpty,
    /// At least one object-tree row is visible.
    Populated,
}

/// State of session history relevant to command discovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryPosture {
    /// History is disabled in configuration.
    Disabled,
    /// History is configured but paused for this session.
    Paused,
    /// History is enabled but has no entries yet.
    Empty,
    /// At least one statement is available.
    Available,
}

/// The facts that determine useful discovery copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiscoveryContext {
    /// The pane receiving the next key.
    pub focus: Focus,
    /// The current connection posture.
    pub connection: ConnectionPosture,
    /// Whether the editor contains non-whitespace SQL.
    pub editor_has_text: bool,
    /// The current query/result posture.
    pub result: ResultPosture,
    /// The current object-tree posture.
    pub objects: ObjectPosture,
    /// The current history posture.
    pub history: HistoryPosture,
    /// Whether schema completion can be requested from the editor.
    pub completion_enabled: bool,
    /// Whether an object is selected for definition/dependency actions.
    pub has_selected_object: bool,
}

/// Derives the current discovery context without changing the model.
#[must_use]
pub fn context(model: &Model) -> DiscoveryContext {
    DiscoveryContext {
        focus: model.focus,
        connection: connection_posture(&model.connection),
        editor_has_text: !model.editor.text().trim().is_empty(),
        result: result_posture(model),
        objects: object_posture(model),
        history: history_posture(model),
        completion_enabled: model.completion.enabled,
        has_selected_object: has_selected_object(model),
    }
}

/// Returns the action candidates for the contextual footer, ordered by use.
///
/// Availability is checked after ordering so the bounded rail cannot become a
/// list of labels for actions that would silently do nothing.
#[must_use]
pub fn hint_actions(model: &Model) -> Vec<Action> {
    hint_actions_for(model, &model.presentation)
}

/// Uses the rendering presentation, including an explicit test override.
#[must_use]
pub fn hint_actions_for(
    model: &Model,
    presentation: &crate::app::model::Presentation,
) -> Vec<Action> {
    let ctx = context(model);
    let mut candidates = match ctx.focus {
        Focus::Editor if model.phase.is_busy() => vec![
            Action::Cancel,
            Action::FormatBuffer,
            Action::FocusNext,
            Action::OpenPalette,
            Action::Quit,
        ],
        Focus::Editor if ctx.connection == ConnectionPosture::Connected && ctx.editor_has_text => {
            vec![
                Action::FormatBuffer,
                Action::RunBuffer,
                Action::ExplainPlan,
                Action::Complete,
                Action::OpenPalette,
            ]
        }
        Focus::Editor if ctx.connection == ConnectionPosture::Connected => vec![
            Action::Complete,
            Action::FocusNext,
            Action::OpenPalette,
            Action::ToggleHelp,
            Action::Quit,
        ],
        Focus::Editor => vec![
            Action::OpenConnectionPicker,
            Action::FormatBuffer,
            Action::FocusNext,
            Action::OpenPalette,
            Action::ToggleHelp,
            Action::Quit,
        ],
        Focus::Results if model.phase.is_busy() => vec![
            Action::Cancel,
            Action::FocusNext,
            Action::OpenPalette,
            Action::ToggleHelp,
            Action::Quit,
        ],
        Focus::Results if model.plan.document().is_some() => vec![
            Action::Move(Direction::Down),
            Action::Activate,
            Action::Dismiss,
            Action::FocusNext,
            Action::OpenPalette,
        ],
        Focus::Results if model.visible_result().is_some() => vec![
            Action::RefreshResult,
            Action::Move(Direction::Down),
            Action::CopyValue,
            Action::GenerateCellUpdate,
            Action::ToggleInspector,
            Action::StartFilter,
            Action::OpenResultControls,
            Action::NarrowColumn,
            Action::WidenColumn,
        ],
        Focus::Results => vec![
            Action::FocusNext,
            Action::OpenPalette,
            Action::ToggleHelp,
            Action::Quit,
            Action::ToggleSidebar,
        ],
        Focus::Objects if ctx.objects == ObjectPosture::Populated => vec![
            Action::Activate,
            Action::StartFilter,
            Action::ShowDefinition,
            Action::FocusNext,
            Action::OpenPalette,
        ],
        Focus::Objects if ctx.objects == ObjectPosture::Loading => vec![
            Action::FocusNext,
            Action::OpenPalette,
            Action::ToggleHelp,
            Action::Quit,
        ],
        Focus::Objects => vec![
            Action::ReloadObjects,
            Action::StartFilter,
            Action::FocusNext,
            Action::OpenPalette,
            Action::ToggleHelp,
        ],
    };
    if model.splits.zoomed {
        candidates.insert(0, Action::ZoomPane);
    } else if !model.phase.is_busy() {
        candidates.push(Action::ZoomPane);
    }
    candidates
        .into_iter()
        .filter(|action| action_is_available(model, action))
        .take(presentation.spacing().footer_hint_cap)
        .collect()
}

/// Returns the actions offered by the general command palette.
#[must_use]
pub fn palette_actions(model: &Model) -> Vec<Action> {
    [
        Action::RunBuffer,
        Action::RunStatement,
        Action::ExplainPlan,
        Action::AnalyzePlan,
        Action::CopyValue,
        Action::GenerateCellUpdate,
        Action::RefreshResult,
        Action::FormatBuffer,
        Action::SwitchTheme,
        Action::OpenConnectionPicker,
        Action::Cancel,
        Action::ToggleHelp,
        Action::FocusNext,
        Action::ToggleSidebar,
        Action::ShrinkPane,
        Action::GrowPane,
        Action::ZoomPane,
        Action::StartFilter,
        Action::ReloadObjects,
        Action::Complete,
        Action::OpenResultControls,
        Action::ToggleExpandedRow,
        Action::ToggleInspector,
        Action::ShowDefinition,
        Action::ShowDependencies,
        Action::SaveQuery,
        Action::ExportRows,
        Action::OpenQuery,
        Action::OpenHistory,
        Action::ToggleHistoryRecording,
        Action::Quit,
    ]
    .into_iter()
    .filter(|action| action_is_available(model, action))
    .collect()
}

/// Returns the exact retained source when an explicit refresh is safe to offer.
///
/// This is intentionally shared by discovery and the reducer. A visible result
/// can outlive the editor text that produced it, so the editor is never a
/// fallback source. The statement impact check is advisory, as it is everywhere
/// else in the client; PostgreSQL remains the authority.
pub fn retained_result_refresh_source(model: &Model) -> Result<String, String> {
    if model.focus != Focus::Results {
        return Err("focus the Results pane before refreshing".to_owned());
    }
    if model.phase.is_busy() {
        return Err("wait for the current query or cancellation to finish".to_owned());
    }
    if model.plan.is_visible() {
        return Err("close the query plan before refreshing a retained result".to_owned());
    }
    if model.transaction == TransactionState::Failed {
        return Err("recover the failed transaction before refreshing".to_owned());
    }
    if !model.connection.is_usable() {
        return Err("reconnect before refreshing the retained result".to_owned());
    }
    if model.visible_result().is_none() {
        return Err("a completed retained result is required".to_owned());
    }
    let Some(source) = model.last_sql.clone() else {
        return Err("the retained result has no source SQL to refresh".to_owned());
    };
    let parsed = statements::split(&source);
    if parsed.len() != 1 {
        return Err(
            "refresh is limited to one retained statement; run a single statement explicitly"
                .to_owned(),
        );
    }
    let impact = classify(&parsed[0]);
    if impact != Impact::Read {
        return Err(format!(
            "the retained source {} and will not be replayed as a refresh",
            impact.label()
        ));
    }
    Ok(source)
}

/// Whether an existing action can do useful work in the current model.
#[must_use]
pub fn action_is_available(model: &Model, action: &Action) -> bool {
    let ctx = context(model);
    match action {
        Action::RunBuffer => {
            ctx.connection == ConnectionPosture::Connected
                && !model.phase.is_busy()
                && ctx.editor_has_text
        }
        Action::RunStatement => {
            ctx.connection == ConnectionPosture::Connected
                && !model.phase.is_busy()
                && crate::query::statements::statement_at(
                    model.editor.text(),
                    model.editor.cursor(),
                )
                .is_some()
        }
        Action::ExplainPlan => plan_action_is_available(model, false),
        Action::AnalyzePlan => plan_action_is_available(model, true),
        Action::CopyValue => copy_action_is_available(model),
        Action::GenerateCellUpdate => cell_update_action_is_available(model),
        Action::RefreshResult => retained_result_refresh_source(model).is_ok(),
        Action::FormatBuffer => ctx.editor_has_text,
        Action::OpenConnectionPicker => {
            !model.connection_profiles.is_empty()
                && !model.phase.is_busy()
                && !matches!(model.connection, ConnectionState::Connecting { .. })
        }
        Action::Cancel => model.phase.is_busy(),
        Action::ToggleHelp
        | Action::FocusNext
        | Action::ToggleSidebar
        | Action::ShrinkPane
        | Action::GrowPane
        | Action::ZoomPane
        | Action::OpenPalette
        | Action::SwitchTheme
        | Action::Quit => true,
        Action::BeginPrefix => false,
        Action::ToggleErrorDetail => model.error.is_some(),
        Action::Dismiss => {
            model.help_open
                || model.tree.filtering
                || !model.tree.filter.is_empty()
                || (model.plan.is_visible() && !model.plan.is_loading())
                || model.error.is_some()
        }
        Action::Move(_) => match ctx.focus {
            Focus::Editor => true,
            Focus::Results => model.plan.document().is_some() || ctx.result == ResultPosture::Rows,
            Focus::Objects => ctx.objects == ObjectPosture::Populated,
        },
        Action::Activate => {
            model.plan.document().is_some()
                || (ctx.focus == Focus::Objects && ctx.objects == ObjectPosture::Populated)
        }
        Action::Insert(_) | Action::Backspace => false,
        Action::StartFilter => match ctx.focus {
            Focus::Results => {
                matches!(
                    ctx.result,
                    ResultPosture::Rows | ResultPosture::FilteredEmpty
                ) && !model.phase.is_busy()
                    && !model.plan.is_visible()
            }
            Focus::Objects => model.sidebar_visible && !model.tree.loading,
            Focus::Editor => false,
        },
        Action::ReloadObjects => {
            model.sidebar_visible
                && ctx.connection == ConnectionPosture::Connected
                && !model.tree.loading
        }
        Action::Complete => {
            ctx.focus == Focus::Editor && ctx.connection == ConnectionPosture::Connected
        }
        Action::ToggleExpandedRow | Action::ToggleInspector => {
            ctx.focus == Focus::Results
                && ctx.result == ResultPosture::Rows
                && !model.plan.is_visible()
        }
        Action::OpenResultControls | Action::NarrowColumn | Action::WidenColumn => {
            ctx.focus == Focus::Results
                && !model.plan.is_visible()
                && model
                    .visible_result()
                    .is_some_and(|set| !set.columns.is_empty())
        }
        Action::SaveQuery => ctx.editor_has_text,
        Action::ExportRows => {
            ctx.focus == Focus::Results
                && !model.plan.is_visible()
                && !model.displayed_rows().is_empty()
        }
        Action::OpenQuery => true,
        Action::ShowDefinition | Action::ShowDependencies => {
            ctx.focus == Focus::Objects && ctx.has_selected_object
        }
        Action::OpenHistory | Action::ToggleHistoryRecording => {
            ctx.history != HistoryPosture::Disabled
        }
        Action::DeleteForward
        | Action::DeleteWordLeft
        | Action::MoveWord(_)
        | Action::MoveLineStart
        | Action::MoveLineEnd
        | Action::MoveBufferStart
        | Action::MoveBufferEnd
        | Action::MovePage(_)
        | Action::Undo
        | Action::Redo => false,
    }
}

/// Explains why a currently unavailable action is not listed as immediately
/// actionable. This is used for chord entries that remain searchable so a
/// person can learn the prerequisite instead of meeting a silent no-op.
#[must_use]
pub fn action_prerequisite(model: &Model, action: &Action) -> Option<String> {
    if action_is_available(model, action) {
        return None;
    }
    let ctx = context(model);
    let connection = match ctx.connection {
        ConnectionPosture::Disconnected => Some("connect first"),
        ConnectionPosture::Connecting => Some("wait for the connection"),
        ConnectionPosture::Lost => Some("reconnect first"),
        ConnectionPosture::Failed => Some("resolve the connection failure first"),
        ConnectionPosture::Connected => None,
    };
    Some(match action {
        Action::RunBuffer | Action::RunStatement => connection
            .map(str::to_owned)
            .or_else(|| (!ctx.editor_has_text).then(|| "type SQL in the editor".to_owned()))
            .or_else(|| {
                model
                    .phase
                    .is_busy()
                    .then(|| "wait for the current query".to_owned())
            })
            .unwrap_or_else(|| "place the cursor in a SQL statement".to_owned()),
        Action::ExplainPlan | Action::AnalyzePlan => {
            plan_action_prerequisite(model, matches!(action, Action::AnalyzePlan), connection)
        }
        Action::FormatBuffer => "SQL in the editor".to_owned(),
        Action::OpenConnectionPicker => {
            if model.connection_profiles.is_empty() {
                "a named profile in config.toml".to_owned()
            } else if model.phase.is_busy() {
                "the current query or plan to finish".to_owned()
            } else {
                "the connection attempt to finish".to_owned()
            }
        }
        Action::Cancel => "a query must be running".to_owned(),
        Action::ToggleErrorDetail => "an error must be visible".to_owned(),
        Action::Dismiss => "a help, filter, or error surface must be open".to_owned(),
        Action::StartFilter => match ctx.focus {
            Focus::Results => "a completed result must be retained".to_owned(),
            Focus::Objects => "the object tree must finish loading".to_owned(),
            Focus::Editor => "focus the Results or Objects pane".to_owned(),
        },
        Action::ReloadObjects => connection
            .map(str::to_owned)
            .unwrap_or_else(|| "the object tree must not already be loading".to_owned()),
        Action::Complete => connection
            .map(str::to_owned)
            .unwrap_or_else(|| "focus the editor".to_owned()),
        Action::OpenResultControls => {
            if ctx.focus != Focus::Results {
                "focus the Results pane".to_owned()
            } else {
                "a retained result with columns".to_owned()
            }
        }
        Action::NarrowColumn | Action::WidenColumn => {
            if ctx.focus != Focus::Results {
                "focus the Results pane".to_owned()
            } else {
                "a retained result column to resize".to_owned()
            }
        }
        Action::CopyValue => {
            if ctx.focus != Focus::Results {
                "focus Results on a retained text cell".to_owned()
            } else if model.phase.is_busy() {
                "wait for the current query".to_owned()
            } else if model.plan.is_visible() {
                "close the query plan first".to_owned()
            } else {
                "a selected retained result cell".to_owned()
            }
        }
        Action::GenerateCellUpdate => {
            if ctx.focus != Focus::Results {
                "focus the Results pane".to_owned()
            } else if model.phase.is_busy() {
                "wait for the current query".to_owned()
            } else if model.plan.is_visible() {
                "close the query plan first".to_owned()
            } else if model.environment().is_production() {
                "a connection not classified as production".to_owned()
            } else if model.connection.info().is_some_and(|info| info.read_only) {
                "a read-write session".to_owned()
            } else if model.last_sql.is_none() {
                "a retained result with its SELECT source".to_owned()
            } else {
                "a selected retained result cell".to_owned()
            }
        }
        Action::RefreshResult => retained_result_refresh_source(model)
            .err()
            .unwrap_or_else(|| "an eligible retained result".to_owned()),
        Action::ToggleExpandedRow | Action::ToggleInspector => {
            if ctx.focus != Focus::Results {
                "focus the Results pane".to_owned()
            } else {
                "retained result rows".to_owned()
            }
        }
        Action::SaveQuery => "SQL in the editor".to_owned(),
        Action::ExportRows => "retained rows on screen".to_owned(),
        Action::OpenQuery => "a saved query to open".to_owned(),
        Action::ShowDefinition | Action::ShowDependencies => {
            "a selected object in the tree".to_owned()
        }
        Action::OpenHistory | Action::ToggleHistoryRecording => {
            "history enabled in configuration".to_owned()
        }
        Action::OpenPalette
        | Action::SwitchTheme
        | Action::ToggleHelp
        | Action::FocusNext
        | Action::ToggleSidebar
        | Action::ShrinkPane
        | Action::GrowPane
        | Action::ZoomPane
        | Action::Quit => "this surface to close first".to_owned(),
        Action::Move(_) => "focus a pane with something to move through".to_owned(),
        Action::Activate => {
            if ctx.focus == Focus::Results {
                "a ready plan or retained result".to_owned()
            } else {
                "focus a populated object tree".to_owned()
            }
        }
        Action::BeginPrefix
        | Action::Insert(_)
        | Action::Backspace
        | Action::DeleteForward
        | Action::DeleteWordLeft
        | Action::MoveWord(_)
        | Action::MoveLineStart
        | Action::MoveLineEnd
        | Action::MoveBufferStart
        | Action::MoveBufferEnd
        | Action::MovePage(_)
        | Action::Undo
        | Action::Redo => "a different active surface".to_owned(),
    })
}

/// Groups an action by the intent a person would search for.
#[must_use]
pub const fn palette_group(action: &Action) -> &'static str {
    match action {
        Action::RunBuffer
        | Action::RunStatement
        | Action::ExplainPlan
        | Action::AnalyzePlan
        | Action::FormatBuffer
        | Action::Complete
        | Action::SaveQuery => "Editor",
        Action::OpenResultControls
        | Action::NarrowColumn
        | Action::WidenColumn
        | Action::CopyValue
        | Action::GenerateCellUpdate
        | Action::RefreshResult
        | Action::ToggleExpandedRow
        | Action::ToggleInspector
        | Action::ExportRows => "Results",
        Action::StartFilter
        | Action::ReloadObjects
        | Action::ShowDefinition
        | Action::ShowDependencies => "Objects",
        Action::OpenQuery | Action::OpenHistory | Action::ToggleHistoryRecording => "Files/History",
        Action::OpenConnectionPicker | Action::SwitchTheme => "Session",
        Action::Cancel
        | Action::ToggleHelp
        | Action::FocusNext
        | Action::ToggleSidebar
        | Action::ShrinkPane
        | Action::GrowPane
        | Action::ZoomPane
        | Action::Quit
        | Action::OpenPalette
        | Action::BeginPrefix
        | Action::ToggleErrorDetail
        | Action::Dismiss
        | Action::Move(_)
        | Action::Insert(_)
        | Action::Backspace
        | Action::Activate
        | Action::DeleteForward
        | Action::DeleteWordLeft
        | Action::MoveWord(_)
        | Action::MoveLineStart
        | Action::MoveLineEnd
        | Action::MoveBufferStart
        | Action::MoveBufferEnd
        | Action::MovePage(_)
        | Action::Undo
        | Action::Redo => "Session",
    }
}

/// Explains the prerequisites behind an empty or narrowed command palette.
#[must_use]
pub fn palette_note(model: &Model) -> String {
    let ctx = context(model);
    let mut notes = vec![format!("Focus: {}.", ctx.focus.label())];
    match ctx.connection {
        ConnectionPosture::Disconnected => notes.push("Connect before running SQL.".to_owned()),
        ConnectionPosture::Connecting => {
            notes.push("Wait for the connection before running SQL.".to_owned())
        }
        ConnectionPosture::Lost => notes.push("Reconnect before running SQL.".to_owned()),
        ConnectionPosture::Failed => {
            notes.push("Resolve the connection failure before running SQL.".to_owned())
        }
        ConnectionPosture::Connected => {}
    }
    if !ctx.editor_has_text {
        notes.push("Run and save actions need SQL in the editor.".to_owned());
    }
    if matches!(
        ctx.result,
        ResultPosture::None | ResultPosture::NoRows | ResultPosture::FilteredEmpty
    ) {
        notes.push("Result actions need retained rows.".to_owned());
    }
    if ctx.result == ResultPosture::Unavailable {
        notes.push(format!(
            "{}. {}",
            crate::query::result::RETAINED_DATA_UNAVAILABLE,
            crate::query::result::RETAINED_DATA_GUIDANCE
        ));
    }
    if ctx.result == ResultPosture::FailedTransaction {
        notes.push("Run ROLLBACK before sending another query.".to_owned());
    }
    if matches!(
        ctx.objects,
        ObjectPosture::Loading | ObjectPosture::Empty | ObjectPosture::FilteredEmpty
    ) {
        notes.push("Object actions appear after the catalogue has rows.".to_owned());
    }
    if ctx.history == HistoryPosture::Empty {
        notes.push("No history is available until a statement finishes.".to_owned());
    }
    notes.push(
        "Ready actions are listed; unavailable chords explain their prerequisite. Type to search; Esc closes."
            .to_owned(),
    );
    notes.join(" ")
}

fn plan_action_is_available(model: &Model, _analyze: bool) -> bool {
    model.connection.is_usable()
        && !model.phase.is_busy()
        && model.transaction != TransactionState::Failed
        && current_statement(model).is_some_and(|sql| !is_explain_statement(&sql))
}

fn copy_action_is_available(model: &Model) -> bool {
    if model.focus != Focus::Results || model.phase.is_busy() || model.plan.is_visible() {
        return false;
    }
    let Some(source_row) = model.selected_source_row() else {
        return false;
    };
    model
        .visible_result()
        .filter(|set| set.is_available())
        .is_some_and(|set| {
            model
                .result_grid
                .visible_columns(set.columns.len())
                .contains(&model.selected_column)
                && set
                    .rows
                    .get(source_row)
                    .and_then(|row| row.get(model.selected_column))
                    .is_some()
        })
}

fn cell_update_action_is_available(model: &Model) -> bool {
    if model.focus != Focus::Results
        || model.phase.is_busy()
        || model.plan.is_visible()
        || model.environment().is_production()
        || model.connection.info().is_some_and(|info| info.read_only)
        || !model.connection.is_usable()
        || model.transaction == TransactionState::Failed
        || model.last_sql.is_none()
    {
        return false;
    }
    let Some(source_row) = model.selected_source_row() else {
        return false;
    };
    model
        .visible_result()
        .filter(|set| set.is_available())
        .is_some_and(|set| {
            model
                .result_grid
                .visible_columns(set.columns.len())
                .contains(&model.selected_column)
                && set
                    .rows
                    .get(source_row)
                    .and_then(|row| row.get(model.selected_column))
                    .is_some()
        })
}

fn plan_action_prerequisite(
    model: &Model,
    analyze: bool,
    connection: Option<&'static str>,
) -> String {
    if let Some(connection) = connection {
        return connection.to_owned();
    }
    if model.phase.is_busy() {
        return "wait for the current query or plan".to_owned();
    }
    if model.transaction == TransactionState::Failed {
        return "run ROLLBACK before asking for a plan".to_owned();
    }
    let Some(sql) = current_statement(model) else {
        return "place the cursor in one SQL statement".to_owned();
    };
    if is_explain_statement(&sql) {
        return "place the cursor in the target statement, not an EXPLAIN".to_owned();
    }
    if analyze {
        "the statement must be ready; Analyze plan will ask for confirmation".to_owned()
    } else {
        "the statement must be ready for PostgreSQL".to_owned()
    }
}

fn current_statement(model: &Model) -> Option<String> {
    let source =
        crate::query::error_location::source_at_cursor(model.editor.text(), model.editor.cursor())?;
    let sql = model.editor.text()[source.start..source.end]
        .trim()
        .to_owned();
    (!sql.is_empty() && crate::query::statements::split(&sql).len() == 1).then_some(sql)
}

fn is_explain_statement(sql: &str) -> bool {
    let uncommented = crate::query::statements::strip_comments(sql);
    uncommented
        .split_whitespace()
        .next()
        .is_some_and(|word| word.eq_ignore_ascii_case("explain"))
}

fn connection_posture(connection: &ConnectionState) -> ConnectionPosture {
    match connection {
        ConnectionState::Disconnected => ConnectionPosture::Disconnected,
        ConnectionState::Connecting { .. } => ConnectionPosture::Connecting,
        ConnectionState::Connected(_) => ConnectionPosture::Connected,
        ConnectionState::Lost { .. } => ConnectionPosture::Lost,
        ConnectionState::Failed(_) => ConnectionPosture::Failed,
    }
}

fn result_posture(model: &Model) -> ResultPosture {
    if model.transaction == TransactionState::Failed {
        return ResultPosture::FailedTransaction;
    }
    if model.phase.is_busy() {
        return ResultPosture::Running;
    }
    let Some(set) = model.visible_result() else {
        return ResultPosture::None;
    };
    if !set.is_available() {
        return ResultPosture::Unavailable;
    }
    if set.rows.is_empty() {
        return ResultPosture::NoRows;
    }
    if !model.result_filter.trim().is_empty() && model.filtered_rows().is_empty() {
        ResultPosture::FilteredEmpty
    } else {
        ResultPosture::Rows
    }
}

fn object_posture(model: &Model) -> ObjectPosture {
    if !model.sidebar_visible {
        return ObjectPosture::Hidden;
    }
    if model.tree.error.is_some() {
        return ObjectPosture::Failed;
    }
    if model.tree.loading && model.tree.roots.is_empty() {
        return ObjectPosture::Loading;
    }
    let rows = model.tree.rows();
    if rows.is_empty() && !model.tree.filter.trim().is_empty() {
        ObjectPosture::FilteredEmpty
    } else if rows.is_empty() {
        ObjectPosture::Empty
    } else {
        ObjectPosture::Populated
    }
}

fn history_posture(model: &Model) -> HistoryPosture {
    if model.history_disabled {
        HistoryPosture::Disabled
    } else if model.history_paused {
        HistoryPosture::Paused
    } else if model.history.is_empty() {
        HistoryPosture::Empty
    } else {
        HistoryPosture::Available
    }
}

fn has_selected_object(model: &Model) -> bool {
    let Some(row) = model.tree.selected_row() else {
        return false;
    };
    model
        .tree
        .node(&row.path)
        .is_some_and(|node| matches!(node.kind, crate::app::tree::NodeKind::Object(_)))
}

#[cfg(test)]
mod tests;
