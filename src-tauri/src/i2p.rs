//! Embedded I2P router via emissary-core.
//!
//! I2P has NO exit nodes - traffic stays inside the I2P network.
//! Only usable for Matrix and SimpleX (not Telegram/WhatsApp).
//! Bootstrap takes 10-15 minutes on first run, 3-5 minutes cached.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tracing::{error, info, warn};

/// SOCKS proxy port for I2P traffic.
pub const SOCKS_PORT: u16 = 4447;

/// Our Matrix homeserver's I2P hidden service address.
pub const MATRIX_I2P_ADDR: &str = "aho2me4wz2wbayiviw5tax77iftuh4xy54qckzfm6s3oxcngpulq.b32.i2p";

pub struct I2PStats {
    pub bytes_in: AtomicU64,
    pub bytes_out: AtomicU64,
    pub active_tunnels: AtomicUsize,
    pub bootstrap_time: std::sync::Mutex<Option<Instant>>,
}

impl I2PStats {
    pub fn new() -> Self {
        Self {
            bytes_in: AtomicU64::new(0),
            bytes_out: AtomicU64::new(0),
            active_tunnels: AtomicUsize::new(0),
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

pub struct I2PManager {
    bootstrapped: bool,
    bootstrap_start: Option<Instant>,
    pub stats: Arc<I2PStats>,
}

impl I2PManager {
    pub fn new() -> Self {
        Self {
            bootstrapped: false,
            bootstrap_start: None,
            stats: Arc::new(I2PStats::new()),
        }
    }

    /// Bootstrap the I2P router via emissary-core.
    pub async fn bootstrap(&mut self, data_dir: PathBuf) -> Result<(), String> {
        if self.bootstrapped {
            return Ok(());
        }

        let i2p_dir = data_dir.join("i2p");
        let _ = std::fs::create_dir_all(&i2p_dir);

        info!("I2P: bootstrapping emissary router...");
        info!("I2P: data dir = {:?}", i2p_dir);
        info!("I2P: first boot takes 10-15 minutes, please wait...");

        self.bootstrap_start = Some(Instant::now());

        // Generate random crypto keys for transport protocols
        let mut ntcp2_iv = [0u8; 16];
        let mut ntcp2_key = [0u8; 32];
        let mut ssu2_intro = [0u8; 32];
        let mut ssu2_static = [0u8; 32];
        getrandom(&mut ntcp2_iv);
        getrandom(&mut ntcp2_key);
        getrandom(&mut ssu2_intro);
        getrandom(&mut ssu2_static);

        // Build config with NTCP2 + SSU2 transports enabled
        let mut config = emissary_core::Config {
            ntcp2: Some(emissary_core::Ntcp2Config {
                ipv4: true,
                ipv6: false,
                ipv4_host: None,
                ipv6_host: None,
                port: 25515,
                publish: true,
                iv: ntcp2_iv,
                key: ntcp2_key,
                ml_kem: None,
                disable_pq: true,
            }),
            ssu2: Some(emissary_core::Ssu2Config {
                ipv4: true,
                ipv6: false,
                ipv4_host: None,
                ipv6_host: None,
                ipv4_mtu: None,
                ipv6_mtu: None,
                port: 25515,
                publish: true,
                intro_key: ssu2_intro,
                static_key: ssu2_static,
                ml_kem: None,
                disable_pq: true,
            }),
            samv3_config: Some(emissary_core::SamConfig {
                tcp_port: 7656,
                udp_port: 7655,
                host: "127.0.0.1".to_string(),
            }),
            ..Default::default()
        };

        info!("I2P: config created with NTCP2 port=25515, SSU2 port=25515, SAM=7656");

        // Create storage for persisting router state
        let storage =
            emissary_util::storage::Storage::new::<emissary_util::runtime::tokio::Runtime>(Some(
                i2p_dir.clone(),
            ))
            .await
            .map_err(|e| format!("I2P storage error: {e}"))?;

        // Reseed: download router infos from HTTPS reseed servers
        // This is REQUIRED on first boot - without it, the router knows no peers
        let netdb_dir = i2p_dir.join("netDb");
        let needs_reseed = !netdb_dir.exists()
            || std::fs::read_dir(&netdb_dir)
                .map(|entries| entries.count() < 10)
                .unwrap_or(true);

        if needs_reseed {
            info!("I2P: reseeding from HTTPS servers (downloading router infos)...");
            match emissary_util::reseeder::Reseeder::reseed::<
                emissary_util::runtime::tokio::Runtime,
            >(None, true)
            .await
            {
                Ok(router_infos) => {
                    info!("I2P: reseed got {} router infos", router_infos.len());
                    for (name, ri) in &router_infos {
                        let _ = storage.store_router_info(name.clone(), ri.clone()).await;
                        config.routers.push(ri.clone());
                    }
                    info!(
                        "I2P: stored {} router infos to netDb and config",
                        router_infos.len()
                    );
                }
                Err(e) => {
                    warn!("I2P: reseed failed: {e}");
                    warn!("I2P: router may not be able to find peers without reseed");
                }
            }
        } else {
            info!("I2P: netDb has existing routers, skipping reseed");
        }

        // Create the router with reseeded data
        let (router, _events, _router_info) = emissary_core::router::Router::<
            emissary_util::runtime::tokio::Runtime,
        >::new(config, None, Some(Arc::new(storage)))
        .await
        .map_err(|e| format!("I2P router creation failed: {e}"))?;

        info!("I2P: router created, spawning event loop...");

        // Router implements Future - spawn it as a background task
        tokio::spawn(async move {
            info!("I2P: router event loop started");
            router.await;
            warn!("I2P: router event loop ended");
        });

        // Wait for SOCKS proxy to become available
        info!("I2P: waiting for SOCKS proxy on 127.0.0.1:{SOCKS_PORT}...");
        let mut attempts = 0u32;
        loop {
            match tokio::net::TcpStream::connect(format!("127.0.0.1:{SOCKS_PORT}")).await {
                Ok(_) => {
                    info!("I2P: SOCKS proxy ready on 127.0.0.1:{SOCKS_PORT}");
                    break;
                }
                Err(_) => {
                    attempts += 1;
                    if attempts % 30 == 0 {
                        let elapsed = self.bootstrap_start.unwrap().elapsed().as_secs();
                        info!("I2P: still bootstrapping... ({elapsed}s elapsed)");
                    }
                    if attempts > 900 {
                        return Err(
                            "I2P: bootstrap timeout (SOCKS proxy not available after 15 min)"
                                .into(),
                        );
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
            }
        }

        let elapsed = self.bootstrap_start.unwrap().elapsed();
        info!(
            "I2P: bootstrapped successfully in {:.1}s",
            elapsed.as_secs_f64()
        );

        *self.stats.bootstrap_time.lock().unwrap() = Some(Instant::now());
        self.bootstrapped = true;
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn shutdown(&mut self) {
        info!("I2P: shutting down router");
        self.bootstrapped = false;
        *self.stats.bootstrap_time.lock().unwrap() = None;
    }

    pub fn is_bootstrapped(&self) -> bool {
        self.bootstrapped
    }

    pub fn proxy_url(&self) -> String {
        format!("socks5h://127.0.0.1:{SOCKS_PORT}")
    }
}

/// Fill buffer with random bytes using the OS RNG.
fn getrandom(buf: &mut [u8]) {
    use std::io::Read;
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        let _ = f.read_exact(buf);
    } else {
        // Windows fallback
        for b in buf.iter_mut() {
            *b = (std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .subsec_nanos()
                & 0xFF) as u8;
        }
    }
}
