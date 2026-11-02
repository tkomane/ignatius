//! The object tree.
//!
//! A pure data structure with no I/O. Expanding a node does not load anything:
//! it marks the node as loading and asks the application for an effect, and the
//! answer arrives later as a message. That is what keeps a 400-schema database
//! from freezing the interface, and it is why every rule below is testable
//! without a server.
//!
//! Loads carry a request identity. A response whose identity is not the one the
//! node is waiting for is discarded, the same rule that stops a slow query from
//! overwriting a newer result. Without it, collapsing and re-expanding a node
//! quickly would populate it with the wrong answer.

use crate::diagnostics::Diagnostic;
use crate::postgres::metadata::{ColumnInfo, ObjectKind, ObjectSummary, SchemaSummary};

/// Identity of a metadata load, so a stale answer can be recognised.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RequestId(pub u64);

/// A position in the tree: indices from the roots downwards.
pub type NodePath = Vec<usize>;

/// What a node represents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeKind {
    /// A schema.
    Schema(SchemaSummary),
    /// The group holding every object of one kind inside a schema.
    Group {
        /// Schema the group belongs to.
        schema: String,
        /// Kind of object the group holds.
        kind: ObjectKind,
        /// How many the catalogue reported.
        count: i64,
    },
    /// A single object.
    Object(ObjectSummary),
    /// A column of a relation.
    Column(ColumnInfo),
    /// The installed extensions, which belong to the database rather than to
    /// any one schema and so sit at the root.
    Extensions,
    /// A statement about why there is nothing to show.
    Message(String),
}

/// What the interface should draw for a row, without knowing about icons.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowKind {
    /// A schema, and whether the role may use it.
    Schema {
        /// Whether the current role has USAGE on it.
        usable: bool,
    },
    /// A group of objects of one kind.
    Group(ObjectKind),
    /// An object, and whether the current role can read it.
    Object {
        /// What it is.
        kind: ObjectKind,
        /// Whether the current role can read its contents.
        readable: bool,
    },
    /// A column.
    Column {
        /// Whether it is part of the primary key.
        primary_key: bool,
        /// Whether it accepts NULL.
        nullable: bool,
    },
    /// An explanatory message.
    Message,
}

/// One node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// What this node is.
    pub kind: NodeKind,
    /// Children, empty until loaded.
    pub children: Vec<Node>,
    /// Whether children are shown.
    pub expanded: bool,
    /// Whether children have been fetched.
    pub loaded: bool,
    /// The load this node is waiting for, when it is waiting.
    pub pending: Option<RequestId>,
}

impl Node {
    /// A node that has not been opened yet.
    #[must_use]
    pub const fn new(kind: NodeKind) -> Self {
        Self {
            kind,
            children: Vec::new(),
            expanded: false,
            loaded: false,
            pending: None,
        }
    }

    /// Whether this node can have children at all.
    #[must_use]
    pub fn is_expandable(&self) -> bool {
        match &self.kind {
            NodeKind::Schema(summary) => summary.usable && summary.total() > 0,
            NodeKind::Group { count, .. } => *count > 0,
            // Relations show their columns; other objects are leaves.
            NodeKind::Object(object) => matches!(
                object.kind,
                ObjectKind::Table
                    | ObjectKind::View
                    | ObjectKind::MaterializedView
                    | ObjectKind::PartitionedTable
                    | ObjectKind::ForeignTable
            ),
            // The extensions node is always worth opening: how many there are
            // is not known until it is, and a database always has at least
            // plpgsql unless someone removed it.
            NodeKind::Extensions => true,
            NodeKind::Column(_) | NodeKind::Message(_) => false,
        }
    }

    /// The label drawn for this node. Always carries the meaning in words, so
    /// the tree reads correctly with icons switched off.
    #[must_use]
    pub fn label(&self) -> String {
        match &self.kind {
            NodeKind::Schema(summary) => summary.name.clone(),
            NodeKind::Group { kind, count, .. } => format!("{} ({count})", kind.plural()),
            NodeKind::Object(object) => object.name.clone(),
            NodeKind::Column(column) => column.name.clone(),
            NodeKind::Extensions => "extensions".to_owned(),
            NodeKind::Message(text) => text.clone(),
        }
    }

