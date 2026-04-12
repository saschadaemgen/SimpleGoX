//! Tauri commands for Tor routing control and live statistics.

use crate::commands::AppState;
use crate::i2p::I2PManager;
use crate::sidecar::SidecarManager;
use crate::tor::{ProxyConnection, TorManager, TorMode, TorRouting, SOCKS_PORT};
use sgx_core::{SgxClient, SgxConfig};
use sgx_proto::messenger::v1::*;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tauri::{Emitter, State};
use tokio::sync::Mutex;
use tracing::info;

// ---------------------------------------------------------------------------
// Routing
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn tor_set_protocol(
    protocol: String,
    mode: String,
    onion_address: Option<String>,
    tor: State<'_, Mutex<TorManager>>,
    i2p: State<'_, Mutex<I2PManager>>,
    app_state: State<'_, AppState>,
    sidecar: State<'_, Arc<SidecarManager>>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    let tor_mode = match mode.as_str() {
        "direct" => TorMode::Direct,
        "tor" => TorMode::Tor,
        "i2p" => TorMode::I2P,
        "onion" => TorMode::TorOnion {
            onion_address: onion_address.ok_or("Onion address required")?,
        },
        _ => return Err(format!("Unknown mode: {mode}")),
    };

    // I2P validation: only Matrix and SimpleX can use I2P (no exit nodes)
    if tor_mode == TorMode::I2P && (protocol == "telegram" || protocol == "whatsapp") {
        return Err("I2P is not available for this protocol (no exit nodes)".into());
    }

    let data_dir = dirs::data_local_dir()
        .unwrap_or_default()
        .join("simplego-x");

    // Emit bootstrapping state
    let _ = app.emit("tor-state", "bootstrapping");

    let mut tor_guard = tor.lock().await;
    tor_guard
        .set_routing(&protocol, tor_mode.clone(), data_dir.clone())
        .await?;
    let proxy_url = if tor_guard.is_bootstrapped() {
        Some(tor_guard.socks_proxy_url())
    } else {
        None
    };
    let is_bootstrapped = tor_guard.is_bootstrapped();
    drop(tor_guard);

    // Emit connected/disconnected state
    let _ = app.emit(
        "tor-state",
        if is_bootstrapped {
            "connected"
        } else {
            "disconnected"
        },
    );

    // Auto IP check after bootstrap
    if is_bootstrapped {
        if let Some(ref purl) = proxy_url {
            let purl = purl.clone();
            let app_clone = app.clone();
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                info!("Auto IP-Check: starting (5s after bootstrap)...");
                let proxy = match reqwest::Proxy::all(&purl) {
                    Ok(p) => p,
                    Err(e) => {
                        tracing::error!("Auto IP-Check: proxy error: {e}");
                        return;
                    }
                };
                let client = match reqwest::ClientBuilder::new()
                    .proxy(proxy)
                    .connect_timeout(std::time::Duration::from_secs(60))
                    .timeout(std::time::Duration::from_secs(60))
                    .build()
                {
                    Ok(c) => c,
                    Err(e) => {
                        tracing::error!("Auto IP-Check: client error: {e}");
                        return;
                    }
                };
                // Try HTTP first (no TLS through Tor)
                info!("Auto IP-Check: trying HTTP...");
                match client.get("http://api.ipify.org").send().await {
                    Ok(resp) => match resp.text().await {
                        Ok(ip) => {
                            let ip = ip.trim().to_string();
                            info!(">>> Auto IP-Check: EXIT IP = {ip} (HTTP) <<<");
                            let _ = app_clone.emit("tor-exit-ip", &ip);
                            return;
                        }
                        Err(e) => tracing::warn!("Auto IP-Check: HTTP body: {e}"),
                    },
                    Err(e) => tracing::warn!("Auto IP-Check: HTTP failed: {e}"),
                }
                // Fallback HTTPS
                info!("Auto IP-Check: trying HTTPS...");
                match client.get("https://api.ipify.org").send().await {
                    Ok(resp) => match resp.text().await {
                        Ok(ip) => {
                            let ip = ip.trim().to_string();
                            info!(">>> Auto IP-Check: EXIT IP = {ip} (HTTPS) <<<");
                            let _ = app_clone.emit("tor-exit-ip", &ip);
                        }
                        Err(e) => tracing::error!("Auto IP-Check: HTTPS body: {e}"),
                    },
                    Err(e) => tracing::error!("Auto IP-Check: HTTPS also failed: {e}"),
                }
            });
        }
    }

    // MATRIX: Rebuild client with/without proxy
    if protocol == "matrix" {
        // For I2P: bootstrap emissary and use I2P proxy + .b32.i2p homeserver
        let (proxy, homeserver_override) = match tor_mode {
            TorMode::Direct => (None, None),
            TorMode::Tor | TorMode::TorOnion { .. } => (proxy_url.clone(), None),
            TorMode::I2P => {
                // Bootstrap I2P if not running
                let mut i2p_guard = i2p.lock().await;
                if !i2p_guard.is_bootstrapped() {
                    let _ = app.emit("i2p-state", "bootstrapping");
                    info!("I2P: bootstrapping emissary for Matrix...");
                    i2p_guard.bootstrap(data_dir.clone()).await?;
                    let _ = app.emit("i2p-state", "connected");
                }
                let i2p_proxy = i2p_guard.proxy_url();
                drop(i2p_guard);
                (
                    Some(i2p_proxy),
                    Some(format!("http://{}:8448", crate::i2p::MATRIX_I2P_ADDR)),
                )
            }
        };

        info!(
            "Rebuilding Matrix client: proxy={:?}, homeserver_override={:?}",
            proxy, homeserver_override
        );
        app_state.sync_cancel.lock().await.cancel();
        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

        let mut client_guard = app_state.client.lock().await;
        if client_guard.is_some() {
            let config_path = SgxConfig::default_config_path();
            let config = SgxConfig::from_file(&config_path)
                .map_err(|e| format!("Config read failed: {e}"))?;

            *client_guard = None;
            drop(client_guard);
            tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

            let new_client = if let Some(hs) = homeserver_override {
                SgxClient::new_with_i2p(config, proxy.unwrap_or_default(), hs)
                    .await
                    .map_err(|e| format!("I2P client build failed: {e}"))?
            } else {
                SgxClient::new_with_proxy(config, proxy)
                    .await
                    .map_err(|e| format!("Client rebuild failed: {e}"))?
            };

            new_client
                .restore_session()
                .await
                .map_err(|e| format!("Session restore failed: {e}"))?;

            info!("Tor: Matrix client rebuilt and session restored");

            let sync_client = new_client.clone_inner();
            let cancel = tokio_util::sync::CancellationToken::new();
            let cancel_clone = cancel.clone();

            {
                let mut guard = app_state.client.lock().await;
                *guard = Some(new_client);
                *app_state.sync_cancel.lock().await = cancel;
            }

            crate::commands::spawn_sync_with_cancel(sync_client, &app, cancel_clone);
            info!("Tor: Matrix sync restarted");
        }
    }

    // TELEGRAM: Set/remove SOCKS proxy via gRPC
    if protocol == "telegram" {
        if let Some(mut client) = sidecar.get_client("telegram").await {
            match tor_mode {
                TorMode::Direct => {
                    info!("Tor: disabling Telegram proxy");
                    let _ = client
                        .set_proxy(SetProxyRequest {
                            enabled: false,
                            server: String::new(),
                            port: 0,
                        })
                        .await;
                }
                _ => {
                    info!("Tor: setting Telegram proxy to 127.0.0.1:{SOCKS_PORT}");
                    let _ = client
                        .set_proxy(SetProxyRequest {
                            enabled: true,
                            server: "127.0.0.1".into(),
                            port: SOCKS_PORT as i32,
                        })
                        .await;
                }
            }
        }
    }

    // Auto-save routing to disk for restore on next app start
    {
        let tor_guard = tor.lock().await;
        let routing = tor_guard.routing().clone();
        let routing_file = data_dir.join("tor-routing.json");
        if let Ok(json) = serde_json::to_string_pretty(&routing) {
            let _ = std::fs::write(&routing_file, &json);
            info!("Tor: routing saved to {:?}", routing_file);
        }
    }

    Ok(format!("{protocol} routing updated"))
}

