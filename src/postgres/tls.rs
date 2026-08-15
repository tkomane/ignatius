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
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Everything the transport policy needs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TlsOptions {
    /// What protection was asked for.
    pub mode: SslMode,
    /// An explicit trust root, instead of the operating system's.
    pub root_cert: Option<PathBuf>,
    /// A client certificate to present.
    pub client_cert: Option<PathBuf>,
    /// The key for that certificate.
    pub client_key: Option<PathBuf>,
}

/// Builds the rustls configuration for a policy, or `None` when TLS is disabled.
pub fn client_config(options: &TlsOptions) -> Result<Option<rustls::ClientConfig>, Diagnostic> {
    if options.mode == SslMode::Disable {
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

    let verifier: Arc<dyn ServerCertVerifier> = if options.mode.verifies_certificate() {
        let base = base_verifier(options, &provider)?;
        if options.mode == SslMode::VerifyCa {
            // verify-ca checks the chain and deliberately does not check the
            // host name. That is a weaker guarantee than verify-full and is
            // described that way everywhere it is shown.
            Arc::new(NameAgnostic { inner: base })
        } else {
            base
        }
    } else {
        // require and prefer: encrypt, verify nothing, and say so.
        Arc::new(NoIdentityCheck::new(
            rustls::crypto::ring::default_provider(),
        ))
    };

    let builder = builder
        .dangerous()
        .with_custom_certificate_verifier(verifier);

    let config = match (&options.client_cert, &options.client_key) {
        (Some(cert), Some(key)) => {
            let chain = load_certificates(cert)?;
            let key = load_private_key(key)?;
            builder.with_client_auth_cert(chain, key).map_err(|err| {
                Diagnostic::new(
                    DiagnosticKind::Tls,
                    "the client certificate and key could not be used together",
                    "preparing the TLS configuration",
                )
                .likely_cause(err.to_string())
                .next_action("check that the key belongs to the certificate")
            })?
        }
        (Some(_), None) | (None, Some(_)) => {
            return Err(Diagnostic::new(
                DiagnosticKind::Tls,
                "a client certificate needs both a certificate and a key",
                "preparing the TLS configuration",
            )
            .likely_cause("only one of sslcert and sslkey was given")
            .next_action("supply both, or neither"));
        }
        (None, None) => builder.with_no_client_auth(),
    };

    Ok(Some(config))
}

/// The verifier that checks the chain: explicit roots when given, otherwise the
/// operating system's own trust store.
fn base_verifier(
    options: &TlsOptions,
    provider: &Arc<rustls::crypto::CryptoProvider>,
) -> Result<Arc<dyn ServerCertVerifier>, Diagnostic> {
    let Some(path) = &options.root_cert else {
        let verifier =
            rustls_platform_verifier::Verifier::new(provider.clone()).map_err(|err| {
                Diagnostic::new(
                    DiagnosticKind::Tls,
                    "could not read this machine's certificate trust store",
                    "preparing certificate verification",
                )
                .likely_cause(err.to_string())
                .next_action(
                    "check the system trust store, supply sslrootcert, or connect with \
                 sslmode=require to accept encryption without an identity check",
                )
            })?;
        return Ok(Arc::new(verifier));
    };

    // An explicit root replaces the system store rather than adding to it,
    // which is what libpq's sslrootcert does and what someone pinning an
    // internal authority expects.
    let mut roots = rustls::RootCertStore::empty();
    for certificate in load_certificates(path)? {
        roots.add(certificate).map_err(|err| {
            Diagnostic::new(
                DiagnosticKind::Tls,
                format!("{} is not a usable trust root", path.display()),
                "preparing certificate verification",
            )
            .likely_cause(err.to_string())
            .next_action("check that the file holds a certificate authority in PEM form")
        })?;
    }

    rustls::client::WebPkiServerVerifier::builder_with_provider(Arc::new(roots), provider.clone())
        .build()
        .map(|verifier| verifier as Arc<dyn ServerCertVerifier>)
        .map_err(|err| {
            Diagnostic::new(
                DiagnosticKind::Tls,
                "the trust root could not be used",
                "preparing certificate verification",
            )
            .likely_cause(err.to_string())
            .next_action("check the certificate in sslrootcert")
        })
}

fn load_certificates(path: &Path) -> Result<Vec<CertificateDer<'static>>, Diagnostic> {
    use rustls::pki_types::pem::PemObject;
    CertificateDer::pem_file_iter(path)
        .and_then(|iter| iter.collect::<Result<Vec<_>, _>>())
        .map_err(|err| {
            Diagnostic::new(
                DiagnosticKind::Tls,
                format!("could not read certificates from {}", path.display()),
                "preparing the TLS configuration",
            )
            .likely_cause(err.to_string())
            .next_action("check the path, and that the file is PEM-encoded")
        })
}

fn load_private_key(path: &Path) -> Result<PrivateKeyDer<'static>, Diagnostic> {
    use rustls::pki_types::pem::PemObject;
    PrivateKeyDer::from_pem_file(path).map_err(|err| {
        Diagnostic::new(
            DiagnosticKind::Tls,
            format!("could not read a private key from {}", path.display()),
            "preparing the TLS configuration",
        )
        .likely_cause(err.to_string())
        .next_action(
            "check the path, and that the key is PEM-encoded and not encrypted: \
             encrypted keys are not supported yet",
        )
    })
}

/// Verifies the chain but not the host name, which is what `verify-ca` means.
///
/// It exists for the setup where a certificate is trusted but presented under a
/// name that does not match, such as a load balancer or an internal address. It
/// is a real, weaker guarantee, and the interface never shows it as verify-full.
#[derive(Debug)]
struct NameAgnostic {
    inner: Arc<dyn ServerCertVerifier>,
}

impl ServerCertVerifier for NameAgnostic {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        match self.inner.verify_server_cert(
            end_entity,
            intermediates,
            server_name,
            ocsp_response,
            now,
        ) {
            // The one error this mode forgives, and only this one. Every other
            // failure, including an untrusted or expired chain, still fails.
            Err(rustls::Error::InvalidCertificate(rustls::CertificateError::NotValidForName))
            | Err(rustls::Error::InvalidCertificate(
                rustls::CertificateError::NotValidForNameContext { .. },
            )) => Ok(ServerCertVerified::assertion()),
            other => other,
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.inner.supported_verify_schemes()
    }
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
}
