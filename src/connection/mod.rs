//! Connection targets, precedence, TLS policy and secret references.

pub mod target;

pub use target::{
    ConnectionArgs, ConnectionTarget, DEFAULT_PORT, EnvSnapshot, Environment, Host, ResolutionNote,
    SslMode, parse_connection_string, resolve,
};
