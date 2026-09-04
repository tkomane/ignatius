//! Completion state owned by the pure application model.

use crate::query::completion::{Candidate, CompletionCatalog, CompletionResult};

/// The state of the catalogue snapshot used for local completion.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum CatalogStatus {
    /// No connection has supplied a snapshot yet.
    #[default]
    NotLoaded,
    /// A snapshot read is in flight.
    Loading {
        /// Identity of the read that may update this state.
        request: u64,
    },
    /// A snapshot is ready for local filtering.
    Ready {
        /// The snapshot returned by PostgreSQL.
        catalog: CompletionCatalog,
        /// Runtime-generated time label, kept out of the reducer's clock.
        loaded_at: String,
    },
    /// The read failed. Keywords can still be offered, but objects must not be
    /// described as absent.
    Unavailable {
        /// Safe diagnostic headline.
        message: String,
    },
}

impl CatalogStatus {
    /// The ready snapshot, if one exists.
    #[must_use]
    pub fn catalog(&self) -> Option<&CompletionCatalog> {
        match self {
            Self::Ready { catalog, .. } => Some(catalog),
            Self::NotLoaded | Self::Loading { .. } | Self::Unavailable { .. } => None,
        }
    }

    /// A short status sentence for the menu.
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::NotLoaded => "Schema snapshot not loaded yet".to_owned(),
            Self::Loading { .. } => "Loading schema snapshot".to_owned(),
            Self::Ready { loaded_at, .. } => {
                format!("Schema snapshot loaded at {loaded_at}; reload objects to refresh")
            }
            Self::Unavailable { message } => {
                format!("Schema snapshot unavailable: {message}; keywords remain available")
            }
        }
    }

    /// Whether the status is waiting on a request.
    #[must_use]
    pub const fn is_loading(&self) -> bool {
        matches!(self, Self::Loading { .. })
    }
}

/// The active completion menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionMenu {
    /// Candidate list and exact replacement range.
    pub result: CompletionResult,
    /// Visible selected candidate. It changes only through navigation or
    /// explicit acceptance; opening the menu never edits the buffer.
    pub selected: usize,
    /// Whether the person explicitly requested the list. Automatic lists with
    /// a short or already-exact prefix are suppressed to keep typing quiet.
    pub explicit: bool,
}

impl CompletionMenu {
    /// Builds a menu with the first candidate visibly selected.
    #[must_use]
    pub fn new(result: CompletionResult, explicit: bool) -> Self {
        Self {
            selected: 0,
            result,
            explicit,
        }
    }

    /// The selected candidate, if any.
    #[must_use]
    pub fn selected_candidate(&self) -> Option<&Candidate> {
        self.result.candidates.get(self.selected)
    }

    /// Moves the visible selection, clamping at the visible list boundaries.
    pub fn move_selection(&mut self, delta: isize) {
        let length = self.result.candidates.len();
        if length == 0 {
            self.selected = 0;
            return;
        }
        self.selected = if delta.is_negative() {
            self.selected
                .saturating_sub(delta.unsigned_abs())
                .min(length - 1)
        } else {
            self.selected.saturating_add(delta as usize).min(length - 1)
        };
    }

    /// Keeps the selected label when a local refresh changes the result list.
    #[must_use]
    pub fn refreshed(&self, result: CompletionResult) -> Self {
        let selected_label = self
            .selected_candidate()
            .map(|candidate| candidate.label.clone());
        let selected = selected_label
            .and_then(|label| {
                result
                    .candidates
                    .iter()
                    .position(|candidate| candidate.label == label)
            })
            .unwrap_or(0);
        Self {
            result,
            selected,
            explicit: self.explicit,
        }
    }
}

/// Completion settings and transient menu state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionState {
    /// Whether typing may open an automatic menu.
    pub enabled: bool,
    /// Current catalogue status.
    pub catalog: CatalogStatus,
    /// Active menu, when one is visible.
    pub menu: Option<CompletionMenu>,
    next_request: u64,
}

impl Default for CompletionState {
    fn default() -> Self {
        Self {
            enabled: true,
            catalog: CatalogStatus::NotLoaded,
            menu: None,
            next_request: 0,
        }
    }
}

impl CompletionState {
    /// Starts a catalogue request and returns its identity.
    pub fn begin_loading(&mut self) -> u64 {
        self.next_request = self.next_request.wrapping_add(1).max(1);
        let request = self.next_request;
        self.catalog = CatalogStatus::Loading { request };
        self.menu = None;
        request
    }

    /// Whether a response belongs to the request currently in flight.
    #[must_use]
    pub fn accepts(&self, request: u64) -> bool {
        matches!(self.catalog, CatalogStatus::Loading { request: active } if active == request)
    }

    /// The current catalogue snapshot, if ready.
    #[must_use]
    pub fn catalog(&self) -> Option<&CompletionCatalog> {
        self.catalog.catalog()
    }
}
