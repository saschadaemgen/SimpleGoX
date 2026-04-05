//! SimpleGoX Matrix client wrapper.
//!
//! Provides a high-level interface around [`matrix_sdk::Client`]
//! with SimpleGoX-specific defaults and configuration.

use matrix_sdk::{
    authentication::matrix::MatrixSession,
    config::SyncSettings,
    encryption::CrossSigningStatus,
    ruma::{
        api::client::{receipt::create_receipt, uiaa},
        events::{
            receipt::ReceiptThread,
            room::{
                member::StrippedRoomMemberEvent,
                message::{MessageType, OriginalSyncRoomMessageEvent, RoomMessageEventContent},
            },
        },
        OwnedEventId,
    },
    store::RoomLoadSettings,
    Client, Room, SessionMeta, SessionTokens,
};
use tracing::info;

use crate::{SgxConfig, SgxError};

/// A plain-data representation of an incoming text message.
/// Contains no matrix-sdk types so it can safely cross crate boundaries
/// (e.g. Tauri IPC) without leaking the SDK.
#[derive(Debug, Clone, serde::Serialize)]
pub struct IncomingMessage {
    pub event_id: String,
    pub room_id: String,
    pub room_name: String,
    pub sender: String,
    pub body: String,
    /// Milliseconds since Unix epoch.
    pub timestamp: u64,
}

/// Payload for typing indicator events.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TypingPayload {
    pub room_id: String,
    pub user_ids: Vec<String>,
}

/// Plain-data description of a joined room.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RoomSummary {
    pub room_id: String,
    pub name: String,
    pub is_encrypted: bool,
    pub unread_count: u64,
    pub notification_count: u64,
}

/// High-level SimpleGoX Matrix client.
///
/// Wraps [`matrix_sdk::Client`] with opinionated defaults:
/// - SQLite-backed persistent storage
/// - E2E encryption enabled by default
/// - Sliding Sync ready
pub struct SgxClient {
    inner: Client,
    config: SgxConfig,
}

impl SgxClient {
    /// Create a new client from the given configuration.
    ///
    /// This sets up the SQLite store and configures encryption.
    pub async fn new(config: SgxConfig) -> Result<Self, SgxError> {
        config.ensure_data_dir()?;

        let builder = Client::builder()
            .homeserver_url(&config.homeserver_url)
            .sqlite_store(&config.data_dir, None);

        let inner = builder.build().await?;

        info!(
            homeserver = %config.homeserver_url,
            data_dir = %config.data_dir.display(),
            "SimpleGoX client initialized"
        );

        Ok(Self { inner, config })
    }

    /// Log in with username and password.
    ///
    /// After a successful login the caller must persist the session fields
    /// written to `config` so that [`restore_session`](Self::restore_session)
    /// can reload them later.
    pub async fn login(&self, password: &str) -> Result<(), SgxError> {
        self.inner
            .matrix_auth()
            .login_username(&self.config.username, password)
            .initial_device_display_name("SimpleGoX Terminal")
            .await?;

        info!(user = %self.config.username, "Login successful");
        Ok(())
    }

    /// Extract the current session credentials from the inner SDK client.
    ///
    /// Must be called after a successful [`login`](Self::login). Returns the
    /// fields that the caller should write into [`SgxConfig`] before saving.
    pub fn session_credentials(
        &self,
    ) -> Result<(String, String, String, Option<String>), SgxError> {
        let session = self
            .inner
            .matrix_auth()
            .session()
            .ok_or_else(|| SgxError::Auth("No session available".to_string()))?;

        Ok((
            session.meta.user_id.to_string(),
            session.meta.device_id.to_string(),
            session.tokens.access_token,
            session.tokens.refresh_token,
        ))
    }

