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

/// A named stage of establishing a session, in the order it can occur.
///
/// The vocabulary is pinned by the specification: a connecting display names
/// one of these stages, and a stage that does not apply is skipped rather than
/// shown as done.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectingStep {
    /// Choosing and resolving a named profile into a target.
    ResolvingProfile,
    /// Obtaining a credential, such as a cloud provider token.
    AcquiringCredential,
    /// Negotiating transport encryption with the server.
    TlsHandshake,
    /// Opening the session and authenticating.
    ServerHandshake,
    /// Reading the catalogue the object tree shows.
    LoadingCatalogue,
}

impl ConnectingStep {
    /// The exact phrase the interface and diagnostics use.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::ResolvingProfile => "resolving profile",
            Self::AcquiringCredential => "acquiring credential",
            Self::TlsHandshake => "TLS handshake",
            Self::ServerHandshake => "server handshake",
            Self::LoadingCatalogue => "loading catalogue",
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
    Connecting {
        /// The stage currently in progress.
        step: ConnectingStep,
        /// A display-safe target, such as a profile name or `user@host:port/db`.
        target: String,
    },
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

    /// Whether the connection is active and ready for catalog queries.
    #[must_use]
    pub const fn is_ready(&self) -> bool {
        self.is_usable()
    }

    /// Short text for the status bar. Never colour alone.
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::Disconnected => "Not connected".to_owned(),
            Self::Connecting { step, target } if target.is_empty() => {
                format!("Connecting: {}", step.label())
            }
            Self::Connecting { step, target } => {
                format!("Connecting to {target}: {}", step.label())
            }
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
    /// Whether to include the DROP statement header in DDL generation and display.
    pub include_drop: bool,
}

impl Definition {
    /// The lines of the definition, empty while it is loading.
    #[must_use]
    pub fn lines(&self) -> Vec<String> {
        self.definition
            .as_ref()
            .map(|definition| {
                definition
                    .full_script(self.include_drop)
                    .lines()
                    .map(ToOwned::to_owned)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Number of lines in the active script, empty while loading.
    #[must_use]
    pub fn line_count(&self) -> usize {
        self.definition
            .as_ref()
            .map(|definition| {
                let base = definition.text.lines().count();
                if self.include_drop {
                    base.saturating_add(2)
                } else {
                    base
                }
            })
            .unwrap_or(0)
    }

    /// Scrolls down, stopping with the last line on screen.
    pub fn scroll_down(&mut self, height: usize) {
        let max = self.line_count().saturating_sub(height);
        if self.scroll < max {
            self.scroll += 1;
        }
    }

    /// Scrolls up.
    pub const fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }
}

/// Interactive state for the entity-relationship (ER) diagram overlay.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SchemaDiagramState {
    /// Complete loaded entity-relationship graph.
    pub graph: crate::diagram::SchemaGraph,
    /// Currently focused index in `graph.tables`.
    pub selected_table: usize,
    /// Traversal depth around the selected table.
    pub depth: crate::diagram::DiagramHopDepth,
    /// Selected output format (Unicode, ASCII, or Mermaid).
    pub format: crate::diagram::DiagramFormat,
    /// Vertical scroll offset for the rendered diagram pane.
    pub scroll_y: usize,
    /// Horizontal scroll offset for the rendered diagram pane.
    pub scroll_x: usize,
    /// Whether the diagram graph is currently loading in the background.
    pub loading: bool,
    /// Failure message when catalog reading fails.
    pub error: Option<String>,
    /// Pre-rendered text lines for fast viewport rendering.
    pub rendered_lines: Vec<String>,
    /// Active schema filter, if any.
    pub filter_schema: Option<String>,
}

impl SchemaDiagramState {
    /// Constructs an initial diagram state from a graph.
    #[must_use]
    pub fn new(graph: crate::diagram::SchemaGraph, unicode: bool) -> Self {
        let mut state = Self {
            graph,
            selected_table: 0,
            depth: crate::diagram::DiagramHopDepth::AllSchema,
            format: if unicode {
                crate::diagram::DiagramFormat::Unicode
            } else {
                crate::diagram::DiagramFormat::Ascii
            },
            scroll_y: 0,
            scroll_x: 0,
            loading: false,
            error: None,
            rendered_lines: Vec::new(),
            filter_schema: None,
        };
        state.rebuild_rendered(unicode);
        state
    }

    /// Re-renders diagram text into `self.rendered_lines` according to active filters.
    pub fn rebuild_rendered(&mut self, unicode: bool) {
        let view_graph = if let Some(schema) = &self.filter_schema {
            self.graph.filter_schema(schema)
        } else {
            self.graph.clone()
        };

        let final_graph = match self.depth {
            crate::diagram::DiagramHopDepth::AllSchema => view_graph,
            crate::diagram::DiagramHopDepth::OneHop | crate::diagram::DiagramHopDepth::TwoHops => {
                if let Some(table) = self.graph.tables.get(self.selected_table) {
                    view_graph.filter_neighborhood(&table.qualified_name(), self.depth)
                } else {
                    view_graph
                }
            }
        };

        let format = if self.format == crate::diagram::DiagramFormat::Mermaid {
            crate::diagram::DiagramFormat::Mermaid
        } else if unicode {
            crate::diagram::DiagramFormat::Unicode
        } else {
            crate::diagram::DiagramFormat::Ascii
        };

        let rendered = final_graph.render(format);
        self.rendered_lines = rendered.lines().map(ToOwned::to_owned).collect();
    }

    /// Selects the next table in `graph.tables` if available.
    pub fn select_next(&mut self, unicode: bool) {
        if !self.graph.tables.is_empty() && self.selected_table + 1 < self.graph.tables.len() {
            self.selected_table += 1;
            if self.depth != crate::diagram::DiagramHopDepth::AllSchema {
                self.rebuild_rendered(unicode);
            }
        }
    }

    /// Selects the previous table in `graph.tables` if available.
    pub fn select_prev(&mut self, unicode: bool) {
        if self.selected_table > 0 {
            self.selected_table -= 1;
            if self.depth != crate::diagram::DiagramHopDepth::AllSchema {
                self.rebuild_rendered(unicode);
            }
        }
    }

    /// Scrolls down by `amount` lines.
    pub fn scroll_down(&mut self, amount: usize) {
        let max = self.rendered_lines.len().saturating_sub(1);
        self.scroll_y = (self.scroll_y + amount).min(max);
    }

    /// Scrolls up by `amount` lines.
    pub const fn scroll_up(&mut self, amount: usize) {
        self.scroll_y = self.scroll_y.saturating_sub(amount);
    }

    /// Scrolls right by `amount` columns.
    pub const fn scroll_right(&mut self, amount: usize) {
        self.scroll_x = self.scroll_x.saturating_add(amount);
    }

    /// Scrolls left by `amount` columns.
    pub const fn scroll_left(&mut self, amount: usize) {
        self.scroll_x = self.scroll_x.saturating_sub(amount);
    }

    /// Sets neighbourhood depth and rebuilds the rendered diagram.
    pub fn set_depth(&mut self, depth: crate::diagram::DiagramHopDepth, unicode: bool) {
        self.depth = depth;
        self.rebuild_rendered(unicode);
    }

    /// Sets diagram format and rebuilds the rendered diagram.
    pub fn set_format(&mut self, format: crate::diagram::DiagramFormat, unicode: bool) {
        self.format = format;
        self.rebuild_rendered(unicode);
    }
}

/// Interactive state for the server activity and locks dashboard overlay.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ServerActivityState {
    /// Complete bounded server activity and lock contention snapshot.
    pub snapshot: crate::activity::ServerActivitySnapshot,
    /// Currently focused index in `snapshot.backends`.
    pub selected_backend: usize,
    /// Current view mode (Backends list vs Locks tree).
    pub mode: crate::activity::ActivityViewMode,
    /// Vertical scroll offset for the rendered text pane.
    pub scroll_y: usize,
    /// Horizontal scroll offset for the rendered text pane.
    pub scroll_x: usize,
    /// Whether the snapshot is currently loading in the background.
    pub loading: bool,
    /// Failure message when telemetry reading fails.
    pub error: Option<String>,
    /// Pre-rendered text lines for fast viewport rendering.
    pub rendered_lines: Vec<String>,
}

impl ServerActivityState {
    /// Constructs an initial server activity state from a snapshot.
    #[must_use]
    pub fn new(snapshot: crate::activity::ServerActivitySnapshot, unicode: bool) -> Self {
        let mut state = Self {
            snapshot,
            selected_backend: 0,
            mode: crate::activity::ActivityViewMode::Backends,
            scroll_y: 0,
            scroll_x: 0,
            loading: false,
            error: None,
            rendered_lines: Vec::new(),
        };
        state.rebuild_rendered(unicode);
        state
    }

    /// Re-renders activity text into `self.rendered_lines` according to active mode.
    pub fn rebuild_rendered(&mut self, unicode: bool) {
        let rendered = self.snapshot.render_text(self.mode, unicode);
        self.rendered_lines = rendered.lines().map(ToOwned::to_owned).collect();
    }

    /// Selects the next backend process if available.
    pub fn select_next(&mut self) {
        if !self.snapshot.backends.is_empty()
            && self.selected_backend + 1 < self.snapshot.backends.len()
        {
            self.selected_backend += 1;
        }
    }

    /// Selects the previous backend process if available.
    pub fn select_prev(&mut self) {
        if self.selected_backend > 0 {
            self.selected_backend -= 1;
        }
    }

    /// Toggles between active backends and hierarchical lock blocker tree.
    pub fn toggle_mode(&mut self, unicode: bool) {
        self.mode = self.mode.toggle();
        self.rebuild_rendered(unicode);
    }

    /// Returns the currently selected backend process, if any.
    #[must_use]
    pub fn selected_process(&self) -> Option<&crate::activity::BackendProcess> {
        self.snapshot.backends.get(self.selected_backend)
    }

    /// Scrolls down by `amount` lines.
    pub fn scroll_down(&mut self, amount: usize) {
        let max = self.rendered_lines.len().saturating_sub(1);
        self.scroll_y = (self.scroll_y + amount).min(max);
    }

    /// Scrolls up by `amount` lines.
    pub const fn scroll_up(&mut self, amount: usize) {
        self.scroll_y = self.scroll_y.saturating_sub(amount);
    }

    /// Scrolls right by `amount` columns.
    pub const fn scroll_right(&mut self, amount: usize) {
        self.scroll_x = self.scroll_x.saturating_add(amount);
    }

    /// Scrolls left by `amount` columns.
    pub const fn scroll_left(&mut self, amount: usize) {
        self.scroll_x = self.scroll_x.saturating_sub(amount);
    }
}

/// Interactive state for the roles, privilege grants, and RLS auditor overlay.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SecurityAuditState {
    /// Complete bounded security audit snapshot across roles, tables, and grants.
    pub snapshot: crate::audit::SecurityAuditSnapshot,
    /// Currently active view section (Roles, RLS Policies, or Grants).
    pub section: crate::audit::AuditSection,
    /// Currently focused index in the active section list.
    pub selected_index: usize,
    /// Vertical scroll offset for the rendered text pane.
    pub scroll_y: usize,
    /// Horizontal scroll offset for the rendered text pane.
    pub scroll_x: usize,
    /// Whether the audit snapshot is currently loading in the background.
    pub loading: bool,
    /// Failure message when audit catalog reading fails.
    pub error: Option<String>,
    /// Pre-rendered text lines for fast viewport rendering.
    pub rendered_lines: Vec<String>,
}

impl SecurityAuditState {
    /// Constructs an initial security audit state from a snapshot.
    #[must_use]
    pub fn new(snapshot: crate::audit::SecurityAuditSnapshot, unicode: bool) -> Self {
        let mut state = Self {
            snapshot,
            section: crate::audit::AuditSection::Roles,
            selected_index: 0,
            scroll_y: 0,
            scroll_x: 0,
            loading: false,
            error: None,
            rendered_lines: Vec::new(),
        };
        state.rebuild_rendered(unicode);
        state
    }

    /// Total items in the currently active section.
    #[must_use]
    pub fn item_count(&self) -> usize {
        match self.section {
            crate::audit::AuditSection::Roles => self.snapshot.roles.len(),
            crate::audit::AuditSection::RlsPolicies => self.snapshot.tables.len(),
            crate::audit::AuditSection::Grants => self.snapshot.grants.len(),
        }
    }

    /// Re-renders audit report text into `self.rendered_lines` according to active section.
    pub fn rebuild_rendered(&mut self, unicode: bool) {
        let rendered = self.snapshot.render_text(Some(self.section), unicode);
        self.rendered_lines = rendered.lines().map(ToOwned::to_owned).collect();
    }

    /// Selects the next item in the active section list if available.
    pub fn select_next(&mut self) {
        let count = self.item_count();
        if count > 0 && self.selected_index + 1 < count {
            self.selected_index += 1;
        }
    }