    /// Secondary text drawn after the label.
    #[must_use]
    pub fn detail(&self) -> Option<String> {
        match &self.kind {
            NodeKind::Schema(summary) => {
                if summary.usable {
                    None
                } else {
                    Some("no permission".to_owned())
                }
            }
            NodeKind::Group { .. } | NodeKind::Message(_) | NodeKind::Extensions => None,
            NodeKind::Object(object) => {
                if object.readable {
                    object.detail.clone()
                } else {
                    Some("not readable".to_owned())
                }
            }
            NodeKind::Column(column) => {
                let mut parts = vec![column.data_type.clone()];
                if column.primary_key {
                    parts.push("primary key".to_owned());
                } else if !column.nullable {
                    parts.push("not null".to_owned());
                }
                Some(parts.join(", "))
            }
        }
    }

    /// The presentation kind for this node.
    #[must_use]
    pub fn row_kind(&self) -> RowKind {
        match &self.kind {
            NodeKind::Schema(summary) => RowKind::Schema {
                usable: summary.usable,
            },
            NodeKind::Group { kind, .. } => RowKind::Group(*kind),
            NodeKind::Object(object) => RowKind::Object {
                kind: object.kind,
                readable: object.readable,
            },
            NodeKind::Column(column) => RowKind::Column {
                primary_key: column.primary_key,
                nullable: column.nullable,
            },
            NodeKind::Extensions => RowKind::Group(ObjectKind::Extension),
            NodeKind::Message(_) => RowKind::Message,
        }
    }

    /// The SQL-safe qualified name, for nodes that name something in the database.
    #[must_use]
    pub fn qualified_sql(&self) -> Option<String> {
        match &self.kind {
            NodeKind::Object(object) => Some(object.qualified_sql()),
            NodeKind::Schema(summary) => {
                Some(crate::postgres::metadata::quote_identifier(&summary.name))
            }
            _ => None,
        }
    }
}

/// A flattened row ready to draw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeRow {
    /// Indentation depth.
    pub depth: usize,
    /// The label.
    pub label: String,
    /// Secondary text.
    pub detail: Option<String>,
    /// What to draw it as.
    pub row_kind: RowKind,
    /// Whether it can be opened.
    pub expandable: bool,
    /// Whether it is open.
    pub expanded: bool,
    /// Whether it is waiting for a load.
    pub loading: bool,
    /// Path to the node, for selection and expansion.
    pub path: NodePath,
}

/// What a load should fetch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MetadataQuery {
    /// Every schema, with counts.
    Schemas,
    /// The objects of one kind in one schema.
    Objects {
        /// Schema to look in.
        schema: String,
        /// Kind to list.
        kind: ObjectKind,
    },
    /// What a relation is made of: its columns and its indexes.
    Relation {
        /// Schema the relation lives in.
        schema: String,
        /// Relation name.
        relation: String,
    },
    /// The extensions installed in this database.
    Extensions,
}

/// What a load returned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MetadataPayload {
    /// Schemas with counts.
    Schemas(Vec<SchemaSummary>),
    /// Objects of one kind.
    Objects(Vec<ObjectSummary>),
    /// A relation's columns and the indexes on it, in that order.
    Relation {
        /// Its columns, in attribute order.
        columns: Vec<ColumnInfo>,
        /// The indexes on it.
        indexes: Vec<ObjectSummary>,
    },
}

/// The object tree and its selection.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ObjectTree {
    /// Top-level nodes: the schemas.
    pub roots: Vec<Node>,
    /// Selected row in the flattened list.
    pub selected: usize,
    /// Filter text. Only rows matching it, and their ancestors, are shown.
    pub filter: String,
    /// Whether the filter input has focus.
    pub filtering: bool,
    /// Whether the root load is in flight.
    pub loading: bool,
    /// Set when the tree could not be loaded at all.
    pub error: Option<Diagnostic>,
    /// Counter handing out request identities.
    next_request: u64,
}

impl ObjectTree {
    /// Clears server-derived state while preserving the request counter.
    ///
    /// A connection switch can leave a metadata task in flight. Keeping the
    /// counter means any new request receives a different identity, so an old
    /// answer cannot populate a newly selected connection's tree.
    pub fn reset_for_connection(&mut self) {
        self.roots.clear();
        self.selected = 0;
        self.filter.clear();
        self.filtering = false;
        self.loading = false;
        self.error = None;
        self.next_request = self.next_request.wrapping_add(1).max(1);
    }

