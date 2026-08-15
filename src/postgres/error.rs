//! Turning driver and server errors into layered diagnostics.
//!
//! A PostgreSQL error already carries most of what a person needs: a SQLSTATE, a
//! message, often a detail, a hint, and a position in the statement. None of that
//! is discarded. What this module adds is the outer layer - what was being
//! attempted, the likely cause in plain language for the SQLSTATEs people
//! actually hit, and a safe next action.

use crate::connection::ConnectionTarget;
use crate::diagnostics::{Diagnostic, DiagnosticKind};
use tokio_postgres::error::{DbError, ErrorPosition, SqlState};

/// How the driver renders any TLS-class failure.
///
/// Matched by text because the driver's error kind is private. The string is
/// pinned by a test that asks the driver for a real TLS failure, so an upstream
/// change to the wording fails the build rather than silently reclassifying
/// every TLS problem as a network one.
const TLS_KIND_MESSAGE: &str = "error performing TLS handshake";

/// How the driver reports that it had no password to offer.
///
/// Matched by text for the same reason as the TLS wording, and pinned by a live
/// test so an upstream change cannot silently turn an authentication problem
/// back into a reported network problem.
const MISSING_PASSWORD: &str = "password missing";

/// Builds a diagnostic for a failure while connecting.
#[must_use]
pub fn from_connect_error(err: &tokio_postgres::Error, target: &ConnectionTarget) -> Diagnostic {
    let attempted = format!(
        "connecting to {} with sslmode={}",
        target.safe_display(),
        target.sslmode.as_str()
    );

    if let Some(db) = err.as_db_error() {
        let kind = if db.code().code().starts_with("28") {
            DiagnosticKind::Authentication
        } else {
            DiagnosticKind::Connection
        };
        return decorate(
            Diagnostic::new(kind, db.message().to_owned(), attempted),
            db,
            connect_advice(db.code(), target),
        );
    }

    if let Some(tls) = find_source::<rustls::Error>(err) {
        return Diagnostic::new(DiagnosticKind::Tls, "the TLS handshake failed", attempted)
            .likely_cause(tls_cause(tls, target))
            .next_action(tls_action(target))
            .technical("sslmode", target.sslmode.as_str())
            .technical("Requested guarantee", target.sslmode.guarantee())
            .technical("TLS error", tls.to_string());
    }

    // A server that refuses TLS outright never reaches the rustls layer, so
    // there is no rustls error to find. The driver still classifies it as a TLS
    // failure, and its own wording for that class is what is matched here. A
    // refusal to encrypt must not be reported as a network problem: the network
    // worked, and the answer was no.
    if err.to_string() == TLS_KIND_MESSAGE {
        let detail = std::error::Error::source(err).map_or_else(
            || "the server would not establish TLS".to_owned(),
            std::string::ToString::to_string,
        );
        return Diagnostic::new(
            DiagnosticKind::Tls,
            "TLS could not be established",
            attempted,
        )
        .likely_cause(detail.clone())
        .next_action(tls_action(target))
        .technical("sslmode", target.sslmode.as_str())
        .technical("Requested guarantee", target.sslmode.guarantee())
        .technical("TLS error", detail);
    }

    // The server asked for a password and the client had none. That is an
    // authentication problem, not a network one: the server was reached and it
    // said no. Classifying it as a connection failure would send a script
    // looking for a firewall.
    if std::error::Error::source(err)
        .map(std::string::ToString::to_string)
        .is_some_and(|source| source == MISSING_PASSWORD)
    {
        return Diagnostic::new(
            DiagnosticKind::Authentication,
            format!("the server requires a password for role {:?}", target.user),
            attempted,
        )
        .likely_cause("no password was supplied by the target, the environment, or a password file")
        .next_action(
            "put the password in a password file, or supply it in the connection string. \
             See docs/support/compatibility.md for the routes this build reads.",
        )
        .technical("Role", target.user.clone())
        .technical("Database", target.database.clone());
    }

    let io = find_source::<std::io::Error>(err);
    let cause = io.map_or_else(|| err.to_string(), std::string::ToString::to_string);
    Diagnostic::new(
        DiagnosticKind::Connection,
        format!("could not reach {}", target.host.display()),
        attempted,
    )
    .likely_cause(cause)
    .next_action(format!(
        "check that PostgreSQL is listening on {}:{}, and that a firewall is not blocking it",
        target.host.display(),
        target.port
    ))
    .technical("Host", target.host.display())
    .technical("Port", target.port.to_string())
    .technical("Timeout", format!("{:?}", target.connect_timeout))
}

