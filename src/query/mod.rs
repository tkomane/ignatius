//! Statement boundaries, result model, value rendering and output formats.

pub mod statements;
pub mod value;

pub mod classify;
pub mod completion;
pub mod error_location;
pub mod export;
pub mod format;
pub mod highlight;
pub mod identifiers;
pub mod parameters;
pub mod plan;
pub mod result;
pub mod update;

pub use classify::{Impact, classify, classify_all};
pub use export::{Abandoned, Export, Finished};
pub use format::{FormatError, FormattedSql, MAX_FORMAT_BYTES, ProtectedRegion, format_sql};
pub use highlight::{TokenKind, kind_at, tokens};
pub use identifiers::quote_identifier;
pub use parameters::{
    MAX_DISTINCT_PARAMETERS, NamedPlaceholder, ParameterBindings, ParameterError,
    ParameterTemplate, discover as discover_parameters,
};
pub use plan::{AttentionBasis, PlanDocument, PlanFact, PlanNode, PlanParseError};
pub use result::{Execution, ExecutionStatus, JobId, Notice, ResultSet, StatementResult};
pub use statements::{Statement, split, statement_at};
pub use update::{
    MAX_UPDATE_SQL_BYTES, RelationReference, UpdateColumn, UpdatePlan, UpdateRefusal, UpdateSource,
    parse_source as parse_update_source, plan_update,
};
pub use value::{Cell, NULL_MARKER, display_width, sanitize_for_display, truncate_to_width};
