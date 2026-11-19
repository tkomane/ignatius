//! The PostgreSQL adapter: connection, execution, cancellation, error mapping.
//!
//! This is the only module that knows the driver exists. Everything above it
//! works with the pure types in [`crate::query`] and [`crate::diagnostics`].

pub mod bench;
pub mod cluster;
pub mod crud;
mod dbdoc;
pub mod drill;
pub mod error;
mod fleet;
pub mod growth;
pub mod metadata;
mod notices;
pub mod partition;
pub(crate) mod plan_history;
pub mod plandiff;
pub mod replication;
pub mod session;
pub mod snapshot;
mod structured;
pub mod tls;

pub use metadata::{ColumnInfo, ObjectKind, ObjectSummary, SchemaSummary, quote_identifier};
pub use session::{
    CancelHandle, Session, SessionInfo, StreamEvent, StreamStop, connect, connect_reported,
};
pub use tls::TlsState;