    /// Selects the previous item in the active section list if available.
    pub fn select_prev(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    /// Cycles to the next section tab (Roles -> RlsPolicies -> Grants -> Roles).
    pub fn cycle_section(&mut self, unicode: bool) {
        self.section = self.section.next();
        self.selected_index = 0;
        self.scroll_y = 0;
        self.scroll_x = 0;
        self.rebuild_rendered(unicode);
    }

    /// Directly sets the active section tab.
    pub fn set_section(&mut self, section: crate::audit::AuditSection, unicode: bool) {
        self.section = section;
        self.selected_index = 0;
        self.scroll_y = 0;
        self.scroll_x = 0;
        self.rebuild_rendered(unicode);
    }

    /// Generates reviewed SQL for the currently selected item in the active section.
    #[must_use]
    pub fn drafted_sql(&self) -> Option<String> {
        match self.section {
            crate::audit::AuditSection::Roles => {
                let role = self.snapshot.roles.get(self.selected_index)?;
                Some(crate::audit::SecurityAuditSnapshot::reviewed_role_sql(role))
            }
            crate::audit::AuditSection::RlsPolicies => {
                let table = self.snapshot.tables.get(self.selected_index)?;
                Some(crate::audit::SecurityAuditSnapshot::reviewed_rls_remediation_sql(table))
            }
            crate::audit::AuditSection::Grants => {
                let grant = self.snapshot.grants.get(self.selected_index)?;
                Some(crate::audit::SecurityAuditSnapshot::reviewed_grant_sql(
                    grant,
                ))
            }
        }
    }

    /// Scrolls down by `amount` lines.
    pub fn scroll_down(&mut self, amount: usize) {
        let max = self.rendered_lines.len().saturating_sub(1);
        self.scroll_y = (self.scroll_y + amount).min(max);
    }

    /// Scrolls up by `amount` lines.
    pub const fn scroll_up(&mut self, amount: usize) {
        self.scroll_y = self.scroll_y.saturating_sub(amount);
    }

    /// Scrolls right by `amount` columns.
    pub const fn scroll_right(&mut self, amount: usize) {
        self.scroll_x = self.scroll_x.saturating_add(amount);
    }

    /// Scrolls left by `amount` columns.
    pub const fn scroll_left(&mut self, amount: usize) {
        self.scroll_x = self.scroll_x.saturating_sub(amount);
    }
}

/// Which view tab is active in the Schema Diff & Migration Designer overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SchemaDiffTab {
    /// Visual schema diff tree (+, ~, -).
    #[default]
    DiffTree,
    /// Interactive Table Designer with live DDL preview.
    TableDesigner,
    /// Transactional SQL migration script (BEGIN; ... COMMIT;).
    MigrationSql,
}

impl SchemaDiffTab {
    /// Human-readable title for tabs.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::DiffTree => "Schema Diff",
            Self::TableDesigner => "Table Designer",
            Self::MigrationSql => "Migration SQL",
        }
    }

    /// Cycles to the next tab.
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::DiffTree => Self::TableDesigner,
            Self::TableDesigner => Self::MigrationSql,
            Self::MigrationSql => Self::DiffTree,
        }
    }
}

/// Interactive state for the Schema Diff, Table Designer, and Migration Generator overlay (`030-visual-schema-designer-and-migrations`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SchemaDiffState {
    /// Source schema name (default: "public").
    pub source_schema: String,
    /// Target schema name (e.g. "staging" or "public").
    pub target_schema: String,
    /// Loaded structural difference between the two schemas.
    pub diff: Option<crate::migration::SchemaDiff>,
    /// Active tab in the overlay.
    pub active_tab: SchemaDiffTab,
    /// Interactive table draft in the Visual Table Designer.
    pub blueprint: crate::migration::TableBlueprint,
    /// Scroll offset Y in lines.
    pub scroll_y: usize,
    /// Scroll offset X in columns.
    pub scroll_x: usize,
    /// Whether schema metadata is actively loading in background.
    pub loading: bool,
    /// Failure message if metadata query failed.
    pub error: Option<String>,
    /// Pre-rendered diff tree lines.
    pub tree_lines: Vec<String>,
    /// Pre-rendered migration SQL script.
    pub migration_sql: String,
}

impl SchemaDiffState {
    /// Constructs a new schema diff state.
    #[must_use]
    pub fn new(
        diff: Option<crate::migration::SchemaDiff>,
        source_schema: String,
        target_schema: String,
        unicode: bool,
    ) -> Self {
        let mut state = Self {
            source_schema,
            target_schema,
            diff,
            active_tab: SchemaDiffTab::DiffTree,
            blueprint: crate::migration::TableBlueprint::default(),
            scroll_y: 0,
            scroll_x: 0,
            loading: false,
            error: None,
            tree_lines: Vec::new(),
            migration_sql: String::new(),
        };
        state.rebuild(unicode);
        state
    }

    /// Re-renders diff tree and migration SQL from the current diff.
    pub fn rebuild(&mut self, unicode: bool) {
        if let Some(ref d) = self.diff {
            let tree = crate::migration::render_diff_tree(d, unicode);
            self.tree_lines = tree.lines().map(ToOwned::to_owned).collect();
            let options = crate::migration::MigrationOptions::default();
            self.migration_sql = crate::migration::generate_migration_sql(d, &options);
        } else {
            self.tree_lines = vec!["  (Loading schema metadata...)".to_owned()];
            self.migration_sql = "-- Loading schema metadata...\n".to_owned();
        }
    }

    /// Cycles active tab.
    pub fn cycle_tab(&mut self, unicode: bool) {
        self.active_tab = self.active_tab.next();
        self.scroll_y = 0;
        self.scroll_x = 0;
        self.rebuild(unicode);
    }

    /// Directly sets active tab.
    pub fn set_tab(&mut self, tab: SchemaDiffTab, unicode: bool) {
        self.active_tab = tab;
        self.scroll_y = 0;
        self.scroll_x = 0;
        self.rebuild(unicode);
    }

    /// Returns the SQL to load into the SQL editor buffer.
    #[must_use]
    pub fn drafted_sql(&self) -> String {
        match self.active_tab {
            SchemaDiffTab::TableDesigner => self.blueprint.to_create_table_sql(),
            SchemaDiffTab::DiffTree | SchemaDiffTab::MigrationSql => self.migration_sql.clone(),
        }
    }

    /// Scrolls down by `amount` lines.
    pub fn scroll_down(&mut self, amount: usize) {
        let max = match self.active_tab {
            SchemaDiffTab::DiffTree => self.tree_lines.len().saturating_sub(1),
            SchemaDiffTab::MigrationSql => self.migration_sql.lines().count().saturating_sub(1),
            SchemaDiffTab::TableDesigner => self.blueprint.columns.len().saturating_sub(1),
        };
        self.scroll_y = (self.scroll_y + amount).min(max);
    }

    /// Scrolls up by `amount` lines.
    pub const fn scroll_up(&mut self, amount: usize) {
        self.scroll_y = self.scroll_y.saturating_sub(amount);
    }

    /// Scrolls right by `amount` columns.
    pub const fn scroll_right(&mut self, amount: usize) {
        self.scroll_x = self.scroll_x.saturating_add(amount);
    }

    /// Scrolls left by `amount` columns.
    pub const fn scroll_left(&mut self, amount: usize) {
        self.scroll_x = self.scroll_x.saturating_sub(amount);
    }

    /// Moves focus to next column in Table Designer.
    pub fn next_column(&mut self) {
        if self.blueprint.focused_column + 1 < self.blueprint.columns.len() {
            self.blueprint.focused_column += 1;
        }
    }

    /// Moves focus to previous column in Table Designer.
    pub fn prev_column(&mut self) {
        if self.blueprint.focused_column > 0 {
            self.blueprint.focused_column -= 1;
        }
    }
}

/// Active step in the bulk data import wizard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ImportWizardStep {
    /// Step 1: File selection and sample data preview.
    #[default]
    SourceFile,
    /// Step 2: Interactive column mapping and type validation.
    ColumnMapping,
    /// Step 3: Transactional review and SQL execution.
    Execute,
}

impl ImportWizardStep {
    /// Next step in sequence.
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::SourceFile => Self::ColumnMapping,
            Self::ColumnMapping => Self::Execute,
            Self::Execute => Self::SourceFile,
        }
    }

    /// Previous step in sequence.
    #[must_use]
    pub const fn prev(self) -> Self {
        match self {
            Self::SourceFile => Self::Execute,
            Self::ColumnMapping => Self::SourceFile,
            Self::Execute => Self::ColumnMapping,
        }
    }
}

/// Physical query session and resolved target captured for one import.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportConnection {
    /// Profile generation in which the user session was installed.
    pub generation: u64,
    /// Runtime-minted identity even when the same profile reconnects.
    pub session_id: NoticeSessionId,
    /// PostgreSQL database named by the connected session.
    pub database: String,
    /// Resolved target classification used to admit this import.
    pub environment: Environment,
}

/// One production import waiting for its exact database name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PendingImport {
    pub connection: ImportConnection,
    pub options: crate::transfer::ImportOptions,
    pub typed: String,
}

/// Interactive state for the bulk data import wizard (`028-data-import-export-and-backup`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ImportWizardState {
    /// Active wizard step.
    pub step: ImportWizardStep,
    /// Path to the source file on disk.
    pub file_path: String,
    /// Destination schema.
    pub schema: String,
    /// Destination table.
    pub table: String,
    /// Detected or chosen transfer format.
    pub format: crate::transfer::TransferFormat,
    /// Delimiter byte.
    pub delimiter: u8,
    /// Whether file has header row.
    pub has_header: bool,
    /// String representing NULL.
    pub null_string: String,
    /// Quote character.
    pub quote: u8,
    /// Escape character.
    pub escape: u8,
    /// Loaded file preview and inferred column mappings.
    pub preview: Option<crate::transfer::ImportPreview>,
    /// Selected column index in the column mapping grid.
    pub selected_column_idx: usize,
    /// Whether this is a dry-run test (transaction rollback).
    pub dry_run: bool,
    /// Whether import runs inside transaction.
    pub transactional: bool,
    /// Whether file preview or import execution is currently loading.
    pub loading: bool,
    /// Error message if preview or execution failed.
    pub error: Option<String>,
    /// Result summary of the import.
    pub result: Option<crate::transfer::ImportResult>,
    /// Generated COPY SQL statement for review.
    pub copy_sql: String,
    /// Scroll offset Y.
    pub scroll_y: usize,
}

impl ImportWizardState {
    /// Creates a new import wizard state targeting a schema and table.
    #[must_use]
    pub fn new(schema: String, table: String) -> Self {
        let mut state = Self {
            step: ImportWizardStep::SourceFile,
            file_path: String::new(),
            schema: if schema.is_empty() {
                "public".to_owned()
            } else {
                schema
            },
            table,
            format: crate::transfer::TransferFormat::Csv,
            delimiter: b',',
            has_header: true,
            null_string: String::new(),
            quote: b'"',
            escape: b'"',
            preview: None,
            selected_column_idx: 0,
            dry_run: false,
            transactional: true,
            loading: false,
            error: None,
            result: None,
            copy_sql: String::new(),
            scroll_y: 0,
        };
        state.rebuild_sql();
        state
    }

    /// Rebuilds the preview SQL statement from current mappings and options.
    pub fn rebuild_sql(&mut self) {
        self.copy_sql = crate::transfer::build_copy_from_sql(&self.options());
    }

    /// The exact COPY configuration shown in the wizard.
    pub(crate) fn options(&self) -> crate::transfer::ImportOptions {
        let active_cols: Vec<String> = if let Some(ref prev) = self.preview {
            prev.column_mappings
                .iter()
                .filter(|m| !m.skip)
                .map(|m| m.target_column.clone())
                .collect()
        } else {
            Vec::new()
        };

        crate::transfer::ImportOptions {
            schema: self.schema.clone(),
            table: self.table.clone(),
            file_path: self.file_path.clone(),
            format: self.format,
            delimiter: self.delimiter,
            has_header: self.has_header,
            null_string: self.null_string.clone(),
            quote: self.quote,
            escape: self.escape,
            columns: active_cols,
            dry_run: self.dry_run,
            transactional: self.transactional,
        }
    }

    /// Toggles the skipped state of the selected column mapping.
    pub fn toggle_column_skip(&mut self) {
        if let Some(mapping) = self
            .preview
            .as_mut()
            .and_then(|prev| prev.column_mappings.get_mut(self.selected_column_idx))
        {
            mapping.skip = !mapping.skip;
        }
        self.rebuild_sql();
    }

    /// Moves the column mapping selection index.
    pub fn move_column_selection(&mut self, delta: isize) {
        let count = self.preview.as_ref().map_or(0, |p| p.column_mappings.len());
        if count == 0 {
            self.selected_column_idx = 0;
            return;
        }
        let next = (self.selected_column_idx as isize + delta).clamp(0, count as isize - 1);
        self.selected_column_idx = next as usize;
    }
}

/// Active step inside the routine runner and profiler overlay (`032-plpgsql-routine-runner-and-profiler`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RoutineRunnerStep {
    /// Select routine to test from database.
    #[default]
    Routines,
    /// Configure arguments and run test harness.
    Arguments,
    /// View profiling report, diagnostics, and findings.
    Profile,
}

impl RoutineRunnerStep {
    /// Advances to the next step.
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Routines => Self::Arguments,
            Self::Arguments => Self::Profile,
            Self::Profile => Self::Routines,
        }
    }

    /// Moves to the previous step.
    #[must_use]
    pub const fn prev(self) -> Self {
        match self {
            Self::Routines => Self::Profile,
            Self::Arguments => Self::Routines,
            Self::Profile => Self::Arguments,
        }
    }
}

/// Interactive state for the routine runner and PL/pgSQL profiler (`032-plpgsql-routine-runner-and-profiler`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RoutineRunnerState {
    /// Active tab/step.
    pub step: RoutineRunnerStep,
    /// Discovered routines in catalog.
    pub routines: Vec<crate::routine::RoutineSignature>,
    /// Selected index in the routine list.
    pub selected_routine_idx: usize,
    /// Arguments mapped by parameter name.
    pub args: std::collections::BTreeMap<String, String>,
    /// Selected argument index when editing arguments.
    pub selected_arg_idx: usize,
    /// Text buffer when typing an argument value.
    pub arg_input: String,
    /// Whether to wrap execution in an automatic savepoint rollback.
    pub rollback_savepoint: bool,
    /// Last execution result.
    pub last_result: Option<crate::routine::RoutineExecutionResult>,
    /// Last profiling report.
    pub last_profile: Option<crate::routine::RoutineProfileReport>,
    /// Whether a background operation is currently running.
    pub executing: bool,
    /// Scroll offset for viewing long reports or source code.
    pub scroll_y: u16,
    /// Filter string for fuzzy-matching routine names.
    pub filter: String,
}

