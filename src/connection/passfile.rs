//! Password files, in PostgreSQL's own `.pgpass` format.
//!
//! Reference: <https://www.postgresql.org/docs/current/libpq-pgpass.html>
//!
//! This is the credential route that keeps a password off the command line, out
//! of the environment, and out of shell history, which is why it matters more
//! than its size suggests.
//!
//! Two behaviours are deliberate and differ from simply doing what libpq does:
//!
//! - A file with permissions that let anyone else read it is **not used**, and
//!   the user is told why. libpq also refuses, but quietly enough to be missed.
//! - The outcome is always reported, including "found the file, no line matched".
//!   Silence there is how people lose an hour to a wildcard in the wrong column.

use crate::diagnostics::redact_text;
use secrecy::SecretString;
use std::path::{Path, PathBuf};

/// What happened when the password file was consulted.
///
/// Deliberately not comparable: a secret should never be compared by accident,
/// and the outcomes that matter are matched by shape rather than by equality.
#[derive(Debug, Clone)]
pub enum Lookup {
    /// No file exists at the path.
    NotFound {
        /// Where it was looked for.
        path: PathBuf,
    },
    /// The file exists but others can read it, so it was not used.
    Insecure {
        /// Where it is.
        path: PathBuf,
        /// The permission bits, for the message.
        mode: u32,
    },
    /// The file was read and no line matched this connection.
    NoMatch {
        /// Where it is.
        path: PathBuf,
        /// How many usable lines it had, so an empty file is distinguishable.
        entries: usize,
    },
    /// A line matched.
    Matched {
        /// Where it is.
        path: PathBuf,
        /// The password. Never displayed.
        password: SecretString,
        /// Which line matched, one-based, so the user can find it.
        line: usize,
    },
    /// The file could not be read.
    Unreadable {
        /// Where it is.
        path: PathBuf,
        /// Why, already redacted.
        reason: String,
    },
}

impl Lookup {
    /// The password, when one was found.
    #[must_use]
    pub fn password(&self) -> Option<&SecretString> {
        match self {
            Self::Matched { password, .. } => Some(password),
            _ => None,
        }
    }

    /// A note worth showing the user, when there is one.
    ///
    /// A match is silent: the user asked for this and it worked. Everything else
    /// explains itself, because a password file that is not being used looks
    /// exactly like one that is until the connection fails.
    #[must_use]
    pub fn note(&self) -> Option<String> {
        match self {
            Self::NotFound { .. } | Self::Matched { .. } => None,
            Self::Insecure { path, mode } => Some(format!(
                "{} is readable by others (mode {:04o}), so it was not used. \
                 Run: chmod 600 {}",
                path.display(),
                mode & 0o777,
                path.display()
            )),
            Self::NoMatch { path, entries } => Some(format!(
                "no line in {} matched this host, port, database and user ({entries} line(s) read)",
                path.display()
            )),
            Self::Unreadable { path, reason } => {
                Some(format!("{} could not be read: {reason}", path.display()))
            }
        }
    }
}

/// The default location, honouring `PGPASSFILE`.
///
/// Reference: `PGPASSFILE`, then `~/.pgpass` on Unix and
/// `%APPDATA%\postgresql\pgpass.conf` on Windows.
#[must_use]
pub fn default_path() -> Option<PathBuf> {
    if let Some(explicit) = std::env::var_os("PGPASSFILE").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(explicit));
    }
    #[cfg(unix)]
    {
        directories::BaseDirs::new().map(|dirs| dirs.home_dir().join(".pgpass"))
    }
    #[cfg(windows)]
    {
        std::env::var_os("APPDATA").map(|appdata| {
            PathBuf::from(appdata)
                .join("postgresql")
                .join("pgpass.conf")
        })
    }
}

/// Looks up a password for a connection.
///
/// `host` should be the host name as the user gave it, or the socket directory.
/// libpq matches a socket connection against `localhost`, and so does this.
#[must_use]
pub fn lookup(path: &Path, host: &str, port: u16, database: &str, user: &str) -> Lookup {
    if !path.exists() {
        return Lookup::NotFound {
            path: path.to_path_buf(),
        };
    }

    if let Some(mode) = insecure_mode(path) {
        return Lookup::Insecure {
            path: path.to_path_buf(),
            mode,
        };
    }

    let contents = match std::fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(err) => {
            return Lookup::Unreadable {
                path: path.to_path_buf(),
                reason: redact_text(&err.to_string()),
            };
        }
    };

    let host = if host.starts_with('/') {
        "localhost"
    } else {
        host
    };
    let port = port.to_string();
    let mut entries = 0usize;

    for (index, raw) in contents.lines().enumerate() {
        let line = raw.trim_end();
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let Some(fields) = split_fields(line) else {
            continue;
        };
        entries += 1;
        if matches(&fields[0], host)
            && matches(&fields[1], &port)
            && matches(&fields[2], database)
            && matches(&fields[3], user)
        {
            return Lookup::Matched {
                path: path.to_path_buf(),
                password: SecretString::from(fields[4].clone()),
                line: index + 1,
            };
        }
    }

    Lookup::NoMatch {
        path: path.to_path_buf(),
        entries,
    }
}

/// Whether a field matches, honouring the `*` wildcard.
fn matches(field: &str, value: &str) -> bool {
    field == "*" || field == value
}

/// Splits a line into its five fields on unescaped colons.
///
/// A backslash escapes the next character, which is how a password containing a
/// colon or a backslash is written. Returns `None` for a line that does not have
/// five fields, which is how a malformed line is skipped rather than guessed at.
fn split_fields(line: &str) -> Option<[String; 5]> {
    let mut fields: Vec<String> = Vec::with_capacity(5);
    let mut current = String::new();
    let mut chars = line.chars();

    while let Some(ch) = chars.next() {
        match ch {
            '\\' => {
                // Whatever follows is literal, including a colon or a backslash.
                if let Some(escaped) = chars.next() {
                    current.push(escaped);
                }
            }
            ':' if fields.len() < 4 => {
                fields.push(std::mem::take(&mut current));
            }
            other => current.push(other),
        }
    }
    fields.push(current);

    if fields.len() != 5 {
        return None;
    }
    let mut iter = fields.into_iter();
    Some([
        iter.next()?,
        iter.next()?,
        iter.next()?,
        iter.next()?,
        iter.next()?,
    ])
}

/// Returns the mode when the file is readable by anyone but its owner.
fn insecure_mode(path: &Path) -> Option<u32> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(path).ok()?.permissions().mode();
        // Group or world having any access at all is enough to disqualify it.
        (mode & 0o077 != 0).then_some(mode)
    }
    #[cfg(not(unix))]
    {
        // Windows has no equivalent bit to check, and this program does not
        // claim to evaluate an ACL. The file's protection is the profile's.
        let _ = path;
        None
    }
}

#[cfg(test)]
mod tests;
