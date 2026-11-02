//! Structured PostgreSQL query plans.
//!
//! This module owns the meaning of a plan, not its presentation or transport.
//! PostgreSQL's JSON EXPLAIN response is parsed into a bounded tree so the
//! interactive client can render it without treating the server's text as
//! terminal control input or allowing an unexpectedly large plan to consume
//! unbounded memory.

use serde::Deserialize;
use serde_json::{Map, Value};
use std::fmt;

/// Maximum JSON response accepted for one plan.
pub const MAX_PLAN_JSON_BYTES: usize = 1024 * 1024;
/// Maximum number of nodes retained in one plan document.
pub const MAX_PLAN_NODES: usize = 500;
/// Maximum number of node levels retained in one plan document.
pub const MAX_PLAN_DEPTH: usize = 64;

// One plan level is represented by a node object, a Plans array, and a child
// node object in JSON. The small allowance covers the top-level envelope and
// harmless nested values while keeping the explicitly unbounded JSON parser
// below a safe structural depth.
const MAX_JSON_NESTING: usize = MAX_PLAN_DEPTH * 3 + 16;

const DISPLAY_FACTS: &[&str] = &[
    "Alias",
    "Buffers",
    "CTE Name",
    "Filter",
    "Function Call",
    "Function Name",
    "Group Key",
    "Hash Cond",
    "Index Cond",
    "Join Filter",
    "Merge Cond",
    "Output",
    "Parallel Aware",
    "Recheck Cond",
    "Relation Name",
    "Rows Removed by Filter",
    "Rows Removed by Index Recheck",
    "Schema",
    "Sort Key",
    "Subplan Name",
    "Temp Read Blocks",
    "Temp Written Blocks",
    "Workers Launched",
    "Workers Planned",
];

/// Why a structured plan response could not become a truthful document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanParseError {
    /// The response exceeds the client-side memory boundary.
    ResponseTooLarge,
    /// The response is not valid JSON.
    MalformedJson,
    /// The JSON does not have PostgreSQL's plan envelope.
    MissingRoot,
    /// A node is missing a required operation name or has the wrong shape.
    InvalidNode,
}

impl fmt::Display for PlanParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ResponseTooLarge => "the plan response exceeded the display bound",
            Self::MalformedJson => "the plan response was not valid JSON",
            Self::MissingRoot => "the plan response did not contain a Plan root",
            Self::InvalidNode => "the plan response contained an invalid plan node",
        })
    }
}

impl std::error::Error for PlanParseError {}

/// The metric used to mark the node needing attention.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttentionBasis {
    /// Planner cost units from a non-executing EXPLAIN.
    EstimatedCost,
    /// Observed total time multiplied by loops from EXPLAIN ANALYZE.
    MeasuredTime,
}

impl AttentionBasis {
    /// The words shown beside the attention marker.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::EstimatedCost => "estimated cost",
            Self::MeasuredTime => "measured time",
        }
    }
}

/// One bounded JSON EXPLAIN response.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanDocument {
    /// Whether PostgreSQL executed the target while collecting metrics.
    pub analyzed: bool,
    /// The top-level plan node.
    pub root: PlanNode,
    /// Server-reported planning time, in milliseconds.
    pub planning_time_ms: Option<f64>,
    /// Server-reported total execution time, in milliseconds.
    pub execution_time_ms: Option<f64>,
    /// Number of nodes retained in the bounded tree.
    pub node_count: usize,
    /// Whether the valid response exceeded a display bound.
    pub truncated: bool,
    /// Path to the node with the largest relevant attention score.
    pub attention_path: Vec<usize>,
}

