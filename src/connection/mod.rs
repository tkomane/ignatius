//! Connection targets, precedence, TLS policy and secret references.

pub mod passfile;
pub mod service;
pub mod target;

pub use passfile::Lookup as PassfileLookup;
pub use service::ServiceFile;
pub use target::{
    ConnectionArgs, ConnectionTarget, DEFAULT_PORT, EnvSnapshot, Environment, Host, ResolutionNote,
    SslMode, parse_connection_string, resolve,
};
