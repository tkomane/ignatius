//! Ignatius - a full-powered PostgreSQL workbench for the terminal.
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
pub mod analytics;
pub mod app;
pub mod audit;
pub mod bench;
pub mod branding;
pub mod buffer;
pub mod cli;
pub mod clipboard;
pub mod cluster;
pub mod config;
pub mod connection;
pub mod crud;
pub mod dbdoc;
pub mod ddl;
pub mod diagnostics;
pub mod diagram;
pub mod drill;
pub mod exit_code;
mod fleet;
pub mod growth;
pub mod history;
pub mod lint;
pub mod mask;
pub mod metacommand;
pub mod migrate;
pub mod migration;
pub mod ops;
pub mod partition;
pub mod perf;
mod plan_history;
pub mod plandiff;
pub mod platform;
pub mod pool;
pub mod postgres;
pub mod queries;
pub mod query;
pub mod recovery;
pub mod replication;
pub mod routine;
pub mod runbook;
pub mod seed;
pub mod snapshot;
pub mod snippet;
mod structured;
pub mod sync;
pub mod telemetry;
pub mod transfer;
pub mod ui;

pub use exit_code::ExitCode;
