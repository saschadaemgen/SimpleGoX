//! Embedded Tor client via Arti with per-protocol routing.
//!
//! Runs a local SOCKS5 proxy on 127.0.0.1:19150 that bridges to Arti.
//! Tracks throughput, connections, and latency for the dashboard.

use arti_client::TorClient;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Mutex as TokioMutex;
use tracing::{error, info, warn};

/// Port for the local SOCKS5 proxy bridge.
pub const SOCKS_PORT: u16 = 19150;

// ---------------------------------------------------------------------------
// Routing types
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TorMode {
    Direct,
    TorExit,
    TorOnion { onion_address: String },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TorRouting {
    pub matrix: TorMode,
    pub telegram: TorMode,
    pub simplex: TorMode,
    pub whatsapp: TorMode,
}

impl Default for TorRouting {
    fn default() -> Self {
        Self {
            matrix: TorMode::Direct,
            telegram: TorMode::Direct,
            simplex: TorMode::Direct,
            whatsapp: TorMode::Direct,
        }
    }
}

impl TorRouting {
    pub fn any_enabled(&self) -> bool {
        self.matrix != TorMode::Direct
            || self.telegram != TorMode::Direct
            || self.simplex != TorMode::Direct
            || self.whatsapp != TorMode::Direct
    }
}

// ---------------------------------------------------------------------------
// Statistics
// ---------------------------------------------------------------------------

/// Live statistics collected from the SOCKS proxy bridge.
pub struct TorStats {
    pub bytes_in: AtomicU64,
    pub bytes_out: AtomicU64,
    pub active_connections: AtomicUsize,
    pub total_connections: AtomicU64,
    pub bootstrap_time: std::sync::Mutex<Option<Instant>>,
}

impl TorStats {
    pub fn new() -> Self {
        Self {
            bytes_in: AtomicU64::new(0),
            bytes_out: AtomicU64::new(0),
            active_connections: AtomicUsize::new(0),
            total_connections: AtomicU64::new(0),
            bootstrap_time: std::sync::Mutex::new(None),
        }
    }

    pub fn uptime_secs(&self) -> u64 {
        self.bootstrap_time
            .lock()
            .unwrap()
            .map(|t| t.elapsed().as_secs())
            .unwrap_or(0)
    }
}

/// An active proxy connection for the Network tab.
#[derive(Clone, Serialize)]
pub struct ProxyConnection {
    pub id: u64,
    pub destination: String,
    pub protocol: String,
    pub bytes_in: u64,
    pub bytes_out: u64,
}

// ---------------------------------------------------------------------------
// TorManager
// ---------------------------------------------------------------------------

pub struct TorManager {
    client: Option<Arc<TorClient<tor_rtcompat::PreferredRuntime>>>,
    routing: TorRouting,
    bootstrapped: bool,
    pub stats: Arc<TorStats>,
    pub connections: Arc<TokioMutex<HashMap<u64, ProxyConnection>>>,
}

impl TorManager {
    pub fn new() -> Self {
        Self {
            client: None,
            routing: TorRouting::default(),
            bootstrapped: false,
            stats: Arc::new(TorStats::new()),
            connections: Arc::new(TokioMutex::new(HashMap::new())),
        }
    }

    pub fn socks_proxy_url(&self) -> String {
        format!("socks5h://127.0.0.1:{SOCKS_PORT}")
    }

    pub async fn bootstrap(&mut self, data_dir: PathBuf) -> Result<(), String> {
        if self.bootstrapped {
            return Ok(());
        }

        info!("Tor: bootstrapping Arti...");
        let tor_dir = data_dir.join("tor");
        let _ = std::fs::create_dir_all(&tor_dir);

        let config = arti_client::config::TorClientConfigBuilder::from_directories(
            tor_dir.join("state"),
            tor_dir.join("cache"),
        )
        .build()
        .map_err(|e| format!("Tor config error: {e}"))?;

        info!(
            "Tor: connecting to Tor network (data dir: {:?})...",
            tor_dir
        );

        let client = TorClient::create_bootstrapped(config)
            .await
            .map_err(|e| format!("Tor bootstrap failed: {e}"))?;

        let client_arc = Arc::new(client);
        info!("Tor: bootstrapped successfully");

        *self.stats.bootstrap_time.lock().unwrap() = Some(Instant::now());

        // Start the SOCKS5 proxy bridge with stats tracking
        start_socks_proxy(
            client_arc.clone(),
            self.stats.clone(),
            self.connections.clone(),
        );

        self.client = Some(client_arc);
        self.bootstrapped = true;
        Ok(())
    }

    pub async fn shutdown(&mut self) {
        info!("Tor: shutting down");
        self.client = None;
        self.bootstrapped = false;
        *self.stats.bootstrap_time.lock().unwrap() = None;
    }

    pub async fn set_routing(
        &mut self,
        protocol: &str,
        mode: TorMode,
        data_dir: PathBuf,
    ) -> Result<(), String> {
        match protocol {
            "matrix" => self.routing.matrix = mode,
            "telegram" => self.routing.telegram = mode,
            "simplex" => self.routing.simplex = mode,
            "whatsapp" => self.routing.whatsapp = mode,
            _ => return Err(format!("Unknown protocol: {protocol}")),
        }

        if self.routing.any_enabled() && !self.bootstrapped {
            self.bootstrap(data_dir).await?;
        } else if !self.routing.any_enabled() && self.bootstrapped {
            self.shutdown().await;
        }

        info!(
            "Tor: routing updated - {}: {:?}",
            protocol,
            self.mode_for(protocol)
        );
        Ok(())
    }

    pub fn routing(&self) -> &TorRouting {
        &self.routing
    }

    pub fn is_bootstrapped(&self) -> bool {
        self.bootstrapped
    }

    #[allow(dead_code)]
    pub fn client(&self) -> Option<Arc<TorClient<tor_rtcompat::PreferredRuntime>>> {
        self.client.clone()
    }

    pub fn mode_for(&self, protocol: &str) -> TorMode {
        match protocol {
            "matrix" => self.routing.matrix.clone(),
            "telegram" => self.routing.telegram.clone(),
            "simplex" => self.routing.simplex.clone(),
            "whatsapp" => self.routing.whatsapp.clone(),
            _ => TorMode::Direct,
        }
    }
}

// ---------------------------------------------------------------------------
// SOCKS5 Proxy Bridge
// ---------------------------------------------------------------------------

fn start_socks_proxy(
    tor_client: Arc<TorClient<tor_rtcompat::PreferredRuntime>>,
    stats: Arc<TorStats>,
    connections: Arc<TokioMutex<HashMap<u64, ProxyConnection>>>,
) {
    tokio::spawn(async move {
        let addr = format!("127.0.0.1:{SOCKS_PORT}");
        let listener = match tokio::net::TcpListener::bind(&addr).await {
            Ok(l) => {
                info!("Tor: SOCKS5 proxy listening on {addr}");
                l
            }
            Err(e) => {
                error!("Tor: failed to bind SOCKS5 proxy on {addr}: {e}");
                return;
            }
        };

        loop {
            let (stream, peer) = match listener.accept().await {
                Ok(s) => s,
                Err(e) => {
                    warn!("Tor: SOCKS proxy accept error: {e}");
                    continue;
                }
            };

            let client = tor_client.clone();
            let stats = stats.clone();
            let conns = connections.clone();
            tokio::spawn(async move {
                if let Err(e) = handle_socks_connection(stream, client, stats, conns).await {
                    warn!("Tor: SOCKS connection from {peer} failed: {e}");
                }
            });
        }
    });
}

async fn handle_socks_connection(
    mut stream: tokio::net::TcpStream,
    tor_client: Arc<TorClient<tor_rtcompat::PreferredRuntime>>,
    stats: Arc<TorStats>,
    connections: Arc<TokioMutex<HashMap<u64, ProxyConnection>>>,
) -> Result<(), String> {
    // SOCKS5 greeting
    let mut buf = [0u8; 258];
    let n = stream
        .read(&mut buf)
        .await
        .map_err(|e| format!("read greeting: {e}"))?;
    if n < 2 || buf[0] != 0x05 {
        return Err("Not a SOCKS5 request".into());
    }

    stream
        .write_all(&[0x05, 0x00])
        .await
        .map_err(|e| format!("write auth: {e}"))?;

    // SOCKS5 CONNECT
    let n = stream
        .read(&mut buf)
        .await
        .map_err(|e| format!("read request: {e}"))?;
    if n < 7 || buf[0] != 0x05 || buf[1] != 0x01 {
        return Err("Invalid SOCKS5 CONNECT".into());
    }

    let (host, port) = match buf[3] {
        0x01 => {
            if n < 10 {
                return Err("IPv4 too short".into());
            }
            let ip = format!("{}.{}.{}.{}", buf[4], buf[5], buf[6], buf[7]);
            let p = u16::from_be_bytes([buf[8], buf[9]]);
            (ip, p)
        }
        0x03 => {
            let len = buf[4] as usize;
            if n < 5 + len + 2 {
                return Err("Domain too short".into());
            }
            let h = String::from_utf8_lossy(&buf[5..5 + len]).to_string();
            let p = u16::from_be_bytes([buf[5 + len], buf[6 + len]]);
            (h, p)
        }
        _ => return Err(format!("Unsupported address type: {}", buf[3])),
    };

    // Connect through Tor
    info!("SOCKS proxy: CONNECT to {host}:{port} via Tor...");
    let tor_stream = tor_client
        .connect((host.as_str(), port))
        .await
        .map_err(|e| {
            error!("SOCKS proxy: Tor connect FAILED to {host}:{port}: {e}");
            format!("Tor connect to {host}:{port}: {e}")
        })?;
    info!("SOCKS proxy: Tor connection established to {host}:{port}");

    // SOCKS5 success reply
    stream
        .write_all(&[0x05, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00])
        .await
        .map_err(|e| format!("write reply: {e}"))?;
    info!("SOCKS proxy: piping data for {host}:{port}");

    // Track connection
    let conn_id = stats.total_connections.fetch_add(1, Ordering::Relaxed);
    stats.active_connections.fetch_add(1, Ordering::Relaxed);

    let dest = format!("{host}:{port}");
    let protocol = guess_protocol(&dest);
    {
        let mut conns = connections.lock().await;
        conns.insert(
            conn_id,
            ProxyConnection {
                id: conn_id,
                destination: dest,
                protocol,
                bytes_in: 0,
                bytes_out: 0,
            },
        );
    }

    // Bidirectional pipe with byte counting
    let (mut client_read, mut client_write) = stream.into_split();
    let (mut tor_read, mut tor_write) = tor_stream.split();

    let stats_up = stats.clone();
    let stats_down = stats.clone();

    let upload = async move {
        let mut b = [0u8; 8192];
        loop {
            let n = match client_read.read(&mut b).await {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            stats_up.bytes_out.fetch_add(n as u64, Ordering::Relaxed);
            if tor_write.write_all(&b[..n]).await.is_err() {
                break;
            }
            if tor_write.flush().await.is_err() {
                break;
            }
        }
    };

    let download = async move {
        let mut b = [0u8; 8192];
        loop {
            let n = match tor_read.read(&mut b).await {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            stats_down.bytes_in.fetch_add(n as u64, Ordering::Relaxed);
            if client_write.write_all(&b[..n]).await.is_err() {
                break;
            }
            if client_write.flush().await.is_err() {
                break;
            }
        }
    };

    tokio::select! {
        _ = upload => {}
        _ = download => {}
    }

    // Cleanup
    stats.active_connections.fetch_sub(1, Ordering::Relaxed);
    connections.lock().await.remove(&conn_id);

    Ok(())
}

fn guess_protocol(dest: &str) -> String {
    if dest.contains("simplego.dev") || dest.contains(":8448") || dest.contains("matrix.org") {
        "matrix".into()
    } else if dest.contains("telegram") || dest.contains("149.154.") || dest.contains("91.108.") {
        "telegram".into()
    } else {
        "unknown".into()
    }
}