impl RoutineRunnerState {
    /// Selected routine signature if available.
    #[must_use]
    pub fn selected_routine(&self) -> Option<&crate::routine::RoutineSignature> {
        let filtered = self.filtered_routines();
        filtered.get(self.selected_routine_idx).copied()
    }

    /// Filtered list of routines matching the filter text.
    #[must_use]
    pub fn filtered_routines(&self) -> Vec<&crate::routine::RoutineSignature> {
        if self.filter.trim().is_empty() {
            self.routines.iter().collect()
        } else {
            let needle = self.filter.to_lowercase();
            self.routines
                .iter()
                .filter(|r| {
                    r.name.to_lowercase().contains(&needle)
                        || r.schema.to_lowercase().contains(&needle)
                })
                .collect()
        }
    }

    /// Moves selection in the filtered routine list.
    pub fn move_routine_selection(&mut self, delta: isize) {
        let total = self.filtered_routines().len();
        if total == 0 {
            self.selected_routine_idx = 0;
            return;
        }
        let current = self.selected_routine_idx as isize;
        let new_idx = (current + delta).clamp(0, total as isize - 1);
        self.selected_routine_idx = new_idx as usize;
    }

    /// Moves selection in the argument list.
    pub fn move_arg_selection(&mut self, delta: isize) {
        let total = self
            .selected_routine()
            .map_or(0, |r| r.input_params().len());
        if total == 0 {
            self.selected_arg_idx = 0;
            return;
        }
        let current = self.selected_arg_idx as isize;
        let new_idx = (current + delta).clamp(0, total as isize - 1);
        self.selected_arg_idx = new_idx as usize;
    }
}

/// Active step / tab in the maintenance operations and pub/sub monitor overlay (`033-listen-notify-and-maintenance-ops`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MaintenanceOpsStep {
    /// Pub/Sub channel listener and notification dispatcher.
    #[default]
    PubSub,
    /// Maintenance operations runner (VACUUM, ANALYZE, REINDEX).
    Maintain,
    /// Live progress telemetry monitor (pg_stat_progress_*).
    Progress,
}

impl MaintenanceOpsStep {
    /// Advances to the next tab.
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::PubSub => Self::Maintain,
            Self::Maintain => Self::Progress,
            Self::Progress => Self::PubSub,
        }
    }

    /// Moves to the previous tab.
    #[must_use]
    pub const fn prev(self) -> Self {
        match self {
            Self::PubSub => Self::Progress,
            Self::Maintain => Self::PubSub,
            Self::Progress => Self::Maintain,
        }
    }
}

/// Interactive state for the maintenance operations runner and pub/sub monitor (`033-listen-notify-and-maintenance-ops`).
#[derive(Debug, Clone, PartialEq)]
pub struct MaintenanceOpsState {
    /// Active tab/step.
    pub step: MaintenanceOpsStep,
    /// Subscribed channel names.
    pub channels: Vec<String>,
    /// Notification channel input buffer.
    pub channel_input: String,
    /// Notification payload input buffer.
    pub payload_input: String,
    /// Editing mode for input buffers: 0 = channel, 1 = payload, 2 = target table.
    pub edit_focus: usize,
    /// Log of received notifications.
    pub notifications: Vec<crate::ops::PubSubNotification>,
    /// Selected index in notification log or progress table.
    pub selected_idx: usize,
    /// Maintenance operation kind (Vacuum, Analyze, Reindex).
    pub maintenance_kind: crate::ops::MaintenanceKind,
    /// Target relation or table name for maintenance.
    pub target_relation: String,
    /// Whether user confirmed maintenance on production.
    pub confirm_production: bool,
    /// Last maintenance execution result.
    pub last_result: Option<crate::ops::MaintenanceResult>,
    /// Live progress entries from pg_stat_progress_*.
    pub progress: Vec<crate::ops::MaintenanceProgressEntry>,
    /// Whether a background operation is currently running.
    pub executing: bool,
    /// Scroll offset for viewing logs or telemetry.
    pub scroll_y: u16,
    /// Status message displayed in the overlay footer.
    pub status_message: Option<String>,
}

impl Default for MaintenanceOpsState {
    fn default() -> Self {
        Self {
            step: MaintenanceOpsStep::PubSub,
            channels: Vec::new(),
            channel_input: "events".to_owned(),
            payload_input: String::new(),
            edit_focus: 0,
            notifications: Vec::new(),
            selected_idx: 0,
            maintenance_kind: crate::ops::MaintenanceKind::Vacuum {
                full: false,
                freeze: false,
                verbose: true,
                analyze: true,
            },
            target_relation: String::new(),
            confirm_production: false,
            last_result: None,
            progress: Vec::new(),
            executing: false,
            scroll_y: 0,
            status_message: None,
        }
    }
}

impl MaintenanceOpsState {
    /// Scroll down.
    pub fn scroll_down(&mut self) {
        self.scroll_y = self.scroll_y.saturating_add(1);
    }

    /// Scroll up.
    pub fn scroll_up(&mut self) {
        self.scroll_y = self.scroll_y.saturating_sub(1);
    }

    /// Moves selection in the notification log or progress list.
    pub fn move_selection(&mut self, delta: isize) {
        let total = match self.step {
            MaintenanceOpsStep::PubSub => self.notifications.len(),
            MaintenanceOpsStep::Maintain => 0,
            MaintenanceOpsStep::Progress => self.progress.len(),
        };
        if total == 0 {
            self.selected_idx = 0;
            return;
        }
        let current = self.selected_idx as isize;
        let new_idx = (current + delta).clamp(0, total as isize - 1);
        self.selected_idx = new_idx as usize;
    }

    /// Cycles the maintenance operation kind.
    pub fn cycle_maintenance_kind(&mut self) {
        self.maintenance_kind = match self.maintenance_kind {
            crate::ops::MaintenanceKind::Vacuum { full: false, .. } => {
                crate::ops::MaintenanceKind::Vacuum {
                    full: true,
                    freeze: false,
                    verbose: true,
                    analyze: true,
                }
            }
            crate::ops::MaintenanceKind::Vacuum { full: true, .. } => {
                crate::ops::MaintenanceKind::Analyze { verbose: true }
            }
            crate::ops::MaintenanceKind::Analyze { .. } => crate::ops::MaintenanceKind::Reindex {
                concurrently: true,
                scope: crate::ops::ReindexScope::Table,
            },
            crate::ops::MaintenanceKind::Reindex {
                concurrently: true,
                scope,
            } => crate::ops::MaintenanceKind::Reindex {
                concurrently: false,
                scope,
            },
            crate::ops::MaintenanceKind::Reindex {
                concurrently: false,
                ..
            } => crate::ops::MaintenanceKind::Cluster { verbose: true },
            crate::ops::MaintenanceKind::Cluster { .. } => crate::ops::MaintenanceKind::Vacuum {
                full: false,
                freeze: false,
                verbose: true,
                analyze: true,
            },
        };
    }
}

/// State for the Query Performance and pg_stat_statements Forensics overlay.
#[derive(Debug, Clone, PartialEq)]
pub struct QueryPerformanceState {
    /// Telemetry snapshot gathered from pg_stat_statements.
    pub snapshot: Option<crate::perf::QueryPerformanceSnapshot>,
    /// Active sort order.
    pub sort_order: crate::perf::PerformanceSortOrder,
    /// Selected index in the ranked statements list.
    pub selected_idx: usize,
    /// Whether an asynchronous fetch is currently in flight.
    pub loading: bool,
    /// Whether reset confirmation prompt is active.
    pub confirm_reset: bool,
    /// Status or diagnostic message shown in the overlay footer.
    pub status_message: Option<String>,
}

impl Default for QueryPerformanceState {
    fn default() -> Self {
        Self {
            snapshot: None,
            sort_order: crate::perf::PerformanceSortOrder::TotalTime,
            selected_idx: 0,
            loading: true,
            confirm_reset: false,
            status_message: None,
        }
    }
}

impl QueryPerformanceState {
    /// Number of ranked statements available.
    #[must_use]
    pub fn len(&self) -> usize {
        self.snapshot.as_ref().map_or(0, |s| s.queries.len())
    }

    /// Whether there are no statements available.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The currently selected entry, if any.
    #[must_use]
    pub fn selected_entry(&self) -> Option<&crate::perf::StatStatementEntry> {
        self.snapshot
            .as_ref()
            .and_then(|s| s.queries.get(self.selected_idx))
    }

    /// Moves selection in the ranked statement list.
    pub fn move_selection(&mut self, delta: isize) {
        let total = self.len();
        if total == 0 {
            self.selected_idx = 0;
            return;
        }
        let current = self.selected_idx as isize;
        let new_idx = (current + delta).clamp(0, total as isize - 1);
        self.selected_idx = new_idx as usize;
    }
}

/// State for the Index Advisor and Bloat Forensics overlay.
#[derive(Debug, Clone, PartialEq)]
pub struct IndexAdvisorState {
    /// Findings gathered from catalog analysis.
    pub report: Option<crate::advisor::AdvisorReport>,
    /// Active tab in the advisor overlay (0: Redundant/Unused, 1: B-Tree Bloat, 2: Unindexed FKs).
    pub active_tab: usize,
    /// Selected item index in the active tab.
    pub selected_idx: usize,
    /// Whether an asynchronous fetch is currently in flight.
    pub loading: bool,
    /// Status or diagnostic message shown in footer.
    pub status_message: Option<String>,
}

impl Default for IndexAdvisorState {
    fn default() -> Self {
        Self {
            report: None,
            active_tab: 0,
            selected_idx: 0,
            loading: true,
            status_message: None,
        }
    }
}

impl IndexAdvisorState {
    /// Items count for the currently active tab.
    #[must_use]
    pub fn current_tab_len(&self) -> usize {
        let Some(report) = &self.report else {
            return 0;
        };
        match self.active_tab {
            0 => report.redundant_indexes.len(),
            1 => report.bloated_indexes.len(),
            2 => report.unindexed_foreign_keys.len(),
            _ => 0,
        }
    }

    /// Moves selection up or down within the current tab.
    pub fn move_selection(&mut self, delta: isize) {
        let total = self.current_tab_len();
        if total == 0 {
            self.selected_idx = 0;
            return;
        }
        let current = self.selected_idx as isize;
        let new_idx = (current + delta).clamp(0, total as isize - 1);
        self.selected_idx = new_idx as usize;
    }

    /// Switches the active tab (0: Redundant, 1: Bloat, 2: Unindexed).
    pub fn set_tab(&mut self, tab: usize) {
        self.active_tab = tab.min(2);
        self.selected_idx = 0;
    }

    /// Gets the suggested remediation SQL for the selected item in the active tab.
    #[must_use]
    pub fn selected_sql(&self) -> Option<String> {
        let report = self.report.as_ref()?;
        match self.active_tab {
            0 => report
                .redundant_indexes
                .get(self.selected_idx)
                .map(|r| r.suggested_drop_sql()),
            1 => report
                .bloated_indexes
                .get(self.selected_idx)
                .map(|b| b.suggested_reindex_sql()),
            2 => report
                .unindexed_foreign_keys
                .get(self.selected_idx)
                .map(|fk| fk.suggested_create_sql()),
            _ => None,
        }
    }
}

/// State for the Transaction, WAL, and Checkpointer Observability overlay.
#[derive(Debug, Clone, PartialEq)]
pub struct WalObservabilityState {
    /// Telemetry gathered from catalog analysis.
    pub report: Option<crate::telemetry::TelemetryReport>,
    /// Active tab in the observability overlay (0: Checkpoints & WAL, 1: Transactions & XID Freeze, 2: Replication & Standbys).
    pub active_tab: usize,
    /// Selected item index in the active tab (e.g. database list or standby list).
    pub selected_idx: usize,
    /// Whether an asynchronous fetch is currently in flight.
    pub loading: bool,
    /// Status or diagnostic message shown in footer.
    pub status_message: Option<String>,
}

impl Default for WalObservabilityState {
    fn default() -> Self {
        Self {
            report: None,
            active_tab: 0,
            selected_idx: 0,
            loading: true,
            status_message: None,
        }
    }
}

impl WalObservabilityState {
    /// Items count for the currently active tab.
    #[must_use]
    pub fn current_tab_len(&self) -> usize {
        let Some(report) = &self.report else {
            return 0;
        };
        match self.active_tab {
            0 => 1,
            1 => report.databases.len().max(report.xid_freeze.len()),
            2 => report.standbys.len(),
            _ => 0,
        }
    }

    /// Moves selection up or down within the current tab.
    pub fn move_selection(&mut self, delta: isize) {
        let total = self.current_tab_len();
        if total == 0 {
            self.selected_idx = 0;
            return;
        }
        let current = self.selected_idx as isize;
        let new_idx = (current + delta).clamp(0, total as isize - 1);
        self.selected_idx = new_idx as usize;
    }

    /// Switches the active tab (0: Checkpoints & WAL, 1: Transactions & XID Freeze, 2: Replication & Standbys).
    pub fn set_tab(&mut self, tab: usize) {
        self.active_tab = tab.min(2);
        self.selected_idx = 0;
    }
}

/// State for the Connection Pool & PgBouncer Telemetry overlay.
#[derive(Debug, Clone, PartialEq)]
pub struct ConnectionPoolState {
    /// Telemetry gathered from catalog or pooler.
    pub report: Option<crate::pool::PoolReport>,
    /// Active tab in the overlay (0: Pool Allocations & Queues, 1: Server Connections & Saturation, 2: Client Wait Forensics).
    pub active_tab: usize,
    /// Selected item index in the active tab.
    pub selected_idx: usize,
    /// Whether an asynchronous fetch is currently in flight.
    pub loading: bool,
    /// Status or diagnostic message shown in footer.
    pub status_message: Option<String>,
}

impl Default for ConnectionPoolState {
    fn default() -> Self {
        Self {
            report: None,
            active_tab: 0,
            selected_idx: 0,
            loading: true,
            status_message: None,
        }
    }
}

