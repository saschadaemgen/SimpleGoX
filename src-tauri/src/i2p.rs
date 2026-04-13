//! I2P integration via i2pd sidecar process.
//!
//! i2pd is a C++ I2P router with a built-in SOCKS5 proxy.
//! We spawn it as a child process (same pattern as sgx-telegram)
//! and connect through its SOCKS proxy on port 4447.
//!
//! No custom SAM bridge needed - i2pd handles everything internally.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::Emitter;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tracing::{error, info, warn};

/// SOCKS5 proxy port (i2pd default).
pub const SOCKS_PORT: u16 = 4447;

/// i2pd web console port.
const CONSOLE_PORT: u16 = 7070;

/// Our Matrix homeserver's I2P hidden service address.
pub const MATRIX_I2P_ADDR: &str =
    "aho2me4wz2wbayiviw5tax77iftuh4xy54qckzfm6s3oxcngpulq.b32.i2p";

// ---------------------------------------------------------------------------
// Status event helper
// ---------------------------------------------------------------------------

/// Emit a structured I2P status event to the frontend.
/// The frontend uses both `state` (for banner mode) and `detail` (for log/ticker).
pub fn emit_i2p_status(app: &tauri::AppHandle, state: &str, detail: &str) {
    info!("I2P: [{state}] {detail}");
    let _ = app.emit(
        "i2p-status",
        serde_json::json!({ "state": state, "detail": detail }),
    );
}


// ---------------------------------------------------------------------------
// I2PManager
// ---------------------------------------------------------------------------

/// Manages the i2pd child process lifecycle.
pub struct I2PManager {
    process: Option<Child>,
    bootstrapped: bool,
    /// Path to the i2pd binary (remembered for watchdog restarts).
    i2pd_path: Option<PathBuf>,
    /// Data directory for i2pd state.
    data_dir: Option<PathBuf>,
    /// Set to true by shutdown() to cancel any in-flight bootstrap.
    cancelled: Arc<AtomicBool>,
}

