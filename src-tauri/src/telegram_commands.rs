//! Tauri commands for multi-messenger sidecar backends.

use crate::sidecar::SidecarManager;
use serde::{Deserialize, Serialize};
use sgx_proto::messenger::v1::*;
use std::sync::Arc;
use tauri::{Emitter, State};

/// Fixed absolute path for TDLib data, shared between sidecar spawn and removal.
/// Uses %LOCALAPPDATA%/simplego-x/tdlib-data on Windows, ~/.local/share/simplego-x/tdlib-data elsewhere.
pub fn tdlib_data_dir() -> std::path::PathBuf {
    let base = dirs::data_local_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    base.join("simplego-x").join("tdlib-data")
}

/// Frontend-friendly chat format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrontendChat {
    pub id: String,
    pub backend: String,
    pub title: String,
    pub chat_type: String,
    pub avatar_url: String,
    pub last_message_body: String,
    pub last_message_time: i64,
    pub unread_count: i32,
    pub is_encrypted: bool,
    pub is_muted: bool,
    pub is_pinned: bool,
    pub badge_label: String,
    pub badge_color: String,
}

impl From<Chat> for FrontendChat {
    fn from(chat: Chat) -> Self {
        let chat_id = chat.chat_id.as_ref();
        let backend = chat_id.map(|c| c.backend.clone()).unwrap_or_default();
        let id = chat_id.map(|c| c.id.clone()).unwrap_or_default();

        let (last_body, last_time) = chat
            .last_message
            .as_ref()
            .map(|m| {
                let body = match &m.content {
                    Some(unified_message::Content::Text(t)) => t.body.clone(),
                    Some(unified_message::Content::Media(media)) => {
                        if media.caption.is_empty() {
                            "[Media]".to_string()
                        } else {
                            media.caption.clone()
                        }
                    }
                    _ => String::new(),
                };
                let time = m.timestamp.as_ref().map(|t| t.seconds).unwrap_or(0);
                (body, time)
            })
            .unwrap_or_default();

        let (badge_label, badge_color) = match backend.as_str() {
            "telegram" => ("TG".to_string(), "#61afef".to_string()),
            "simplex" => ("SX".to_string(), "#c678dd".to_string()),
            "whatsapp" => ("WA".to_string(), "#98c379".to_string()),
            _ => ("MX".to_string(), "#3fb9a8".to_string()),
        };

        FrontendChat {
            id,
            backend,
            title: chat.title,
            chat_type: format!("{}", chat.chat_type),
            avatar_url: chat.avatar_url,
            last_message_body: last_body,
            last_message_time: last_time,
            unread_count: chat.unread_count,
            is_encrypted: chat.is_encrypted,
            is_muted: chat.is_muted,
            is_pinned: chat.is_pinned,
            badge_label,
            badge_color,
        }
    }
}

/// Frontend-friendly message format (compatible with Matrix IncomingMessage).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrontendMessage {
    pub event_id: String,
    pub sender: String,
    pub sender_display_name: String,
    pub body: String,
    pub timestamp: u64,
    pub is_own: bool,
    pub is_edited: bool,
    pub is_redacted: bool,
    pub reply_to_event_id: Option<String>,
    pub reply_to_body: Option<String>,
    pub reply_to_sender: Option<String>,
    pub reactions: Vec<String>,
    pub backend: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrontendBackendInfo {
    pub backend_id: String,
    pub display_name: String,
    pub is_authenticated: bool,
    pub badge_label: String,
    pub badge_color: String,
}

// ==================== Commands ====================

