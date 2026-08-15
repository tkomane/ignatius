//! Configuration paths, schema, validation, atomic writes and migration.

pub mod paths;
pub mod schema;
pub mod store;

pub use paths::Paths;
pub use schema::{
    CURRENT_SCHEMA_VERSION, ColorMode, Config, ConnectionConfig, GlyphMode, QueryConfig,
    ThemeChoice, UiConfig, ValidationIssue,
};
pub use store::{ConfigSource, Loaded, MigrationReport, load, migrate, save};