impl I2PManager {
    pub fn new() -> Self {
        Self {
            process: None,
            bootstrapped: false,
            i2pd_path: None,
            data_dir: None,
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Get a clone of the cancellation flag.
    /// The caller (tor_commands.rs) passes this to long-running operations
    /// so they abort when shutdown() is called.
    pub fn cancelled(&self) -> Arc<AtomicBool> {
        self.cancelled.clone()
    }

    pub fn is_bootstrapped(&self) -> bool {
        self.bootstrapped
    }

    pub fn proxy_url(&self) -> String {
        format!("socks5h://127.0.0.1:{SOCKS_PORT}")
    }

    /// Shut down the i2pd child process and cancel any in-flight bootstrap.
    pub async fn shutdown(&mut self) {
        self.cancelled.store(true, Ordering::SeqCst);

        if let Some(mut child) = self.process.take() {
            let pid = child.id();
            info!("I2P: stopping i2pd (pid {pid:?})");
            let _ = child.kill().await;

            // Wait up to 3s for graceful exit, then force-kill
            match tokio::time::timeout(std::time::Duration::from_secs(3), child.wait()).await {
                Ok(_) => info!("I2P: i2pd stopped gracefully"),
                Err(_) => {
                    if let Some(p) = pid {
                        force_kill_pid(p);
                    }
                    warn!("I2P: i2pd force-killed after timeout");
                }
            }
        }
        self.bootstrapped = false;
    }

    /// Start i2pd and wait for its SOCKS proxy to become available.
    ///
    /// After this returns Ok, the SOCKS proxy is accepting connections but
    /// tunnels may not be built yet. The caller should keep the banner in
    /// "bootstrapping" state until the first Matrix sync succeeds.
    pub async fn bootstrap(
        &mut self,
        app_data_dir: PathBuf,
        app: &tauri::AppHandle,
    ) -> Result<(), String> {
        // Kill our own old process
        self.shutdown().await;

        // Reset cancellation flag for this new bootstrap run
        self.cancelled.store(false, Ordering::SeqCst);

        // Kill any stale i2pd processes from previous crashes
        kill_stale_i2pd().await;

        let cancelled = self.cancelled.clone();

        emit_i2p_status(app, "bootstrapping", "Searching for i2pd binary...");
        let i2pd_path = find_i2pd_binary(&app_data_dir)?;
        emit_i2p_status(
            app,
            "bootstrapping",
            &format!("Found i2pd at {}", i2pd_path.display()),
        );

        let i2p_dir = app_data_dir.join("i2pd");
        std::fs::create_dir_all(&i2p_dir)
            .map_err(|e| format!("I2P: create data dir: {e}"))?;

        self.i2pd_path = Some(i2pd_path.clone());
        self.data_dir = Some(i2p_dir.clone());

        emit_i2p_status(app, "bootstrapping", "Starting i2pd daemon...");

        let mut cmd = Command::new(&i2pd_path);
        cmd.arg("--datadir").arg(&i2p_dir)
            .arg("--socksproxy.enabled").arg("true")
            .arg("--socksproxy.port").arg(SOCKS_PORT.to_string())
            .arg("--socksproxy.address").arg("127.0.0.1")
            .arg("--httpproxy.enabled").arg("false")
            .arg("--sam.enabled").arg("false")
            .arg("--http.enabled").arg("true")
            .arg("--http.port").arg(CONSOLE_PORT.to_string())
            .arg("--http.address").arg("127.0.0.1")
            .arg("--http.strictheaders").arg("false")
            .arg("--log").arg("stdout")
            .arg("--loglevel").arg("warn")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);

        // Windows: hide console window, taskbar icon, and notification
        #[cfg(windows)]
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW

        let child = cmd
            .spawn()
            .map_err(|e| format!("I2P: failed to start i2pd: {e}"))?;

        let pid = child.id().unwrap_or(0);
        emit_i2p_status(
            app,
            "bootstrapping",
            &format!("i2pd started (pid {pid})"),
        );
        self.process = Some(child);

        // Wait for SOCKS proxy with real handshake
        if cancelled.load(Ordering::SeqCst) {
            info!("I2P: bootstrap cancelled before SOCKS wait");
            return Err("I2P: bootstrap cancelled".into());
        }
        emit_i2p_status(
            app,
            "bootstrapping",
            &format!("Waiting for SOCKS proxy on port {SOCKS_PORT}..."),
        );
        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(120);

        loop {
            if cancelled.load(Ordering::SeqCst) {
                info!("I2P: bootstrap cancelled during SOCKS wait");
                self.shutdown().await;
                return Err("I2P: bootstrap cancelled".into());
            }

            if start.elapsed() > timeout {
                emit_i2p_status(app, "error", "SOCKS proxy did not start within 120s");
                self.shutdown().await;
                return Err("I2P: SOCKS proxy not ready after 120s".into());
            }

            // Check process is still alive
            if let Some(ref mut child) = self.process {
                if let Ok(Some(status)) = child.try_wait() {
                    self.process = None;
                    let msg = format!("i2pd exited unexpectedly: {status}");
                    emit_i2p_status(app, "error", &msg);
                    return Err(format!("I2P: {msg}"));
                }
            }

            if check_socks_health().await {
                break;
            }

            tokio::time::sleep(std::time::Duration::from_secs(2)).await;

            let elapsed = start.elapsed().as_secs();
            if elapsed % 10 == 0 && elapsed > 0 {
                emit_i2p_status(
                    app,
                    "bootstrapping",
                    "Waiting for SOCKS proxy...",
                );
            }
        }

        emit_i2p_status(
            app,
            "bootstrapping",
            "SOCKS proxy ready, waiting for I2P network...",
        );
        self.bootstrapped = true;

        // NOTE: We do NOT emit "connected" here. The SOCKS proxy is open but
        // tunnels are not built yet. The caller (tor_commands.rs) emits
        // "connected" only after the first successful Matrix operation.

        Ok(())
    }
}

/// Find the i2pd binary. Search order:
/// 1. App data dir (user-placed binary)
/// 2. System PATH
fn find_i2pd_binary(app_data_dir: &PathBuf) -> Result<PathBuf, String> {
    let local = app_data_dir.join("i2pd.exe");
    if local.exists() {
        return Ok(local);
    }

    let local_nix = app_data_dir.join("i2pd");
    if local_nix.exists() {
        return Ok(local_nix);
    }

    if let Ok(path) = which::which("i2pd") {
        return Ok(path);
    }

    Err(format!(
        "I2P: i2pd not found. Place i2pd.exe in {:?} or install to PATH. \
         Download: https://github.com/PurpleI2P/i2pd/releases",
        app_data_dir
    ))
}

// ---------------------------------------------------------------------------
// Process management helpers
// ---------------------------------------------------------------------------

/// Create a hidden std::process::Command on Windows (CREATE_NO_WINDOW).
fn hidden_command(program: &str) -> std::process::Command {
    let mut cmd = std::process::Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    cmd
}

/// Kill all stale i2pd processes from previous app runs.
async fn kill_stale_i2pd() {
    #[cfg(windows)]
    {
        let mut cmd = hidden_command("taskkill");
        cmd.args(["/IM", "i2pd.exe", "/F"]);
        if let Ok(output) = cmd.output() {
            if output.status.success() {
                info!("I2P: killed stale i2pd processes");
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
        }
    }
    #[cfg(unix)]
    {
        let _ = std::process::Command::new("killall")
            .args(["-q", "i2pd"])
            .output();
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
}

/// Force-kill a specific process by PID (fallback when kill_on_drop fails).
fn force_kill_pid(pid: u32) {
    #[cfg(windows)]
    {
        let mut cmd = hidden_command("taskkill");
        cmd.args(["/PID", &pid.to_string(), "/F"]);
        let _ = cmd.output();
    }
    #[cfg(unix)]
    {
        let _ = std::process::Command::new("kill")
            .args(["-9", &pid.to_string()])
            .output();
    }
}

/// Kill all i2pd processes. Called on app exit to prevent zombies.
pub fn kill_all_i2pd() {
    #[cfg(windows)]
    {
        let mut cmd = hidden_command("taskkill");
        cmd.args(["/IM", "i2pd.exe", "/F"]);
        let _ = cmd.output();
    }
    #[cfg(unix)]
    {
        let _ = std::process::Command::new("killall")
            .args(["-q", "i2pd"])
            .output();
    }
}

/// Wait until a real SOCKS5 CONNECT to the .b32.i2p homeserver succeeds.
/// Emits "bootstrapping" with progress every 5s, then "connected" on success.
/// Gives up after 5 minutes with an "error" status.
pub async fn wait_for_tunnel_ready(app: tauri::AppHandle, cancelled: Arc<AtomicBool>) {
    let mut attempts = 0u32;
    let max_attempts = 60u32; // 60 * 5s = 5 minutes

    loop {
        if cancelled.load(Ordering::SeqCst) {
            info!("I2P: tunnel readiness check cancelled");
            return;
        }

        attempts += 1;
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;

        if cancelled.load(Ordering::SeqCst) {
            return;
        }

        // Try a real SOCKS5 CONNECT through i2pd to our homeserver
        let ok = test_socks_connect_to_homeserver().await;

        if ok {
            emit_i2p_status(&app, "connected", "Matrix connected via I2P");
            return;
        }

        emit_i2p_status(
            &app,
            "bootstrapping",
            "Building I2P tunnels...",
        );

        if attempts >= max_attempts {
            emit_i2p_status(
                &app,
                "error",
                "Could not reach homeserver via I2P after 5 minutes",
            );
            return;
        }
    }
}

/// Test a real SOCKS5 CONNECT to our Matrix homeserver through the i2pd proxy.
/// Returns true if the tunnel is working.
async fn test_socks_connect_to_homeserver() -> bool {
    let Ok(Ok(mut stream)) = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        TcpStream::connect(format!("127.0.0.1:{SOCKS_PORT}")),
    )
    .await
    else {
        return false;
    };

    // SOCKS5 greeting
    if stream.write_all(&[0x05, 0x01, 0x00]).await.is_err() {
        return false;
    }
    let _ = stream.flush().await;

    let mut buf = [0u8; 2];
    if tokio::time::timeout(std::time::Duration::from_secs(3), stream.read_exact(&mut buf))
        .await
        .is_err()
    {
        return false;
    }
    if buf[0] != 0x05 {
        return false;
    }

    // SOCKS5 CONNECT to homeserver:8448
    let domain = MATRIX_I2P_ADDR.as_bytes();
    let mut req = vec![0x05, 0x01, 0x00, 0x03, domain.len() as u8];
    req.extend_from_slice(domain);
    req.extend_from_slice(&8448u16.to_be_bytes());

    if stream.write_all(&req).await.is_err() {
        return false;
    }
    let _ = stream.flush().await;

    // Read SOCKS5 reply (min 10 bytes for IPv4 response)
    let mut resp = [0u8; 10];
    match tokio::time::timeout(std::time::Duration::from_secs(30), stream.read_exact(&mut resp))
        .await
    {
        Ok(Ok(_)) => resp[1] == 0x00, // 0x00 = success
        _ => false,
    }
}

/// SOCKS5 health check - real handshake, not just TCP connect.
pub async fn check_socks_health() -> bool {
    let Ok(Ok(mut stream)) = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        TcpStream::connect(format!("127.0.0.1:{SOCKS_PORT}")),
    )
    .await
    else {
        return false;
    };

    if stream.write_all(&[0x05, 0x01, 0x00]).await.is_err() {
        return false;
    }
    let _ = stream.flush().await;

    let mut buf = [0u8; 2];
    match tokio::time::timeout(std::time::Duration::from_secs(2), stream.read_exact(&mut buf))
        .await
    {
        Ok(Ok(_)) => buf[0] == 0x05,
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Watchdog
// ---------------------------------------------------------------------------

/// Monitors i2pd SOCKS proxy health every 30s.
/// After 3 consecutive failures, restarts the process.
pub async fn start_watchdog(i2p: Arc<Mutex<I2PManager>>, app: tauri::AppHandle) {
    let mut consecutive_failures = 0u32;

    loop {
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;

        let manager = i2p.lock().await;
        if !manager.is_bootstrapped() {
            continue;
        }
        drop(manager);

        if check_socks_health().await {
            if consecutive_failures > 0 {
                info!("I2P Watchdog: recovered");
            }
            consecutive_failures = 0;
        } else {
            consecutive_failures += 1;
            warn!(
                "I2P Watchdog: health check failed ({consecutive_failures} consecutive)"
            );

            if consecutive_failures >= 3 {
                warn!("I2P Watchdog: restarting i2pd...");
                emit_i2p_status(&app, "reconnecting", "Restarting i2pd after health check failure...");

                let mut manager = i2p.lock().await;
                let data_dir = manager
                    .data_dir
                    .clone()
                    .unwrap_or_else(|| {
                        dirs::data_local_dir()
                            .unwrap_or_default()
                            .join("simplego-x")
                    });

                let cancelled = manager.cancelled();
                match manager.bootstrap(data_dir, &app).await {
                    Ok(_) => {
                        info!("I2P Watchdog: restarted, verifying tunnel...");
                        let app_bg = app.clone();
                        tokio::spawn(async move {
                            wait_for_tunnel_ready(app_bg, cancelled).await;
                        });
                        consecutive_failures = 0;
                    }
                    Err(e) => {
                        error!("I2P Watchdog: restart failed: {e}");
                        emit_i2p_status(&app, "error", &format!("Restart failed: {e}"));
                    }
                }
            }
        }
    }
}
