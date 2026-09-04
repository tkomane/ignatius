//! Statement boundaries, result model, value rendering and output formats.

pub mod statements;
pub mod value;

pub mod classify;
pub mod completion;
pub mod export;
pub mod highlight;
pub mod identifiers;
pub mod result;

pub use classify::{Impact, classify, classify_all};
pub use export::{Abandoned, Export, Finished};
pub use highlight::{TokenKind, kind_at, tokens};
pub use identifiers::quote_identifier;
pub use result::{Execution, ExecutionStatus, JobId, Notice, ResultSet, StatementResult};
pub use statements::{Statement, split, statement_at};
pub use value::{Cell, NULL_MARKER, display_width, sanitize_for_display, truncate_to_width};