    /// Restore a previously persisted session from the config.
    ///
    /// The SQLite store holds crypto keys and room state, but the access
    /// token and session metadata are NOT stored there. They must be provided
    /// from the persisted config via [`SgxConfig`] fields `user_id`,
    /// `device_id`, `access_token`, and optionally `refresh_token`.
    pub async fn restore_session(&self) -> Result<(), SgxError> {
        use matrix_sdk::ruma::{DeviceId, UserId};

        let user_id_str = self
            .config
            .user_id
            .as_deref()
            .ok_or_else(|| SgxError::Auth("No user_id in config".to_string()))?;
        let device_id_str = self
            .config
            .device_id
            .as_deref()
            .ok_or_else(|| SgxError::Auth("No device_id in config".to_string()))?;
        let access_token = self
            .config
            .access_token
            .as_deref()
            .ok_or_else(|| SgxError::Auth("No access_token in config".to_string()))?;

        let user_id = UserId::parse(user_id_str)
            .map_err(|e| SgxError::Auth(format!("Invalid user_id in config: {e}")))?;
        let device_id: &DeviceId = device_id_str.into();

        let matrix_session = MatrixSession {
            meta: SessionMeta {
                user_id: user_id.to_owned(),
                device_id: device_id.to_owned(),
            },
            tokens: SessionTokens {
                access_token: access_token.to_owned(),
                refresh_token: self.config.refresh_token.clone(),
            },
        };

        self.inner
            .matrix_auth()
            .restore_session(matrix_session, RoomLoadSettings::default())
            .await?;

        info!(user = %user_id_str, "Session restored");

        // TODO(season-2): Subscribe to session changes for token refresh.
        // When the homeserver issues a new access_token via refresh_token,
        // we must persist the updated tokens back to the config file.
        // Use client.subscribe_to_session_changes() and spawn a task
        // that writes new tokens to disk whenever they change.

        Ok(())
    }

    /// Bootstrap cross-signing keys if not already set up.
    ///
    /// Uses `bootstrap_cross_signing_if_needed` so repeated logins are a
    /// no-op. The first attempt always fails with a UIA challenge; the
    /// password is then sent as `AuthData::Password` on the retry.
    pub async fn bootstrap_cross_signing(&self, password: &str) -> Result<(), SgxError> {
        let username = self.config.username.clone();

        if let Err(e) = self
            .inner
            .encryption()
            .bootstrap_cross_signing_if_needed(None)
            .await
        {
            if let Some(response) = e.as_uiaa_response() {
                let mut pw = uiaa::Password::new(
                    uiaa::UserIdentifier::UserIdOrLocalpart(username),
                    password.to_owned(),
                );
                pw.session = response.session.clone();

                self.inner
                    .encryption()
                    .bootstrap_cross_signing(Some(uiaa::AuthData::Password(pw)))
                    .await?;
            } else {
                return Err(e.into());
            }
        }

        info!("Cross-signing keys created and uploaded");
        Ok(())
    }

    /// Enable recovery (key backup + recovery key) if not already active.
    ///
    /// Returns the recovery key string that the user must save.
    pub async fn enable_recovery(&self) -> Result<String, SgxError> {
        let recovery = self.inner.encryption().recovery();

        let recovery_key = recovery
            .enable()
            .await
            .map_err(|e| SgxError::Crypto(format!("Failed to enable recovery: {e}")))?;

        info!("Recovery enabled and key backup created");
        Ok(recovery_key)
    }

    /// Query the local cross-signing key status.
    pub async fn cross_signing_status(&self) -> Option<CrossSigningStatus> {
        self.inner.encryption().cross_signing_status().await
    }

    /// Run a single sync against the homeserver.
    ///
    /// This fetches the room list, encryption keys and pending events once
    /// and returns. Use this before sending a message so the client has the
    /// room state and crypto sessions.
    pub async fn sync_once(&self) -> Result<(), SgxError> {
        info!("Running initial sync...");
        self.inner.sync_once(SyncSettings::default()).await?;
        Ok(())
    }

    /// Look up a room by ID (`!abc:server`) or alias (`#name:server`).
    ///
    /// If the string starts with `#` the alias is resolved via the homeserver
    /// first.
    pub async fn resolve_room(&self, room_str: &str) -> Result<Room, SgxError> {
        use matrix_sdk::ruma::{RoomAliasId, RoomId};

        if room_str.starts_with('!') {
            let room_id = RoomId::parse(room_str)
                .map_err(|e| SgxError::Config(format!("Invalid room ID: {e}")))?;
            self.inner
                .get_room(&room_id)
                .ok_or_else(|| SgxError::Config(format!("Room {room_str} not found locally")))
        } else if room_str.starts_with('#') {
            let alias = RoomAliasId::parse(room_str)
                .map_err(|e| SgxError::Config(format!("Invalid room alias: {e}")))?;
            let response = self.inner.resolve_room_alias(&alias).await?;
            self.inner.get_room(&response.room_id).ok_or_else(|| {
                SgxError::Config(format!(
                    "Room {} resolved to {} but not joined",
                    room_str, response.room_id
                ))
            })
        } else {
            Err(SgxError::Config(
                "Room must start with '!' (ID) or '#' (alias)".to_string(),
            ))
        }
    }