    /// Hands out the next request identity.
    pub fn allocate_request(&mut self) -> RequestId {
        self.next_request += 1;
        RequestId(self.next_request)
    }

    /// Marks the root load as started.
    pub fn begin_loading(&mut self) {
        self.loading = true;
        self.error = None;
    }

    /// Replaces the roots with freshly loaded schemas.
    pub fn set_schemas(&mut self, schemas: Vec<SchemaSummary>) {
        self.roots = schemas
            .into_iter()
            .map(|schema| Node::new(NodeKind::Schema(schema)))
            // Extensions belong to the database, not to a schema, so they sit
            // beside the schemas rather than inside one of them.
            .chain(std::iter::once(Node::new(NodeKind::Extensions)))
            .collect();
        self.loading = false;
        self.error = None;
        self.selected = 0;
    }

    /// Records a failure to load the tree.
    pub fn set_error(&mut self, error: Diagnostic) {
        self.loading = false;
        self.error = Some(error);
    }

    /// Borrows a node by path.
    #[must_use]
    pub fn node(&self, path: &[usize]) -> Option<&Node> {
        let mut nodes = &self.roots;
        let mut current = None;
        for index in path {
            let node = nodes.get(*index)?;
            nodes = &node.children;
            current = Some(node);
        }
        current
    }

    /// Borrows a node by path, walking down one level at a time.
    fn node_mut(&mut self, path: &[usize]) -> Option<&mut Node> {
        let (first, rest) = path.split_first()?;
        let mut node = self.roots.get_mut(*first)?;
        for index in rest {
            node = node.children.get_mut(*index)?;
        }
        Some(node)
    }

    /// The rows to draw, in order, honouring expansion and the filter.
    #[must_use]
    pub fn rows(&self) -> Vec<TreeRow> {
        let mut rows = Vec::new();
        let filter = self.filter.trim().to_ascii_lowercase();
        for (index, node) in self.roots.iter().enumerate() {
            collect_rows(node, vec![index], 0, &filter, &mut rows);
        }
        rows
    }

    /// The currently selected row, if any.
    #[must_use]
    pub fn selected_row(&self) -> Option<TreeRow> {
        let rows = self.rows();
        rows.get(self.selected.min(rows.len().saturating_sub(1)))
            .cloned()
    }

    /// Moves the selection, clamped to the visible rows.
    pub fn move_selection(&mut self, delta: isize) {
        let count = self.rows().len();
        if count == 0 {
            self.selected = 0;
            return;
        }
        let current = isize::try_from(self.selected).unwrap_or(0);
        let next = (current + delta).clamp(0, isize::try_from(count - 1).unwrap_or(0));
        self.selected = usize::try_from(next).unwrap_or(0);
    }

    /// Expands the selected node, returning the load it needs.
    ///
    /// Returns `None` when the node cannot be expanded, is already loaded, or is
    /// already waiting for an answer.
    pub fn expand_selected(&mut self) -> Option<(RequestId, NodePath, MetadataQuery)> {
        let row = self.selected_row()?;
        if !row.expandable {
            return None;
        }
        let path = row.path.clone();
        let query = self.query_for(&path)?;

        let request = self.allocate_request();
        let node = self.node_mut(&path)?;
        node.expanded = true;
        if node.loaded || node.pending.is_some() {
            return None;
        }
        node.pending = Some(request);
        Some((request, path, query))
    }

    /// Collapses the selected node, or moves to its parent when already closed.
    pub fn collapse_selected(&mut self) {
        let Some(row) = self.selected_row() else {
            return;
        };
        let path = row.path.clone();
        if row.expanded {
            if let Some(node) = self.node_mut(&path) {
                node.expanded = false;
            }
            return;
        }
        if path.len() > 1 {
            let parent = &path[..path.len() - 1];
            if let Some(index) = self.rows().iter().position(|r| r.path == parent) {
                self.selected = index;
            }
        }
    }

