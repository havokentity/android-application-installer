//! TLS configuration for ADB pairing (self-signed certs, no verification).

use std::sync::Arc;
use tokio_rustls::TlsAcceptor;
use tokio_rustls::TlsConnector;

/// TLS certificate verifier that accepts any certificate (for ADB pairing).
/// ADB pairing uses self-signed certificates with `SSL_VERIFY_NONE`.
#[derive(Debug)]
#[allow(dead_code)]
struct NoVerifier;

impl tokio_rustls::rustls::client::danger::ServerCertVerifier for NoVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &tokio_rustls::rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[tokio_rustls::rustls::pki_types::CertificateDer<'_>],
        _server_name: &tokio_rustls::rustls::pki_types::ServerName<'_>,
        _ocsp: &[u8],
        _now: tokio_rustls::rustls::pki_types::UnixTime,
    ) -> Result<
        tokio_rustls::rustls::client::danger::ServerCertVerified,
        tokio_rustls::rustls::Error,
    > {
        Ok(tokio_rustls::rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &tokio_rustls::rustls::pki_types::CertificateDer<'_>,
        _dss: &tokio_rustls::rustls::DigitallySignedStruct,
    ) -> Result<
        tokio_rustls::rustls::client::danger::HandshakeSignatureValid,
        tokio_rustls::rustls::Error,
    > {
        Ok(tokio_rustls::rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &tokio_rustls::rustls::pki_types::CertificateDer<'_>,
        _dss: &tokio_rustls::rustls::DigitallySignedStruct,
    ) -> Result<
        tokio_rustls::rustls::client::danger::HandshakeSignatureValid,
        tokio_rustls::rustls::Error,
    > {
        Ok(tokio_rustls::rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<tokio_rustls::rustls::SignatureScheme> {
        use tokio_rustls::rustls::SignatureScheme;
        vec![
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA512,
            SignatureScheme::ED25519,
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
        ]
    }
}

/// Build a [`TlsConnector`] for connecting to the phone's pairing server.
/// Uses TLS 1.3 only with no certificate verification (matching AOSP).
#[allow(dead_code)]
fn build_tls_connector() -> Result<TlsConnector, String> {
    let config = tokio_rustls::rustls::ClientConfig::builder_with_protocol_versions(
        &[&tokio_rustls::rustls::version::TLS13],
    )
    .dangerous()
    .with_custom_certificate_verifier(Arc::new(NoVerifier))
    .with_no_client_auth();

    Ok(TlsConnector::from(Arc::new(config)))
}

#[allow(dead_code)]
/// Generate a self-signed TLS certificate and build a [`TlsAcceptor`].
fn build_tls_acceptor() -> Result<TlsAcceptor, String> {
    let ck = rcgen::generate_simple_self_signed(vec!["adb".into()])
        .map_err(|e| format!("TLS cert generation failed: {e}"))?;

    let cert_der = tokio_rustls::rustls::pki_types::CertificateDer::from(
        ck.cert.der().to_vec(),
    );
    let key_der = tokio_rustls::rustls::pki_types::PrivateKeyDer::from(
        tokio_rustls::rustls::pki_types::PrivatePkcs8KeyDer::from(
            ck.key_pair.serialize_der(),
        ),
    );

    // AOSP mandates TLS 1.3 only for ADB pairing
    // (SSL_CTX_set_min/max_proto_version both set to TLS1_3_VERSION)
    let config = tokio_rustls::rustls::ServerConfig::builder_with_protocol_versions(
            &[&tokio_rustls::rustls::version::TLS13],
        )
        .with_no_client_auth()
        .with_single_cert(vec![cert_der], key_der)
        .map_err(|e| format!("TLS config failed: {e}"))?;

    Ok(TlsAcceptor::from(Arc::new(config)))
}