    /// Server-side logout. Invalidates the access token.
    pub async fn logout(&self) -> Result<(), SgxError> {
        self.inner.matrix_auth().logout().await?;
        info!("Logged out from server");
        Ok(())
    }

    /// Return a summary of every joined room.
    pub async fn joined_rooms_summary(&self) -> Vec<RoomSummary> {
        let mut out = Vec::new();
        for room in self.inner.joined_rooms() {
            let name = room
                .display_name()
                .await
                .map(|n| n.to_string())
                .unwrap_or_else(|_| room.room_id().to_string());
            let is_encrypted = room.encryption_state().is_encrypted();
            let unread_count = room.num_unread_messages();
            let notification_count = room.num_unread_notifications();
            out.push(RoomSummary {
                room_id: room.room_id().to_string(),
                name,
                is_encrypted,
                unread_count,
                notification_count,
            });
        }
        out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        out
    }

    /// Send a plain text message to a room identified by its ID string.
    pub async fn send_to_room(&self, room_id_str: &str, message: &str) -> Result<(), SgxError> {
        use matrix_sdk::ruma::RoomId;
        let room_id = RoomId::parse(room_id_str)
            .map_err(|e| SgxError::Config(format!("Invalid room ID: {e}")))?;
        let room = self
            .inner
            .get_room(&room_id)
            .ok_or_else(|| SgxError::Config(format!("Room {room_id_str} not found")))?;
        let content = RoomMessageEventContent::text_plain(message);
        room.send(content).await?;
        info!(room = %room_id_str, "Message sent");
        Ok(())
    }

    /// Send a typing notice for a room.
    pub async fn send_typing(&self, room_id_str: &str, typing: bool) -> Result<(), SgxError> {
        use matrix_sdk::ruma::RoomId;
        let room_id = RoomId::parse(room_id_str)
            .map_err(|e| SgxError::Config(format!("Invalid room ID: {e}")))?;
        let room = self
            .inner
            .get_room(&room_id)
            .ok_or_else(|| SgxError::Config(format!("Room {room_id_str} not found")))?;
        room.typing_notice(typing).await?;
        Ok(())
    }

    /// Mark a message as read by sending a read receipt.
    pub async fn mark_as_read(
        &self,
        room_id_str: &str,
        event_id_str: &str,
    ) -> Result<(), SgxError> {
        use matrix_sdk::ruma::{EventId, RoomId};
        let room_id = RoomId::parse(room_id_str)
            .map_err(|e| SgxError::Config(format!("Invalid room ID: {e}")))?;
        let event_id: OwnedEventId = EventId::parse(event_id_str)
            .map_err(|e| SgxError::Config(format!("Invalid event ID: {e}")))?;
        let room = self
            .inner
            .get_room(&room_id)
            .ok_or_else(|| SgxError::Config(format!("Room {room_id_str} not found")))?;
        room.send_single_receipt(
            create_receipt::v3::ReceiptType::Read,
            ReceiptThread::Unthreaded,
            event_id,
        )
        .await?;
        Ok(())
    }