/// Start the sgx-telegram sidecar process and connect to it.
/// API credentials are read from TG_API_ID / TG_API_HASH env vars with dev fallbacks.
#[tauri::command]
pub async fn tg_start_sidecar(
    app: tauri::AppHandle,
    sidecar: State<'_, Arc<SidecarManager>>,
    port: u16,
) -> Result<String, String> {
    use tauri_plugin_shell::ShellExt;

    let api_id = std::env::var("TG_API_ID").unwrap_or_else(|_| "34883771".to_string());
    let api_hash = std::env::var("TG_API_HASH")
        .unwrap_or_else(|_| "18be2f35cff67932d69d661faefe8fc3".to_string());
    let port_str = port.to_string();
    let data_dir = tdlib_data_dir();
    let data_dir_str = data_dir.to_string_lossy().to_string();

    tracing::info!(">>> tg_start_sidecar: data_dir={data_dir_str}");

    // Ensure parent directory exists
    if let Some(parent) = data_dir.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let cmd = app.shell().command("sgx-telegram").args([
        "--api-id",
        &api_id,
        "--api-hash",
        &api_hash,
        "--port",
        &port_str,
        "--data-dir",
        &data_dir_str,
    ]);

    cmd.spawn()
        .map_err(|e| format!("Failed to start sidecar: {e}"))?;

    // Wait a moment then connect
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    sidecar.connect("telegram", port).await?;

    Ok("Sidecar started and connected".into())
}

#[tauri::command]
pub async fn tg_connect(
    sidecar: State<'_, Arc<SidecarManager>>,
    port: u16,
) -> Result<String, String> {
    sidecar.connect("telegram", port).await?;
    Ok("Connected to Telegram sidecar".into())
}

#[derive(Debug, Clone, Serialize)]
pub struct TgAuthState {
    pub state: String,
    pub code_type: String,
}

fn parse_auth_state(state: AuthState) -> TgAuthState {
    match state.state {
        Some(auth_state::State::WaitPhone(_)) => TgAuthState {
            state: "wait_phone".into(),
            code_type: String::new(),
        },
        Some(auth_state::State::WaitCode(wc)) => TgAuthState {
            state: "wait_code".into(),
            code_type: wc.phone_number_hint, // carries code_type from sidecar
        },
        Some(auth_state::State::WaitPassword(_)) => TgAuthState {
            state: "wait_password".into(),
            code_type: String::new(),
        },
        Some(auth_state::State::Ready(_)) => TgAuthState {
            state: "ready".into(),
            code_type: String::new(),
        },
        Some(auth_state::State::LoggedOut(_)) => TgAuthState {
            state: "logged_out".into(),
            code_type: String::new(),
        },
        Some(auth_state::State::Error(_)) => TgAuthState {
            state: "error".into(),
            code_type: String::new(),
        },
        _ => TgAuthState {
            state: "unknown".into(),
            code_type: String::new(),
        },
    }
}

#[tauri::command]
pub async fn tg_get_auth_state(
    sidecar: State<'_, Arc<SidecarManager>>,
) -> Result<TgAuthState, String> {
    let mut client = sidecar
        .get_client("telegram")
        .await
        .ok_or("Telegram sidecar not connected")?;

    let response = client
        .get_auth_state(GetAuthStateRequest {})
        .await
        .map_err(|e| format!("gRPC error: {e}"))?;

    Ok(parse_auth_state(response.into_inner()))
}

#[tauri::command]
pub async fn tg_submit_phone(
    sidecar: State<'_, Arc<SidecarManager>>,
    phone: String,
) -> Result<TgAuthState, String> {
    let mut client = sidecar
        .get_client("telegram")
        .await
        .ok_or("Telegram sidecar not connected")?;

    client
        .submit_phone_number(SubmitPhoneNumberRequest {
            phone_number: phone,
        })
        .await
        .map_err(|e| format!("gRPC error: {e}"))?;

    // Poll for the resulting state (includes code_type)
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    let response = client
        .get_auth_state(GetAuthStateRequest {})
        .await
        .map_err(|e| format!("gRPC error: {e}"))?;

    Ok(parse_auth_state(response.into_inner()))
}

#[tauri::command]
pub async fn tg_submit_code(
    sidecar: State<'_, Arc<SidecarManager>>,
    code: String,
) -> Result<String, String> {
    let mut client = sidecar
        .get_client("telegram")
        .await
        .ok_or("Telegram sidecar not connected")?;

    client
        .submit_auth_code(SubmitAuthCodeRequest { code })
        .await
        .map_err(|e| format!("gRPC error: {e}"))?;

    Ok("Authenticated".into())
}

