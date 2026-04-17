//! Custom TLS certificate verifier for SMP server fingerprint pinning.
//!
//! SimpleX identifies servers by the SHA-256 hash of the full end-entity
//! certificate DER, base64url-encoded without padding.

use anyhow::Result;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::CryptoProvider;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, Error, SignatureScheme};
use sha2::{Digest, Sha256};

/// Default fingerprint for smp.simplego.dev
#[allow(dead_code)] // Used when creating default SmpServerAddr
pub const SIMPLEGO_SMP_FINGERPRINT: &str = "Fhx5bd0q_k6XnLf7FMXmf78DbKlcagEMiBop88tsm0o";

/// Verifies server identity by comparing SHA-256 of the full end-entity
/// certificate DER against an expected fingerprint.
#[derive(Debug)]
pub struct FingerprintVerifier {
    expected_fingerprint: String,
    provider: CryptoProvider,
}

impl FingerprintVerifier {
    /// Create a verifier from a base64url-encoded (no padding) SHA-256 fingerprint.
    pub fn new(fingerprint_b64: &str) -> Result<Self> {
        let stripped = fingerprint_b64.trim_end_matches('=');
        // Validate it decodes to 32 bytes
        let decoded = URL_SAFE_NO_PAD
            .decode(stripped)
            .map_err(|e| anyhow::anyhow!("Invalid fingerprint base64: {e}"))?;
        if decoded.len() != 32 {
            return Err(anyhow::anyhow!(
                "Fingerprint must be 32 bytes, got {}",
                decoded.len()
            ));
        }
        Ok(Self {
            expected_fingerprint: stripped.to_string(),
            provider: rustls::crypto::ring::default_provider(),
        })
    }
}

impl ServerCertVerifier for FingerprintVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> std::result::Result<ServerCertVerified, Error> {
        // SimpleX: SHA-256 of full DER of the CA certificate (intermediate[0])
        if intermediates.is_empty() {
            return Err(Error::General(
                "No intermediate certificate in chain".into(),
            ));
        }

        let ca_cert = &intermediates[0];
        let hash = Sha256::digest(ca_cert.as_ref());
        let got_fp = URL_SAFE_NO_PAD.encode(&hash);

        if got_fp == self.expected_fingerprint {
            tracing::debug!("TLS fingerprint verified: {got_fp}");
            Ok(ServerCertVerified::assertion())
        } else {
            Err(Error::General(format!(
                "Certificate fingerprint mismatch. Expected: {}, Got: {got_fp}",
                self.expected_fingerprint
            )))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, Error> {
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
    ) -> std::result::Result<HandshakeSignatureValid, Error> {
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
