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

#[test]
fn parameterized_diagnostics_preserve_recovery_without_advice_about_hidden_detail() {
    let cancelled = parameterized_server_error(&SqlState::QUERY_CANCELED, 3);
    assert_eq!(cancelled.kind, DiagnosticKind::Cancelled);
    assert_eq!(cancelled.statement_number, Some(3));
    assert!(
        cancelled
            .next_action
            .expect("action")
            .contains("nothing was retried")
    );

    let failed_transaction = parameterized_server_error(&SqlState::IN_FAILED_SQL_TRANSACTION, 2);
    assert_eq!(failed_transaction.kind, DiagnosticKind::Query);
    assert_eq!(
        failed_transaction.next_action.as_deref(),
        Some("run ROLLBACK to end the failed transaction")
    );
    let duplicate = parameterized_server_error(&SqlState::UNIQUE_VIOLATION, 1);
    assert!(!duplicate.next_action.expect("action").contains("Detail"));
    assert!(duplicate.object.is_none());
}

#[test]
fn untrusted_sqlstate_must_be_exactly_five_uppercase_ascii_letters_or_digits() {
    for valid in ["23514", "22P02", "XX999", "00000"] {
        assert_eq!(safe_sqlstate(valid), Some(valid));
    }
    for invalid in [
        "",
        "22p02",
        "235140",
        "23\n14",
        "✓001",
        "secret-derived-code",
    ] {
        assert_eq!(safe_sqlstate(invalid), None);
        let diagnostic = parameterized_server_error(&SqlState::from_code(invalid), 1);
        assert!(diagnostic.technical.is_empty());
    }
}
