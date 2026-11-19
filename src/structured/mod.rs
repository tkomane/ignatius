//! Bounded views and edits of PostgreSQL's retained, server-rendered text.
//! A tree describes spans of the source. It never serializes the untouched
//! parts of a value through an intermediate numeric or text representation.

mod spatial;
mod text;

use std::ops::Range;

pub(crate) const MAX_BYTES: usize = 64 * 1024;
const MAX_DEPTH: usize = 32;
const MAX_NODES: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Jsonb,
    Array { delimiter: u8 },
    Range,
    Geometry,
    Geography,
    Vector,
    HalfVector,
    SparseVector,
}

impl Kind {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Jsonb => "JSONB",
            Self::Array { .. } => "Array",
            Self::Range => "Range",
            Self::Geometry => "Geometry",
            Self::Geography => "Geography",
            Self::Vector => "Vector",
            Self::HalfVector => "Half vector",
            Self::SparseVector => "Sparse vector",
        }
    }
}

/// Provenance from PostgreSQL's column description, carried into a guarded write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Origin {
    pub(crate) table: u32,
    pub(crate) attribute: i16,
    pub(crate) type_oid: u32,
    pub(crate) typmod: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Column {
    pub(crate) kind: Kind,
    pub(crate) origin: Option<Origin>,
    pub(crate) partial_edit: bool,
}

impl Column {
    pub(crate) fn edit_input(self, tree: &Tree<'_>, node: usize) -> Option<Input> {
        self.origin?;
        if node == 0 || self.partial_edit {
            tree.input(node)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Input {
    Whole,
    Json,
    ArrayElement(u8),
    RangeBound,
    Coordinate,
}

impl Input {
    pub(crate) const fn hint(self) -> &'static str {
        match self {
            Self::Whole => "Enter a whole PostgreSQL value in the type's input syntax.",
            Self::Json => "Enter one JSON value. Use double quotes for a JSON string.",
            Self::ArrayElement(_) => "Enter one PostgreSQL array element. Quote text NULL.",
            Self::RangeBound => "Enter one PostgreSQL range bound. Empty means unbounded.",
            Self::Coordinate => "Enter one finite numeric coordinate.",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Error {
    Limit,
    Invalid,
    Unsupported,
    NotEditable,
}

impl Error {
    pub(crate) const fn description(self) -> &'static str {
        match self {
            Self::Limit => "Structured view limited to 64 KiB, 32 levels and 512 nodes.",
            Self::Invalid => "Value does not match the supported type syntax.",
            Self::Unsupported => "This representation is available as server text.",
            Self::NotEditable => "Select an editable node in a complete structured view.",
        }
    }
}

pub(crate) type Parsed<T> = Result<T, Error>;

pub(crate) struct Node {
    pub(crate) parent: Option<usize>,
    pub(crate) label: String,
    pub(crate) depth: usize,
    pub(crate) summary: Option<String>,
    span: Range<usize>,
    input: Option<Input>,
}

pub(crate) struct Tree<'a> {
    raw: &'a str,
    kind: Kind,
    pub(crate) nodes: Vec<Node>,
    pub(crate) truncated: bool,
}

impl<'a> Tree<'a> {
    pub(crate) fn parse(kind: Kind, raw: &'a str) -> Parsed<Self> {
        if raw.len() > MAX_BYTES {
            return Err(Error::Limit);
        }
        let mut tree = Self {
            raw,
            kind,
            nodes: Vec::new(),
            truncated: false,
        };
        let result = match kind {
            Kind::Geometry | Kind::Geography => spatial::parse(&mut tree, raw),
            _ => text::parse(&mut tree, kind, raw),
        };
        match result {
            Ok(()) => Ok(tree),
            Err(Error::Limit) if !tree.nodes.is_empty() => {
                tree.truncated = true;
                Ok(tree)
            }
            Err(error) => Err(error),
        }
    }

    fn add(&mut self, parent: Option<usize>, label: String) -> Parsed<usize> {
        let depth = parent.map_or(0, |index| self.nodes[index].depth + 1);
        if depth >= MAX_DEPTH || self.nodes.len() >= MAX_NODES {
            return Err(Error::Limit);
        }
        let index = self.nodes.len();
        self.nodes.push(Node {
            parent,
            label,
            depth,
            summary: None,
            span: 0..0,
            input: None,
        });
        Ok(index)
    }

    fn set_text(&mut self, index: usize, span: Range<usize>, input: Option<Input>) {
        self.nodes[index].span = span;
        self.nodes[index].input = input;
    }

    fn summarize(&mut self, index: usize, summary: impl Into<String>) {
        self.nodes[index].summary = Some(summary.into());
    }

    pub(crate) fn preview(&self, index: usize) -> &str {
        let node = &self.nodes[index];
        node.summary
            .as_deref()
            .unwrap_or(&self.raw[node.span.clone()])
    }

    pub(crate) fn path(&self, mut index: usize) -> String {
        let mut segments = Vec::new();
        while let Some(parent) = self.nodes[index].parent {
            segments.push(self.nodes[index].label.as_str());
            index = parent;
        }
        segments.push("$");
        segments.reverse();
        segments.join(" / ")
    }

    pub(crate) fn input(&self, index: usize) -> Option<Input> {
        self.nodes
            .get(index)
            .filter(|_| !self.truncated)
            .and_then(|node| node.input)
    }

    pub(crate) fn replace(&self, index: usize, replacement: &str) -> Parsed<String> {
        let input = self.input(index).ok_or(Error::NotEditable)?;
        let span = &self.nodes[index].span;
        if replacement.contains('\0') {
            return Err(Error::Invalid);
        }
        let length = self.raw.len() - span.len() + replacement.len();
        if length > MAX_BYTES {
            return Err(Error::Limit);
        }
        if input == Input::Whole && matches!(self.kind, Kind::Geometry | Kind::Geography) {
            if replacement.trim().is_empty() {
                return Err(Error::Invalid);
            }
            // The server validates its spatial input, including EWKT, dimensionality,
            // SRID and column constraints. No coordinate is reserialized by the client.
            return Ok(replacement.to_owned());
        }
        text::validate_input(input, replacement)?;
        let mut value = String::with_capacity(length);
        value.push_str(&self.raw[..span.start]);
        value.push_str(replacement);
        value.push_str(&self.raw[span.end..]);
        let patched = Tree::parse(self.kind, &value)?;
        if patched.truncated {
            return Err(Error::Limit);
        }
        Ok(value)
    }
}

#[cfg(test)]
mod tests;
