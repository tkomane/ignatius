//! Local state for the plan view.
//!
//! A plan is fetched once and then explored locally. Selection and collapsed
//! branches are view state, so movement and expansion never produce an effect
//! and cannot re-run the statement.

use crate::query::plan::{PlanDocument, PlanNode};
use crate::query::result::JobId;
use std::collections::BTreeSet;

/// The visible plan surface and its local navigation state.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlanView {
    /// Whether a request is loading, ready, failed, or absent.
    pub status: PlanStatus,
    /// Child-index path of the selected node.
    pub selected_path: Vec<usize>,
    /// Child-index paths whose descendants are hidden.
    pub collapsed: BTreeSet<Vec<usize>>,
}

impl PlanView {
    /// Starts a new plain or analyzed plan request.
    #[must_use]
    pub fn loading(job: JobId, analyzed: bool) -> Self {
        Self {
            status: PlanStatus::Loading { job, analyzed },
            selected_path: Vec::new(),
            collapsed: BTreeSet::new(),
        }
    }

    /// Whether any plan surface is currently taking the Results pane.
    #[must_use]
    pub const fn is_visible(&self) -> bool {
        !matches!(self.status, PlanStatus::Hidden)
    }

    /// Whether a plan request is in flight.
    #[must_use]
    pub const fn is_loading(&self) -> bool {
        matches!(self.status, PlanStatus::Loading { .. })
    }

    /// The active plan job, if any.
    #[must_use]
    pub const fn job(&self) -> Option<JobId> {
        match self.status {
            PlanStatus::Loading { job, .. } => Some(job),
            PlanStatus::Hidden | PlanStatus::Ready(_) | PlanStatus::Failed { .. } => None,
        }
    }

    /// Whether the current request measured execution.
    #[must_use]
    pub const fn analyzed(&self) -> bool {
        match self.status {
            PlanStatus::Loading { analyzed, .. } | PlanStatus::Failed { analyzed, .. } => analyzed,
            PlanStatus::Ready(ref document) => document.analyzed,
            PlanStatus::Hidden => false,
        }
    }

    /// Stores a successful document and resets local selection state.
    pub fn ready(&mut self, document: PlanDocument) {
        self.status = PlanStatus::Ready(Box::new(document));
        self.selected_path.clear();
        self.collapsed.clear();
    }

    /// Stores a failure without turning it into a generic ordinary query error.
    pub fn failed(
        &mut self,
        analyzed: bool,
        headline: impl Into<String>,
        next_action: impl Into<String>,
    ) {
        self.status = PlanStatus::Failed {
            analyzed,
            headline: headline.into(),
            next_action: next_action.into(),
        };
        self.selected_path.clear();
        self.collapsed.clear();
    }

    /// Dismisses the plan and its local view state.
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// Returns the ready document, if one is on screen.
    #[must_use]
    pub fn document(&self) -> Option<&PlanDocument> {
        match &self.status {
            PlanStatus::Ready(document) => Some(document),
            PlanStatus::Hidden | PlanStatus::Loading { .. } | PlanStatus::Failed { .. } => None,
        }
    }

    /// Returns the selected node when a plan is ready.
    #[must_use]
    pub fn selected_node(&self) -> Option<&PlanNode> {
        self.document()?.root.at_path(&self.selected_path)
    }

    /// Returns every visible node path in depth-first order.
    #[must_use]
    pub fn visible_paths(&self) -> Vec<Vec<usize>> {
        let Some(document) = self.document() else {
            return Vec::new();
        };
        let mut paths = Vec::new();
        collect_visible_paths(&document.root, &mut Vec::new(), &self.collapsed, &mut paths);
        paths
    }

    /// Moves the selection through visible nodes only.
    pub fn move_selection(&mut self, delta: isize) {
        let paths = self.visible_paths();
        if paths.is_empty() {
            self.selected_path.clear();
            return;
        }
        let current = paths
            .iter()
            .position(|path| path == &self.selected_path)
            .unwrap_or(0);
        let last = isize::try_from(paths.len().saturating_sub(1)).unwrap_or(0);
        let next = (isize::try_from(current).unwrap_or(0) + delta).clamp(0, last);
        self.selected_path = paths[usize::try_from(next).unwrap_or(0)].clone();
    }

