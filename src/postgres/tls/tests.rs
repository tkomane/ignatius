use super::*;

#[test]
fn disable_produces_no_tls_configuration() {
    assert!(
        client_config(&TlsOptions {
            mode: SslMode::Disable,
            ..TlsOptions::default()
        })
        .expect("build")
        .is_none()
    );
}

#[test]
fn every_other_mode_produces_a_configuration() {
    for mode in [
        SslMode::Prefer,
        SslMode::Require,
        SslMode::VerifyCa,
        SslMode::VerifyFull,
    ] {
        assert!(
            client_config(&TlsOptions {
                mode,
                ..TlsOptions::default()
            })
            .expect("build")
            .is_some(),
            "{mode:?} should produce a TLS configuration"
        );
    }
}

#[test]
fn require_and_verify_full_are_never_described_identically() {
    let require = TlsState::Active {
        version: "TLSv1.3".into(),
        cipher: "TLS_AES_256_GCM_SHA384".into(),
        mode: SslMode::Require,
    };
    let verify = TlsState::Active {
        version: "TLSv1.3".into(),
        cipher: "TLS_AES_256_GCM_SHA384".into(),
        mode: SslMode::VerifyFull,
    };
    assert_ne!(require.description(), verify.description());
    assert!(require.description().contains("no identity check"));
    assert!(verify.description().contains("matching host name"));
    assert!(require.is_encrypted() && verify.is_encrypted());
}

#[test]
fn an_unencrypted_session_says_so_rather_than_staying_silent() {
    assert!(!TlsState::NotNegotiated.is_encrypted());
    assert_eq!(TlsState::NotNegotiated.label(), "TLS not negotiated");
    assert!(
        TlsState::NotNegotiated
            .description()
            .contains("not encrypted")
    );
    assert!(TlsState::Disabled.description().contains("not encrypted"));
}

#[test]
fn an_unconfirmable_tls_state_is_reported_as_unknown_not_as_encrypted() {
    assert!(!TlsState::Unknown.is_encrypted());
    assert_eq!(TlsState::Unknown.label(), "TLS state unknown");
    assert!(
        TlsState::Unknown
            .description()
            .contains("cannot be confirmed")
    );
}

#[test]
fn labels_stay_short_enough_for_a_status_bar() {
    let state = TlsState::Active {
        version: "TLSv1.3".into(),
        cipher: "TLS_AES_256_GCM_SHA384".into(),
        mode: SslMode::VerifyFull,
    };
    assert!(state.label().len() <= 24, "{}", state.label());
    assert!(state.label().contains("verify-full"));
}
