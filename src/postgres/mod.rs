//! The PostgreSQL adapter: connection, execution, cancellation, error mapping.
//!
//! This is the only module that knows the driver exists. Everything above it
//! works with the pure types in [`crate::query`] and [`crate::diagnostics`].

pub mod error;
pub mod metadata;
pub mod session;
pub mod tls;

pub use metadata::{ColumnInfo, ObjectKind, ObjectSummary, SchemaSummary, quote_identifier};
pub use session::{CancelHandle, Session, SessionInfo, StreamEvent, StreamStop, connect};
pub use tls::TlsState;
