//! Connection targets, precedence, TLS policy and secret references.

pub mod cloud;
pub mod passfile;
pub mod service;
pub mod target;

pub use passfile::Lookup as PassfileLookup;
pub use service::ServiceFile;
pub use target::{
    ConnectionArgs, ConnectionTarget, DEFAULT_PORT, Endpoint, EnvSnapshot, Environment, Host,
    MAX_ENDPOINTS, MAX_SEARCH_PATH_SCHEMAS, ResolutionNote, SslMode, TargetSessionAttrs,
    format_set_search_path_sql, format_timeout_duration, parse_connection_string,
    parse_search_path, parse_timeout_duration, resolve,
};
