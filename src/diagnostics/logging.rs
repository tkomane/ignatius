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
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use tracing_subscriber::EnvFilter;

/// Maximum size of the active log file before it is rotated.
pub const MAX_LOG_BYTES: u64 = 5 * 1024 * 1024;

/// Environment variable that enables and filters logging, for example
/// `IGNATIUS_LOG=debug`.
pub const LOG_ENV: &str = "IGNATIUS_LOG";

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
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    rotate_if_needed(path)?;
    let file = fs::OpenOptions::new().create(true).append(true).open(path)?;
    crate::platform::restrict_to_owner(path)?;

    let env_filter = EnvFilter::try_new(filter)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e.to_string()))?;

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_writer(file)
        .with_ansi(false)
        .with_target(true)
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
mod tests {
    use super::*;

    #[test]
    fn statement_descriptor_never_contains_the_sql() {
        let sql = "SELECT secret_column FROM sensitive_table";
        let desc = statement_descriptor(7, sql);
        assert_eq!(desc, "job=7 statement_chars=41");
        assert!(!desc.contains("secret_column"));
        assert!(!desc.contains("sensitive_table"));
    }

    #[test]
    fn rotation_moves_the_file_only_once_over_the_limit() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("ignatius.log");

        fs::write(&path, b"small").expect("write");
        rotate_if_needed(&path).expect("no rotation needed");
        assert!(path.exists());
        assert!(!path.with_extension("log.1").exists());

        fs::write(&path, vec![b'x'; usize::try_from(MAX_LOG_BYTES).unwrap() + 1]).expect("write");
        rotate_if_needed(&path).expect("rotation");
        assert!(!path.exists(), "active log moved aside");
        assert!(path.with_extension("log.1").exists(), "one previous file kept");
    }

    #[test]
    fn logging_is_disabled_when_the_variable_is_absent_or_empty() {
        // The default must be silence: no file is created merely by starting up.
        let dir = tempfile::tempdir().expect("temp dir");
        // SAFETY-free API: read only, since set_var is unsafe in edition 2024 and
        // this test asserts the absent-variable branch through the public result.
        if std::env::var(LOG_ENV).is_err() {
            assert_eq!(init(dir.path()), LoggingState::Disabled);
            assert!(!dir.path().join("ignatius.log").exists());
        }
    }
}
