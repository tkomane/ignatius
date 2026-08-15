//! TLS policy.
//!
//! Two rules govern everything here:
//!
//! 1. A TLS failure is never retried without TLS. If the requested mode says TLS
//!    is required, a handshake failure ends the attempt.
//! 2. `require` and `verify-full` are never described as the same thing.
//!    `require` encrypts and verifies nothing; the status bar and the connection
//!    report both say so.
//!
//! Trust roots come from the operating system through `rustls-platform-verifier`,
//! so the Keychain on macOS, the Windows certificate stores, and the usual Linux
//! bundles all work without the user exporting anything.

use crate::connection::SslMode;
use crate::diagnostics::{Diagnostic, DiagnosticKind};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::CryptoProvider;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use std::sync::Arc;

/// Builds the rustls configuration for a mode, or `None` when TLS is disabled.
pub fn client_config(mode: SslMode) -> Result<Option<rustls::ClientConfig>, Diagnostic> {
    if mode == SslMode::Disable {
        return Ok(None);
    }

    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let builder = rustls::ClientConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()
        .map_err(|err| {
            Diagnostic::new(
                DiagnosticKind::Tls,
                "could not initialise TLS",
                "preparing the TLS configuration",
            )
            .likely_cause(err.to_string())
            .next_action("report this: the built-in TLS provider should always initialise")
        })?;

    let config = if mode.verifies_certificate() {
        let verifier = rustls_platform_verifier::Verifier::new(provider).map_err(|err| {
            Diagnostic::new(
                DiagnosticKind::Tls,
                "could not read this machine's certificate trust store",
                "preparing certificate verification",
            )
            .likely_cause(err.to_string())
            .next_action(
                "check the system trust store, or connect with sslmode=require to accept \
                 encryption without an identity check",
            )
        })?;
        builder
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(verifier))
            .with_no_client_auth()
    } else {
        // sslmode=require: encrypt, verify nothing. This is what libpq's `require`
        // means, and the interface labels it as encryption without identity.
        builder
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(NoIdentityCheck::new(
                rustls::crypto::ring::default_provider(),
            )))
            .with_no_client_auth()
    };

    Ok(Some(config))
}

/// A verifier that accepts any certificate.
///
/// Used only for `sslmode=require` and `sslmode=prefer`, where libpq also does no
/// verification. Signature checking is still delegated to the crypto provider, so
/// the handshake itself remains sound; what is absent is any statement about who
/// is on the other end. Every place this applies says so in words.
#[derive(Debug)]
struct NoIdentityCheck {
    provider: Arc<CryptoProvider>,
}

impl NoIdentityCheck {
    fn new(provider: CryptoProvider) -> Self {
        Self {
            provider: Arc::new(provider),
        }
    }
}

impl ServerCertVerifier for NoIdentityCheck {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// What protection the session actually ended up with, according to the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TlsState {
    /// TLS was not requested.
    Disabled,
    /// TLS was requested but the server did not use it. Only reachable under
    /// `prefer`, and always shown rather than glossed over.
    NotNegotiated,
    /// The server would not say whether TLS is in use, because `pg_stat_ssl` was
    /// not readable. Reported as unknown rather than assumed from the request.
    Unknown,
    /// TLS is in use.
    Active {
        /// Protocol version reported by the server, for example `TLSv1.3`.
        version: String,
        /// Cipher suite reported by the server.
        cipher: String,
        /// The mode that was requested, which determines what was verified.
        mode: SslMode,
    },
}

impl TlsState {
    /// A compact label for the status bar. Always states what was verified.
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::Disabled => "TLS off".to_owned(),
            Self::NotNegotiated => "TLS not negotiated".to_owned(),
            Self::Unknown => "TLS state unknown".to_owned(),
            Self::Active { mode, version, .. } => format!("TLS {version} {}", mode.as_str()),
        }
    }

    /// The full sentence used in the connection report and the error panel.
    #[must_use]
    pub fn description(&self) -> String {
        match self {
            Self::Disabled => "The connection is not encrypted (sslmode=disable).".to_owned(),
            Self::NotNegotiated => {
                "The server did not offer TLS and sslmode=prefer allows that, so this \
                 connection is not encrypted."
                    .to_owned()
            }
            Self::Unknown => {
                "This session could not read pg_stat_ssl, so whether the connection is \
                 encrypted cannot be confirmed from here."
                    .to_owned()
            }
            Self::Active {
                version,
                cipher,
                mode,
            } => format!(
                "{version} using {cipher}. With sslmode={}, this gives {}.",
                mode.as_str(),
                mode.guarantee()
            ),
        }
    }

    /// Whether traffic is encrypted.
    #[must_use]
    pub const fn is_encrypted(&self) -> bool {
        matches!(self, Self::Active { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disable_produces_no_tls_configuration() {
        assert!(client_config(SslMode::Disable).expect("build").is_none());
    }

    #[test]
    fn every_other_mode_produces_a_configuration() {
        for mode in [SslMode::Prefer, SslMode::Require, SslMode::VerifyFull] {
            assert!(
                client_config(mode).expect("build").is_some(),
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
}
