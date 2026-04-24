//! SMP transport layer - TLS connection to SMP servers.
//!
//! Phase 1: TLS connect skeleton with fingerprint verification and ALPN.
//! No SMP framing or commands yet - that comes in Phase 2.
//!
//! SMP uses 16KB fixed-size transport blocks over TLS 1.3.

use crate::tls_verifier::FingerprintVerifier;
use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use tokio_rustls::TlsConnector;
use tracing::{info, warn};

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

        // Connect via SOCKS5 proxy or direct. Both branches produce a bare
        // `tokio::net::TcpStream` (the SOCKS5 branch unwraps via
        // `Socks5Stream::into_inner`), which is required so we can install
        // SO_KEEPALIVE on the raw socket below before TLS wraps it.
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

        // Briefing 044 W1: enable TCP SO_KEEPALIVE on the bare socket BEFORE
        // the TLS upgrade. Parameters mirror the SimpleGo reference
        // (idle=30s, interval=15s, retries=4). Defence-in-depth behind the
        // app-layer PING task from Phase W2; catches cases where the PING
        // task itself dies or is starved by tokio runtime pressure.
        //
        // Failure is non-fatal: some Linux kernel configs and container
        // sandboxes refuse `setsockopt(TCP_KEEPIDLE)` and similar. App-layer
        // PING alone is sufficient (GoChat ships without SO_KEEPALIVE and
        // works fine), so we warn and proceed.
        {
            use socket2::{SockRef, TcpKeepalive};
            let sock_ref = SockRef::from(&tcp);
            // `with_retries` is gated to Linux/BSD/macOS/Android in socket2
            // 0.5 (maps to TCP_KEEPCNT). Windows does not expose a per-socket
            // retry count via setsockopt; the retry budget there is a
            // system-wide registry value. Apply the common knobs first and
            // append `.with_retries` only on platforms that support it.
            let keepalive = TcpKeepalive::new()
                .with_time(Duration::from_secs(30))
                .with_interval(Duration::from_secs(15));
            #[cfg(not(target_os = "windows"))]
            let keepalive = keepalive.with_retries(4);
            match sock_ref.set_tcp_keepalive(&keepalive) {
                Ok(()) => {
                    #[cfg(not(target_os = "windows"))]
                    info!("SMP: TCP keep-alive enabled (idle=30s interval=15s retries=4)");
                    #[cfg(target_os = "windows")]
                    info!(
                        "SMP: TCP keep-alive enabled (idle=30s interval=15s; retry count governed by OS)"
                    );
                }
                Err(e) => warn!(
                    "SMP: TCP keep-alive setup failed: {e}. App-layer PING will cover liveness."
                ),
            }
        }

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
