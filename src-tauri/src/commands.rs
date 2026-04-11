//! Tauri IPC commands for SimpleGoX Desktop.
//!
//! Every function annotated with `#[tauri::command]` is callable from the
//! WebView via `window.__TAURI__.core.invoke(...)`. No access tokens,
//! encryption keys or matrix-sdk types ever cross the IPC boundary.

use serde::Serialize;
use sgx_core::{
    IncomingMessage, IotDevice, IotStatusPayload, RoomDetail, RoomMemberInfo, RoomSettings,
    RoomSummary, SgxClient, SgxConfig, TypingPayload, UserProfile,
};
use std::sync::Arc;
use tauri::{Emitter, State};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use tracing::info;

// ---------------------------------------------------------------------------
// Shared application state
// ---------------------------------------------------------------------------

pub struct AppState {
    pub client: Arc<Mutex<Option<SgxClient>>>,
    pub sync_cancel: Arc<Mutex<CancellationToken>>,
}

// ---------------------------------------------------------------------------
// Payload types
// ---------------------------------------------------------------------------

#[derive(Serialize, Clone)]
pub struct LoginResult {
    pub user_id: String,
    pub device_id: String,
    pub homeserver: String,
}

#[derive(Serialize)]
pub struct AppSettings {
    pub user_id: String,
    pub device_id: String,
    pub homeserver: String,
}

// ---------------------------------------------------------------------------
// Helper: start the background sync loop
// ---------------------------------------------------------------------------

fn spawn_sync(sync_client: SgxClient, app: &tauri::AppHandle, cancel: CancellationToken) {
    let app_msg = app.clone();
    let app_typing = app.clone();
    let app_iot = app.clone();
    let app_rx = app.clone();

    tokio::spawn(async move {
        let on_message = move |msg: IncomingMessage| {
            let _ = app_msg.emit("new-message", msg);
        };
        let on_typing = move |payload: TypingPayload| {
            let _ = app_typing.emit("typing", payload);
        };
        let on_iot = move |payload: IotStatusPayload| {
            let _ = app_iot.emit("iot-status", payload);
        };
        let on_reaction = move |reaction: sgx_core::IncomingReaction| {
            let _ = app_rx.emit("new-reaction", reaction);
        };

        tokio::select! {
            result = sync_client.sync_with_all_callbacks(on_message, on_typing, on_iot, on_reaction) => {
                if let Err(e) = result {
                    if !cancel.is_cancelled() {
                        tracing::error!("Sync loop ended with error: {e}");
                    }
                }
            }
            _ = cancel.cancelled() => {
                info!("Sync loop cancelled by logout");
            }
        }
    });
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
        recovery_key: None,
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

    // Cross-signing (best-effort)
    if let Err(e) = client.bootstrap_cross_signing(&password).await {
        tracing::warn!("Cross-signing bootstrap failed (may already exist): {e}");
    }

    // Recovery key (best-effort, save to config if successful)
    match client.enable_recovery().await {
        Ok(key) => {
            config.recovery_key = Some(key);
            if let Err(e) = config.save_to_file(&config_path) {
                tracing::error!("Failed to persist recovery key to config: {e}");
            } else {
                tracing::info!("Recovery key saved to config");
            }
        }
        Err(e) => tracing::warn!("Recovery enable failed (may already exist): {e}"),
    }

    let sync_client = client.clone_inner();
    let cancel = CancellationToken::new();

    {
        let mut guard = state.client.lock().await;
        *guard = Some(client);
        *state.sync_cancel.lock().await = cancel.clone();
    }

    spawn_sync(sync_client, &app, cancel);

    info!(user = %user_id, "Login complete, sync started");

    Ok(LoginResult {
        user_id,
        device_id,
        homeserver,
    })
}

