//! Connection service files, in PostgreSQL's own `pg_service.conf` format.
//!
//! Reference: <https://www.postgresql.org/docs/current/libpq-pgservice.html>
//!
//! A service file lets someone write `service=orders-prod` instead of a host, a
//! port, a database and a user, and it is how teams share connection details
//! without sharing a password. Supporting it means an existing `psql` setup
//! works here unchanged, which is the difference between trying this tool and
//! not bothering.
//!
//! Parameters this build does not implement are reported by name rather than
//! ignored, exactly as they are when they arrive in a connection string.

use crate::diagnostics::{Diagnostic, DiagnosticKind};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// A parsed service file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ServiceFile {
    /// Service name to its parameters, in file order within each service.
    services: BTreeMap<String, BTreeMap<String, String>>,
    /// Where it was read from.
    path: PathBuf,
}

impl ServiceFile {
    /// The parameters of one service.
    #[must_use]
    pub fn service(&self, name: &str) -> Option<&BTreeMap<String, String>> {
        self.services.get(name)
    }

    /// Every service name, for error messages and for `doctor`.
    #[must_use]
    pub fn names(&self) -> Vec<&str> {
        self.services.keys().map(String::as_str).collect()
    }

    /// Where this file was read from.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// The default location, honouring `PGSERVICEFILE` and `PGSYSCONFDIR`.
#[must_use]
pub fn default_path() -> Option<PathBuf> {
    if let Some(explicit) = std::env::var_os("PGSERVICEFILE").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(explicit));
    }
    if let Some(dirs) = directories::BaseDirs::new() {
        let personal = dirs.home_dir().join(".pg_service.conf");
        if personal.exists() {
            return Some(personal);
        }
    }
    // The system-wide file is only used when there is no personal one, which is
    // libpq's own order.
    std::env::var_os("PGSYSCONFDIR")
        .filter(|v| !v.is_empty())
        .map(|dir| PathBuf::from(dir).join("pg_service.conf"))
}

/// Reads and parses a service file.
///
/// A malformed line is an error rather than something to skip: a service file is
/// how a whole team reaches a database, and quietly ignoring half of it would
/// send someone to the wrong server.
pub fn load(path: &Path) -> Result<ServiceFile, Diagnostic> {
    let contents = std::fs::read_to_string(path).map_err(|err| {
        Diagnostic::new(
            DiagnosticKind::Config,
            format!("could not read {}", path.display()),
            "reading the service file",
        )
        .likely_cause(err.to_string())
        .next_action("check the path and its permissions, or unset PGSERVICEFILE")
    })?;

    let mut services: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    let mut current: Option<String> = None;

    for (index, raw) in contents.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            let name = name.trim().to_owned();
            services.entry(name.clone()).or_default();
            current = Some(name);
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return Err(malformed(
                path,
                index + 1,
                "expected key=value or [service]",
            ));
        };
        let Some(service) = current.as_ref() else {
            return Err(malformed(
                path,
                index + 1,
                "a parameter appears before any [service] heading",
            ));
        };
        services
            .entry(service.clone())
            .or_default()
            .insert(key.trim().to_ascii_lowercase(), value.trim().to_owned());
    }

    Ok(ServiceFile {
        services,
        path: path.to_path_buf(),
    })
}

fn malformed(path: &Path, line: usize, reason: &str) -> Diagnostic {
    Diagnostic::new(
        DiagnosticKind::Config,
        format!("{} is not a valid service file", path.display()),
        "reading the service file",
    )
    .likely_cause(format!("line {line}: {reason}"))
    .next_action("fix the line, or point PGSERVICEFILE at a different file")
    .technical("Path", path.display().to_string())
    .technical("Line", line.to_string())
}