impl ConnectionPoolState {
    /// Items count for the currently active tab.
    #[must_use]
    pub fn current_tab_len(&self) -> usize {
        let Some(report) = &self.report else {
            return 0;
        };
        match self.active_tab {
            0 => report.pools.len(),
            1 => 1,
            2 => report.waiting_clients.len(),
            _ => 0,
        }
    }

    /// Moves selection up or down within the current tab.
    pub fn move_selection(&mut self, delta: isize) {
        let total = self.current_tab_len();
        if total == 0 {
            self.selected_idx = 0;
            return;
        }
        let current = self.selected_idx as isize;
        let new_idx = (current + delta).clamp(0, total as isize - 1);
        self.selected_idx = new_idx as usize;
    }

    /// Switches the active tab (0: Pools & Queues, 1: Server Connections & Saturation, 2: Client Wait Forensics).
    pub fn set_tab(&mut self, tab: usize) {
        self.active_tab = tab.min(2);
        self.selected_idx = 0;
    }

    /// SQL query text of the currently selected waiting client, if any.
    #[must_use]
    pub fn selected_query(&self) -> Option<String> {
        let Some(report) = &self.report else {
            return None;
        };
        if self.active_tab == 2 {
            report
                .waiting_clients
                .get(self.selected_idx)
                .map(|w| w.query.clone())
        } else {
            None
        }
    }
}

/// State for the Data Masking & Anonymization Rules overlay.
#[derive(Debug, Clone, PartialEq)]
pub struct DataMaskingState {
    /// Masking analysis gathered from catalog.
    pub report: Option<crate::mask::MaskReport>,
    /// Active tab in the overlay (0: Sensitive Columns & Rules, 1: Generated Masked Views SQL).
    pub active_tab: usize,
    /// Selected item index in the active tab.
    pub selected_idx: usize,
    /// View name prefix used for generated views (default "masked_").
    pub view_prefix: String,
    /// Whether an asynchronous fetch is currently in flight.
    pub loading: bool,
    /// Status or diagnostic message shown in footer.
    pub status_message: Option<String>,
}

impl Default for DataMaskingState {
    fn default() -> Self {
        Self {
            report: None,
            active_tab: 0,
            selected_idx: 0,
            view_prefix: "masked_".to_string(),
            loading: true,
            status_message: None,
        }
    }
}

impl DataMaskingState {
    /// Items count for the currently active tab.
    #[must_use]
    pub fn current_tab_len(&self) -> usize {
        let Some(report) = &self.report else {
            return 0;
        };
        match self.active_tab {
            0 => report.candidates.len(),
            1 => report.policies.len(),
            _ => 0,
        }
    }

    /// Moves selection up or down within the current tab.
    pub fn move_selection(&mut self, delta: isize) {
        let total = self.current_tab_len();
        if total == 0 {
            self.selected_idx = 0;
            return;
        }
        let current = self.selected_idx as isize;
        let new_idx = (current + delta).clamp(0, total as isize - 1);
        self.selected_idx = new_idx as usize;
    }

    /// Switches the active tab (0: Sensitive Columns & Rules, 1: Generated Masked Views SQL).
    pub fn set_tab(&mut self, tab: usize) {
        self.active_tab = tab.min(1);
        self.selected_idx = 0;
    }

    /// Cycles the masking rule for the currently selected candidate column.
    pub fn cycle_rule(&mut self) {
        let Some(report) = &mut self.report else {
            return;
        };
        if report.candidates.is_empty() || self.selected_idx >= report.candidates.len() {
            return;
        }
        let cand = &mut report.candidates[self.selected_idx];
        cand.rule = match &cand.rule {
            crate::mask::MaskRule::Email => crate::mask::MaskRule::Phone,
            crate::mask::MaskRule::Phone => crate::mask::MaskRule::FullName,
            crate::mask::MaskRule::FullName => crate::mask::MaskRule::HashHex {
                salt: "ignatius_salt".to_string(),
            },
            crate::mask::MaskRule::HashHex { .. } => crate::mask::MaskRule::Redact {
                text: "[REDACTED]".to_string(),
            },
            crate::mask::MaskRule::Redact { .. } => crate::mask::MaskRule::Nullify,
            crate::mask::MaskRule::Nullify => crate::mask::MaskRule::RandomNumber {
                min: 1000,
                max: 9999,
            },
            crate::mask::MaskRule::RandomNumber { .. } => crate::mask::MaskRule::Email,
            crate::mask::MaskRule::CustomSql(_) => crate::mask::MaskRule::Email,
        };
        let rule_name = cand.rule.name();
        let col_name = cand.column.clone();
        let schema_name = cand.schema.clone();
        let table_name = cand.table.clone();
        let new_rule = cand.rule.clone();

        if let Some(policy) = report
            .policies
            .iter_mut()
            .find(|p| p.schema == schema_name && p.table == table_name)
        {
            if let Some((_, r)) = policy.column_rules.iter_mut().find(|(c, _)| c == &col_name) {
                *r = new_rule;
            } else {
                policy.column_rules.push((col_name.clone(), new_rule));
            }
        }

        self.status_message = Some(format!(
            "Assigned rule {rule_name} to {schema_name}.{table_name}.{col_name}"
        ));
    }

    /// Generated SQL definition for all masked views.
    #[must_use]
    pub fn generated_sql(&self) -> Option<String> {
        let report = self.report.as_ref()?;
        Some(report.render_sql(&self.view_prefix))
    }
}

/// State for the Cross-Database Copy and Sync Pipelines overlay.
#[derive(Debug, Clone, PartialEq)]
pub struct DataSyncState {
    /// Dependency graph gathered from catalog.
    pub graph: Option<crate::sync::DependencyGraph>,
    /// Calculated sync plan (topological insertion order and truncation order).
    pub plan: Option<crate::sync::SyncPlan>,
    /// Active tab in the overlay (0: Dependency Graph & Sync Plan, 1: Live Transfer Telemetry).
    pub active_tab: usize,
    /// Selected item index in the active tab.
    pub selected_idx: usize,
    /// Target connection target or string.
    pub target_endpoint: String,
    /// Whether dry-run mode is enabled (default true for safety).
    pub dry_run: bool,
    /// Whether truncate target tables before insertion is requested.
    pub truncate: bool,
    /// Whether an asynchronous operation is in flight.
    pub loading: bool,
    /// Status or diagnostic message shown in footer.
    pub status_message: Option<String>,
    /// Live sync report or telemetry, if executed.
    pub sync_report: Option<crate::sync::SyncReport>,
}

impl Default for DataSyncState {
    fn default() -> Self {
        Self {
            graph: None,
            plan: None,
            active_tab: 0,
            selected_idx: 0,
            target_endpoint: String::new(),
            dry_run: true,
            truncate: false,
            loading: true,
            status_message: None,
            sync_report: None,
        }
    }
}

impl DataSyncState {
    /// Items count for the currently active tab.
    #[must_use]
    pub fn current_tab_len(&self) -> usize {
        match self.active_tab {
            0 => self.plan.as_ref().map_or(0, |p| p.insert_order.len()),
            1 => self
                .sync_report
                .as_ref()
                .map_or(0, |r| r.table_progress.len()),
            _ => 0,
        }
    }

    /// Moves selection up or down within the current tab.
    pub fn move_selection(&mut self, delta: isize) {
        let total = self.current_tab_len();
        if total == 0 {
            self.selected_idx = 0;
            return;
        }
        let current = self.selected_idx as isize;
        let next = (current + delta).clamp(0, total.saturating_sub(1) as isize);
        self.selected_idx = next as usize;
    }

    /// Switches the active tab and resets selected index.
    pub fn set_tab(&mut self, tab: usize) {
        self.active_tab = tab;
        self.selected_idx = 0;
    }

    /// Toggles the dry-run safety flag.
    pub fn toggle_dry_run(&mut self) {
        self.dry_run = !self.dry_run;
        self.status_message = Some(if self.dry_run {
            "Dry-run mode ENABLED: verification queries only (no writes)".to_string()
        } else {
            "Dry-run mode DISABLED: live streaming writes enabled".to_string()
        });
    }

    /// Toggles the truncate-before-insertion flag.
    pub fn toggle_truncate(&mut self) {
        self.truncate = !self.truncate;
        self.status_message = Some(if self.truncate {
            "Truncate target tables ENABLED (in reverse topological order)".to_string()
        } else {
            "Truncate target tables DISABLED".to_string()
        });
    }
}

/// State for the Synthetic Test Fixture Generator overlay.
#[derive(Debug, Clone, PartialEq)]
pub struct SyntheticSeedState {
    /// Inferred table generators from catalog.
    pub tables: Vec<crate::seed::TableGenerator>,
    /// Number of rows to generate per table.
    pub rows_per_table: usize,
    /// 64-bit seed used for reproducible PRNG.
    pub seed: u64,
    /// Active tab in the overlay (0: Table Generator Strategies, 1: Generated Fixture Preview).
    pub active_tab: usize,
    /// Selected item index in the active tab.
    pub selected_idx: usize,
    /// Whether an asynchronous operation is in flight.
    pub loading: bool,
    /// Status or diagnostic message shown in footer.
    pub status_message: Option<String>,
    /// Last generated fixture report.
    pub report: Option<crate::seed::FixtureReport>,
}

impl Default for SyntheticSeedState {
    fn default() -> Self {
        Self {
            tables: Vec::new(),
            rows_per_table: 10,
            seed: 42,
            active_tab: 0,
            selected_idx: 0,
            loading: true,
            status_message: None,
            report: None,
        }
    }
}

impl SyntheticSeedState {
    /// Generates fixture report using current tables, row count, and seed.
    pub fn regenerate(&mut self) {
        if !self.tables.is_empty() {
            self.report = Some(crate::seed::generate_fixtures(
                &self.tables,
                self.rows_per_table,
                self.seed,
            ));
        }
    }

    /// Items count for the currently active tab.
    #[must_use]
    pub fn current_tab_len(&self) -> usize {
        match self.active_tab {
            0 => self.tables.len(),
            1 => self.report.as_ref().map_or(0, |r| r.tables.len()),
            _ => 0,
        }
    }

    /// Moves selection up or down within the current tab.
    pub fn move_selection(&mut self, delta: isize) {
        let total = self.current_tab_len();
        if total == 0 {
            self.selected_idx = 0;
            return;
        }
        let current = self.selected_idx as isize;
        let next = (current + delta).clamp(0, total.saturating_sub(1) as isize);
        self.selected_idx = next as usize;
    }

    /// Switches the active tab and resets selected index.
    pub fn set_tab(&mut self, tab: usize) {
        self.active_tab = tab;
        self.selected_idx = 0;
    }

    /// Cycles the seed and regenerates fixtures.
    pub fn cycle_seed(&mut self) {
        self.seed = self.seed.wrapping_add(1);
        if self.seed == 0 {
            self.seed = 1;
        }
        self.regenerate();
        self.status_message = Some(format!("PRNG Seed updated to: {}", self.seed));
    }

    /// Cycles the target row count per table and regenerates.
    pub fn cycle_rows(&mut self) {
        self.rows_per_table = match self.rows_per_table {
            5 => 10,
            10 => 25,
            25 => 50,
            50 => 100,
            _ => 5,
        };
        self.regenerate();
        self.status_message = Some(format!(
            "Row target updated to: {} rows/table",
            self.rows_per_table
        ));
    }
}

/// Interactive SQL snippet and scratchpad library overlay state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnippetLibraryState {
    /// Pure snippet library holding built-in and user-loaded snippets.
    pub library: crate::snippet::SnippetLibrary,
    /// Optional directory where user snippets are persisted.
    pub directory: Option<std::path::PathBuf>,
    /// Status or notification message shown in footer.
    pub status_message: Option<String>,
}

impl Default for SnippetLibraryState {
    fn default() -> Self {
        Self::new()
    }
}

impl SnippetLibraryState {
    /// Creates a new snippet library state with default built-ins.
    #[must_use]
    pub fn new() -> Self {
        Self {
            library: crate::snippet::SnippetLibrary::new(),
            directory: None,
            status_message: None,
        }
    }

    /// Creates a new snippet library state loaded from a user snippets directory.
    #[must_use]
    pub fn with_directory(dir: std::path::PathBuf) -> Self {
        let mut lib = crate::snippet::SnippetLibrary::new();
        lib.load_user_directory(&dir);
        Self {
            library: lib,
            directory: Some(dir),
            status_message: None,
        }
    }
}

/// Interactive state of the crash recovery notification banner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRecoveryBanner {
    /// The loaded crash recovery journal.
    pub journal: crate::recovery::RecoveryJournal,
    /// Path to the recovery directory.
    pub recovery_dir: std::path::PathBuf,
}

impl SessionRecoveryBanner {
    /// Creates a new recovery banner from a journal and directory.
    #[must_use]
    pub fn new(
        journal: crate::recovery::RecoveryJournal,
        recovery_dir: std::path::PathBuf,
    ) -> Self {
        Self {
            journal,
            recovery_dir,
        }
    }
}

/// Controls how statement execution timing is displayed in the status chrome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TimingDisplay {
    /// Quiet-chrome default: show elapsed duration once a query reaches 200ms.
    #[default]
    Threshold200ms,
    /// Always show completed query elapsed duration (`\timing on`).
    Always,
    /// Suppress idle completed elapsed duration (`\timing off`).
    Off,
}

/// State for an active bounded `\watch` periodic execution loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchState {
    /// SQL query executed at each interval tick.
    pub sql: String,
    /// Interval in seconds between ticks (`1..=3600`).
    pub interval_secs: u64,
    /// 1-based execution count for the active watch session.
    pub iteration: u64,
}

/// State for the Logical Replication & Slots Manager overlay (`051`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogicalReplicationState {
    /// Loaded replication snapshot from PostgreSQL catalog.
    pub report: Option<crate::replication::ReplicationReport>,
    /// Active tab (0: Slots, 1: Publications, 2: Subscriptions).
    pub active_tab: usize,
    /// Selected index within the active tab.
    pub selected_idx: usize,
    /// Whether an asynchronous catalog fetch is currently in flight.
    pub loading: bool,
    /// Status or feedback message shown in overlay footer.
    pub status_message: Option<String>,
}

