//! SMP transport layer - TLS connection to SMP servers.
//!
//! Phase 1: TLS connect skeleton with fingerprint verification and ALPN.
//! No SMP framing or commands yet - that comes in Phase 2.
//!
//! SMP uses 16KB fixed-size transport blocks over TLS 1.3.

use crate::tls_verifier::FingerprintVerifier;
use anyhow::Result;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use tokio_rustls::TlsConnector;
use tracing::info;

/// SMP transport block size (always exactly 16384 bytes).
#[allow(dead_code)]
pub const BLOCK_SIZE: usize = 16384;

/// Padding byte for SMP blocks.
#[allow(dead_code)]
pub const PADDING_BYTE: u8 = 0x23; // '#'

/// Default SMP server port.
#[allow(dead_code)] // Used in Phase 2
pub const DEFAULT_PORT: u16 = 5223;

/// SMP server address with fingerprint for TLS verification.
#[derive(Clone, Debug)]
pub struct SmpServerAddr {
    pub host: String,
    pub port: u16,
    pub fingerprint: String,
}

/// SMP transport client. Phase 1: TLS connection only.
#[allow(dead_code)] // Used in Phase 2
pub struct SmpClient {
    pub addr: SmpServerAddr,
    socks5_proxy: Option<String>,
}

impl SmpClient {
    /// Create a new client for the given server. Does not connect yet.
    #[allow(dead_code)] // Used in Phase 2
    pub fn new(addr: SmpServerAddr, socks5_proxy: Option<String>) -> Self {
        Self { addr, socks5_proxy }
    }

    /// Establish a TLS connection to the SMP server.
    /// Uses fingerprint pinning instead of WebPKI.
    /// ALPN is set to "smp/1".
    pub async fn connect(&self) -> Result<TlsStream<TcpStream>> {
        let verifier = FingerprintVerifier::new(&self.addr.fingerprint)?;

        let config = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(verifier))
            .with_no_client_auth();

        let mut config = config;
        config.alpn_protocols = vec![b"smp/1".to_vec()];

        let connector = TlsConnector::from(Arc::new(config));

        // Connect via SOCKS5 proxy or direct
        let tcp = if let Some(ref proxy) = self.socks5_proxy {
            info!("SMP: connecting via SOCKS5 proxy {proxy}");
            let target = format!("{}:{}", self.addr.host, self.addr.port);
            tokio_socks::tcp::Socks5Stream::connect(proxy.as_str(), target.as_str())
                .await
                .map_err(|e| anyhow::anyhow!("SOCKS5 connect failed: {e}"))?
                .into_inner()
        } else {
            TcpStream::connect(format!("{}:{}", self.addr.host, self.addr.port)).await?
        };

        let domain = rustls::pki_types::ServerName::try_from(self.addr.host.as_str())
            .map_err(|_| anyhow::anyhow!("Invalid server name: {}", self.addr.host))?
            .to_owned();

        let tls = connector.connect(domain, tcp).await?;
        info!(
            "SMP: TLS connected to {}:{} (fingerprint verified)",
            self.addr.host, self.addr.port
        );

        Ok(tls)
    }

    /// Test the TLS connection to the server. Returns Ok if handshake succeeds.
    #[allow(dead_code)]
    pub async fn test_connection(&self) -> Result<()> {
        let _stream = self.connect().await?;
        info!("SMP: test connection successful");
        Ok(())
    }
}
