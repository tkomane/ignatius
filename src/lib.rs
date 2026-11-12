//! Ignatius - a terminal-native PostgreSQL workbench.
//!
//! The library carries all domain behaviour. The binary in `src/main.rs` is a thin
//! process entry point. Nothing below `ui` may depend on the terminal, and nothing
//! in `ui` may open a database connection or write a file: side effects are owned by
//! the application layer and executed behind the ports in [`postgres`] and [`config`].
//!
//! Module map:
//!
//! - [`app`] - application model, messages, pure reducer, effect requests
//! - [`cli`] - command tree, output contracts, exit codes
//! - [`config`] - paths, schema, validation, atomic writes, migration
//! - [`connection`] - connection target resolution, TLS policy, secret references
//! - [`postgres`] - driver adapter, execution, cancellation, error mapping
//! - [`queries`] - saved queries, as ordinary `.sql` files
//! - [`query`] - statement boundaries, jobs, result model, value rendering
//! - [`ui`] - terminal lifecycle, theme tokens, layout, keymap
//! - [`diagnostics`] - redaction, layered diagnostics, logging, doctor
//! - [`history`] - what was run, and the rules about keeping it
//! - [`platform`] - narrow macOS, Windows and Linux differences

pub mod activity;
pub mod advisor;
pub mod app;
pub mod audit;
pub mod branding;
pub mod buffer;
pub mod cli;
pub mod clipboard;
pub mod config;
pub mod connection;
pub mod ddl;
pub mod diagnostics;
pub mod diagram;
pub mod exit_code;
pub mod history;
pub mod mask;
pub mod migration;
pub mod ops;
pub mod perf;
pub mod platform;
pub mod pool;
pub mod postgres;
pub mod queries;
pub mod query;
pub mod routine;
pub mod seed;
pub mod sync;
pub mod telemetry;
pub mod transfer;
pub mod ui;

pub use exit_code::ExitCode;