impl Default for LogicalReplicationState {
    fn default() -> Self {
        Self {
            report: None,
            active_tab: 0,
            selected_idx: 0,
            loading: true,
            status_message: None,
        }
    }
}

impl LogicalReplicationState {
    /// Number of items visible in the current tab.
    #[must_use]
    pub fn current_tab_len(&self) -> usize {
        let Some(report) = &self.report else {
            return 0;
        };
        match self.active_tab {
            0 => report.slots.len(),
            1 => report.publications.len(),
            2 => report.subscriptions.len(),
            _ => 0,
        }
    }

    /// Moves selection within the current tab bounds.
    pub fn move_selection(&mut self, delta: isize) {
        let total = self.current_tab_len();
        if total == 0 {
            self.selected_idx = 0;
            return;
        }
        let next = (self.selected_idx as isize + delta).clamp(0, total as isize - 1);
        self.selected_idx = next as usize;
    }

    /// Switches active tab and resets row selection.
    pub fn set_tab(&mut self, tab: usize) {
        self.active_tab = tab.min(2);
        self.selected_idx = 0;
    }

    /// Generates a reviewed SQL statement for the selected slot/publication/subscription.
    #[must_use]
    pub fn selected_reviewed_sql(&self) -> Option<(String, String)> {
        let report = self.report.as_ref()?;
        match self.active_tab {
            0 => report.slots.get(self.selected_idx).map(|slot| {
                (
                    format!("Drop slot '{}'", slot.slot_name),
                    crate::replication::drop_slot_sql(&slot.slot_name),
                )
            }),
            1 => report.publications.get(self.selected_idx).map(|pub_| {
                (
                    format!("Publication '{}' template", pub_.name),
                    crate::replication::create_publication_sql(&pub_.name, pub_.all_tables),
                )
            }),
            2 => report.subscriptions.get(self.selected_idx).map(|sub| {
                (
                    format!(
                        "{} subscription '{}'",
                        if sub.enabled { "Disable" } else { "Enable" },
                        sub.name
                    ),
                    crate::replication::alter_subscription_toggle_sql(&sub.name, !sub.enabled),
                )
            }),
            _ => None,
        }
    }
}

/// State for the Server Settings, HBA, Extensions, Tablespaces & FDW overlay (`052`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterConfigState {
    /// Loaded cluster report from PostgreSQL catalog.
    pub report: Option<crate::cluster::ClusterReport>,
    /// Active tab (0: Settings, 1: HBA Auth, 2: Extensions, 3: Tablespaces, 4: FDWs).
    pub active_tab: usize,
    /// Selected index within the active filtered tab.
    pub selected_idx: usize,
    /// When true, narrows to non-default settings, HBA risks, or upgradable/installed extensions.
    pub modified_only: bool,
    /// Optional substring filter applied within the overlay.
    pub filter_text: String,
    /// Whether an asynchronous catalog fetch is currently in flight.
    pub loading: bool,
    /// Status or feedback message shown in overlay footer.
    pub status_message: Option<String>,
}

impl Default for ClusterConfigState {
    fn default() -> Self {
        Self {
            report: None,
            active_tab: 0,
            selected_idx: 0,
            modified_only: false,
            filter_text: String::new(),
            loading: true,
            status_message: None,
        }
    }
}

impl ClusterConfigState {
    /// Number of items visible in the current filtered tab.
    #[must_use]
    pub fn current_tab_len(&self) -> usize {
        let Some(report) = &self.report else {
            return 0;
        };
        let f = (!self.filter_text.trim().is_empty()).then_some(self.filter_text.as_str());
        match self.active_tab {
            0 => report.filtered_settings(self.modified_only, f).len(),
            1 => report.filtered_hba_rules(f).len(),
            2 => report.filtered_extensions(f).len(),
            3 => report.filtered_tablespaces(f).len(),
            4 => report.filtered_fdws(f).len(),
            _ => 0,
        }
    }

    /// Moves selection within the current filtered tab bounds.
    pub fn move_selection(&mut self, delta: isize) {
        let total = self.current_tab_len();
        if total == 0 {
            self.selected_idx = 0;
            return;
        }
        let next = (self.selected_idx as isize + delta).clamp(0, total as isize - 1);
        self.selected_idx = next as usize;
    }

    /// Switches active tab and resets row selection.
    pub fn set_tab(&mut self, tab: usize) {
        self.active_tab = tab.min(4);
        self.selected_idx = 0;
    }

    /// Generates a reviewed SQL statement for the selected cluster configuration item.
    #[must_use]
    pub fn selected_reviewed_sql(&self) -> Option<(String, String)> {
        let report = self.report.as_ref()?;
        let f = (!self.filter_text.trim().is_empty()).then_some(self.filter_text.as_str());
        match self.active_tab {
            0 => report
                .filtered_settings(self.modified_only, f)
                .get(self.selected_idx)
                .map(|s| {
                    let reload = if s.context == "sighup" {
                        format!("\n{}", crate::cluster::reload_conf_sql())
                    } else {
                        String::new()
                    };
                    (
                        format!("Reviewed ALTER SYSTEM for '{}'", s.name),
                        format!(
                            "{}{reload}",
                            crate::cluster::alter_system_sql(&s.name, &s.setting)
                        ),
                    )
                }),
            2 => report
                .filtered_extensions(f)
                .get(self.selected_idx)
                .map(|ext| {
                    if let (Some(_installed), Some(default_ver)) =
                        (&ext.installed_version, &ext.default_version)
                    {
                        if ext.upgrade_available {
                            (
                                format!("Upgrade extension '{}'", ext.name),
                                crate::cluster::alter_extension_update_sql(&ext.name, default_ver),
                            )
                        } else {
                            (
                                format!("Extension '{}' DDL", ext.name),
                                crate::cluster::create_extension_sql(&ext.name),
                            )
                        }
                    } else {
                        (
                            format!("Install extension '{}'", ext.name),
                            crate::cluster::create_extension_sql(&ext.name),
                        )
                    }
                }),
            3 => report
                .filtered_tablespaces(f)
                .get(self.selected_idx)
                .map(|ts| {
                    (
                        format!("Tablespace '{}' DDL", ts.name),
                        crate::cluster::create_tablespace_sql(ts),
                    )
                }),
            4 => report.filtered_fdws(f).get(self.selected_idx).map(|fdw| {
                let mut sql = crate::cluster::create_fdw_sql(fdw);
                for srv in &fdw.servers {
                    sql.push('\n');
                    sql.push_str(&crate::cluster::create_server_sql(srv, &fdw.name));
                }
                (format!("Foreign Data Wrapper '{}' DDL", fdw.name), sql)
            }),
            _ => None,
        }
    }
}

/// State for the Keyset-Paginated Data Browser & Batch CRUD overlay (`053`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataBrowserState {
    /// Active target schema.
    pub schema: String,
    /// Active target relation.
    pub table: String,
    /// Current keyset browse request.
    pub request: crate::crud::BrowseRequest,
    /// Loaded browse page, if available.
    pub page: Option<crate::crud::BrowsePage>,
    /// Selected row index within the visible page.
    pub selected_row: usize,
    /// Selected column index within the visible page.
    pub selected_col: usize,
    /// Whether an asynchronous browse query is in flight.
    pub loading: bool,
    /// Status or diagnostic message shown in overlay footer.
    pub status_message: Option<String>,
}

impl DataBrowserState {
    /// Initializes a new loading state for `schema.table`.
    #[must_use]
    pub fn new(schema: impl Into<String>, table: impl Into<String>) -> Self {
        let schema = schema.into();
        let table = table.into();
        let request = crate::crud::BrowseRequest {
            schema: schema.clone(),
            table: table.clone(),
            columns: Vec::new(),
            pk_columns: Vec::new(),
            sort_column: None,
            sort_order: crate::crud::SortOrder::Asc,
            page_size: crate::crud::DEFAULT_PAGE_SIZE,
            cursor: None,
            direction: crate::crud::PageDirection::Next,
            filter_predicate: None,
        };
        Self {
            schema,
            table,
            request,
            page: None,
            selected_row: 0,
            selected_col: 0,
            loading: true,
            status_message: None,
        }
    }

    /// Moves selected row within bounds of the loaded page.
    pub fn move_row(&mut self, delta: isize) {
        let total = self.page.as_ref().map_or(0, |p| p.rows.len());
        if total == 0 {
            self.selected_row = 0;
            return;
        }
        let next = (self.selected_row as isize + delta).clamp(0, total as isize - 1);
        self.selected_row = next as usize;
    }

    /// Moves selected column within bounds of the loaded page.
    pub fn move_col(&mut self, delta: isize) {
        let total = self.page.as_ref().map_or(0, |p| p.columns.len());
        if total == 0 {
            self.selected_col = 0;
            return;
        }
        let next = (self.selected_col as isize + delta).clamp(0, total as isize - 1);
        self.selected_col = next as usize;
    }

    /// Synthesizes a reviewed transactional CRUD script for the selected row/cell.
    pub fn reviewed_crud_sql(&self, delete_row: bool) -> Result<(String, String), String> {
        let Some(page) = &self.page else {
            return Err("No browse page loaded yet.".to_owned());
        };
        if page.rows.is_empty() {
            if delete_row {
                return Err("Cannot generate DELETE on an empty page.".to_owned());
            }
            let values: Vec<(String, Option<String>)> =
                page.columns.iter().map(|c| (c.clone(), None)).collect();
            let plan = crate::crud::BatchCrudPlan {
                schema: page.schema.clone(),
                table: page.table.clone(),
                pk_columns: page.pk_columns.clone(),
                mutations: vec![crate::crud::RowMutation::Insert { values }],
                dry_run_rollback: false,
            };
            let sql = plan.build_reviewed_sql().map_err(|d| d.headline)?;
            return Ok((
                format!("Reviewed INSERT into {}.{}", page.schema, page.table),
                sql,
            ));
        }
        if page.pk_columns.is_empty() {
            return Err(
                "Table has no primary key; reviewed UPDATE/DELETE is disabled for safety."
                    .to_owned(),
            );
        }
        let row = &page.rows[self.selected_row.min(page.rows.len() - 1)];
        let mut pk_match = Vec::with_capacity(page.pk_columns.len());
        for pk in &page.pk_columns {
            let Some(idx) = page.columns.iter().position(|c| c == pk) else {
                return Err(format!(
                    "Primary key column '{pk}' missing from page projection."
                ));
            };
            let Some(Some(val)) = row.get(idx) else {
                return Err(format!(
                    "Primary key column '{pk}' is NULL on selected row."
                ));
            };
            pk_match.push((pk.clone(), val.clone()));
        }
        let mutation = if delete_row {
            crate::crud::RowMutation::Delete { pk_match }
        } else {
            let col_idx = self.selected_col.min(page.columns.len().saturating_sub(1));
            let col_name = page.columns[col_idx].clone();
            let current = row.get(col_idx).cloned().flatten();
            crate::crud::RowMutation::Update {
                pk_match,
                changes: vec![(col_name, current)],
            }
        };
        let plan = crate::crud::BatchCrudPlan {
            schema: page.schema.clone(),
            table: page.table.clone(),
            pk_columns: page.pk_columns.clone(),
            mutations: vec![mutation],
            dry_run_rollback: false,
        };
        let sql = plan.build_reviewed_sql().map_err(|d| d.headline)?;
        let op = if delete_row { "DELETE" } else { "UPDATE" };
        Ok((
            format!("Reviewed {op} on {}.{}", page.schema, page.table),
            sql,
        ))
    }
}

/// State for Extended EXPLAIN (BUFFERS, WAL, SETTINGS) & Plan Diff overlay (`054`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlanDiffState {
    /// Baseline query SQL (Plan A).
    pub baseline_sql: String,
    /// Candidate query SQL (Plan B).
    pub candidate_sql: String,
    /// Captured baseline plan snapshot.
    pub baseline: Option<crate::plandiff::PlanSnapshot>,
    /// Captured candidate plan snapshot.
    pub candidate: Option<crate::plandiff::PlanSnapshot>,
    /// Computed A/B plan comparison report.
    pub report: Option<crate::plandiff::PlanDiffReport>,
    /// Active tab (`0`: Summary & Diff, `1`: Node Comparison, `2`: Buffers / WAL / Settings).
    pub active_tab: usize,
    /// Selected item row index within active tab.
    pub selected_idx: usize,
    /// Whether extended EXPLAIN uses `ANALYZE, BUFFERS, WAL` inside rollback transaction.
    pub analyze_mode: bool,
    /// Whether an async EXPLAIN capture is in flight.
    pub loading: bool,
    /// Status or diagnostic message shown in footer.
    pub status_message: Option<String>,
}

impl PlanDiffState {
    /// Returns item count in the currently active tab.
    #[must_use]
    pub fn current_tab_len(&self) -> usize {
        match self.active_tab {
            0 => self.report.as_ref().map_or(0, |r| r.findings.len()),
            1 => self.report.as_ref().map_or(0, |r| r.node_changes.len()),
            2 => self.report.as_ref().map_or(
                self.baseline.as_ref().map_or(0, |p| p.settings.len()),
                |r| r.setting_changes.len(),
            ),
            _ => 0,
        }
    }

    /// Moves selection within the current tab bounds.
    pub fn move_selection(&mut self, delta: isize) {
        let total = self.current_tab_len();
        if total == 0 {
            self.selected_idx = 0;
            return;
        }
        let next = (self.selected_idx as isize + delta).clamp(0, total as isize - 1);
        self.selected_idx = next as usize;
    }

    /// Switches active tab and resets row selection.
    pub fn set_tab(&mut self, tab: usize) {
        self.active_tab = tab.min(2);
        self.selected_idx = 0;
    }