/// Save routing config to disk (called from frontend as backup).
#[tauri::command]
pub async fn tor_save_routing(routing: TorRouting) -> Result<(), String> {
    let data_dir = dirs::data_local_dir()
        .unwrap_or_default()
        .join("simplego-x");
    let routing_file = data_dir.join("tor-routing.json");
    let json =
        serde_json::to_string_pretty(&routing).map_err(|e| format!("Serialize error: {e}"))?;
    std::fs::write(&routing_file, json).map_err(|e| format!("Write error: {e}"))?;
    info!("Tor: routing config saved to {:?}", routing_file);
    Ok(())
}

/// Read routing config from the persistent JSON file (single source of truth).
#[tauri::command]
pub async fn tor_get_saved_routing() -> Result<serde_json::Value, String> {
    let path = dirs::data_local_dir()
        .unwrap_or_default()
        .join("simplego-x")
        .join("tor-routing.json");

    if path.exists() {
        let content = std::fs::read_to_string(&path).map_err(|e| format!("Read error: {e}"))?;
        serde_json::from_str(&content).map_err(|e| format!("Parse error: {e}"))
    } else {
        Ok(serde_json::json!({
            "matrix": "direct",
            "telegram": "direct",
            "simplex": "direct",
            "whatsapp": "direct"
        }))
    }
}

#[tauri::command]
pub async fn tor_get_routing(tor: State<'_, Mutex<TorManager>>) -> Result<TorRouting, String> {
    Ok(tor.lock().await.routing().clone())
}

#[tauri::command]
pub async fn tor_get_status(tor: State<'_, Mutex<TorManager>>) -> Result<bool, String> {
    Ok(tor.lock().await.is_bootstrapped())
}