impl PlanDocument {
    /// Parses PostgreSQL's JSON EXPLAIN envelope.
    pub fn parse(json: &str, analyzed: bool) -> Result<Self, PlanParseError> {
        if json.len() > MAX_PLAN_JSON_BYTES {
            return Err(PlanParseError::ResponseTooLarge);
        }
        if !json_nesting_within_bound(json) {
            return Err(PlanParseError::MalformedJson);
        }
        let mut deserializer = serde_json::Deserializer::from_str(json);
        deserializer.disable_recursion_limit();
        let value = Value::deserialize(&mut deserializer)
            .and_then(|value| deserializer.end().map(|()| value))
            .map_err(|_| PlanParseError::MalformedJson)?;
        let Some(entries) = value.as_array() else {
            return Err(PlanParseError::MissingRoot);
        };
        let Some(envelope) = entries.first().and_then(Value::as_object) else {
            return Err(PlanParseError::MissingRoot);
        };
        let Some(root_value) = envelope.get("Plan") else {
            return Err(PlanParseError::MissingRoot);
        };

        let mut budget = ParseBudget::default();
        let (root, truncated) = parse_node(root_value, 0, &mut budget)?;
        let attention_path = root.attention_path(analyzed).unwrap_or_default();

        Ok(Self {
            analyzed,
            root,
            planning_time_ms: finite_number(envelope.get("Planning Time")),
            execution_time_ms: finite_number(envelope.get("Execution Time")),
            node_count: budget.nodes,
            truncated,
            attention_path,
        })
    }

    /// The metric used by the attention marker for this document.
    #[must_use]
    pub const fn attention_basis(&self) -> AttentionBasis {
        if self.analyzed {
            AttentionBasis::MeasuredTime
        } else {
            AttentionBasis::EstimatedCost
        }
    }

    /// Whether a path points at the node marked for attention.
    #[must_use]
    pub fn is_attention_path(&self, path: &[usize]) -> bool {
        self.has_attention() && self.attention_path == path
    }

    /// Whether any node has the metric needed for an attention marker.
    #[must_use]
    pub fn has_attention(&self) -> bool {
        self.root.attention_score(self.analyzed).is_some()
    }
}

/// One operation in a PostgreSQL plan tree.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanNode {
    /// PostgreSQL's operation name, for example `Seq Scan`.
    pub node_type: String,
    /// Relation name when the server supplied one.
    pub relation: Option<String>,
    /// Index name when the server supplied one.
    pub index: Option<String>,
    /// Estimated startup cost in planner cost units.
    pub startup_cost: Option<f64>,
    /// Estimated total cost in planner cost units.
    pub total_cost: Option<f64>,
    /// Estimated rows per node execution.
    pub plan_rows: Option<f64>,
    /// Estimated row width.
    pub plan_width: Option<f64>,
    /// Observed startup time per loop, in milliseconds.
    pub actual_startup_time: Option<f64>,
    /// Observed total time per loop, in milliseconds.
    pub actual_total_time: Option<f64>,
    /// Observed rows per loop.
    pub actual_rows: Option<f64>,
    /// Number of times this node ran.
    pub actual_loops: Option<f64>,
    /// Bounded supplementary server facts retained for the UI.
    pub facts: Vec<PlanFact>,
    /// Child operations in PostgreSQL's order.
    pub children: Vec<PlanNode>,
}

impl PlanNode {
    /// The actual-to-estimated row ratio when the comparison is meaningful.
    #[must_use]
    pub fn estimate_ratio(&self) -> Option<f64> {
        let actual = self.actual_rows?;
        let estimated = self.plan_rows?;
        if !actual.is_finite() || !estimated.is_finite() || estimated <= 0.0 {
            return None;
        }
        Some(actual / estimated)
    }

    /// Whether the observed row count differs by at least one order of
    /// magnitude from the estimate.
    #[must_use]
    pub fn estimate_mismatch(&self) -> bool {
        self.estimate_ratio()
            .is_some_and(|ratio| ratio >= 10.0 || ratio <= 0.1)
    }

    /// The score used for the document's attention marker.
    #[must_use]
    pub fn attention_score(&self, analyzed: bool) -> Option<f64> {
        if analyzed {
            let time = self.actual_total_time?;
            let loops = self.actual_loops?;
            if time.is_finite() && loops.is_finite() && time >= 0.0 && loops > 0.0 {
                Some(time * loops)
            } else {
                None
            }
        } else {
            self.total_cost
                .filter(|cost| cost.is_finite() && *cost >= 0.0)
        }
    }