    /// Recomputes `report` whenever both `baseline` and `candidate` are loaded.
    pub fn recompute_diff(&mut self) {
        if let (Some(base), Some(cand)) = (&self.baseline, &self.candidate) {
            self.report = Some(crate::plandiff::diff_plans(base.clone(), cand.clone()));
        } else {
            self.report = None;
        }
    }
}

/// State for the Zero-Query Result Analytics overlay (`055`).
#[derive(Debug, Clone, PartialEq)]
pub struct ResultAnalyticsState {
    /// Computed column statistics, histograms, and sparklines.
    pub analytics: crate::analytics::ResultAnalytics,
    /// Active tab (`0`: Column Profiles & Sparklines, `1`: Top Values & Distributions).
    pub active_tab: usize,
    /// Selected column index.
    pub selected_col: usize,
    /// Status message shown in footer.
    pub status_message: Option<String>,
}

impl ResultAnalyticsState {
    /// Creates an analytics state directly from a visible [`crate::query::result::ResultSet`].
    #[must_use]
    pub fn from_result(set: &crate::query::result::ResultSet) -> Self {
        Self {
            analytics: crate::analytics::profile_result_set(set),
            active_tab: 0,
            selected_col: 0,
            status_message: None,
        }
    }

    /// Moves column selection within bounds.
    pub fn move_selection(&mut self, delta: isize) {
        let total = self.analytics.columns.len();
        if total == 0 {
            self.selected_col = 0;
            return;
        }
        let next = (self.selected_col as isize + delta).clamp(0, total as isize - 1);
        self.selected_col = next as usize;
    }

    /// Switches active tab (`0` or `1`).
    pub fn set_tab(&mut self, tab: usize) {
        self.active_tab = tab.min(1);
    }
}

/// State for the Query Latency Benchmark & A/B Regression Suite overlay (`056`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct QueryBenchState {
    /// Warmup, measured iteration, and rollback safety parameters.
    pub config: crate::bench::BenchConfig,
    /// Baseline SQL query (Query A).
    pub baseline_sql: String,
    /// Candidate SQL query (Query B), when two statements are benchmarked.
    pub candidate_sql: String,
    /// Captured baseline benchmark statistics.
    pub baseline: Option<crate::bench::BenchStats>,
    /// Captured candidate benchmark statistics.
    pub candidate: Option<crate::bench::BenchStats>,
    /// Computed single-query or A/B comparison benchmark report.
    pub report: Option<crate::bench::BenchReport>,
    /// Active tab (`0`: Summary & Latency, `1`: Iteration Samples & Sparkline).
    pub active_tab: usize,
    /// Selected item row index within the active tab.
    pub selected_idx: usize,
    /// Whether an async benchmark run is currently in flight.
    pub loading: bool,
    /// Whether cancellation has been requested for the current benchmark run.
    pub cancelling: bool,
    /// Status or diagnostic message shown in footer.
    pub status_message: Option<String>,
    /// Query B's run, waiting for Query A to report. A and B share the
    /// session, so they run one after the other: run together, one run's
    /// `ROLLBACK` could end the other's `BEGIN` and let its write commit.
    pub queued_candidate: Option<crate::bench::BenchConfig>,
}

impl QueryBenchState {
    /// Returns item count in the currently active tab.
    #[must_use]
    pub fn current_tab_len(&self) -> usize {
        match self.active_tab {
            0 => usize::from(self.report.is_some()) * 9,
            1 => self
                .baseline
                .as_ref()
                .map_or(0, |b| b.samples.len())
                .max(self.candidate.as_ref().map_or(0, |c| c.samples.len())),
            _ => 0,
        }
    }

    /// Moves selection within the current tab bounds.
    pub fn move_selection(&mut self, delta: isize) {
        let total = self.current_tab_len();
        if total == 0 {
            self.selected_idx = 0;
            return;
        }
        let next = (self.selected_idx as isize + delta).clamp(0, total as isize - 1);
        self.selected_idx = next as usize;
    }

    /// Switches active tab (`0` or `1`) and resets row selection.
    pub fn set_tab(&mut self, tab: usize) {
        self.active_tab = tab.min(1);
        self.selected_idx = 0;
    }

    /// Recomputes `report` from `baseline` and optional `candidate` statistics.
    pub fn recompute_report(&mut self) {
        if let Some(base) = &self.baseline {
            self.report = Some(crate::bench::compare_benchmarks(
                base.clone(),
                self.candidate.clone(),
                None,
            ));
        } else {
            self.report = None;
        }
    }
}

/// State for the Schema Snapshot & Drift Guard overlay (`057`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SchemaSnapshotState {
    /// Optional single-schema filter (`None` captures all user schemas).
    pub schema_filter: Option<String>,
    /// Pinned baseline snapshot used for local live drift checks.
    pub baseline: Option<crate::snapshot::SchemaSnapshot>,
    /// Most recently captured live schema snapshot.
    pub current: Option<crate::snapshot::SchemaSnapshot>,
    /// Computed drift diff report between `baseline` and `current`.
    pub diff: Option<crate::snapshot::SnapshotDiffReport>,
    /// Active tab (`0`: Overview & Drift, `1`: Tables).
    pub active_tab: usize,
    /// Selected item row index within the active tab.
    pub selected_idx: usize,
    /// Whether an async catalog snapshot capture is in flight.
    pub loading: bool,
    /// Status or diagnostic message shown in footer.
    pub status_message: Option<String>,
}

impl SchemaSnapshotState {
    /// Returns item count in the currently active tab.
    #[must_use]
    pub fn current_tab_len(&self) -> usize {
        match self.active_tab {
            0 => self.diff.as_ref().map_or(0, |d| d.findings.len()),
            1 => self.current.as_ref().map_or(0, |s| s.tables.len()),
            _ => 0,
        }
    }

    /// Moves selection within the current tab bounds.
    pub fn move_selection(&mut self, delta: isize) {
        let total = self.current_tab_len();
        if total == 0 {
            self.selected_idx = 0;
            return;
        }
        let next = (self.selected_idx as isize + delta).clamp(0, total as isize - 1);
        self.selected_idx = next as usize;
    }

    /// Switches active tab (`0` or `1`) and resets row selection.
    pub fn set_tab(&mut self, tab: usize) {
        self.active_tab = tab.min(1);
        self.selected_idx = 0;
    }

    /// Pins the currently captured snapshot as the baseline and recomputes the drift report.
    pub fn capture_as_baseline(&mut self) {
        if let Some(current) = self.current.clone() {
            self.baseline = Some(current);
            self.recompute_diff();
        }
    }

    /// Recomputes `diff` whenever both `baseline` and `current` snapshots exist.
    pub fn recompute_diff(&mut self) {
        if let (Some(base), Some(curr)) = (&self.baseline, &self.current) {
            self.diff = Some(crate::snapshot::diff_snapshots(base, curr));
        } else {
            self.diff = None;
        }
    }
}

/// State for the Interactive SQL Markdown Runbook overlay (`058`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqlRunbookState {
    /// Parsed SQL runbook from the active editor buffer.
    pub runbook: crate::runbook::SqlRunbook,
    /// Active tab (`0`: Cells, `1`: Safety Dry-Run).
    pub active_tab: usize,
    /// Selected cell index within `runbook.cells`.
    pub selected_idx: usize,
    /// Whether write/DDL cells are permitted in the dry-run preview.
    pub allow_writes: bool,
    /// Status or diagnostic message shown in footer.
    pub status_message: Option<String>,
}

impl SqlRunbookState {
    /// Parses `sql` into a bounded [`crate::runbook::SqlRunbook`] for overlay inspection.
    #[must_use]
    pub fn from_buffer(sql: &str) -> Self {
        Self {
            runbook: crate::runbook::parse_buffer_runbook(sql, "active-buffer"),
            active_tab: 0,
            selected_idx: 0,
            allow_writes: false,
            status_message: None,
        }
    }

    /// Moves cell selection within bounds.
    pub fn move_selection(&mut self, delta: isize) {
        let total = self.runbook.cells.len();
        if total == 0 {
            self.selected_idx = 0;
            return;
        }
        let next = (self.selected_idx as isize + delta).clamp(0, total as isize - 1);
        self.selected_idx = next as usize;
    }

    /// Switches active tab (`0`: Cells, `1`: Safety Dry-Run).
    pub fn set_tab(&mut self, tab: usize) {
        self.active_tab = tab.min(1);
    }

    /// Returns the currently highlighted runbook cell, if any.
    #[must_use]
    pub fn selected_cell(&self) -> Option<&crate::runbook::RunbookCell> {
        self.runbook.cells.get(self.selected_idx)
    }

    /// Computes a pure dry-run execution preview reflecting `allow_writes`.
    #[must_use]
    pub fn dry_run_report(&self) -> crate::runbook::RunbookReport {
        crate::runbook::plan_dry_run(&self.runbook, self.allow_writes, None)
    }
}

/// Interactive state of the offline SQL lint findings overlay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LintFindingsState {
    /// Findings for the current buffer, deterministically ordered.
    pub findings: Vec<crate::lint::Finding>,
    /// Selected finding index.
    pub selected: usize,
}

impl LintFindingsState {
    /// Builds the overlay for one lint run.
    #[must_use]
    pub fn new(findings: Vec<crate::lint::Finding>) -> Self {
        Self {
            findings,
            selected: 0,
        }
    }

    /// Moves the selection within bounds.
    pub fn move_selection(&mut self, delta: isize) {
        if self.findings.is_empty() {
            self.selected = 0;
            return;
        }
        let last = self.findings.len() - 1;
        self.selected = self.selected.saturating_add_signed(delta).min(last);
    }

    /// The selected finding, if any.
    #[must_use]
    pub fn selected_finding(&self) -> Option<&crate::lint::Finding> {
        self.findings.get(self.selected)
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
    /// Self-contained Field Industrial HTML report.
    Html,
    /// Streaming Apache Parquet binary column file.
    Parquet,
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
            Self::Html => "HTML",
            Self::Parquet => "Parquet",
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
            Self::Html => ".html",
            Self::Parquet => ".parquet",
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
            Self::Html => "self-contained Field Industrial HTML report",
            Self::Parquet => "columnar binary Apache Parquet file",
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

/// Where the SQL for a submission came from, for mapping PostgreSQL positions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmissionSource {
    /// Editor SQL is mappable while the original buffer and revision are current.
    Editor {
        /// Stable identity of the submitting tab.
        buffer_id: u64,
        /// Revision of the submitting editor.
        revision: u64,
        /// Original span for Run Statement, or no span for Run Buffer.
        statement: Option<StatementSource>,
    },
    /// SQL submitted by a standalone watch or timer tick.
    WatchedQuery,
    /// SQL submitted by the explicit retained-result refresh.
    RetainedResult,
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
    /// Source of the submission waiting for these parameter values.
    pub source: Option<SubmissionSource>,
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
    pub fn new(sql: String, source: Option<SubmissionSource>, names: Vec<String>) -> Self {
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
    /// Source of the submission waiting for confirmation.
    pub source: Option<SubmissionSource>,
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
    pub(crate) patch: Option<crate::app::inspect::Patch>,
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
    pub(crate) patch: Option<crate::app::inspect::Patch>,
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
            patch: None,
            typed: SecretString::from(String::new()),
        }
    }

    /// Adds one Unicode scalar value to the replacement.
    pub fn push(&mut self, character: char) {
        self.append(character.encode_utf8(&mut [0_u8; 4]));
    }

    /// Appends a paste as one edit, within the structured-input limit.
    pub(crate) fn append(&mut self, fragment: &str) -> bool {
        let current = self.typed.expose_secret();
        if self.patch.is_some()
            && current.len().saturating_add(fragment.len()) > crate::structured::MAX_BYTES
        {
            return false;
        }
        let mut typed = String::with_capacity(current.len().saturating_add(fragment.len()));
        typed.push_str(current);
        typed.push_str(fragment);
        self.typed = SecretString::from(typed);
        true
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

/// A value-free outcome from delivering a paste that did not reach the editor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PasteNotice {
    /// The text was over the accepted paste limit.
    Refused,
    /// A surface that does not accept text owned input when the paste arrived.
    Ignored,
}

impl PasteNotice {
    /// Plain-language status for the editor footer.
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::Refused => "Paste refused: the text is over the 1 MiB limit.".to_owned(),
            Self::Ignored => "Paste ignored: this surface does not accept pasted text.".to_owned(),
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

/// How the interface is being presented.
///
/// The active theme, glyph tier and reduced-motion choice live here rather than
/// in the runtime so a presentation change is an ordinary message with an
/// ordinary next frame. The renderer holds no copy.
#[derive(Debug, Clone)]
pub struct Presentation {
    /// The configured spacing choice, independent of terminal layout tier.
    pub density: crate::config::Density,
    /// Palette and whether colour may be emitted.
    pub theme: crate::ui::theme::Theme,
    /// Which icons and box characters may be drawn.
    pub glyphs: crate::ui::glyphs::Glyphs,
    /// When true, indicators are static text rather than animation.
    pub reduced_motion: bool,
    /// The colour depth the theme emits at, kept beside the theme so a report
    /// can name it without reaching into the palette.
    pub color_depth: crate::ui::theme::ColorDepth,
}

impl Presentation {
    /// Builds a presentation.
    #[must_use]
    pub const fn new(
        theme: crate::ui::theme::Theme,
        glyphs: crate::ui::glyphs::Glyphs,
        reduced_motion: bool,
    ) -> Self {
        Self {
            density: crate::config::Density::Comfortable,
            color_depth: theme.depth,
            theme,
            glyphs,
            reduced_motion,
        }
    }

    /// Selects a contracted spacing table without changing existing defaults.
    #[must_use]
    pub const fn with_density(mut self, density: crate::config::Density) -> Self {
        self.density = density;
        self
    }