/// Resolves a named service into connection parameters.
///
/// Returns a diagnostic naming the services that do exist when the requested one
/// does not, because a typo in a service name is otherwise a connection failure
/// with no explanation.
pub fn resolve(name: &str, path: Option<&Path>) -> Result<BTreeMap<String, String>, Diagnostic> {
    let Some(path) = path else {
        return Err(Diagnostic::new(
            DiagnosticKind::Config,
            format!("service {name:?} was requested but no service file was found"),
            "resolving a connection service",
        )
        .likely_cause("no PGSERVICEFILE, no ~/.pg_service.conf, and no PGSYSCONFDIR")
        .next_action("create ~/.pg_service.conf, or set PGSERVICEFILE"));
    };

    let file = load(path)?;
    file.service(name).cloned().ok_or_else(|| {
        let known = file.names();
        let listing = if known.is_empty() {
            "the file defines no services".to_owned()
        } else {
            format!("it defines: {}", known.join(", "))
        };
        Diagnostic::new(
            DiagnosticKind::Config,
            format!("service {name:?} is not in {}", path.display()),
            "resolving a connection service",
        )
        .likely_cause(listing)
        .next_action("check the spelling, or add the service to the file")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(contents: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("pg_service.conf");
        std::fs::write(&path, contents).expect("write");
        (dir, path)
    }

    #[test]
    fn services_and_their_parameters_are_read() {
        let (_dir, path) = write(
            "# the team's databases\n\
             [orders-prod]\n\
             host=db.example.net\n\
             port=6432\n\
             dbname=orders\n\
             user=app\n\
             \n\
             [orders-dev]\n\
             host=localhost\n\
             dbname=orders_dev\n",
        );
        let file = load(&path).expect("parse");
        assert_eq!(file.names(), vec!["orders-dev", "orders-prod"]);

        let prod = file.service("orders-prod").expect("service");
        assert_eq!(prod["host"], "db.example.net");
        assert_eq!(prod["port"], "6432");
        assert_eq!(prod["dbname"], "orders");
        assert_eq!(prod["user"], "app");

        let dev = file.service("orders-dev").expect("service");
        assert_eq!(dev["host"], "localhost");
        assert!(
            !dev.contains_key("port"),
            "a service only carries what it states"
        );
    }

    #[test]
    fn keys_are_case_insensitive_and_values_keep_their_case() {
        let (_dir, path) = write("[s]\nHost=Db.Example.Net\nDBNAME=Orders\n");
        let file = load(&path).expect("parse");
        let service = file.service("s").expect("service");
        assert_eq!(service["host"], "Db.Example.Net");
        assert_eq!(service["dbname"], "Orders");
    }

    #[test]
    fn whitespace_and_comments_are_ignored() {
        let (_dir, path) =
            write("\n   # a comment\n\n  [spaced]  \n  host = db  \n\n# another\n  port=5432\n");
        let service = load(&path)
            .expect("parse")
            .service("spaced")
            .cloned()
            .expect("service");
        assert_eq!(service["host"], "db");
        assert_eq!(service["port"], "5432");
    }

    #[test]
    fn a_malformed_line_is_an_error_rather_than_something_to_skip() {
        // Skipping half a service file would quietly send someone to the wrong
        // server, which is worse than refusing to read it.
        let (_dir, path) = write("[s]\nhost=db\nthis is not a parameter\n");
        let error = load(&path).expect_err("must refuse");
        assert!(
            error
                .technical
                .iter()
                .any(|f| f.label == "Line" && f.value == "3")
        );
        assert!(error.next_action.is_some());
    }

    #[test]
    fn a_parameter_before_any_service_heading_is_an_error() {
        let (_dir, path) = write("host=db\n[s]\nport=5432\n");
        let error = load(&path).expect_err("must refuse");
        assert!(
            error
                .likely_cause
                .unwrap_or_default()
                .contains("before any"),
            "it should say what is wrong"
        );
    }

    #[test]
    fn an_unknown_service_lists_the_ones_that_exist() {
        let (_dir, path) = write("[orders-prod]\nhost=db\n\n[orders-dev]\nhost=localhost\n");
        let error = resolve("orders-stage", Some(&path)).expect_err("must fail");
        let cause = error.likely_cause.expect("a cause");
        assert!(cause.contains("orders-prod"), "{cause}");
        assert!(cause.contains("orders-dev"), "{cause}");
        assert!(error.next_action.is_some());
    }

    #[test]
    fn asking_for_a_service_with_no_file_says_where_to_put_one() {
        let error = resolve("anything", None).expect_err("must fail");
        assert!(
            error.headline.contains("no service file"),
            "{}",
            error.headline
        );
        assert!(
            error
                .next_action
                .expect("action")
                .contains("pg_service.conf"),
            "it should say where to create one"
        );
    }

    #[test]
    fn an_empty_service_is_valid_and_carries_nothing() {
        let (_dir, path) = write("[empty]\n");
        let service = resolve("empty", Some(&path)).expect("resolve");
        assert!(service.is_empty());
    }
}