    /// Start the sync loop, forwarding messages and typing events via callbacks.
    ///
    /// Also registers auto-join. Blocks until the sync is cancelled.
    pub async fn sync_with_callbacks<F, T>(
        &self,
        on_message: F,
        on_typing: T,
    ) -> Result<(), SgxError>
    where
        F: Fn(IncomingMessage) + Send + Sync + 'static,
        T: Fn(TypingPayload) + Send + Sync + 'static,
    {
        use matrix_sdk::ruma::events::typing::SyncTypingEvent;

        info!("Starting sync loop (with callbacks)...");

        // Auto-join rooms when invited
        self.inner.add_event_handler(
            |ev: StrippedRoomMemberEvent, client: Client, room: Room| async move {
                if ev.state_key == client.user_id().unwrap().as_str() {
                    if let Err(e) = room.join().await {
                        tracing::warn!("Failed to auto-join room: {e}");
                    } else {
                        tracing::info!("Auto-joined room {}", room.room_id());
                    }
                }
            },
        );

        // Message handler
        let msg_cb = std::sync::Arc::new(on_message);
        self.inner
            .add_event_handler(move |ev: OriginalSyncRoomMessageEvent, room: Room| {
                let cb = msg_cb.clone();
                async move {
                    if let MessageType::Text(text) = &ev.content.msgtype {
                        let room_name = room
                            .display_name()
                            .await
                            .map(|n| n.to_string())
                            .unwrap_or_else(|_| "unknown".to_string());
                        let ts: u64 = ev.origin_server_ts.0.into();
                        cb(IncomingMessage {
                            event_id: ev.event_id.to_string(),
                            room_id: room.room_id().to_string(),
                            room_name,
                            sender: ev.sender.to_string(),
                            body: text.body.clone(),
                            timestamp: ts,
                        });
                    }
                }
            });

        // Typing handler
        let typing_cb = std::sync::Arc::new(on_typing);
        self.inner
            .add_event_handler(move |ev: SyncTypingEvent, room: Room| {
                let cb = typing_cb.clone();
                async move {
                    cb(TypingPayload {
                        room_id: room.room_id().to_string(),
                        user_ids: ev.content.user_ids.iter().map(|u| u.to_string()).collect(),
                    });
                }
            });

        self.inner.sync(SyncSettings::default()).await?;
        Ok(())
    }

    /// Start the sync loop, forwarding every incoming text message to `on_message`.
    ///
    /// Also registers auto-join. Blocks until the sync is cancelled.
    pub async fn sync_with_callback<F>(&self, on_message: F) -> Result<(), SgxError>
    where
        F: Fn(IncomingMessage) + Send + Sync + 'static,
    {
        self.sync_with_callbacks(on_message, |_| {}).await
    }

    /// Start the sync loop. This blocks until cancelled.
    pub async fn sync(&self) -> Result<(), SgxError> {
        info!("Starting sync loop...");

        // Auto-join rooms when invited
        self.inner.add_event_handler(
            |ev: StrippedRoomMemberEvent, client: Client, room: Room| async move {
                if ev.state_key == client.user_id().unwrap().as_str() {
                    if let Err(e) = room.join().await {
                        tracing::warn!("Failed to auto-join room: {e}");
                    } else {
                        tracing::info!("Auto-joined room {}", room.room_id());
                    }
                }
            },
        );

        // Add a handler for incoming text messages
        self.inner
            .add_event_handler(|ev: OriginalSyncRoomMessageEvent, room: Room| async move {
                if let MessageType::Text(text) = &ev.content.msgtype {
                    let sender = ev.sender;
                    let room_name = room
                        .display_name()
                        .await
                        .map(|n| n.to_string())
                        .unwrap_or_else(|_| "unknown".to_string());

                    info!(
                        room = %room_name,
                        sender = %sender,
                        "{}",
                        text.body
                    );
                }
            });

        self.inner.sync(SyncSettings::default()).await?;
        Ok(())
    }

    /// Send a text message to a room.
    pub async fn send_message(&self, room: &Room, message: &str) -> Result<(), SgxError> {
        let content = RoomMessageEventContent::text_plain(message);
        room.send(content).await?;
        info!(room_id = %room.room_id(), "Message sent");
        Ok(())
    }

    /// Get a reference to the underlying Matrix SDK client.
    pub fn inner(&self) -> &Client {
        &self.inner
    }

    /// Create a cheap clone of this client that shares the same SDK state.
    ///
    /// `matrix_sdk::Client` is `Arc`-based, so cloning is inexpensive.
    /// Use this to hand a copy to a long-running sync task while keeping
    /// the original available for short-lived commands behind a mutex.
    pub fn clone_inner(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            config: self.config.clone(),
        }
    }

    /// Get the current configuration.
    pub fn config(&self) -> &SgxConfig {
        &self.config
    }
}