#[tauri::command]
pub async fn tg_submit_password(
    sidecar: State<'_, Arc<SidecarManager>>,
    password: String,
) -> Result<String, String> {
    let mut client = sidecar
        .get_client("telegram")
        .await
        .ok_or("Telegram sidecar not connected")?;

    client
        .submit_password(SubmitPasswordRequest { password })
        .await
        .map_err(|e| format!("gRPC error: {e}"))?;

    Ok("Authenticated".into())
}

#[tauri::command]
pub async fn tg_list_chats(
    sidecar: State<'_, Arc<SidecarManager>>,
    limit: i32,
) -> Result<Vec<FrontendChat>, String> {
    let mut client = sidecar
        .get_client("telegram")
        .await
        .ok_or("Telegram sidecar not connected")?;

    let response = client
        .list_chats(ListChatsRequest {
            limit,
            offset_order: 0,
        })
        .await
        .map_err(|e| format!("gRPC error: {e}"))?;

    Ok(response
        .into_inner()
        .chats
        .into_iter()
        .map(FrontendChat::from)
        .collect())
}

#[tauri::command]
pub async fn get_all_chats(
    sidecar: State<'_, Arc<SidecarManager>>,
) -> Result<Vec<FrontendChat>, String> {
    let chats = sidecar.list_all_chats(100).await?;
    Ok(chats.into_iter().map(FrontendChat::from).collect())
}

#[tauri::command]
pub async fn tg_get_messages(
    sidecar: State<'_, Arc<SidecarManager>>,
    chat_id: String,
    limit: i32,
) -> Result<Vec<FrontendMessage>, String> {
    tracing::info!(">>> tg_get_messages called: chat_id={chat_id} limit={limit}");

    let mut client = sidecar.get_client("telegram").await.ok_or_else(|| {
        tracing::error!(">>> tg_get_messages: sidecar NOT connected");
        "Telegram sidecar not connected".to_string()
    })?;

    tracing::info!(">>> tg_get_messages: got gRPC client, calling get_messages...");

    let response = client
        .get_messages(GetMessagesRequest {
            chat_id: Some(ChatId {
                backend: "telegram".into(),
                id: chat_id.clone(),
            }),
            limit,
            from_message_id: String::new(),
        })
        .await
        .map_err(|e| {
            tracing::error!(">>> tg_get_messages: gRPC FAILED: {e}");
            format!("gRPC error: {e}")
        })?;

    let raw_msgs = response.into_inner().messages;
    tracing::info!(
        ">>> tg_get_messages: got {} messages from sidecar",
        raw_msgs.len()
    );

    let messages = raw_msgs
        .into_iter()
        .map(|m| {
            let body = match &m.content {
                Some(unified_message::Content::Text(t)) => t.body.clone(),
                Some(unified_message::Content::Media(media)) => {
                    if media.caption.is_empty() {
                        "[Media]".to_string()
                    } else {
                        media.caption.clone()
                    }
                }
                Some(unified_message::Content::Sticker(s)) => format!("[Sticker: {}]", s.emoji),
                None => String::new(),
            };

            let msg_id = m
                .message_id
                .as_ref()
                .map(|id| id.id.clone())
                .unwrap_or_default();

            // Timestamps in milliseconds (Matrix convention)
            let timestamp = m
                .timestamp
                .as_ref()
                .map(|t| (t.seconds as u64) * 1000)
                .unwrap_or(0);

            let reply_to = if m.reply_to_message_id.is_empty() {
                None
            } else {
                Some(m.reply_to_message_id.clone())
            };

            FrontendMessage {
                event_id: msg_id,
                sender: m.sender_id.clone(),
                sender_display_name: m.sender_name.clone(),
                body,
                timestamp,
                is_own: m.is_outgoing,
                is_edited: m.is_edited,
                is_redacted: false,
                reply_to_event_id: reply_to,
                reply_to_body: None,
                reply_to_sender: None,
                reactions: Vec::new(),
                backend: "telegram".into(),
            }
        })
        .collect();

    Ok(messages)
}