    /// Finds the path to the highest-scoring node, preserving first-seen order
    /// when scores tie.
    #[must_use]
    pub fn attention_path(&self, analyzed: bool) -> Option<Vec<usize>> {
        fn visit(
            node: &PlanNode,
            analyzed: bool,
            path: &mut Vec<usize>,
            best: &mut Option<(f64, Vec<usize>)>,
        ) {
            if let Some(score) = node.attention_score(analyzed)
                && best
                    .as_ref()
                    .is_none_or(|(best_score, _)| score > *best_score)
            {
                *best = Some((score, path.clone()));
            }
            for (index, child) in node.children.iter().enumerate() {
                path.push(index);
                visit(child, analyzed, path, best);
                path.pop();
            }
        }

        let mut best = None;
        visit(self, analyzed, &mut Vec::new(), &mut best);
        best.map(|(_, path)| path)
    }

    /// Finds a node by its child-index path.
    #[must_use]
    pub fn at_path(&self, path: &[usize]) -> Option<&Self> {
        let mut node = self;
        for index in path {
            node = node.children.get(*index)?;
        }
        Some(node)
    }
}

/// A supplementary field retained for a plan node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanFact {
    /// PostgreSQL's field label.
    pub label: String,
    /// JSON value rendered compactly for the UI to sanitise and truncate.
    pub value: String,
}

#[derive(Default)]
struct ParseBudget {
    nodes: usize,
}

fn parse_node(
    value: &Value,
    depth: usize,
    budget: &mut ParseBudget,
) -> Result<(PlanNode, bool), PlanParseError> {
    let Some(object) = value.as_object() else {
        return Err(PlanParseError::InvalidNode);
    };
    let Some(node_type) = object.get("Node Type").and_then(Value::as_str) else {
        return Err(PlanParseError::InvalidNode);
    };
    if node_type.trim().is_empty() {
        return Err(PlanParseError::InvalidNode);
    }
    if budget.nodes >= MAX_PLAN_NODES {
        return Err(PlanParseError::InvalidNode);
    }
    budget.nodes += 1;

    let mut truncated = false;
    let mut children = Vec::new();
    if let Some(plans) = object.get("Plans") {
        let Some(plans) = plans.as_array() else {
            return Err(PlanParseError::InvalidNode);
        };
        if depth + 1 >= MAX_PLAN_DEPTH {
            truncated = !plans.is_empty();
        } else {
            for child in plans {
                if budget.nodes >= MAX_PLAN_NODES {
                    truncated = true;
                    break;
                }
                let (child, child_truncated) = parse_node(child, depth + 1, budget)?;
                truncated |= child_truncated;
                children.push(child);
            }
            if children.len() < plans.len() {
                truncated = true;
            }
        }
    }

    Ok((
        PlanNode {
            node_type: node_type.to_owned(),
            relation: string_field(object, "Relation Name"),
            index: string_field(object, "Index Name"),
            startup_cost: finite_number(object.get("Startup Cost")),
            total_cost: finite_number(object.get("Total Cost")),
            plan_rows: finite_number(object.get("Plan Rows")),
            plan_width: finite_number(object.get("Plan Width")),
            actual_startup_time: finite_number(object.get("Actual Startup Time")),
            actual_total_time: finite_number(object.get("Actual Total Time")),
            actual_rows: finite_number(object.get("Actual Rows")),
            actual_loops: finite_number(object.get("Actual Loops")),
            facts: facts(object),
            children,
        },
        truncated,
    ))
}

fn string_field(object: &Map<String, Value>, key: &str) -> Option<String> {
    object.get(key).and_then(Value::as_str).map(str::to_owned)
}

fn finite_number(value: Option<&Value>) -> Option<f64> {
    value
        .and_then(Value::as_f64)
        .filter(|number| number.is_finite())
}

fn json_nesting_within_bound(json: &str) -> bool {
    let mut depth = 0usize;
    let mut quoted = false;
    let mut escaped = false;
    for ch in json.chars() {
        if quoted {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                quoted = false;
            }
            continue;
        }
        match ch {
            '"' => quoted = true,
            '{' | '[' => {
                depth += 1;
                if depth > MAX_JSON_NESTING {
                    return false;
                }
            }
            '}' | ']' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    !quoted
}

fn facts(object: &Map<String, Value>) -> Vec<PlanFact> {
    DISPLAY_FACTS
        .iter()
        .filter_map(|label| {
            let value = object.get(*label)?;
            let value = match value {
                Value::String(text) => text.clone(),
                Value::Null => return None,
                other => other.to_string(),
            };
            Some(PlanFact {
                label: (*label).to_owned(),
                value,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests;