    /// Expands or collapses the selected node locally.
    pub fn toggle_selected(&mut self) {
        let Some(node) = self.selected_node() else {
            return;
        };
        if node.children.is_empty() {
            return;
        }
        if !self.collapsed.insert(self.selected_path.clone()) {
            self.collapsed.remove(&self.selected_path);
        }
        self.ensure_selection_visible();
    }

    /// Ensures selection remains visible after a branch is collapsed.
    pub fn ensure_selection_visible(&mut self) {
        let paths = self.visible_paths();
        if paths.is_empty() {
            self.selected_path.clear();
        } else if !paths.iter().any(|path| path == &self.selected_path) {
            self.selected_path = paths[0].clone();
        }
    }
}

/// The lifecycle of one plan request.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum PlanStatus {
    /// No plan is taking the Results pane.
    #[default]
    Hidden,
    /// A plan-specific effect is in flight.
    Loading {
        /// Job identity used to reject stale completion.
        job: JobId,
        /// Whether the request is EXPLAIN ANALYZE.
        analyzed: bool,
    },
    /// A valid bounded plan is ready for local navigation.
    Ready(Box<PlanDocument>),
    /// The request did not produce a successful plan.
    Failed {
        /// Whether the failed request would have analyzed the statement.
        analyzed: bool,
        /// Safe headline for the plan pane.
        headline: String,
        /// Safe recovery action for the plan pane.
        next_action: String,
    },
}

fn collect_visible_paths(
    node: &PlanNode,
    path: &mut Vec<usize>,
    collapsed: &BTreeSet<Vec<usize>>,
    output: &mut Vec<Vec<usize>>,
) {
    output.push(path.clone());
    if collapsed.contains(path) {
        return;
    }
    for (index, child) in node.children.iter().enumerate() {
        path.push(index);
        collect_visible_paths(child, path, collapsed, output);
        path.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::plan::PlanDocument;

    fn view() -> PlanView {
        let json = r#"[{"Plan":{"Node Type":"Nested Loop","Plans":[{"Node Type":"Seq Scan","Plans":[{"Node Type":"Result"}]},{"Node Type":"Index Scan"}]}}]"#;
        let mut view = PlanView::default();
        view.ready(PlanDocument::parse(json, false).expect("fixture"));
        view
    }

    #[test]
    fn visible_paths_are_depth_first_and_selection_moves_locally() {
        let mut view = view();
        assert_eq!(
            view.visible_paths(),
            vec![vec![], vec![0], vec![0, 0], vec![1]]
        );
        view.move_selection(1);
        assert_eq!(view.selected_path, vec![0]);
        view.move_selection(1);
        assert_eq!(view.selected_path, vec![0, 0]);
        view.move_selection(100);
        assert_eq!(view.selected_path, vec![1]);
    }

    #[test]
    fn collapsing_hides_descendants_and_keeps_the_selection_on_the_branch() {
        let mut view = view();
        view.move_selection(1);
        view.toggle_selected();
        assert_eq!(view.visible_paths(), vec![vec![], vec![0], vec![1]]);
        assert_eq!(view.selected_path, vec![0]);
        view.toggle_selected();
        assert_eq!(view.visible_paths().len(), 4);
    }

    #[test]
    fn selection_recovers_when_a_selected_descendant_is_hidden() {
        let mut view = view();
        view.move_selection(2);
        assert_eq!(view.selected_path, vec![0, 0]);
        view.selected_path = vec![0, 0];
        view.collapsed.insert(vec![0]);
        view.ensure_selection_visible();
        assert_eq!(view.selected_path, Vec::<usize>::new());
    }

    #[test]
    fn leaves_have_no_expand_action() {
        let mut view = view();
        view.move_selection(2);
        view.toggle_selected();
        assert!(view.collapsed.is_empty());
    }
}
