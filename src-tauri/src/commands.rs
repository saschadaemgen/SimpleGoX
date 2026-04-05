//! Tauri IPC commands for SimpleGoX Desktop.
//!
//! Every function annotated with `#[tauri::command]` is callable from the
//! WebView via `window.__TAURI__.core.invoke(...)`. No access tokens,
//! encryption keys or matrix-sdk types ever cross the IPC boundary.

use serde::Serialize;
use sgx_core::{IncomingMessage, RoomSummary, SgxClient, SgxConfig, TypingPayload};
use std::sync::Arc;
use tauri::{Emitter, State};
use tokio::sync::Mutex;
use tracing::info;

// ---------------------------------------------------------------------------
// Shared application state
// ---------------------------------------------------------------------------

/// Shared state managed by Tauri. The `Option` is `None` before login.
pub struct AppState {
    pub client: Arc<Mutex<Option<SgxClient>>>,
}

// ---------------------------------------------------------------------------
// Payload types (serialised to the frontend via emit / return)
// ---------------------------------------------------------------------------

/// Returned to the frontend after a successful login.
#[derive(Serialize)]
pub struct LoginResult {
    pub user_id: String,
    pub device_id: String,
    pub homeserver: String,
}

/// Account info shown in the Settings screen.
#[derive(Serialize)]
pub struct AppSettings {
    pub user_id: String,
    pub device_id: String,
    pub homeserver: String,
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// Log in, bootstrap cross-signing and start the background sync loop.
#[tauri::command]
pub async fn login(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    homeserver: String,
    username: String,
    password: String,
) -> Result<LoginResult, String> {
    let mut config = SgxConfig {
        homeserver_url: homeserver.clone(),
        username: username.clone(),
        data_dir: dirs::data_local_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("simplego-x"),
        encryption: true,
        user_id: None,
        device_id: None,
        access_token: None,
        refresh_token: None,
    };

    let client = SgxClient::new(config.clone())
        .await
        .map_err(|e| format!("Client init failed: {e}"))?;

    client
        .login(&password)
        .await
        .map_err(|e| format!("Login failed: {e}"))?;

    let (user_id, device_id, access_token, refresh_token) = client
        .session_credentials()
        .map_err(|e| format!("Session extraction failed: {e}"))?;

    config.user_id = Some(user_id.clone());
    config.device_id = Some(device_id.clone());
    config.access_token = Some(access_token);
    config.refresh_token = refresh_token;

    let config_path = SgxConfig::default_config_path();
    config
        .save_to_file(&config_path)
        .map_err(|e| format!("Config save failed: {e}"))?;

    // Cross-signing (best-effort - do not block login if it fails on repeat)
    if let Err(e) = client.bootstrap_cross_signing(&password).await {
        tracing::warn!("Cross-signing bootstrap failed (may already exist): {e}");
    }

    // Clone the inner SDK client for the sync task BEFORE storing in state.
    let sync_client = client.clone_inner();

    // Store client in shared state (available for get_rooms, send, logout)
    {
        let mut guard = state.client.lock().await;
        *guard = Some(client);
    }

    // Start background sync loop with message + typing forwarding
    let app_handle = app.clone();
    let app_handle_typing = app.clone();
    tokio::spawn(async move {
        let on_message = move |msg: IncomingMessage| {
            let _ = app_handle.emit("new-message", msg);
        };
        let on_typing = move |payload: TypingPayload| {
            let _ = app_handle_typing.emit("typing", payload);
        };

        if let Err(e) = sync_client.sync_with_callbacks(on_message, on_typing).await {
            tracing::error!("Sync loop ended with error: {e}");
        }
    });

    info!(user = %user_id, "Login complete, sync started");

    Ok(LoginResult {
        user_id,
        device_id,
        homeserver,
    })
}

/// Return the list of joined rooms.
#[tauri::command]
pub async fn get_rooms(state: State<'_, AppState>) -> Result<Vec<RoomSummary>, String> {
    let guard = state.client.lock().await;
    let client = guard.as_ref().ok_or_else(|| "Not logged in".to_string())?;
    Ok(client.joined_rooms_summary().await)
}

/// Send a plain text message to a room.
#[tauri::command]
pub async fn send_message(
    state: State<'_, AppState>,
    room_id: String,
    message: String,
) -> Result<(), String> {
    let guard = state.client.lock().await;
    let client = guard.as_ref().ok_or_else(|| "Not logged in".to_string())?;
    client
        .send_to_room(&room_id, &message)
        .await
        .map_err(|e| format!("Send failed: {e}"))
}

/// Send a typing notice for a room.
#[tauri::command]
pub async fn send_typing(
    state: State<'_, AppState>,
    room_id: String,
    typing: bool,
) -> Result<(), String> {
    let guard = state.client.lock().await;
    let client = guard.as_ref().ok_or_else(|| "Not logged in".to_string())?;
    client
        .send_typing(&room_id, typing)
        .await
        .map_err(|e| format!("Typing notice failed: {e}"))
}

/// Mark a message as read by sending a read receipt.
#[tauri::command]
pub async fn mark_as_read(
    state: State<'_, AppState>,
    room_id: String,
    event_id: String,
) -> Result<(), String> {
    let guard = state.client.lock().await;
    let client = guard.as_ref().ok_or_else(|| "Not logged in".to_string())?;
    client
        .mark_as_read(&room_id, &event_id)
        .await
        .map_err(|e| format!("Read receipt failed: {e}"))
}

/// Return account info for the settings screen.
///
/// Reads from the persisted config file which has the full session data,
/// rather than the in-memory client config which may lack session fields.
#[tauri::command]
pub async fn get_settings() -> Result<AppSettings, String> {
    let config_path = SgxConfig::default_config_path();
    let cfg = SgxConfig::from_file(&config_path).map_err(|e| format!("Config read failed: {e}"))?;
    Ok(AppSettings {
        user_id: cfg.user_id.unwrap_or_default(),
        device_id: cfg.device_id.unwrap_or_default(),
        homeserver: cfg.homeserver_url,
    })
}

/// Log out on the server and remove local data.
#[tauri::command]
pub async fn logout(state: State<'_, AppState>) -> Result<(), String> {
    let mut guard = state.client.lock().await;

    if let Some(ref client) = *guard {
        client
            .logout()
            .await
            .map_err(|e| format!("Server logout failed: {e}"))?;
    }

    // Remove client from state
    *guard = None;
    drop(guard);

    // Remove local data
    let config_path = SgxConfig::default_config_path();
    if let Ok(config) = SgxConfig::from_file(&config_path) {
        if config.data_dir.exists() {
            let _ = std::fs::remove_dir_all(&config.data_dir);
        }
    }
    if config_path.exists() {
        let _ = std::fs::remove_file(&config_path);
    }

    info!("Logged out and local data removed");
    Ok(())
}