    /// All density-dependent spacing, from the single data table.
    #[must_use]
    pub const fn spacing(&self) -> Spacing {
        match self.density {
            crate::config::Density::Comfortable => SPACING_TABLE[0],
            crate::config::Density::Compact => SPACING_TABLE[1],
        }
    }

    pub(crate) fn icon(&self, icon: crate::ui::glyphs::Icon) -> String {
        self.glyphs.prefix(icon)
    }
}

impl Default for Presentation {
    /// A monochrome Unicode presentation, so a model built by a test still
    /// renders. The runtime always replaces this from configuration and
    /// detection at startup.
    fn default() -> Self {
        Self {
            density: crate::config::Density::Comfortable,
            theme: crate::ui::theme::Theme::new(crate::config::ThemeChoice::Dark, false),
            glyphs: crate::ui::glyphs::Glyphs::new(crate::ui::glyphs::GlyphTier::Unicode),
            reduced_motion: false,
            color_depth: crate::ui::theme::ColorDepth::TrueColor,
        }
    }
}

/// The complete density-dependent spacing contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spacing {
    pub title_padding: u16,
    pub grid_cell_padding: u16,
    pub toolbar_blank_rows: u16,
    pub footer_hint_cap: usize,
    pub overlay_margin: u16,
}

pub(crate) const SPACING_TABLE: [Spacing; 2] = [
    Spacing {
        title_padding: 1,
        grid_cell_padding: 1,
        toolbar_blank_rows: 1,
        footer_hint_cap: 6,
        overlay_margin: 2,
    },
    Spacing {
        title_padding: 0,
        grid_cell_padding: 1,
        toolbar_blank_rows: 0,
        footer_hint_cap: 4,
        overlay_margin: 1,
    },
];

/// Session-local unzoomed preferences. Missing values retain the initial
/// responsive heuristics; rendering clamps them without destroying them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SplitState {
    /// Preferred outer sidebar width in cells.
    pub sidebar_cells: Option<u16>,
    /// Preferred outer editor height in rows.
    pub editor_rows: Option<u16>,
    /// Show the focused pane over the body, preserving both preferences.
    pub zoomed: bool,
}

/// Value-free layout feedback, with ordered wording for narrow footers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutNotice {
    /// The terminal already uses one pane.
    SinglePane,
    /// Keep a diagnostic or transaction recovery surface visible.
    SafetyDetails,
}

impl LayoutNotice {
    /// Full wording followed by the two contracted short forms.
    #[must_use]
    pub const fn wordings(self) -> [&'static str; 3] {
        match self {
            Self::SinglePane => [
                "Layout is already single-pane at this terminal size.",
                "Already single-pane.",
                "Single-pane",
            ],
            Self::SafetyDetails => [
                "Zoom Results to keep error and recovery details visible.",
                "Zoom Results for details.",
                "Zoom Results",
            ],
        }
    }
}

/// Runtime-minted physical-session identity. Queued clones prevent pointer reuse.
#[derive(Clone)]
pub struct NoticeSessionId(std::sync::Arc<()>);
impl NoticeSessionId {
    /// Mints one opaque local identity, without a global counter or server value.
    #[must_use]
    pub fn new() -> Self {
        Self(std::sync::Arc::new(()))
    }
}
impl Default for NoticeSessionId {
    fn default() -> Self {
        Self::new()
    }
}
impl PartialEq for NoticeSessionId {
    fn eq(&self, other: &Self) -> bool {
        std::sync::Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for NoticeSessionId {}
impl std::fmt::Debug for NoticeSessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("NoticeSessionId(..)")
    }
}

/// Which physical runtime route produced client accounting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeSource {
    /// User SQL connection, including shared metadata fallback.
    User,
    /// Dedicated metadata connection.
    Metadata,
}
impl NoticeSource {
    /// Fixed slot; never a per-report history.
    pub(crate) const fn index(self) -> usize {
        match self {
            Self::User => 0,
            Self::Metadata => 1,
        }
    }
    /// Safe built-in description.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::User => "user connection",
            Self::Metadata => "metadata connection",
        }
    }
}

/// Latest client summary, with no retained server prose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoticeSummary {
    /// Client observation scope.
    pub scope: crate::query::result::NoticeScope,
    /// Admitted entries, unavailable when accounting failed.
    pub retained: Option<u16>,
    /// Client counts for that report.
    pub counts: crate::query::result::NoticeCounts,
}
impl From<&crate::query::result::NoticeReport> for NoticeSummary {
    fn from(report: &crate::query::result::NoticeReport) -> Self {
        Self {
            scope: report.scope,
            retained: report.retained,
            counts: report.counts,
        }
    }
}

/// Fixed accounting record for one installed physical session.
#[derive(Debug, Clone)]
pub struct NoticeSourceState {
    /// Captured connection generation.
    pub generation: u64,
    /// Runtime identity, never a pointer/address display.
    pub session_id: NoticeSessionId,
    /// Last committed totals delivered to this model.
    pub checkpoint: crate::query::result::NoticeCheckpoint,
    /// Unacknowledged contribution of this physical source.
    pub pending: crate::query::result::NoticeCounts,
    /// Latest bounded fixed summary.
    pub latest: Option<NoticeSummary>,
}

/// A fixed reader snapshot. Opening does not acknowledge it.
#[derive(Debug, Clone)]
pub struct NoticeSnapshot {
    /// Pending aggregate at opening.
    pub pending: crate::query::result::NoticeCounts,
    /// Pending retired-source aggregate at opening.
    pub previous: crate::query::result::NoticeCounts,
    /// At most the two physical-source records, without server text.
    pub sources: [Option<NoticeSourceState>; 2],
    /// Local pending-state revision at opening.
    pub revision: u64,
}

/// Read-only notice details, separate from sending-review state.
#[derive(Debug, Clone)]
pub struct NoticeDetails {
    /// Captured report, unchanged by later arrivals.
    pub snapshot: NoticeSnapshot,
    /// First wrapped display row.
    pub scroll: usize,
}

/// Fixed-size latest/pending accounting; no server text or report queue.
#[derive(Debug, Clone, Default)]
pub struct NoticeState {
    /// At most two installed physical sources.
    pub sources: [Option<NoticeSourceState>; 2],
    /// All loss awaiting an explicit acknowledgement.
    pub pending: crate::query::result::NoticeCounts,
    /// Unacknowledged counts from retired connections.
    pub previous: crate::query::result::NoticeCounts,
    /// Changes to pending state, independent of draw/ticks.
    pub revision: u64,
    /// Explicit authority for a synchronous physical retirement handoff.
    pub retiring_generation: Option<u64>,
    /// Each source may hand off once at that boundary.
    pub retirement_open: [bool; 2],
    /// At most one latest failed-connect delivery identity per route.
    pub last_attempt: [Option<NoticeSessionId>; 2],
    /// Explicit current-generation authority for one bounded shutdown drain.
    pub uninstalled_retirement: Option<u64>,
    /// Generation whose fixed uninstalled-attempt tokens are retained.
    pub uninstalled_token_generation: Option<u64>,
    /// At most pending2 plus channel2 identities, never a report history.
    pub uninstalled_tokens: [Option<NoticeSessionId>; 4],
}
impl NoticeState {
    /// Begins a bounded retirement authority; ordinary old status stays invalid.
    pub fn begin_retirement(&mut self, generation: u64) {
        self.retiring_generation = Some(generation);
        self.retirement_open = [true, true];
    }
    /// Ends the explicit handoff authority.
    pub fn finish_retirement(&mut self, generation: u64) {
        if self.retiring_generation == Some(generation) {
            self.retiring_generation = None;
            self.retirement_open = [false, false];
        }
    }
    /// Installs only an explicitly observed physical source, preserving old debt.
    pub fn install(&mut self, source: NoticeSource, generation: u64, session_id: NoticeSessionId) {
        let index = source.index();
        if self.sources[index]
            .as_ref()
            .is_some_and(|old| old.generation == generation && old.session_id == session_id)
        {
            return;
        }
        if let Some(old) = self.sources[index].as_ref() {
            let (old_gen, old_id, totals) = (
                old.generation,
                old.session_id.clone(),
                old.checkpoint.totals,
            );
            self.retire(source, old_gen, &old_id, totals);
        }
        self.sources[index] = Some(NoticeSourceState {
            generation,
            session_id,
            checkpoint: crate::query::result::NoticeCheckpoint::default(),
            pending: crate::query::result::NoticeCounts::default(),
            latest: None,
        });
    }
    /// Coalesces one committed checkpoint without double counting it.
    pub fn observe(
        &mut self,
        source: NoticeSource,
        generation: u64,
        session_id: &NoticeSessionId,
        checkpoint: crate::query::result::NoticeCheckpoint,
        latest: Option<NoticeSummary>,
    ) {
        let Some(state) = self.sources[source.index()]
            .as_mut()
            .filter(|state| state.generation == generation && state.session_id == *session_id)
        else {
            return;
        };
        if checkpoint.revision < state.checkpoint.revision {
            return;
        }
        if checkpoint.revision == state.checkpoint.revision && checkpoint.revision != u64::MAX {
            if checkpoint.totals != state.checkpoint.totals {
                return;
            }
            if latest.is_some() {
                state.latest = latest;
            }
            return;
        }
        let delta = checkpoint.totals.since(state.checkpoint.totals);
        state.checkpoint = checkpoint;
        state.pending = state.pending.plus(delta);
        if checkpoint.revision == u64::MAX && state.checkpoint.revision == u64::MAX {
            state.latest = None;
        } else if latest.is_some() {
            state.latest = latest;
        }
        if delta.needs_report() {
            self.pending = self.pending.plus(delta);
            self.revision = self.revision.saturating_add(1);
        }
    }
    /// Adds a nonmutating actual-retirement projection exactly once, then retires identity.
    pub fn retire(
        &mut self,
        source: NoticeSource,
        generation: u64,
        session_id: &NoticeSessionId,
        counts: crate::query::result::NoticeCounts,
    ) {
        let index = source.index();
        if !self.sources[index]
            .as_ref()
            .is_some_and(|state| state.generation == generation && state.session_id == *session_id)
        {
            return;
        }
        let state = self.sources[index]
            .take()
            .expect("matched physical identity");
        let delta = counts.since(state.checkpoint.totals);
        self.pending = self.pending.plus(delta);
        self.previous = self.previous.plus(state.pending.plus(delta));
        self.retirement_open[index] = false;
        if delta.needs_report() {
            self.revision = self.revision.saturating_add(1);
        }
    }
    /// Captures fixed counters/checkpoints without acknowledgement.
    #[must_use]
    pub fn snapshot(&self) -> NoticeSnapshot {
        NoticeSnapshot {
            pending: self.pending,
            previous: self.previous,
            sources: self.sources.clone(),
            revision: self.revision,
        }
    }
    /// Acknowledges only the displayed snapshot; new loss remains pending.
    pub fn acknowledge(&mut self, snapshot: &NoticeSnapshot) {
        if self.revision == snapshot.revision && self.revision != u64::MAX {
            self.pending = crate::query::result::NoticeCounts::default();
            self.previous = crate::query::result::NoticeCounts::default();
            for source in self.sources.iter_mut().flatten() {
                source.pending = crate::query::result::NoticeCounts::default();
            }
        } else {
            self.pending = self.pending.since(snapshot.pending);
            let mut previous_ack = snapshot.previous;
            for (index, old) in snapshot.sources.iter().enumerate() {
                if let Some(old) = old {
                    match self.sources[index].as_mut().filter(|source| {
                        source.generation == old.generation && source.session_id == old.session_id
                    }) {
                        Some(source) => source.pending = source.pending.since(old.pending),
                        None => previous_ack = previous_ack.plus(old.pending),
                    }
                }
            }
            self.previous = self.previous.since(previous_ack);
        }
    }
}

/// The maximum interval between two left `Down` events on the same region for a
/// double-click (`contracts/mouse.md`).
pub const DOUBLE_CLICK_WINDOW: Duration = Duration::from_millis(400);

/// An active pointer drag over a split bar or a result-column rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragTarget {
    /// Dragging the vertical split between the sidebar and the main panes.
    VerticalSplit {
        /// Screen column where the drag started.
        start_column: u16,
        /// Sidebar width in cells when the drag started.
        start_width: u16,
    },
    /// Dragging the horizontal split between the editor and the results pane.
    HorizontalSplit {
        /// Screen row where the drag started.
        start_row: u16,
        /// Editor height in rows when the drag started.
        start_height: u16,
    },
    /// Dragging the right edge of a result-grid column.
    ColumnEdge {
        /// Zero-based source-column index.
        column: usize,
        /// Screen column where the drag started.
        start_column: u16,
        /// Column width in cells when the drag started.
        start_width: usize,
    },
}

/// The most recent left `Down` on a double-clickable region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LastClick {
    /// The hit region that received the first click.
    pub region: crate::ui::widgets::Region,
    /// Monotonic mouse-clock value when the click arrived.
    pub at: Duration,
}

/// Pure pointer interaction state owned by [`Model`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MouseState {
    /// Active split or column-edge drag, if any.
    pub drag: Option<DragTarget>,
    /// Last left `Down` eligible to form a double-click within [`DOUBLE_CLICK_WINDOW`].
    pub last_click: Option<LastClick>,
    /// Monotonic elapsed time used to evaluate [`DOUBLE_CLICK_WINDOW`] without
    /// reading a system clock inside the reducer.
    pub clock: Duration,
}