/// Try to restore a previous session from the config file.
/// Returns `None` if no session is available (show login screen).
#[tauri::command]
pub async fn try_restore_session(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<Option<LoginResult>, String> {
    let config_path = SgxConfig::default_config_path();
    let config = match SgxConfig::from_file(&config_path) {
        Ok(c) => c,
        Err(_) => return Ok(None),
    };

    if !config.has_session() {
        return Ok(None);
    }

    let user_id = config.user_id.clone().unwrap_or_default();
    let device_id = config.device_id.clone().unwrap_or_default();
    let homeserver = config.homeserver_url.clone();

    let client = SgxClient::new(config)
        .await
        .map_err(|e| format!("Client init failed: {e}"))?;

    client
        .restore_session()
        .await
        .map_err(|e| format!("Session restore failed: {e}"))?;

    let sync_client = client.clone_inner();
    let cancel = CancellationToken::new();

    {
        let mut guard = state.client.lock().await;
        *guard = Some(client);
        *state.sync_cancel.lock().await = cancel.clone();
    }

    spawn_sync(sync_client, &app, cancel);

    info!(user = %user_id, "Session restored, sync started");

    Ok(Some(LoginResult {
        user_id,
        device_id,
        homeserver,
    }))
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

/// Return the recovery key from the config, if available.
#[tauri::command]
pub async fn get_recovery_key() -> Result<Option<String>, String> {
    let config_path = SgxConfig::default_config_path();
    let cfg = SgxConfig::from_file(&config_path).map_err(|e| format!("Config read failed: {e}"))?;
    Ok(cfg.recovery_key)
}

/// Send an IoT command to a device in a room.
#[tauri::command]
pub async fn send_iot_command(
    state: State<'_, AppState>,
    room_id: String,
    device_id: String,
    action: String,
    value: serde_json::Value,
) -> Result<(), String> {
    let guard = state.client.lock().await;
    let client = guard.as_ref().ok_or_else(|| "Not logged in".to_string())?;
    client
        .send_iot_command(&room_id, &device_id, &action, value)
        .await
        .map_err(|e| format!("IoT command failed: {e}"))
}

/// Get all IoT devices registered in a room.
#[tauri::command]
pub async fn get_iot_devices(
    state: State<'_, AppState>,
    room_id: String,
) -> Result<Vec<IotDevice>, String> {
    let guard = state.client.lock().await;
    let client = guard.as_ref().ok_or_else(|| "Not logged in".to_string())?;
    client
        .get_iot_devices(&room_id)
        .await
        .map_err(|e| format!("IoT devices fetch failed: {e}"))
}

// ---------------------------------------------------------------------------
// Room management commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn create_room(
    state: State<'_, AppState>,
    name: String,
    is_encrypted: bool,
    is_public: bool,
    topic: Option<String>,
    invite_user_ids: Option<Vec<String>>,
) -> Result<String, String> {
    let guard = state.client.lock().await;
    let client = guard.as_ref().ok_or_else(|| "Not logged in".to_string())?;
    client
        .create_room(
            &name,
            is_encrypted,
            is_public,
            topic.as_deref(),
            invite_user_ids,
        )
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn create_dm(state: State<'_, AppState>, user_id: String) -> Result<String, String> {
    let guard = state.client.lock().await;
    let client = guard.as_ref().ok_or_else(|| "Not logged in".to_string())?;
    client.create_dm(&user_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn join_room(
    state: State<'_, AppState>,
    room_id_or_alias: String,
) -> Result<String, String> {
    let guard = state.client.lock().await;
    let client = guard.as_ref().ok_or_else(|| "Not logged in".to_string())?;
    client
        .join_room(&room_id_or_alias)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn leave_room(state: State<'_, AppState>, room_id: String) -> Result<(), String> {
    let guard = state.client.lock().await;
    let client = guard.as_ref().ok_or_else(|| "Not logged in".to_string())?;
    client.leave_room(&room_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn invite_user(
    state: State<'_, AppState>,
    room_id: String,
    user_id: String,
) -> Result<(), String> {
    let guard = state.client.lock().await;
    let client = guard.as_ref().ok_or_else(|| "Not logged in".to_string())?;
    client
        .invite_user(&room_id, &user_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn kick_user(
    state: State<'_, AppState>,
    room_id: String,
    user_id: String,
    reason: Option<String>,
) -> Result<(), String> {
    let guard = state.client.lock().await;
    let client = guard.as_ref().ok_or_else(|| "Not logged in".to_string())?;
    client
        .kick_user(&room_id, &user_id, reason.as_deref())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn ban_user(
    state: State<'_, AppState>,
    room_id: String,
    user_id: String,
    reason: Option<String>,
) -> Result<(), String> {
    let guard = state.client.lock().await;
    let client = guard.as_ref().ok_or_else(|| "Not logged in".to_string())?;
    client
        .ban_user(&room_id, &user_id, reason.as_deref())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn unban_user(
    state: State<'_, AppState>,
    room_id: String,
    user_id: String,
) -> Result<(), String> {
    let guard = state.client.lock().await;
    let client = guard.as_ref().ok_or_else(|| "Not logged in".to_string())?;
    client
        .unban_user(&room_id, &user_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_room_members(
    state: State<'_, AppState>,
    room_id: String,
) -> Result<Vec<RoomMemberInfo>, String> {
    let guard = state.client.lock().await;
    let client = guard.as_ref().ok_or_else(|| "Not logged in".to_string())?;
    client
        .get_room_members(&room_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_room_info(
    state: State<'_, AppState>,
    room_id: String,
) -> Result<RoomDetail, String> {
    let guard = state.client.lock().await;
    let client = guard.as_ref().ok_or_else(|| "Not logged in".to_string())?;
    client
        .get_room_info(&room_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_room_name(
    state: State<'_, AppState>,
    room_id: String,
    name: String,
) -> Result<(), String> {
    let guard = state.client.lock().await;
    let client = guard.as_ref().ok_or_else(|| "Not logged in".to_string())?;
    client
        .set_room_name(&room_id, &name)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_room_topic(
    state: State<'_, AppState>,
    room_id: String,
    topic: String,
) -> Result<(), String> {
    let guard = state.client.lock().await;
    let client = guard.as_ref().ok_or_else(|| "Not logged in".to_string())?;
    client
        .set_room_topic(&room_id, &topic)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_room_tag(
    state: State<'_, AppState>,
    room_id: String,
    tag: String,
    order: Option<f64>,
) -> Result<(), String> {
    let guard = state.client.lock().await;
    let client = guard.as_ref().ok_or_else(|| "Not logged in".to_string())?;
    client
        .set_room_tag(&room_id, &tag, order)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn remove_room_tag(
    state: State<'_, AppState>,
    room_id: String,
    tag: String,
) -> Result<(), String> {
    let guard = state.client.lock().await;
    let client = guard.as_ref().ok_or_else(|| "Not logged in".to_string())?;
    client
        .remove_room_tag(&room_id, &tag)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn redact_event(
    state: State<'_, AppState>,
    room_id: String,
    event_id: String,
    reason: Option<String>,
) -> Result<(), String> {
    let guard = state.client.lock().await;
    let client = guard.as_ref().ok_or_else(|| "Not logged in".to_string())?;
    client
        .redact_event(&room_id, &event_id, reason.as_deref())
        .await
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Room settings commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn get_room_settings(
    state: State<'_, AppState>,
    room_id: String,
) -> Result<RoomSettings, String> {
    let guard = state.client.lock().await;
    let client = guard.as_ref().ok_or_else(|| "Not logged in".to_string())?;
    client
        .get_room_settings(&room_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_join_rule(
    state: State<'_, AppState>,
    room_id: String,
    join_rule: String,
) -> Result<(), String> {
    let guard = state.client.lock().await;
    let client = guard.as_ref().ok_or_else(|| "Not logged in".to_string())?;
    client
        .set_join_rule(&room_id, &join_rule)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_history_visibility(
    state: State<'_, AppState>,
    room_id: String,
    visibility: String,
) -> Result<(), String> {
    let guard = state.client.lock().await;
    let client = guard.as_ref().ok_or_else(|| "Not logged in".to_string())?;
    client
        .set_history_visibility(&room_id, &visibility)
        .await
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Avatar / Profile commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn resolve_mxc_url(
    state: State<'_, AppState>,
    mxc_uri: String,
    width: Option<u32>,
    height: Option<u32>,
) -> Result<String, String> {
    let guard = state.client.lock().await;
    let client = guard.as_ref().ok_or_else(|| "Not logged in".to_string())?;
    client
        .resolve_mxc_url(&mxc_uri, width, height)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_avatar_base64(
    state: State<'_, AppState>,
    mxc_uri: String,
    width: u32,
    height: u32,
) -> Result<String, String> {
    let guard = state.client.lock().await;
    let client = guard.as_ref().ok_or_else(|| "Not logged in".to_string())?;
    client
        .get_avatar_base64(&mxc_uri, width, height)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_own_profile(state: State<'_, AppState>) -> Result<UserProfile, String> {
    let guard = state.client.lock().await;
    let client = guard.as_ref().ok_or_else(|| "Not logged in".to_string())?;
    client.get_own_profile().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_display_name(state: State<'_, AppState>, name: String) -> Result<(), String> {
    let guard = state.client.lock().await;
    let client = guard.as_ref().ok_or_else(|| "Not logged in".to_string())?;
    client
        .set_display_name(&name)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn upload_avatar(
    state: State<'_, AppState>,
    data: Vec<u8>,
    content_type: String,
) -> Result<String, String> {
    let guard = state.client.lock().await;
    let client = guard.as_ref().ok_or_else(|| "Not logged in".to_string())?;
    client
        .upload_avatar(data, &content_type)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn remove_avatar(state: State<'_, AppState>) -> Result<(), String> {
    let guard = state.client.lock().await;
    let client = guard.as_ref().ok_or_else(|| "Not logged in".to_string())?;
    client.remove_own_avatar().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_room_avatar(
    state: State<'_, AppState>,
    room_id: String,
    data: Vec<u8>,
    content_type: String,
) -> Result<(), String> {
    let guard = state.client.lock().await;
    let client = guard.as_ref().ok_or_else(|| "Not logged in".to_string())?;
    client
        .set_room_avatar(&room_id, data, &content_type)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn remove_room_avatar(state: State<'_, AppState>, room_id: String) -> Result<(), String> {
    let guard = state.client.lock().await;
    let client = guard.as_ref().ok_or_else(|| "Not logged in".to_string())?;
    client
        .remove_room_avatar(&room_id)
        .await
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// File-based avatar upload (reads file directly in Rust)
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn get_room_messages(
    state: State<'_, AppState>,
    room_id: String,
    limit: Option<u32>,
) -> Result<Vec<sgx_core::IncomingMessage>, String> {
    let guard = state.client.lock().await;
    let client = guard.as_ref().ok_or_else(|| "Not logged in".to_string())?;
    client
        .get_room_messages(&room_id, limit.unwrap_or(50))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn send_reply(
    state: State<'_, AppState>,
    room_id: String,
    body: String,
    reply_to_event_id: String,
) -> Result<(), String> {
    let guard = state.client.lock().await;
    let client = guard.as_ref().ok_or_else(|| "Not logged in".to_string())?;
    client
        .send_reply(&room_id, &body, &reply_to_event_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn send_reaction(
    state: State<'_, AppState>,
    room_id: String,
    event_id: String,
    emoji: String,
) -> Result<(), String> {
    let guard = state.client.lock().await;
    let client = guard.as_ref().ok_or_else(|| "Not logged in".to_string())?;
    client
        .send_reaction(&room_id, &event_id, &emoji)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn edit_message(
    state: State<'_, AppState>,
    room_id: String,
    event_id: String,
    new_body: String,
) -> Result<(), String> {
    let guard = state.client.lock().await;
    let client = guard.as_ref().ok_or_else(|| "Not logged in".to_string())?;
    client
        .edit_message(&room_id, &event_id, &new_body)
        .await
        .map_err(|e| e.to_string())
}

fn guess_mime_type(path: &str) -> String {
    let p = path.to_lowercase();
    if p.ends_with(".png") {
        "image/png"
    } else if p.ends_with(".jpg") || p.ends_with(".jpeg") {
        "image/jpeg"
    } else if p.ends_with(".gif") {
        "image/gif"
    } else if p.ends_with(".webp") {
        "image/webp"
    } else {
        "application/octet-stream"
    }
    .to_string()
}

#[tauri::command]
pub async fn upload_avatar_from_path(
    state: State<'_, AppState>,
    file_path: String,
) -> Result<String, String> {
    info!("upload_avatar_from_path: {}", file_path);
    let data = std::fs::read(&file_path).map_err(|e| {
        tracing::error!("Failed to read file {}: {}", file_path, e);
        format!("Failed to read file: {e}")
    })?;
    info!("File read: {} bytes", data.len());
    if data.len() > 5 * 1024 * 1024 {
        return Err("File too large. Maximum 5 MB.".to_string());
    }
    let ct = guess_mime_type(&file_path);
    info!("MIME type: {}", ct);
    let guard = state.client.lock().await;
    let client = guard.as_ref().ok_or_else(|| "Not logged in".to_string())?;
    let result = client.upload_avatar(data, &ct).await.map_err(|e| {
        tracing::error!("Avatar upload failed: {}", e);
        e.to_string()
    })?;
    info!("Avatar uploaded: {}", result);
    Ok(result)
}

#[tauri::command]
pub async fn upload_room_avatar_from_path(
    state: State<'_, AppState>,
    room_id: String,
    file_path: String,
) -> Result<(), String> {
    let data = std::fs::read(&file_path).map_err(|e| format!("Failed to read file: {e}"))?;
    if data.len() > 5 * 1024 * 1024 {
        return Err("File too large. Maximum 5 MB.".to_string());
    }
    let ct = guess_mime_type(&file_path);
    let guard = state.client.lock().await;
    let client = guard.as_ref().ok_or_else(|| "Not logged in".to_string())?;
    client
        .set_room_avatar(&room_id, data, &ct)
        .await
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------

/// Log out on the server and remove local data.
#[tauri::command]
pub async fn logout(state: State<'_, AppState>) -> Result<(), String> {
    // 1. Cancel sync loop FIRST
    info!("logout: cancelling sync loop...");
    state.sync_cancel.lock().await.cancel();
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    // 2. Collect paths BEFORE deleting anything
    let config_path = SgxConfig::default_config_path();
    let config_dir = config_path.parent().map(|p| p.to_path_buf());
    let data_dir = dirs::data_local_dir()
        .unwrap_or_default()
        .join("simplego-x");

    info!("logout: config_dir={:?}", config_dir);
    info!("logout: data_dir={:?}", data_dir);

    // 3. Server-side logout
    let mut guard = state.client.lock().await;
    if let Some(ref client) = *guard {
        info!("logout: server-side logout...");
        let _ = client.logout().await; // Don't fail on server error
    }

    // 4. Drop client to release ALL file handles
    *guard = None;
    drop(guard);

    // 5. Retry deletion - SQLite handles may take time to release
    //    The sync task holds a SgxClient clone; after cancel it may
    //    linger until tokio reclaims the task.
    for attempt in 1..=5 {
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

        let mut all_gone = true;

        // Delete data dir (sqlite3 files)
        if data_dir.exists() {
            info!("logout: attempt {attempt} - deleting {:?}", data_dir);
            match std::fs::remove_dir_all(&data_dir) {
                Ok(_) => info!("logout: data dir deleted on attempt {attempt}"),
                Err(e) => {
                    info!("logout: attempt {attempt} failed: {e}");
                    all_gone = false;
                }
            }
        }

        // Delete config dir
        if let Some(ref cd) = config_dir {
            if cd.exists() {
                let _ = std::fs::remove_dir_all(cd);
            }
        }

        if all_gone {
            break;
        }
    }

    // Final verification
    if data_dir.exists() {
        tracing::error!(
            "logout: DATA DIR STILL EXISTS after 5 attempts: {:?}",
            data_dir
        );
        // Last resort: list what's left
        if let Ok(entries) = std::fs::read_dir(&data_dir) {
            for entry in entries.flatten() {
                tracing::error!("logout: leftover: {:?}", entry.path());
            }
        }
    } else {
        info!("logout: all local data deleted successfully");
    }

    Ok(())
}