#[tauri::command]
pub async fn tg_send_message(
    sidecar: State<'_, Arc<SidecarManager>>,
    chat_id: String,
    text: String,
) -> Result<String, String> {
    let mut client = sidecar
        .get_client("telegram")
        .await
        .ok_or("Telegram sidecar not connected")?;

    let response = client
        .send_message(SendMessageRequest {
            chat_id: Some(ChatId {
                backend: "telegram".into(),
                id: chat_id,
            }),
            content: Some(send_message_request::Content::Text(TextContent {
                body: text,
            })),
            reply_to_message_id: String::new(),
        })
        .await
        .map_err(|e| format!("gRPC error: {e}"))?;

    let msg_id = response
        .into_inner()
        .message_id
        .map(|id| id.id)
        .unwrap_or_default();

    Ok(msg_id)
}

#[tauri::command]
pub async fn get_backends(
    sidecar: State<'_, Arc<SidecarManager>>,
) -> Result<Vec<FrontendBackendInfo>, String> {
    let infos = sidecar.get_all_backend_info().await;
    Ok(infos
        .into_iter()
        .map(|i| FrontendBackendInfo {
            backend_id: i.backend_id,
            display_name: i.display_name,
            is_authenticated: i.is_authenticated,
            badge_label: i.badge_label,
            badge_color: i.badge_color,
        })
        .collect())
}

#[tauri::command]
pub async fn tg_logout(sidecar: State<'_, Arc<SidecarManager>>) -> Result<String, String> {
    tracing::info!(">>> tg_logout: checking for client...");
    let mut client = sidecar.get_client("telegram").await.ok_or_else(|| {
        tracing::error!(">>> tg_logout: NO CLIENT - sidecar not connected");
        "Telegram sidecar not connected".to_string()
    })?;

    tracing::info!(">>> tg_logout: got client, calling gRPC logout...");
    client.logout(LogoutRequest {}).await.map_err(|e| {
        tracing::error!(">>> tg_logout: gRPC FAILED: {e}");
        format!("gRPC logout error: {e}")
    })?;

    tracing::info!(">>> tg_logout: success, disconnecting from SidecarManager");
    sidecar.disconnect("telegram").await;

    Ok("Logged out".into())
}

#[tauri::command]
pub async fn tg_remove_account(sidecar: State<'_, Arc<SidecarManager>>) -> Result<String, String> {
    tracing::info!(">>> tg_remove_account: starting removal...");

    // 1. Logout via gRPC (tells TDLib to end the session)
    if let Some(mut client) = sidecar.get_client("telegram").await {
        tracing::info!(">>> tg_remove_account: sending gRPC logout...");
        let _ = client.logout(LogoutRequest {}).await;
    }

    // 2. Disconnect gRPC client
    sidecar.disconnect("telegram").await;

    // 3. KILL the sidecar process - don't just wait for graceful exit
    tracing::info!(">>> tg_remove_account: killing sidecar process...");
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/IM", "sgx-telegram.exe", "/F"])
            .output();
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = std::process::Command::new("pkill")
            .args(["-f", "sgx-telegram"])
            .output();
    }

    // 4. Wait for process to be fully dead
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // 5. NOW delete tdlib-data - canonical path + legacy fallbacks
    let canonical = tdlib_data_dir();
    let cwd = std::env::current_dir().unwrap_or_default();
    let paths = [
        canonical.clone(),
        cwd.join("tdlib-data"),
        cwd.join("src-tauri").join("tdlib-data"),
    ];

    for path in &paths {
        if path.exists() {
            tracing::info!(">>> tg_remove_account: deleting {:?}", path);
            match std::fs::remove_dir_all(path) {
                Ok(_) => tracing::info!(">>> tg_remove_account: deleted {:?}", path),
                Err(e) => tracing::warn!(">>> tg_remove_account: failed {:?}: {e}", path),
            }
        }
    }

    // 6. Verify deletion
    if canonical.exists() {
        tracing::error!(">>> tg_remove_account: tdlib-data STILL EXISTS after delete!");
    } else {
        tracing::info!(">>> tg_remove_account: tdlib-data successfully deleted");
    }

    Ok("Account removed".into())
}

// ==================== Real-time Update Streaming ====================