/// The complete application state.
#[derive(Debug, Clone, Default)]
pub struct Model {
    /// Identity of the connection whose server-derived facts are in this
    /// model. It starts at one to match the initial runtime connection route.
    pub(crate) connection_generation: u64,
    /// Which pane has focus.
    pub focus: Focus,
    /// Layout shape for this application session, never persisted.
    pub splits: SplitState,
    /// Pure pointer interaction state (active drag target and last-click tracker).
    pub mouse: MouseState,
    /// Feedback retained through ticks until the next user action.
    pub layout_notice: Option<LayoutNotice>,
    /// The active key labels used when discovery surfaces describe actions.
    ///
    /// This is a configuration snapshot supplied by the interactive runtime,
    /// not a second source of key resolution and not persisted onboarding state.
    pub keymap_snapshot: KeymapSnapshot,
    /// How the interface is presented: theme, glyph tier and reduced motion.
    pub presentation: Presentation,
    /// Validated immutable palettes available during this application session.
    pub(crate) theme_catalog: crate::ui::theme::ValidatedThemes,
    /// The SQL buffer.
    pub editor: Editor,
    /// Multi-tab SQL buffer manager coordinating multiple tabs and split views.
    pub buffers: crate::buffer::BufferManager,
    /// Schema-aware completion state and its catalogue snapshot.
    pub completion: crate::app::completion::CompletionState,
    /// Connection state.
    pub connection: ConnectionState,
    /// The connecting stage in progress, or the stage a failure happened in.
    ///
    /// Kept separately from [`ConnectionState::Connecting`] so a failure can
    /// still name the stage it happened in after the state becomes `Failed`.
    pub connecting_step: Option<ConnectingStep>,
    /// Boot reveal state for the identity mark over the initial connecting wait.
    pub banner: crate::ui::widgets::banner::BannerState,
    /// Whether the boot reveal has already started once in this process.
    pub banner_played: bool,
    /// Whether the next catalogue load should open the tree's first level.
    ///
    /// Set on a successful connection, consumed once by the first load, so a
    /// later reload never re-expands over the user's own state.
    pub expand_first_schema_on_load: bool,
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
    /// Fixed loss summary tied to the retained source, never recomputed per frame.
    pub retained_data_loss: crate::query::result::RetentionLossSummary,
    /// The SQL template that produced the retained execution.
    ///
    /// It is kept separately because `Execution` intentionally contains only
    /// server results, and it is the source the update analyzer must revalidate.
    pub last_sql: Option<String>,
    /// Server messages from the most recent execution.
    pub notices: Vec<Notice>,
    /// Fixed server-notice retention accounting, separate from local SAVED/WROTE messages.
    pub notice_state: NoticeState,
    /// Read-only client report captured on explicit opening.
    pub notice_details: Option<NoticeDetails>,
    /// The error being shown, if any.
    pub error: Option<Diagnostic>,
    /// Whether the help overlay is open.
    pub help_open: bool,
    /// First wrapped help row, local to the current help visit.
    pub help_scroll: usize,
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
    /// First wrapped row of the owning one-shot sending review. Content is
    /// derived transiently; SQL and prompted answers are never cached here.
    pub review_scroll: usize,
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
    /// The last value-free paste refusal or acknowledgement shown in the editor footer.
    pub paste_notice: Option<PasteNotice>,
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
    /// The motion clock and the short transitions that decorate a state
    /// change. Only the reducer changes it; widgets read it through
    /// [`Model::motion`].
    pub(crate) motion: crate::ui::motion::Motion,
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
    /// Origin of the SQL for the job in flight.
    pub running_source: Option<SubmissionSource>,
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
    /// Interactive schema entity-relationship (ER) diagram overlay state.
    pub schema_diagram: Option<SchemaDiagramState>,
    /// Request counter for in-flight schema diagram queries.
    pub schema_diagram_request: u64,
    /// Feedback banner shown when diagram action is unavailable.
    pub schema_diagram_notice: Option<String>,
    /// Interactive server activity and lock contention dashboard overlay state.
    pub server_activity: Option<ServerActivityState>,
    /// Request counter for in-flight server activity queries.
    pub server_activity_request: u64,
    /// Interactive roles, privilege grants, and Row-Level Security (RLS) auditor overlay state.
    pub security_audit: Option<SecurityAuditState>,
    /// Request counter for in-flight security audit queries.
    pub security_audit_request: u64,
    /// Interactive schema diff, table designer, and migration generator overlay state.
    pub schema_diff: Option<SchemaDiffState>,
    /// Request counter for in-flight schema diff queries.
    pub schema_diff_request: u64,
    /// Interactive bulk data import wizard overlay state.
    pub import_wizard: Option<ImportWizardState>,
    /// Production confirmation for one captured import configuration.
    pub(crate) pending_import: Option<PendingImport>,
    /// Execution request still outstanding, including after the wizard closes.
    pub(crate) import_in_flight: Option<u64>,
    /// Request counter for in-flight import operations.
    pub import_wizard_request: u64,
    /// Interactive routine runner and PL/pgSQL profiler overlay state.
    pub routine_runner: Option<RoutineRunnerState>,
    /// Request counter for in-flight routine runner operations.
    pub routine_runner_request: u64,
    /// Interactive maintenance operations runner and pub/sub monitor overlay state.
    pub maintenance_ops: Option<MaintenanceOpsState>,
    /// Request counter for in-flight maintenance operations.
    pub maintenance_ops_request: u64,
    /// Interactive query performance and pg_stat_statements forensics overlay state.
    pub query_performance: Option<QueryPerformanceState>,
    /// Request counter for in-flight query performance operations.
    pub query_performance_request: u64,
    /// Interactive index advisor and bloat forensics overlay state.
    pub index_advisor: Option<IndexAdvisorState>,
    /// Request counter for in-flight index advisor queries.
    pub index_advisor_request: u64,
    /// Interactive transaction, WAL, and checkpointer observability overlay state.
    pub wal_observability: Option<WalObservabilityState>,
    /// Request counter for in-flight WAL observability queries.
    pub wal_observability_request: u64,
    /// Interactive connection pool and PgBouncer telemetry overlay state.
    pub connection_pool: Option<ConnectionPoolState>,
    /// Request counter for in-flight connection pool queries.
    pub connection_pool_request: u64,
    /// Interactive data masking and anonymization rules overlay state.
    pub data_masking: Option<DataMaskingState>,
    /// Request counter for in-flight data masking analysis queries.
    pub data_masking_request: u64,
    /// Interactive cross-database copy and sync pipelines overlay state.
    pub data_sync: Option<DataSyncState>,
    /// Request counter for in-flight cross-database sync queries.
    pub data_sync_request: u64,
    /// Interactive synthetic fixture generator overlay state.
    pub synthetic_seed: Option<SyntheticSeedState>,
    /// Request counter for in-flight synthetic seed catalog queries.
    pub synthetic_seed_request: u64,
    /// Interactive snippet and scratchpad library overlay state.
    pub snippet_library: Option<SnippetLibraryState>,
    /// Active crash recovery notification banner, if unrecovered session found at startup.
    pub recovery_banner: Option<SessionRecoveryBanner>,
    /// Active offline lint findings overlay, when one is open.
    pub lint_findings: Option<LintFindingsState>,
    /// Active psql `\watch` periodic polling loop.
    pub watch: Option<WatchState>,
    /// Controls statement execution timing visibility (`\timing`).
    pub timing_display: TimingDisplay,
    /// Interactive logical replication and slots manager overlay state.
    pub logical_replication: Option<LogicalReplicationState>,
    /// Request counter for in-flight logical replication queries.
    pub logical_replication_request: u64,
    /// Interactive server config, HBA, extensions, tablespaces, and FDWs overlay state.
    pub cluster_config: Option<ClusterConfigState>,
    /// Request counter for in-flight cluster configuration queries.
    pub cluster_config_request: u64,
    /// Interactive keyset-paginated live data browser and batch CRUD overlay state.
    pub data_browser: Option<DataBrowserState>,
    /// Request counter for in-flight keyset data browse queries.
    pub data_browser_request: u64,
    /// Interactive extended EXPLAIN and A/B plan diff overlay state.
    pub plan_diff: Option<PlanDiffState>,
    /// Request counter for in-flight extended EXPLAIN snapshots.
    pub plan_diff_request: u64,
    /// Interactive zero-query result analytics profiler overlay state.
    pub result_analytics: Option<ResultAnalyticsState>,
    /// Interactive query latency benchmark and A/B regression suite overlay state.
    pub query_bench: Option<QueryBenchState>,
    /// Request counter for in-flight query benchmark executions.
    pub query_bench_request: u64,
    /// Interactive canonical schema snapshot and drift guard overlay state.
    pub schema_snapshot: Option<SchemaSnapshotState>,
    /// Request counter for in-flight schema snapshot catalog captures.
    pub schema_snapshot_request: u64,
    /// Interactive SQL Markdown runbook overlay state.
    pub sql_runbook: Option<SqlRunbookState>,
}

impl Model {
    /// Builds a model with the configured row cap.
    #[must_use]
    pub fn new(row_cap: usize) -> Self {
        Self {
            connection_generation: 1,
            row_cap,
            size: (crate::ui::MIN_COLUMNS, crate::ui::MIN_ROWS),
            sidebar_visible: true,
            ..Self::default()
        }
    }

    /// Captures the editor identity before prompts or asynchronous execution.
    #[must_use]
    pub fn editor_submission(&self, statement: Option<StatementSource>) -> SubmissionSource {
        SubmissionSource::Editor {
            buffer_id: self.buffers.active_buffer().id,
            revision: self.editor.revision(),
            statement,
        }
    }

    /// Synchronizes the active buffer in `buffers` from `editor`.
    pub fn sync_active_buffer_from_editor(&mut self) {
        self.buffers.active_buffer_mut().editor = self.editor.clone();
    }

    /// Synchronizes `editor` from the active buffer in `buffers`.
    pub fn sync_editor_from_active_buffer(&mut self) {
        self.editor = self.buffers.active_editor().clone();
    }

    /// Whether anything is currently worth animating.
    ///
    /// The runtime uses the two halves of this to stop each ticker, so an idle
    /// client wakes nothing.
    #[must_use]
    pub const fn is_animating(&self) -> bool {
        self.is_busy_animating() || self.is_in_motion()
    }

    /// Whether a busy indicator is on screen: the spinner and meter of a
    /// running statement, or the boot reveal while connecting.
    ///
    /// These are driven by the 90 ms ticker, whose timing the spinner speed,
    /// the boot reveal and the mouse clock depend on.
    #[must_use]
    pub const fn is_busy_animating(&self) -> bool {
        self.phase.is_busy() || matches!(self.connection, ConnectionState::Connecting { .. })
    }

    /// Whether a short transition is active, which the runtime drives with
    /// motion frames at about thirty a second until it settles.
    #[must_use]
    pub const fn is_in_motion(&self) -> bool {
        self.motion.is_active()
    }

    /// The motion clock and active transitions, for widgets to read.
    #[must_use]
    pub const fn motion(&self) -> &crate::ui::motion::Motion {
        &self.motion
    }

    /// Whether a transition may start now.
    ///
    /// Motion plays only in the interactive terminal, and never under reduced
    /// motion, with ASCII glyphs, with colour off, or at 16 colours, where
    /// every transition would show its final state immediately anyway.
    #[must_use]
    pub fn allows_motion(&self) -> bool {
        let theme = &self.presentation.theme;
        self.motion.is_enabled()
            && !self.reduced_motion
            && !self.presentation.reduced_motion
            && !self.presentation.glyphs.is_ascii()
            && theme.color
            && theme.depth.shows_transitions()
    }

    /// Whether a surface is open that must be painted exactly as it is.
    ///
    /// Confirmations, reviews, prompts, the password prompt and the connection
    /// trust surface are where paint-before-accept matters, so while one of
    /// them is open every transition is settled and none starts. Each is named
    /// here rather than inferred, so a new one is a deliberate addition. That
    /// includes the confirmations armed inside a dashboard: the statistics
    /// reset hazard, maintenance acknowledged for production, and the import
    /// wizard's review step, where the next Enter writes rows.
    #[must_use]
    pub fn holds_motion_still(&self) -> bool {
        self.password_prompt.is_some()
            || self.parameter_prompt.is_some()
            || self.name_prompt.is_some()
            || self.connection_details
            || self.pending_run.is_some()
            || self.pending_plan.is_some()
            || self.pending_copy.is_some()
            || self.update_prompt.is_some()
            || self.pending_update.is_some()
            || self
                .query_performance
                .as_ref()
                .is_some_and(|perf| perf.confirm_reset)
            || self
                .maintenance_ops
                .as_ref()
                .is_some_and(|ops| ops.confirm_production)
            || self
                .import_wizard
                .as_ref()
                .is_some_and(|wizard| wizard.step == ImportWizardStep::Execute)
    }

    /// Advances the pure monotonic mouse clock by `elapsed`.
    pub fn advance_mouse_clock(&mut self, elapsed: Duration) {
        self.mouse.clock = self.mouse.clock.saturating_add(elapsed);
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

    /// Discards import state tied to a retired physical connection.
    pub(crate) fn retire_import(&mut self) {
        self.import_wizard = None;
        self.pending_import = None;
        self.import_in_flight = None;
    }

    /// Captures a ready query session through its existing physical installation identity.
    pub(crate) fn import_connection(&self) -> Option<ImportConnection> {
        let ConnectionState::Connected(info) = &self.connection else {
            return None;
        };
        let source = self.notice_state.sources[NoticeSource::User.index()].as_ref()?;
        (source.generation == self.connection_generation).then(|| ImportConnection {
            generation: source.generation,
            session_id: source.session_id.clone(),
            database: info.database.clone(),
            environment: info.environment.clone(),
        })
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
    /// never retained, so they cannot be searched, and every count that mentions
    /// the filter says so.
    #[must_use]
    pub fn filtered_rows(&self) -> Vec<usize> {
        let Some(set) = self.visible_result() else {
            return Vec::new();
        };
        if !set.is_available() {
            return Vec::new();
        }
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
        if !set.is_available() || self.result_filter.trim().is_empty() {
            return set.window_label();
        }
        let matching = self.filtered_rows().len();
        let retained = set.retained();
        if set.retention_reason()
            == Some(crate::query::result::RetentionReason::RetainedResultLimit)
        {
            format!(
                "matching {matching} of {retained} retained rows, of {} returned (retained-result limit reached)",
                set.rows_seen
            )
        } else if set.is_truncated() {
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
mod tests;