/// Builds a diagnostic for a failure while running a statement.
#[must_use]
pub fn from_query_error(err: &tokio_postgres::Error, statement_number: usize) -> Diagnostic {
    let attempted = format!("running statement {statement_number}");

    if let Some(db) = err.as_db_error() {
        let kind = if *db.code() == SqlState::QUERY_CANCELED {
            DiagnosticKind::Cancelled
        } else {
            DiagnosticKind::Query
        };
        let mut diagnostic = decorate(
            Diagnostic::new(kind, db.message().to_owned(), attempted),
            db,
            query_advice(db.code()),
        );
        if let Some(ErrorPosition::Original(position)) = db.position() {
            diagnostic = diagnostic.at_position(*position);
        }
        return diagnostic;
    }

    if err.is_closed() {
        return Diagnostic::new(
            DiagnosticKind::Connection,
            "the connection closed while the statement was running",
            attempted,
        )
        .likely_cause(
            "the server, a proxy, or the network ended the session. Whether the statement \
             committed is not known from here.",
        )
        .next_action(
            "reconnect and check the data before running it again. Nothing is retried \
             automatically.",
        );
    }

    Diagnostic::new(DiagnosticKind::Query, err.to_string(), attempted)
        .next_action("check the statement and the connection, then try again")
}

/// Attaches every structured field the server supplied.
fn decorate(
    diagnostic: Diagnostic,
    db: &DbError,
    advice: (Option<String>, Option<String>),
) -> Diagnostic {
    let mut out = diagnostic
        .technical("SQLSTATE", db.code().code())
        .technical("Severity", db.severity())
        .technical_opt("Detail", db.detail())
        .technical_opt("Hint", db.hint())
        .technical_opt("Where", db.where_())
        .technical_opt("Schema", db.schema())
        .technical_opt("Table", db.table())
        .technical_opt("Column", db.column())
        .technical_opt("Constraint", db.constraint())
        .technical_opt("Routine", db.routine());
    if let (Some(cause), _) = &advice {
        out = out.likely_cause(cause.clone());
    }
    if let (_, Some(action)) = &advice {
        out = out.next_action(action.clone());
    }
    out
}

/// Plain-language cause and action for the SQLSTATEs people hit while connecting.
fn connect_advice(code: &SqlState, target: &ConnectionTarget) -> (Option<String>, Option<String>) {
    if *code == SqlState::INVALID_PASSWORD {
        return (
            Some(format!(
                "the server rejected the password for role {:?}",
                target.user
            )),
            Some("check the role and password. Nothing is retried automatically.".to_owned()),
        );
    }
    if *code == SqlState::INVALID_AUTHORIZATION_SPECIFICATION {
        return (
            Some(format!(
                "the server refused the connection for role {:?}, often because pg_hba.conf has \
                 no matching rule for this client address",
                target.user
            )),
            Some(
                "check pg_hba.conf on the server for a rule covering this host and role".to_owned(),
            ),
        );
    }
    if *code == SqlState::INVALID_CATALOG_NAME {
        return (
            Some(format!(
                "database {:?} does not exist on this server",
                target.database
            )),
            Some("check the database name, or connect to `postgres` and list databases".to_owned()),
        );
    }
    if *code == SqlState::TOO_MANY_CONNECTIONS {
        return (
            Some("the server has reached its connection limit".to_owned()),
            Some("wait and retry, or ask an administrator to free connections".to_owned()),
        );
    }
    (None, None)
}

/// Plain-language cause and action for common statement failures.
fn query_advice(code: &SqlState) -> (Option<String>, Option<String>) {
    if *code == SqlState::UNDEFINED_TABLE {
        return (
            Some("the relation is not visible under the current search_path".to_owned()),
            Some("qualify it with a schema, or check search_path in the status bar".to_owned()),
        );
    }
    if *code == SqlState::UNDEFINED_COLUMN {
        return (
            Some("the column does not exist on that relation".to_owned()),
            Some("check the spelling, or inspect the table's columns".to_owned()),
        );
    }
    if *code == SqlState::IN_FAILED_SQL_TRANSACTION {
        return (
            Some(
                "an earlier statement in this transaction failed, so the server is \
                  refusing everything until the transaction ends"
                    .to_owned(),
            ),
            Some("run ROLLBACK to end the failed transaction".to_owned()),
        );
    }
    if *code == SqlState::QUERY_CANCELED {
        return (
            Some("the statement was cancelled".to_owned()),
            Some("nothing was retried; run it again when ready".to_owned()),
        );
    }
    if *code == SqlState::INSUFFICIENT_PRIVILEGE {
        return (
            Some("the current role does not have permission for this object".to_owned()),
            Some("connect as a role with the required grant".to_owned()),
        );
    }
    if *code == SqlState::UNIQUE_VIOLATION {
        return (
            Some("a row with the same key already exists".to_owned()),
            Some("check the conflicting key shown in Detail".to_owned()),
        );
    }
    (None, None)
}