#[derive(Clone, Serialize)]
struct TgNewMessageEvent {
    chat_id: String,
    event_id: String,
    sender: String,
    sender_display_name: String,
    body: String,
    is_own: bool,
    timestamp: u64,
}

#[derive(Clone, Serialize)]
struct TgChatUpdatedEvent {
    chat_id: String,
    unread_count: i32,
    last_message_body: String,
    last_message_time: u64,
}

#[tauri::command]
pub async fn tg_subscribe_updates(
    app: tauri::AppHandle,
    sidecar: State<'_, Arc<SidecarManager>>,
) -> Result<(), String> {
    let mut client = sidecar
        .get_client("telegram")
        .await
        .ok_or("Telegram sidecar not connected")?;

    tracing::info!(">>> tg_subscribe_updates: connecting to stream...");

    let response = client
        .stream_updates(StreamUpdatesRequest {})
        .await
        .map_err(|e| format!("stream_updates gRPC error: {e}"))?;

    let mut stream = response.into_inner();
    tracing::info!(">>> tg_subscribe_updates: stream connected!");

    // Spawn background task to consume the stream
    tokio::spawn(async move {
        use sgx_proto::messenger::v1::update::Update as U;

        while let Ok(Some(proto_update)) = stream.message().await {
            let Some(u) = proto_update.update else {
                continue;
            };
            match u {
                U::NewMessage(nm) => {
                    if let Some(msg) = nm.message {
                        let chat_id = msg
                            .message_id
                            .as_ref()
                            .map(|id| id.chat_id.clone())
                            .unwrap_or_default();
                        let event_id = msg
                            .message_id
                            .as_ref()
                            .map(|id| id.id.clone())
                            .unwrap_or_default();
                        let body = match &msg.content {
                            Some(unified_message::Content::Text(t)) => t.body.clone(),
                            Some(unified_message::Content::Media(m)) => {
                                if m.caption.is_empty() {
                                    "[Media]".into()
                                } else {
                                    m.caption.clone()
                                }
                            }
                            _ => String::new(),
                        };
                        let ts = msg
                            .timestamp
                            .as_ref()
                            .map(|t| (t.seconds as u64) * 1000)
                            .unwrap_or(0);

                        let event = TgNewMessageEvent {
                            chat_id,
                            event_id,
                            sender: msg.sender_id.clone(),
                            sender_display_name: msg.sender_name.clone(),
                            body,
                            is_own: msg.is_outgoing,
                            timestamp: ts,
                        };
                        tracing::info!(
                            ">>> tg event: new-message chat={} from={}",
                            event.chat_id,
                            event.sender_display_name
                        );
                        let _ = app.emit("tg-new-message", &event);
                    }
                }
                U::ChatUpdated(cu) => {
                    if let Some(chat) = cu.chat {
                        let chat_id = chat
                            .chat_id
                            .as_ref()
                            .map(|c| c.id.clone())
                            .unwrap_or_default();
                        let last_body = chat
                            .last_message
                            .as_ref()
                            .and_then(|m| m.content.as_ref())
                            .map(|c| match c {
                                unified_message::Content::Text(t) => t.body.clone(),
                                _ => "[Media]".into(),
                            })
                            .unwrap_or_default();
                        let last_ts = chat
                            .last_activity
                            .as_ref()
                            .map(|t| (t.seconds as u64) * 1000)
                            .unwrap_or(0);

                        let event = TgChatUpdatedEvent {
                            chat_id,
                            unread_count: chat.unread_count,
                            last_message_body: last_body,
                            last_message_time: last_ts,
                        };
                        let _ = app.emit("tg-chat-updated", &event);
                    }
                }
                U::MessageEdited(ed) => {
                    // Could emit tg-message-edited event later
                    tracing::debug!(">>> tg event: message-edited {:?}", ed.message_id);
                }
                U::MessageDeleted(del) => {
                    tracing::debug!(">>> tg event: message-deleted {:?}", del.message_id);
                }
                _ => {}
            }
        }
        tracing::info!(">>> tg_subscribe_updates: stream ended");
    });

    Ok(())
}