    /// Applies a loaded payload, discarding it when the node moved on.
    ///
    /// Returns whether the payload was used, so a caller can tell a stale answer
    /// from an applied one.
    pub fn apply(&mut self, request: RequestId, path: &[usize], payload: MetadataPayload) -> bool {
        let Some(node) = self.node_mut(path) else {
            return false;
        };
        if node.pending != Some(request) {
            // The node was collapsed and reopened, or the tree was reloaded.
            // Applying this would show an answer to a question nobody asked.
            return false;
        }
        node.pending = None;
        node.loaded = true;
        node.children = match payload {
            MetadataPayload::Schemas(schemas) => schemas
                .into_iter()
                .map(|schema| Node::new(NodeKind::Schema(schema)))
                .collect(),
            MetadataPayload::Objects(objects) => objects
                .into_iter()
                .map(|object| Node::new(NodeKind::Object(object)))
                .collect(),
            // Columns first, then indexes: what the relation holds, then what
            // makes it findable. It is the order `psql` prints them in, and the
            // order people read them in.
            MetadataPayload::Relation { columns, indexes } => columns
                .into_iter()
                .map(|column| Node::new(NodeKind::Column(column)))
                .chain(
                    indexes
                        .into_iter()
                        .map(|index| Node::new(NodeKind::Object(index))),
                )
                .collect(),
        };
        if node.children.is_empty() {
            node.children
                .push(Node::new(NodeKind::Message("nothing here".to_owned())));
        }
        true
    }

    /// Records that a load failed, showing the reason in the tree itself.
    pub fn apply_failure(&mut self, request: RequestId, path: &[usize], reason: &str) -> bool {
        let Some(node) = self.node_mut(path) else {
            return false;
        };
        if node.pending != Some(request) {
            return false;
        }
        node.pending = None;
        node.loaded = true;
        node.children = vec![Node::new(NodeKind::Message(reason.to_owned()))];
        true
    }

    /// The load a node needs when it is opened.
    fn query_for(&self, path: &[usize]) -> Option<MetadataQuery> {
        let node = self.node(path)?;
        match &node.kind {
            NodeKind::Schema(_) => None,
            NodeKind::Group { schema, kind, .. } => Some(MetadataQuery::Objects {
                schema: schema.clone(),
                kind: *kind,
            }),
            NodeKind::Object(object) => Some(MetadataQuery::Relation {
                schema: object.schema.clone(),
                relation: object.name.clone(),
            }),
            NodeKind::Extensions => Some(MetadataQuery::Extensions),
            NodeKind::Column(_) | NodeKind::Message(_) => None,
        }
    }

    /// Builds the group nodes under a schema, which need no round trip because
    /// the counts arrived with the schema list.
    pub fn expand_schema(&mut self, path: &[usize]) -> bool {
        let Some(node) = self.node_mut(path) else {
            return false;
        };
        let NodeKind::Schema(summary) = &node.kind else {
            return false;
        };
        if !summary.usable {
            node.expanded = true;
            node.loaded = true;
            node.children = vec![Node::new(NodeKind::Message(
                "no permission to use this schema".to_owned(),
            ))];
            return true;
        }
        let schema = summary.name.clone();
        let groups: Vec<Node> = ObjectKind::IN_SCHEMA
            .iter()
            .filter_map(|kind| {
                let count = summary.count(*kind);
                (count > 0).then(|| {
                    Node::new(NodeKind::Group {
                        schema: schema.clone(),
                        kind: *kind,
                        count,
                    })
                })
            })
            .collect();
        node.expanded = true;
        node.loaded = true;
        node.children = if groups.is_empty() {
            vec![Node::new(NodeKind::Message("nothing here".to_owned()))]
        } else {
            groups
        };
        true
    }
}

/// Flattens a node and its visible descendants.
fn collect_rows(node: &Node, path: NodePath, depth: usize, filter: &str, rows: &mut Vec<TreeRow>) {
    let label = node.label();
    let matches = filter.is_empty() || label.to_ascii_lowercase().contains(filter);

    let mut child_rows = Vec::new();
    if node.expanded || !filter.is_empty() {
        for (index, child) in node.children.iter().enumerate() {
            let mut child_path = path.clone();
            child_path.push(index);
            collect_rows(child, child_path, depth + 1, filter, &mut child_rows);
        }
    }

    // A node is shown when it matches, or when something beneath it does. That
    // keeps a filtered tree navigable instead of showing orphaned leaves.
    if matches || !child_rows.is_empty() {
        rows.push(TreeRow {
            depth,
            label,
            detail: node.detail(),
            row_kind: node.row_kind(),
            expandable: node.is_expandable(),
            expanded: node.expanded,
            loading: node.pending.is_some(),
            path,
        });
        rows.append(&mut child_rows);
    }
}

#[cfg(test)]
mod tests;