fn tls_cause(err: &rustls::Error, target: &ConnectionTarget) -> String {
    let text = err.to_string();
    if text.contains("NotValidForName") || text.contains("not valid for name") {
        return format!(
            "the certificate does not cover the name {:?}",
            target.host.display()
        );
    }
    if text.contains("Expired") || text.contains("expired") {
        return "the server certificate is outside its validity period, or this machine's clock \
                is wrong"
            .to_owned();
    }
    if text.contains("UnknownIssuer") || text.contains("unknown issuer") {
        return "the certificate was not issued by an authority this machine trusts".to_owned();
    }
    text
}

fn tls_action(target: &ConnectionTarget) -> String {
    if target.sslmode.verifies_certificate() {
        format!(
            "fix the certificate or the trust store. The connection was not retried without TLS: \
             sslmode={} requires {}.",
            target.sslmode.as_str(),
            target.sslmode.guarantee()
        )
    } else {
        "the server's TLS setup could not complete. The connection was not retried without \
         encryption."
            .to_owned()
    }
}

/// Walks the error chain looking for a concrete source type.
fn find_source<T: std::error::Error + 'static>(err: &tokio_postgres::Error) -> Option<&T> {
    let mut source = std::error::Error::source(err);
    while let Some(current) = source {
        if let Some(found) = current.downcast_ref::<T>() {
            return Some(found);
        }
        source = current.source();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::{ConnectionArgs, EnvSnapshot, resolve};

    fn target(uri: &str) -> ConnectionTarget {
        resolve(
            Some(uri),
            &ConnectionArgs::default(),
            &EnvSnapshot::default(),
            &crate::config::ConnectionConfig::default(),
        )
        .expect("resolve")
    }

    #[test]
    fn tls_advice_never_suggests_connecting_without_tls_when_it_was_required() {
        let target = target("postgres://app@db.example.net/orders?sslmode=verify-full");
        let action = tls_action(&target);
        assert!(action.contains("not retried without TLS"), "{action}");
        assert!(
            !action.to_lowercase().contains("use sslmode=disable"),
            "{action}"
        );
    }

    #[test]
    fn tls_causes_name_the_actual_problem() {
        let target = target("postgres://app@db.example.net/orders?sslmode=verify-full");
        let expired = rustls::Error::InvalidCertificate(rustls::CertificateError::Expired);
        assert!(tls_cause(&expired, &target).contains("validity period"));

        let unknown = rustls::Error::InvalidCertificate(rustls::CertificateError::UnknownIssuer);
        assert!(tls_cause(&unknown, &target).contains("trusts"));

        let name = rustls::Error::InvalidCertificate(rustls::CertificateError::NotValidForName);
        assert!(tls_cause(&name, &target).contains("db.example.net"));
    }

    #[test]
    fn connect_advice_covers_the_failures_people_actually_hit() {
        let target = target("postgres://app@db.example.net/orders");
        for state in [
            SqlState::INVALID_PASSWORD,
            SqlState::INVALID_AUTHORIZATION_SPECIFICATION,
            SqlState::INVALID_CATALOG_NAME,
            SqlState::TOO_MANY_CONNECTIONS,
        ] {
            let (cause, action) = connect_advice(&state, &target);
            assert!(cause.is_some(), "{state:?} has no cause");
            assert!(action.is_some(), "{state:?} has no next action");
        }
        // An unknown state gets no invented explanation.
        assert_eq!(
            connect_advice(&SqlState::from_code("XX999"), &target),
            (None, None)
        );
    }

    #[test]
    fn a_failed_transaction_is_told_to_roll_back() {
        let (cause, action) = query_advice(&SqlState::IN_FAILED_SQL_TRANSACTION);
        assert!(cause.expect("cause").contains("earlier statement"));
        assert_eq!(
            action.expect("action"),
            "run ROLLBACK to end the failed transaction"
        );
    }

    #[test]
    fn query_advice_is_absent_rather_than_invented_for_unknown_states() {
        assert_eq!(query_advice(&SqlState::from_code("XX999")), (None, None));
    }

    #[test]
    fn cancellation_is_classified_as_cancelled_not_as_a_query_failure() {
        let (cause, action) = query_advice(&SqlState::QUERY_CANCELED);
        assert!(cause.expect("cause").contains("cancelled"));
        assert!(action.expect("action").contains("nothing was retried"));
    }
}
