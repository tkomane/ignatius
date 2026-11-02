//! Opt-in local logging.
//!
//! Logging is off unless the user turns it on. Nothing is ever sent off the
//! machine. When it is on:
//!
//! - SQL text and row values are never written. Statements are identified by job
//!   id and length only, so a log can be shared without sharing the query.
//! - Every message passes through [`crate::diagnostics::redaction`].
//! - The file is size-bounded: it is rotated to `.1` once it exceeds the limit,
//!   keeping at most one previous file.

use crate::diagnostics::redaction::redact_text;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use tracing::{Event, Metadata, Subscriber};
use tracing_subscriber::filter::{LevelFilter, Targets, filter_fn};
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::{FmtContext, FormatEvent, FormatFields};
use tracing_subscriber::prelude::*;
use tracing_subscriber::registry::LookupSpan;

/// Maximum size of the active log file before it is rotated.
pub const MAX_LOG_BYTES: u64 = 5 * 1024 * 1024;

/// Environment variable that enables logging at one explicit level, for example
/// `IGNATIUS_LOG=debug`.
pub const LOG_ENV: &str = "IGNATIUS_LOG";

/// The only tracing target namespace allowed into the support log.
///
/// Dependency traces are outside our redaction and data-minimisation contract.
/// In particular, database drivers may log complete statements at debug level.
const INTERNAL_TARGET_PREFIX: &str = "ignatius";

fn internal_filter(level: LevelFilter) -> impl Fn(&Metadata<'_>) -> bool + Clone {
    let levels = Targets::new().with_target(INTERNAL_TARGET_PREFIX, level);
    move |metadata| {
        let target = metadata.target();
        (target == INTERNAL_TARGET_PREFIX || target.starts_with("ignatius::"))
            && levels.would_enable(target, metadata.level())
    }
}

fn parse_level(filter: &str) -> io::Result<LevelFilter> {
    match filter.trim().to_ascii_lowercase().as_str() {
        "off" => Ok(LevelFilter::OFF),
        "error" => Ok(LevelFilter::ERROR),
        "warn" => Ok(LevelFilter::WARN),
        "info" => Ok(LevelFilter::INFO),
        "debug" => Ok(LevelFilter::DEBUG),
        "trace" => Ok(LevelFilter::TRACE),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "IGNATIUS_LOG must be one of off, error, warn, info, debug or trace",
        )),
    }
}

/// Formats one complete event into memory, redacts it, then writes it.
///
/// Formatting the whole event before redaction matters: a streaming writer could
/// split a credential across writes and let the pattern boundary escape review.
#[derive(Debug, Default)]
struct RedactingFormatter;

impl<S, N> FormatEvent<S, N> for RedactingFormatter
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup>,
    N: for<'writer> FormatFields<'writer> + 'static,
{
    fn format_event(
        &self,
        context: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        let mut rendered = String::new();
        tracing_subscriber::fmt::format().format_event(
            context,
            Writer::new(&mut rendered),
            event,
        )?;
        writer.write_str(&redact_text(&rendered))
    }
}

/// Result of initialising logging, reported by `doctor` and `version --verbose`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoggingState {
    /// No log filter was requested; nothing is written.
    Disabled,
    /// Logging is active and writing to this file.
    Enabled {
        /// Path of the active log file.
        path: PathBuf,
        /// The filter directive in use.
        filter: String,
    },
    /// Logging was requested but could not be started.
    Unavailable {
        /// Why it could not be started; already redacted.
        reason: String,
    },
}

/// Initialises logging from the environment. Safe to call once per process.
///
/// Returns the resulting state rather than failing: a broken log destination must
/// never stop the user from querying their database.
#[must_use]
pub fn init(log_dir: &Path) -> LoggingState {
    let Ok(filter) = std::env::var(LOG_ENV) else {
        return LoggingState::Disabled;
    };
    if filter.trim().is_empty() {
        return LoggingState::Disabled;
    }

    let path = log_dir.join("ignatius.log");
    match start(&path, &filter) {
        Ok(()) => LoggingState::Enabled {
            path,
            filter: filter.clone(),
        },
        Err(err) => LoggingState::Unavailable {
            reason: redact_text(&err.to_string()),
        },
    }
}

fn start(path: &Path, filter: &str) -> io::Result<()> {
    // Validate before touching the log path. A rejected value must not create,
    // append to or rotate any file as a side effect.
    let level = parse_level(filter)?;
    let filter = filter_fn(internal_filter(level));

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    rotate_if_needed(path)?;
    let file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    crate::platform::restrict_to_owner(path)?;

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .event_format(RedactingFormatter)
                .with_writer(file)
                .with_ansi(false)
                .with_filter(filter),
        )
        .try_init()
        .map_err(|e| io::Error::other(e.to_string()))
}

/// Rotates the log when it has grown past [`MAX_LOG_BYTES`], keeping one old file.
fn rotate_if_needed(path: &Path) -> io::Result<()> {
    let Ok(meta) = fs::metadata(path) else {
        return Ok(());
    };
    if meta.len() <= MAX_LOG_BYTES {
        return Ok(());
    }
    let rotated = path.with_extension("log.1");
    let _ = fs::remove_file(&rotated);
    fs::rename(path, rotated)
}

/// Describes a statement for logging without revealing its text.
///
/// The job id is a plain counter and the length is a character count; neither
/// encodes the SQL or any connection secret.
#[must_use]
pub fn statement_descriptor(job_id: u64, sql: &str) -> String {
    format!("job={job_id} statement_chars={}", sql.chars().count())
}

#[cfg(test)]
mod tests;
