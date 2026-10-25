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
fn the_driver_configuration_mirrors_the_resolved_target() {
    let target = target("postgres://app@db.example.net:6432/orders?sslmode=require");
    let config = build_config(&target);
    assert_eq!(config.get_ports(), [6432]);
    assert_eq!(config.get_user(), Some("app"));
    assert_eq!(config.get_dbname(), Some("orders"));
    assert_eq!(config.get_ssl_mode(), PgSslMode::Require);
    assert_eq!(
        config.get_application_name(),
        Some(crate::branding::DEFAULT_APPLICATION_NAME)
    );
}

#[test]
fn verifying_modes_still_require_tls_at_the_protocol_level() {
    for uri in [
        "postgres://app@db.example.net/orders?sslmode=verify-full",
        "postgres://app@db.example.net/orders?sslmode=require",
    ] {
        assert_eq!(
            build_config(&target(uri)).get_ssl_mode(),
            PgSslMode::Require
        );
    }
    assert_eq!(
        build_config(&target("postgres://app@localhost/orders?sslmode=disable")).get_ssl_mode(),
        PgSslMode::Disable
    );
    assert_eq!(
        build_config(&target("postgres://app@localhost/orders")).get_ssl_mode(),
        PgSslMode::Prefer
    );
}

#[test]
fn posture_is_stated_in_words() {
    let mut info = SessionInfo {
        target: "app@localhost:5432/orders".into(),
        database: "orders".into(),
        user: "app".into(),
        server_version: "18.6".into(),
        backend_pid: "123".into(),
        search_path: "\"$user\", public".into(),
        read_only: false,
        tls: TlsState::Disabled,
        environment: crate::connection::Environment::Local,
    };
    assert_eq!(info.posture(), "read-write");
    info.read_only = true;
    assert_eq!(info.posture(), "read-only");
}

#[test]
fn notice_privacy_scrubs_the_backlog_and_stays_active_for_later_notices() {
    let notices = Mutex::new(Vec::new());
    let privacy = AtomicBool::new(false);
    retain_notice(
        &notices,
        &privacy,
        "NOTICE",
        "ordinary visible notice",
        "00000",
    );
    assert_eq!(
        notices.lock().expect("notices")[0].message,
        "ordinary visible notice"
    );

    activate_notice_privacy(&notices, &privacy);
    assert!(privacy.load(Ordering::SeqCst));
    retain_notice(
        &notices,
        &privacy,
        "secret severity",
        "secret-derived notice",
        "bad-code",
    );
    let held = notices.lock().expect("private notices");
    assert_eq!(held.len(), 2);
    for notice in held.iter() {
        assert_eq!(notice.severity, "NOTICE");
        assert!(notice.message.contains("omitted"));
        assert!(!notice.message.contains("secret"));
    }
    assert_eq!(held[0].code.as_deref(), Some("00000"));
    assert!(held[1].code.is_none());
}
