//! Tauri commands for network routing control (Direct/Tor/I2P) and live statistics.

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
    i2p: State<'_, Arc<Mutex<I2PManager>>>,
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

    // If switching AWAY from I2P, shut down i2pd first
    if !matches!(tor_mode, TorMode::I2P) {
        let mut i2p_guard = i2p.lock().await;
        if i2p_guard.is_bootstrapped() {
            info!("I2P: shutting down (mode changed to {mode})");
            i2p_guard.shutdown().await;
            crate::i2p::emit_i2p_status(&app, "disconnected", "I2P stopped");
        }
        drop(i2p_guard);
    }

    // Emit bootstrapping state for the correct network
    match tor_mode {
        TorMode::I2P => {}  // I2P emits its own detailed status in bootstrap()
        TorMode::Direct => {}
        _ => { crate::tor::emit_tor_status(&app, "bootstrapping", "Connecting to Tor network..."); }
    }

    let mut tor_guard = tor.lock().await;
    if let Err(e) = tor_guard
        .set_routing(&protocol, tor_mode.clone(), data_dir.clone())
        .await
    {
        match tor_mode {
            TorMode::I2P => { crate::i2p::emit_i2p_status(&app, "error", &format!("Routing failed: {e}")); }
            _ => { crate::tor::emit_tor_status(&app, "error", &format!("Routing failed: {e}")); }
        }
        return Err(e);
    }
    let proxy_url = if tor_guard.is_bootstrapped() {
        Some(tor_guard.socks_proxy_url())
    } else {
        None
    };
    let is_bootstrapped = tor_guard.is_bootstrapped();
    drop(tor_guard);

    // Emit connected/disconnected for Tor (I2P emits its own events later)
    if !matches!(tor_mode, TorMode::I2P) {
        if is_bootstrapped {
            // Don't emit "connected" yet - verify exit IP first
            crate::tor::emit_tor_status(&app, "bootstrapping", "Tor ready, verifying exit IP...");
        } else if matches!(tor_mode, TorMode::Direct) {
            let _ = app.emit("tor-status", serde_json::json!({"state": "disconnected", "detail": ""}));
        }
    }

    // Auto IP check after Tor bootstrap - emit "connected" only after IP is verified
    if is_bootstrapped && !matches!(tor_mode, TorMode::I2P) {
        if let Some(ref purl) = proxy_url {
            let purl = purl.clone();
            let app_clone = app.clone();
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                crate::tor::emit_tor_status(&app_clone, "bootstrapping", "Checking exit IP...");

                let proxy = match reqwest::Proxy::all(&purl) {
                    Ok(p) => p,
                    Err(e) => {
                        tracing::error!("IP-Check: proxy error: {e}");
                        crate::tor::emit_tor_status(&app_clone, "connected", "Connected via Tor");
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
                        tracing::error!("IP-Check: client error: {e}");
                        crate::tor::emit_tor_status(&app_clone, "connected", "Connected via Tor");
                        return;
                    }
                };

                // Try HTTP first
                let mut exit_ip = String::new();
                match client.get("http://api.ipify.org").send().await {
                    Ok(resp) => {
                        if let Ok(ip) = resp.text().await {
                            exit_ip = ip.trim().to_string();
                        }
                    }
                    Err(_) => {
                        // Fallback HTTPS
                        if let Ok(resp) = client.get("https://api.ipify.org").send().await {
                            if let Ok(ip) = resp.text().await {
                                exit_ip = ip.trim().to_string();
                            }
                        }
                    }
                }

                if !exit_ip.is_empty() {
                    info!("Tor exit IP: {exit_ip}");
                    let _ = app_clone.emit("tor-exit-ip", &exit_ip);
                    crate::tor::emit_tor_status(
                        &app_clone,
                        "connected",
                        &format!("Connected via Tor (Exit: {exit_ip})"),
                    );
                } else {
                    tracing::warn!("IP-Check: could not determine exit IP");
                    crate::tor::emit_tor_status(&app_clone, "connected", "Connected via Tor");
                }
            });
        }
    }

    // MATRIX: Rebuild client with/without proxy
    if protocol == "matrix" {
        // For I2P: start i2pd sidecar and use I2P proxy + .b32.i2p homeserver
        let (proxy, homeserver_override) = match tor_mode {
            TorMode::Direct => (None, None),
            TorMode::Tor | TorMode::TorOnion { .. } => (proxy_url.clone(), None),
            TorMode::I2P => {
                // Bootstrap I2P if not running
                let mut i2p_guard = i2p.lock().await;
                let _i2p_cancelled = i2p_guard.cancelled();
                if !i2p_guard.is_bootstrapped() {
                    info!("I2P: starting i2pd for Matrix...");
                    match i2p_guard.bootstrap(data_dir.clone(), &app).await {
                        Ok(_) => {
                            crate::i2p::emit_i2p_status(
                                &app,
                                "bootstrapping",
                                "Connecting to Matrix via I2P tunnels...",
                            );
                        }
                        Err(e) => {
                            // Don't emit error if simply cancelled by mode switch
                            if !e.contains("cancelled") {
                                crate::i2p::emit_i2p_status(
                                    &app,
                                    "error",
                                    &format!("Bootstrap failed: {e}"),
                                );
                            }
                            return Err(e);
                        }
                    }

                    // Start watchdog for auto-recovery
                    let i2p_arc = i2p.inner().clone();
                    let app_clone = app.clone();
                    tokio::spawn(async move {
                        crate::i2p::start_watchdog(i2p_arc, app_clone).await;
                    });
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

            // Check cancellation before session restore (user may have switched mode)
            let i2p_cancelled = if matches!(tor_mode, TorMode::I2P) {
                let g = i2p.lock().await;
                Some(g.cancelled())
            } else {
                None
            };

            if let Some(ref c) = i2p_cancelled {
                if c.load(std::sync::atomic::Ordering::SeqCst) {
                    info!("I2P: cancelled before session restore");
                    return Err("I2P: bootstrap cancelled".into());
                }
                crate::i2p::emit_i2p_status(
                    &app,
                    "bootstrapping",
                    "Restoring Matrix session via I2P...",
                );
            }

            new_client
                .restore_session()
                .await
                .map_err(|e| format!("Session restore failed: {e}"))?;

            info!("Routing: Matrix client rebuilt and session restored");

            let sync_client = new_client.clone_inner();
            let cancel = tokio_util::sync::CancellationToken::new();
            let cancel_clone = cancel.clone();

            {
                let mut guard = app_state.client.lock().await;
                *guard = Some(new_client);
                *app_state.sync_cancel.lock().await = cancel;
            }

            crate::commands::spawn_sync_with_cancel(sync_client, &app, cancel_clone);
            info!("Routing: Matrix sync restarted");

            // For I2P: verify actual homeserver connectivity in background.
            // SOCKS port is open but tunnels may not be ready for 2-5 min.
            if let Some(cancelled_flag) = i2p_cancelled {
                let app_bg = app.clone();
                tokio::spawn(async move {
                    crate::i2p::wait_for_tunnel_ready(app_bg, cancelled_flag).await;
                });
            }
        }
    }

    // TELEGRAM: Set/remove SOCKS proxy via gRPC
    if protocol == "telegram" {
        if let Some(mut client) = sidecar.get_client("telegram").await {
            match tor_mode {
                TorMode::Direct => {
                    info!("Routing: disabling Telegram proxy");
                    let _ = client
                        .set_proxy(SetProxyRequest {
                            enabled: false,
                            server: String::new(),
                            port: 0,
                        })
                        .await;
                }
                _ => {
                    info!("Routing: setting Telegram proxy to 127.0.0.1:{SOCKS_PORT}");
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
        let routing_file = data_dir.join("routing-config.json");
        if let Ok(json) = serde_json::to_string_pretty(&routing) {
            let _ = std::fs::write(&routing_file, &json);
            info!("Routing: config saved to {:?}", routing_file);
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
    let routing_file = data_dir.join("routing-config.json");
    let json =
        serde_json::to_string_pretty(&routing).map_err(|e| format!("Serialize error: {e}"))?;
    std::fs::write(&routing_file, json).map_err(|e| format!("Write error: {e}"))?;
    info!("Routing: config saved to {:?}", routing_file);
    Ok(())
}

/// Read routing config from the persistent JSON file (single source of truth).
#[tauri::command]
pub async fn tor_get_saved_routing() -> Result<serde_json::Value, String> {
    let path = dirs::data_local_dir()
        .unwrap_or_default()
        .join("simplego-x")
        .join("routing-config.json");

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

// ---------------------------------------------------------------------------
// I2P Dashboard Stats (from i2pd webconsole on port 7070)
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, Clone, Default)]
pub struct I2pStats {
    pub connected: bool,
    pub uptime: String,
    pub network_status: String,
    pub success_rate: String,
    pub bw_inbound: String,
    pub bw_outbound: String,
    pub received: String,
    pub sent: String,
    pub routers: String,
    pub floodfills: String,
    pub lease_sets: String,
    pub tunnels_in: String,
    pub tunnels_out: String,
    pub transit: String,
    pub client_tunnels: String,
    pub transit_bw: String,
    pub version: String,
}

#[tauri::command]
pub async fn get_i2p_stats() -> Result<I2pStats, String> {
    let resp = reqwest::Client::new()
        .get("http://127.0.0.1:7070/")
        .header("Host", "127.0.0.1:7070")
        .timeout(std::time::Duration::from_secs(3))
        .send()
        .await;

    match resp {
        Ok(r) => {
            let status = r.status();
            let html = r.text().await.unwrap_or_default();
            tracing::info!(
                "I2P Stats: HTTP {} - HTML length: {} chars",
                status,
                html.len()
            );
            if html.len() > 200 {
                tracing::info!("I2P Stats: first 200: {}", &html[..200]);
            }
            let stats = parse_i2pd_console(&html);
            tracing::info!(
                "I2P Stats: uptime='{}' routers='{}' version='{}' status='{}'",
                stats.uptime,
                stats.routers,
                stats.version,
                stats.network_status
            );
            Ok(stats)
        }
        Err(e) => {
            tracing::error!("I2P Stats: HTTP failed: {e}");
            Err(format!("i2pd console unreachable: {e}"))
        }
    }
}

fn parse_i2pd_console(html: &str) -> I2pStats {
    fn extract(html: &str, prefix: &str, suffix: &str) -> String {
        html.find(prefix)
            .and_then(|start| {
                let rest = &html[start + prefix.len()..];
                rest.find(suffix).map(|end| rest[..end].trim().to_string())
            })
            .unwrap_or_default()
    }

    fn extract_after_b(html: &str, label: &str) -> String {
        let tag = format!("<b>{label}</b>");
        extract(html, &tag, "<")
    }

    let uptime = extract(html, "<b>Uptime:</b>", "<");
    let status = extract(html, "<b>Network status:</b>", "<");
    let success = extract(html, "<b>Tunnel creation success rate:</b>", "%");
    let version = extract(html, "<b>Version:</b>", "<");

    // Bandwidth: "Received: 273 KB (1 KiB/s)" pattern
    let received_full = extract(html, "<b>Received:</b>", "<");
    let sent_full = extract(html, "<b>Sent:</b>", "<");

    let (received, bw_in) = if let Some(idx) = received_full.find('(') {
        (
            received_full[..idx].trim().to_string(),
            received_full[idx + 1..].trim_end_matches(')').trim().to_string(),
        )
    } else {
        (received_full.clone(), String::new())
    };

    let (sent, bw_out) = if let Some(idx) = sent_full.find('(') {
        (
            sent_full[..idx].trim().to_string(),
            sent_full[idx + 1..].trim_end_matches(')').trim().to_string(),
        )
    } else {
        (sent_full.clone(), String::new())
    };

    // "Transit: 29.11 KiB (0.00 KiB/s)" - bandwidth, not tunnel count
    let transit_bw = extract(html, "<b>Transit:</b>", "<");

    let routers = extract_after_b(html, "Routers:");
    let floodfills = extract_after_b(html, "Floodfills:");
    let lease_sets = extract_after_b(html, "LeaseSets:");
    let client_tunnels = extract_after_b(html, "Client Tunnels:");
    let transit = extract_after_b(html, "Transit Tunnels:");

    // Inbound/Outbound tunnel counts are not on the main page
    let tunnels_in = String::new();
    let tunnels_out = String::new();

    I2pStats {
        connected: !uptime.is_empty(),
        uptime,
        network_status: status,
        success_rate: if success.is_empty() {
            String::new()
        } else {
            format!("{success}%")
        },
        bw_inbound: bw_in,
        bw_outbound: bw_out,
        received,
        sent,
        routers,
        floodfills,
        lease_sets,
        tunnels_in,
        tunnels_out,
        transit,
        client_tunnels,
        transit_bw,
        version,
    }
}