// ---------------------------------------------------------------------------
// IP Check
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn tor_check_ip(tor: State<'_, Mutex<TorManager>>) -> Result<String, String> {
    use std::error::Error as StdError;

    info!("tor_check_ip: called");
    let proxy_url = {
        let tor = tor.lock().await;
        if !tor.is_bootstrapped() {
            tracing::error!("tor_check_ip: Tor not bootstrapped");
            return Err("Tor is not connected. Enable Tor routing first.".into());
        }
        tor.socks_proxy_url()
    };

    info!("tor_check_ip: proxy URL = {proxy_url}");
    let proxy = reqwest::Proxy::all(&proxy_url).map_err(|e| {
        tracing::error!("tor_check_ip: proxy error: {e}");
        format!("Proxy error: {e}")
    })?;
    let client = reqwest::ClientBuilder::new()
        .proxy(proxy)
        .connect_timeout(std::time::Duration::from_secs(60))
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| {
            tracing::error!("tor_check_ip: client build error: {e}");
            format!("Client: {e}")
        })?;

    // Try HTTP first (simpler, no TLS through Tor)
    info!("tor_check_ip: trying HTTP...");
    match client.get("http://api.ipify.org").send().await {
        Ok(resp) => {
            let ip = resp.text().await.unwrap_or_default().trim().to_string();
            info!(">>> Tor EXIT IP = {ip} (via HTTP) <<<");
            return Ok(ip);
        }
        Err(e) => {
            tracing::warn!("tor_check_ip: HTTP failed: {e}");
            if let Some(src) = e.source() {
                tracing::warn!("  source: {src}");
                if let Some(inner) = src.source() {
                    tracing::warn!("  inner: {inner}");
                }
            }
        }
    }

    // Fallback to HTTPS
    info!("tor_check_ip: trying HTTPS...");
    match client.get("https://api.ipify.org").send().await {
        Ok(resp) => {
            let ip = resp.text().await.unwrap_or_default().trim().to_string();
            info!(">>> Tor EXIT IP = {ip} (via HTTPS) <<<");
            Ok(ip)
        }
        Err(e) => {
            tracing::error!("tor_check_ip: HTTPS also failed: {e}");
            tracing::error!(
                "  is_timeout={} is_connect={}",
                e.is_timeout(),
                e.is_connect()
            );
            if let Some(src) = e.source() {
                tracing::error!("  source: {src}");
            }
            Err(format!("Both HTTP and HTTPS failed: {e}"))
        }
    }
}

// ---------------------------------------------------------------------------
// Live Statistics
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn tor_start_stats(
    app: tauri::AppHandle,
    tor: State<'_, Mutex<TorManager>>,
) -> Result<(), String> {
    let tor_guard = tor.lock().await;
    let stats = tor_guard.stats.clone();
    let proxy_url = tor_guard.socks_proxy_url();
    let bootstrapped = tor_guard.is_bootstrapped();
    drop(tor_guard);

    // Build a reqwest client through the proxy for latency checks
    let latency_client = if bootstrapped {
        let proxy = reqwest::Proxy::all(&proxy_url).ok();
        proxy.and_then(|p| {
            reqwest::ClientBuilder::new()
                .proxy(p)
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .ok()
        })
    } else {
        None
    };

    tokio::spawn(async move {
        let mut prev_in: u64 = 0;
        let mut prev_out: u64 = 0;
        let mut tick: u32 = 0;
        let mut last_latency: Option<u64> = None;

        loop {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            tick += 1;

            let cur_in = stats.bytes_in.load(Ordering::Relaxed);
            let cur_out = stats.bytes_out.load(Ordering::Relaxed);

            let tp_in = (cur_in.saturating_sub(prev_in)) / 2;
            let tp_out = (cur_out.saturating_sub(prev_out)) / 2;
            prev_in = cur_in;
            prev_out = cur_out;

            // Latency every 10 seconds (tick % 5 == 0)
            if tick % 5 == 0 {
                if let Some(ref client) = latency_client {
                    let start = std::time::Instant::now();
                    match client
                        .head("https://www.gstatic.com/generate_204")
                        .send()
                        .await
                    {
                        Ok(_) => {
                            last_latency = Some(start.elapsed().as_millis() as u64);
                        }
                        Err(_) => {
                            last_latency = None;
                        }
                    }
                }
            }

            let payload = serde_json::json!({
                "throughput_in": tp_in,
                "throughput_out": tp_out,
                "bytes_in_total": cur_in,
                "bytes_out_total": cur_out,
                "active_connections": stats.active_connections.load(Ordering::Relaxed),
                "total_connections": stats.total_connections.load(Ordering::Relaxed),
                "uptime_secs": stats.uptime_secs(),
                "latency_ms": last_latency,
            });

            if app.emit("tor-stats", &payload).is_err() {
                break;
            }
        }
    });

    Ok(())
}

// ---------------------------------------------------------------------------
// Connection List
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn tor_get_connections(
    tor: State<'_, Mutex<TorManager>>,
) -> Result<Vec<ProxyConnection>, String> {
    let tor = tor.lock().await;
    let conns = tor.connections.lock().await;
    Ok(conns.values().cloned().collect())
}
