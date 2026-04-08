//! SimpleGoX Matrix client wrapper.
//!
//! Provides a high-level interface around [`matrix_sdk::Client`]
//! with SimpleGoX-specific defaults and configuration.

use matrix_sdk::{
    authentication::matrix::MatrixSession,
    config::SyncSettings,
    encryption::CrossSigningStatus,
    ruma::{
        api::client::{receipt::create_receipt, room::create_room, uiaa},
        assign,
        events::{
            receipt::ReceiptThread,
            room::{
                history_visibility::{HistoryVisibility, RoomHistoryVisibilityEventContent},
                join_rules::{JoinRule, RoomJoinRulesEventContent},
            },
            room::{
                member::StrippedRoomMemberEvent,
                message::{MessageType, OriginalSyncRoomMessageEvent, RoomMessageEventContent},
            },
            tag::{TagInfo, TagName},
            AnySyncTimelineEvent, StateEventType,
        },
        serde::Raw,
        EventId, OwnedEventId, RoomId, RoomOrAliasId, UserId,
    },
    store::RoomLoadSettings,
    Client, Room, RoomMemberships, SessionMeta, SessionTokens,
};
use tracing::info;

use crate::{SgxConfig, SgxError};

// ---------------------------------------------------------------------------
// Payload types (no matrix-sdk types, safe for IPC)
// ---------------------------------------------------------------------------

/// A plain-data representation of an incoming text message.
#[derive(Debug, Clone, serde::Serialize)]
pub struct IncomingMessage {
    pub event_id: String,
    pub room_id: String,
    pub room_name: String,
    pub sender: String,
    pub sender_display_name: Option<String>,
    pub sender_avatar_url: Option<String>,
    pub body: String,
    /// Milliseconds since Unix epoch.
    pub timestamp: u64,
    /// Whether this message was sent by the current user.
    pub is_own: bool,
    /// Reply info (if this message is a reply).
    pub reply_to_event_id: Option<String>,
    /// Whether this message has been edited.
    pub is_edited: bool,
    /// Whether this message has been redacted.
    pub is_redacted: bool,
}

/// A reaction event received from the sync.
#[derive(Debug, Clone, serde::Serialize)]
pub struct IncomingReaction {
    pub event_id: String,
    pub room_id: String,
    pub sender: String,
    pub target_event_id: String,
    pub key: String,
    pub is_own: bool,
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
    pub avatar_url: Option<String>,
    pub is_encrypted: bool,
    pub is_direct: bool,
    pub is_favourite: bool,
    pub is_muted: bool,
    pub unread_count: u64,
    pub notification_count: u64,
}

/// User profile data.
#[derive(Debug, Clone, serde::Serialize)]
pub struct UserProfile {
    pub user_id: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
}

/// Detailed room information for the info panel.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RoomDetail {
    pub room_id: String,
    pub name: Option<String>,
    pub topic: Option<String>,
    pub is_encrypted: bool,
    pub is_direct: bool,
    pub member_count: u64,
}

/// A room member for the member list.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RoomMemberInfo {
    pub user_id: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub power_level: i64,
    pub membership: String,
}

/// Room settings for the settings panel.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RoomSettings {
    pub room_id: String,
    pub name: Option<String>,
    pub topic: Option<String>,
    pub is_encrypted: bool,
    pub join_rule: String,
    pub history_visibility: String,
    pub room_version: String,
    pub canonical_alias: Option<String>,
    pub member_count: u64,
    pub is_direct: bool,
}

/// An IoT device registered in a Matrix room via state event.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct IotDevice {
    pub device_id: String,
    pub device_type: String,
    pub label: String,
    pub icon: String,
    pub online: bool,
}

/// Real-time status update from an IoT device.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct IotStatusPayload {
    pub room_id: String,
    pub device_id: String,
    pub state: Option<bool>,
    pub value: Option<f64>,
    pub unit: Option<String>,
    pub timestamp: u64,
}

// ---------------------------------------------------------------------------
// SgxClient
// ---------------------------------------------------------------------------

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
        // Use client.subscribe_to_session_changes() and spawn a task
        // that writes new tokens to disk whenever they change.

        Ok(())
    }

    /// Bootstrap cross-signing keys if not already set up.
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
    pub async fn sync_once(&self) -> Result<(), SgxError> {
        info!("Running initial sync...");
        self.inner.sync_once(SyncSettings::default()).await?;
        Ok(())
    }

    /// Look up a room by ID (`!abc:server`) or alias (`#name:server`).
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
        let own_user_id = self.inner.user_id();
        let mut out = Vec::new();
        for room in self.inner.joined_rooms() {
            let is_direct = room.direct_targets_length() > 0;

            let default_name = room
                .display_name()
                .await
                .map(|n| n.to_string())
                .unwrap_or_else(|_| room.room_id().to_string());
            let default_avatar = room.avatar_url().map(|u| u.to_string());

            // For DMs: use the partner's display name and avatar
            let (name, avatar_url) = if is_direct {
                if let Some(uid) = own_user_id {
                    match Self::dm_partner_info(&room, uid).await {
                        Some((dn, av)) => (dn.unwrap_or(default_name), av.or(default_avatar)),
                        None => (default_name, default_avatar),
                    }
                } else {
                    (default_name, default_avatar)
                }
            } else {
                (default_name, default_avatar)
            };

            out.push(RoomSummary {
                room_id: room.room_id().to_string(),
                name,
                avatar_url,
                is_encrypted: room.encryption_state().is_encrypted(),
                is_direct,
                is_favourite: room.is_favourite(),
                is_muted: room.is_low_priority(),
                unread_count: room.num_unread_messages(),
                notification_count: room.num_unread_notifications(),
            });
        }
        out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        out
    }

    /// Get the DM partner's display name and avatar URL.
    async fn dm_partner_info(
        room: &Room,
        own_user_id: &matrix_sdk::ruma::UserId,
    ) -> Option<(Option<String>, Option<String>)> {
        let members = room.members(RoomMemberships::ACTIVE).await.ok()?;
        let partner = members.iter().find(|m| m.user_id() != own_user_id)?;
        Some((
            partner.display_name().map(|n| n.to_string()),
            partner.avatar_url().map(|u| u.to_string()),
        ))
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

    // -----------------------------------------------------------------------
    // IoT methods
    // -----------------------------------------------------------------------

    /// Send an IoT command to a device in a room.
    pub async fn send_iot_command(
        &self,
        room_id_str: &str,
        device_id: &str,
        action: &str,
        value: serde_json::Value,
    ) -> Result<(), SgxError> {
        use matrix_sdk::ruma::RoomId;
        let room_id = RoomId::parse(room_id_str)
            .map_err(|e| SgxError::Config(format!("Invalid room ID: {e}")))?;
        let room = self
            .inner
            .get_room(&room_id)
            .ok_or_else(|| SgxError::Config(format!("Room {room_id_str} not found")))?;

        let content = serde_json::json!({
            "device_id": device_id,
            "action": action,
            "value": value,
        });

        room.send_raw("dev.simplego.iot.command", content)
            .await
            .map_err(SgxError::Matrix)?;

        info!(room = %room_id_str, device = %device_id, "IoT command sent");
        Ok(())
    }

    /// Get all IoT devices registered in a room from state events.
    pub async fn get_iot_devices(&self, room_id_str: &str) -> Result<Vec<IotDevice>, SgxError> {
        use matrix_sdk::ruma::RoomId;
        let room_id = RoomId::parse(room_id_str)
            .map_err(|e| SgxError::Config(format!("Invalid room ID: {e}")))?;
        let room = self
            .inner
            .get_room(&room_id)
            .ok_or_else(|| SgxError::Config(format!("Room {room_id_str} not found")))?;

        let raw_events = room
            .get_state_events(StateEventType::from("dev.simplego.iot.device"))
            .await
            .map_err(|e| SgxError::Storage(format!("State fetch failed: {e}")))?;

        let mut devices = Vec::new();
        for raw_event in raw_events {
            // RawAnySyncOrStrippedState is Serialize, so round-trip to Value
            let Ok(json) = serde_json::to_value(&raw_event) else {
                continue;
            };
            if json.is_object() {
                let state_key = json["state_key"].as_str().unwrap_or_default();
                let content = &json["content"];
                if content.is_null() {
                    continue;
                }
                devices.push(IotDevice {
                    device_id: state_key.to_string(),
                    device_type: content["device_type"]
                        .as_str()
                        .unwrap_or("switch")
                        .to_string(),
                    label: content["label"].as_str().unwrap_or(state_key).to_string(),
                    icon: content["icon"].as_str().unwrap_or("device").to_string(),
                    online: content["online"].as_bool().unwrap_or(false),
                });
            }
        }
        Ok(devices)
    }

    // -----------------------------------------------------------------------
    // Room management
    // -----------------------------------------------------------------------

    /// Helper: look up a joined room by ID string.
    fn get_room_by_id(&self, room_id_str: &str) -> Result<Room, SgxError> {
        let room_id = RoomId::parse(room_id_str)
            .map_err(|e| SgxError::InvalidInput(format!("Invalid room ID: {e}")))?;
        self.inner
            .get_room(&room_id)
            .ok_or_else(|| SgxError::RoomNotFound(room_id_str.to_string()))
    }

    /// Helper: parse a user ID string.
    fn parse_user_id(user_id_str: &str) -> Result<matrix_sdk::ruma::OwnedUserId, SgxError> {
        UserId::parse(user_id_str)
            .map_err(|e| SgxError::InvalidInput(format!("Invalid user ID: {e}")))
    }

    /// Create a new Matrix room.
    pub async fn create_room(
        &self,
        name: &str,
        is_encrypted: bool,
        is_public: bool,
        topic: Option<&str>,
        invite_user_ids: Option<Vec<String>>,
    ) -> Result<String, SgxError> {
        let preset = if is_public {
            Some(create_room::v3::RoomPreset::PublicChat)
        } else if is_encrypted {
            Some(create_room::v3::RoomPreset::TrustedPrivateChat)
        } else {
            Some(create_room::v3::RoomPreset::PrivateChat)
        };

        let invite: Vec<matrix_sdk::ruma::OwnedUserId> = invite_user_ids
            .unwrap_or_default()
            .iter()
            .filter_map(|id| UserId::parse(id).ok())
            .collect();

        let mut request = assign!(create_room::v3::Request::new(), {
            name: Some(name.to_string()),
            preset,
            invite,
        });

        if let Some(t) = topic {
            request.topic = Some(t.to_string());
        }

        let room = self.inner.create_room(request).await?;
        let room_id = room.room_id().to_string();
        info!(room = %room_id, "Room created");
        Ok(room_id)
    }

    /// Create a direct message room with a user.
    pub async fn create_dm(&self, user_id_str: &str) -> Result<String, SgxError> {
        let target = Self::parse_user_id(user_id_str)?;

        let request = assign!(create_room::v3::Request::new(), {
            preset: Some(create_room::v3::RoomPreset::TrustedPrivateChat),
            is_direct: true,
            invite: vec![target],
        });

        let room = self.inner.create_room(request).await?;
        let room_id = room.room_id().to_string();
        info!(room = %room_id, "DM created");
        Ok(room_id)
    }

    /// Join a room by ID, alias, or matrix.to link.
    pub async fn join_room(&self, room_id_or_alias: &str) -> Result<String, SgxError> {
        let cleaned = room_id_or_alias.trim().replace("https://matrix.to/#/", "");

        let parsed = <&RoomOrAliasId>::try_from(cleaned.as_str())
            .map_err(|e| SgxError::InvalidInput(format!("Invalid room ID/alias: {e}")))?;

        let room = self.inner.join_room_by_id_or_alias(parsed, &[]).await?;

        let room_id = room.room_id().to_string();
        info!(room = %room_id, "Joined room");
        Ok(room_id)
    }

    /// Leave a room.
    pub async fn leave_room(&self, room_id_str: &str) -> Result<(), SgxError> {
        self.get_room_by_id(room_id_str)?.leave().await?;
        info!(room = %room_id_str, "Left room");
        Ok(())
    }

    /// Invite a user to a room.
    pub async fn invite_user(&self, room_id_str: &str, user_id_str: &str) -> Result<(), SgxError> {
        let user_id = Self::parse_user_id(user_id_str)?;
        self.get_room_by_id(room_id_str)?
            .invite_user_by_id(&user_id)
            .await?;
        info!(room = %room_id_str, user = %user_id_str, "User invited");
        Ok(())
    }

    /// Kick a user from a room.
    pub async fn kick_user(
        &self,
        room_id_str: &str,
        user_id_str: &str,
        reason: Option<&str>,
    ) -> Result<(), SgxError> {
        let user_id = Self::parse_user_id(user_id_str)?;
        self.get_room_by_id(room_id_str)?
            .kick_user(&user_id, reason)
            .await?;
        info!(room = %room_id_str, user = %user_id_str, "User kicked");
        Ok(())
    }

    /// Ban a user from a room.
    pub async fn ban_user(
        &self,
        room_id_str: &str,
        user_id_str: &str,
        reason: Option<&str>,
    ) -> Result<(), SgxError> {
        let user_id = Self::parse_user_id(user_id_str)?;
        self.get_room_by_id(room_id_str)?
            .ban_user(&user_id, reason)
            .await?;
        info!(room = %room_id_str, user = %user_id_str, "User banned");
        Ok(())
    }

    /// Unban a user from a room.
    pub async fn unban_user(&self, room_id_str: &str, user_id_str: &str) -> Result<(), SgxError> {
        let user_id = Self::parse_user_id(user_id_str)?;
        self.get_room_by_id(room_id_str)?
            .unban_user(&user_id, None)
            .await?;
        info!(room = %room_id_str, user = %user_id_str, "User unbanned");
        Ok(())
    }

    /// Get the member list of a room.
    pub async fn get_room_members(
        &self,
        room_id_str: &str,
    ) -> Result<Vec<RoomMemberInfo>, SgxError> {
        let room = self.get_room_by_id(room_id_str)?;
        let members = room.members(RoomMemberships::ACTIVE).await?;
        Ok(members
            .iter()
            .map(|m| RoomMemberInfo {
                user_id: m.user_id().to_string(),
                display_name: m.display_name().map(|n| n.to_string()),
                avatar_url: m.avatar_url().map(|u| u.to_string()),
                power_level: match m.power_level() {
                    matrix_sdk::ruma::events::room::power_levels::UserPowerLevel::Int(v) => {
                        i64::from(v)
                    }
                    _ => i64::MAX,
                },
                membership: format!("{:?}", m.membership()),
            })
            .collect())
    }

    /// Get detailed room information.
    pub async fn get_room_info(&self, room_id_str: &str) -> Result<RoomDetail, SgxError> {
        let room = self.get_room_by_id(room_id_str)?;
        Ok(RoomDetail {
            room_id: room_id_str.to_string(),
            name: room.display_name().await.ok().map(|n| n.to_string()),
            topic: room.topic().map(|t| t.to_string()),
            is_encrypted: room.encryption_state().is_encrypted(),
            is_direct: room.direct_targets_length() > 0,
            member_count: room.joined_members_count(),
        })
    }

    /// Set the room name.
    pub async fn set_room_name(&self, room_id_str: &str, name: &str) -> Result<(), SgxError> {
        self.get_room_by_id(room_id_str)?
            .set_name(name.to_string())
            .await?;
        Ok(())
    }

    /// Set the room topic.
    pub async fn set_room_topic(&self, room_id_str: &str, topic: &str) -> Result<(), SgxError> {
        self.get_room_by_id(room_id_str)?
            .set_room_topic(topic)
            .await?;
        Ok(())
    }

    /// Set a tag on a room (e.g. m.favourite, m.lowpriority).
    pub async fn set_room_tag(
        &self,
        room_id_str: &str,
        tag: &str,
        order: Option<f64>,
    ) -> Result<(), SgxError> {
        let room = self.get_room_by_id(room_id_str)?;
        let tag_name = match tag {
            "m.favourite" => TagName::Favorite,
            "m.lowpriority" => TagName::LowPriority,
            other => TagName::from(other),
        };
        let mut tag_info = TagInfo::new();
        tag_info.order = order;
        room.set_tag(tag_name, tag_info).await?;
        Ok(())
    }

    /// Remove a tag from a room.
    pub async fn remove_room_tag(&self, room_id_str: &str, tag: &str) -> Result<(), SgxError> {
        let room = self.get_room_by_id(room_id_str)?;
        let tag_name = match tag {
            "m.favourite" => TagName::Favorite,
            "m.lowpriority" => TagName::LowPriority,
            other => TagName::from(other),
        };
        room.remove_tag(tag_name).await?;
        Ok(())
    }

    /// Redact (delete) a message.
    pub async fn redact_event(
        &self,
        room_id_str: &str,
        event_id_str: &str,
        reason: Option<&str>,
    ) -> Result<(), SgxError> {
        let room = self.get_room_by_id(room_id_str)?;
        let event_id = EventId::parse(event_id_str)
            .map_err(|e| SgxError::InvalidInput(format!("Invalid event ID: {e}")))?;
        room.redact(&event_id, reason, None).await?;
        info!(room = %room_id_str, event = %event_id_str, "Event redacted");
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Room settings
    // -----------------------------------------------------------------------

    /// Get room settings for the settings panel.
    pub async fn get_room_settings(&self, room_id_str: &str) -> Result<RoomSettings, SgxError> {
        let room = self.get_room_by_id(room_id_str)?;

        let join_rule = room
            .join_rule()
            .map(|r| match r {
                JoinRule::Public => "public".to_string(),
                JoinRule::Invite => "invite".to_string(),
                _ => "invite".to_string(),
            })
            .unwrap_or_else(|| "invite".to_string());

        let history_visibility = room.history_visibility_or_default().to_string();

        let room_version = room
            .version()
            .map(|v| v.to_string())
            .unwrap_or_else(|| "unknown".to_string());

        Ok(RoomSettings {
            room_id: room_id_str.to_string(),
            name: room.display_name().await.ok().map(|n| n.to_string()),
            topic: room.topic().map(|t| t.to_string()),
            is_encrypted: room.encryption_state().is_encrypted(),
            join_rule,
            history_visibility,
            room_version,
            canonical_alias: room.canonical_alias().map(|a| a.to_string()),
            member_count: room.joined_members_count(),
            is_direct: room.direct_targets_length() > 0,
        })
    }

    /// Set the join rule for a room.
    pub async fn set_join_rule(&self, room_id_str: &str, join_rule: &str) -> Result<(), SgxError> {
        let room = self.get_room_by_id(room_id_str)?;
        let rule = match join_rule {
            "public" => JoinRule::Public,
            "invite" => JoinRule::Invite,
            other => {
                return Err(SgxError::InvalidInput(format!(
                    "Unknown join rule: {other}"
                )))
            }
        };
        let content = RoomJoinRulesEventContent::new(rule);
        room.send_state_event(content).await?;
        Ok(())
    }

    /// Set the history visibility for a room.
    pub async fn set_history_visibility(
        &self,
        room_id_str: &str,
        visibility: &str,
    ) -> Result<(), SgxError> {
        let room = self.get_room_by_id(room_id_str)?;
        let vis = match visibility {
            "shared" => HistoryVisibility::Shared,
            "invited" => HistoryVisibility::Invited,
            "joined" => HistoryVisibility::Joined,
            "world_readable" => HistoryVisibility::WorldReadable,
            other => {
                return Err(SgxError::InvalidInput(format!(
                    "Unknown visibility: {other}"
                )))
            }
        };
        let content = RoomHistoryVisibilityEventContent::new(vis);
        room.send_state_event(content).await?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Avatar / Profile
    // -----------------------------------------------------------------------

    /// Convert an mxc:// URI to a thumbnail HTTP URL.
    pub fn resolve_mxc_url(
        &self,
        mxc_uri: &str,
        width: Option<u32>,
        height: Option<u32>,
    ) -> Result<String, SgxError> {
        let mxc = mxc_uri
            .strip_prefix("mxc://")
            .ok_or_else(|| SgxError::InvalidInput(format!("Not an mxc URI: {mxc_uri}")))?;
        let (server, media_id) = mxc
            .split_once('/')
            .ok_or_else(|| SgxError::InvalidInput(format!("Invalid mxc URI: {mxc_uri}")))?;
        let hs = self.config.homeserver_url.trim_end_matches('/');
        if let (Some(w), Some(h)) = (width, height) {
            Ok(format!(
                "{hs}/_matrix/media/v3/thumbnail/{server}/{media_id}?width={w}&height={h}&method=crop"
            ))
        } else {
            Ok(format!(
                "{hs}/_matrix/media/v3/download/{server}/{media_id}"
            ))
        }
    }

    /// Load an avatar thumbnail via the SDK and return it as a base64 data URL.
    /// This bypasses CORS/auth issues since the SDK handles authentication.
    pub async fn get_avatar_base64(
        &self,
        mxc_uri: &str,
        width: u32,
        height: u32,
    ) -> Result<String, SgxError> {
        use matrix_sdk::{
            media::{MediaFormat, MediaRequestParameters, MediaThumbnailSettings},
            ruma::{
                api::client::media::get_content_thumbnail::v3::Method, events::room::MediaSource,
                OwnedMxcUri, UInt,
            },
        };

        let mxc: OwnedMxcUri = mxc_uri.into();
        let settings = MediaThumbnailSettings::with_method(
            Method::Crop,
            UInt::new(u64::from(width)).unwrap_or(UInt::MAX),
            UInt::new(u64::from(height)).unwrap_or(UInt::MAX),
        );

        let request = MediaRequestParameters {
            source: MediaSource::Plain(mxc),
            format: MediaFormat::Thumbnail(settings),
        };

        let data = self.inner.media().get_media_content(&request, true).await?;

        // Detect MIME from magic bytes
        let mime = if data.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
            "image/png"
        } else if data.starts_with(&[0xFF, 0xD8]) {
            "image/jpeg"
        } else if data.starts_with(b"GIF") {
            "image/gif"
        } else {
            "image/png"
        };

        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD.encode(&data);

        Ok(format!("data:{mime};base64,{b64}"))
    }

    /// Get own profile (display name + avatar URL).
    pub async fn get_own_profile(&self) -> Result<UserProfile, SgxError> {
        let account = self.inner.account();
        let display_name = account.get_display_name().await?;
        let avatar_url = account.get_avatar_url().await?;
        let user_id = self
            .inner
            .user_id()
            .map(|u| u.to_string())
            .unwrap_or_default();
        Ok(UserProfile {
            user_id,
            display_name,
            avatar_url: avatar_url.map(|u| u.to_string()),
        })
    }

    /// Set own display name.
    pub async fn set_display_name(&self, name: &str) -> Result<(), SgxError> {
        self.inner.account().set_display_name(Some(name)).await?;
        Ok(())
    }

    /// Upload and set own avatar. Returns the mxc:// URI.
    pub async fn upload_avatar(
        &self,
        data: Vec<u8>,
        content_type: &str,
    ) -> Result<String, SgxError> {
        let mime: mime::Mime = content_type
            .parse()
            .map_err(|e| SgxError::InvalidInput(format!("Invalid MIME type: {e}")))?;
        let mxc = self.inner.account().upload_avatar(&mime, data).await?;
        Ok(mxc.to_string())
    }

    /// Remove own avatar.
    pub async fn remove_own_avatar(&self) -> Result<(), SgxError> {
        self.inner.account().set_avatar_url(None).await?;
        Ok(())
    }

    /// Upload and set a room avatar.
    pub async fn set_room_avatar(
        &self,
        room_id_str: &str,
        data: Vec<u8>,
        content_type: &str,
    ) -> Result<(), SgxError> {
        let room = self.get_room_by_id(room_id_str)?;
        let mime: mime::Mime = content_type
            .parse()
            .map_err(|e| SgxError::InvalidInput(format!("Invalid MIME type: {e}")))?;
        room.upload_avatar(&mime, data, None).await?;
        Ok(())
    }

    /// Remove a room avatar.
    pub async fn remove_room_avatar(&self, room_id_str: &str) -> Result<(), SgxError> {
        let room = self.get_room_by_id(room_id_str)?;
        room.remove_avatar().await?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Reply, Reaction, Edit
    // -----------------------------------------------------------------------

    /// Send a reply to a message.
    pub async fn send_reply(
        &self,
        room_id_str: &str,
        body: &str,
        reply_to_event_id: &str,
    ) -> Result<(), SgxError> {
        use matrix_sdk::ruma::events::room::message::Relation;

        let room = self.get_room_by_id(room_id_str)?;
        let event_id = EventId::parse(reply_to_event_id)
            .map_err(|e| SgxError::InvalidInput(format!("Invalid event ID: {e}")))?;

        let mut content = RoomMessageEventContent::text_plain(body);
        content.relates_to = Some(Relation::Reply {
            in_reply_to: matrix_sdk::ruma::events::relation::InReplyTo::new(event_id.to_owned()),
        });

        room.send(content).await?;
        info!(room = %room_id_str, "Reply sent");
        Ok(())
    }

    /// Send an emoji reaction to a message.
    pub async fn send_reaction(
        &self,
        room_id_str: &str,
        event_id_str: &str,
        emoji: &str,
    ) -> Result<(), SgxError> {
        use matrix_sdk::ruma::events::{reaction::ReactionEventContent, relation::Annotation};

        let room = self.get_room_by_id(room_id_str)?;
        let event_id = EventId::parse(event_id_str)
            .map_err(|e| SgxError::InvalidInput(format!("Invalid event ID: {e}")))?;

        let content =
            ReactionEventContent::new(Annotation::new(event_id.to_owned(), emoji.to_string()));

        room.send(content).await?;
        info!(room = %room_id_str, event = %event_id_str, "Reaction sent");
        Ok(())
    }

    /// Edit a previously sent message.
    pub async fn edit_message(
        &self,
        room_id_str: &str,
        original_event_id: &str,
        new_body: &str,
    ) -> Result<(), SgxError> {
        use matrix_sdk::ruma::events::room::message::Relation;

        let room = self.get_room_by_id(room_id_str)?;
        let event_id = EventId::parse(original_event_id)
            .map_err(|e| SgxError::InvalidInput(format!("Invalid event ID: {e}")))?;

        let new_content = RoomMessageEventContent::text_plain(new_body).into();
        let mut content = RoomMessageEventContent::text_plain(format!("* {new_body}"));
        content.relates_to = Some(Relation::Replacement(
            matrix_sdk::ruma::events::relation::Replacement::new(event_id.to_owned(), new_content),
        ));

        room.send(content).await?;
        info!(room = %room_id_str, event = %original_event_id, "Message edited");
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Message history
    // -----------------------------------------------------------------------

    /// Load the last N messages from a room.
    pub async fn get_room_messages(
        &self,
        room_id_str: &str,
        limit: u32,
    ) -> Result<Vec<IncomingMessage>, SgxError> {
        use matrix_sdk::{room::MessagesOptions, ruma::UInt};

        let room = self.get_room_by_id(room_id_str)?;
        let own_uid = self.inner.user_id().map(|u| u.to_owned());

        let mut opts = MessagesOptions::backward();
        opts.limit = UInt::new(u64::from(limit)).unwrap_or(UInt::MAX);

        let response = room.messages(opts).await?;

        let mut msgs = Vec::new();
        for timeline_ev in &response.chunk {
            let raw = timeline_ev.kind.raw();
            let Ok(any) = raw.deserialize() else {
                continue;
            };

            // Extract text from message events
            let (sender, event_id, ts, body) = match any {
                matrix_sdk::ruma::events::AnySyncTimelineEvent::MessageLike(ml) => {
                    use matrix_sdk::ruma::events::AnySyncMessageLikeEvent;
                    match ml {
                        AnySyncMessageLikeEvent::RoomMessage(
                            matrix_sdk::ruma::events::SyncMessageLikeEvent::Original(orig),
                        ) => {
                            // Skip replacement (edit) events
                            if let Some(ref rel) = orig.content.relates_to {
                                use matrix_sdk::ruma::events::room::message::Relation;
                                if matches!(rel, Relation::Replacement(_)) {
                                    continue;
                                }
                            }
                            let b = match &orig.content.msgtype {
                                MessageType::Text(t) => t.body.clone(),
                                MessageType::Notice(n) => n.body.clone(),
                                _ => continue,
                            };
                            let t: u64 = orig.origin_server_ts.0.into();
                            (orig.sender, orig.event_id, t, b)
                        }
                        _ => continue,
                    }
                }
                _ => continue,
            };

            let is_own = own_uid.as_ref().is_some_and(|u| *u == sender);
            let member = room.get_member_no_sync(&sender).await.ok().flatten();

            msgs.push(IncomingMessage {
                event_id: event_id.to_string(),
                room_id: room_id_str.to_string(),
                room_name: String::new(),
                sender: sender.to_string(),
                sender_display_name: member
                    .as_ref()
                    .and_then(|m| m.display_name().map(|n| n.to_string())),
                sender_avatar_url: member
                    .as_ref()
                    .and_then(|m| m.avatar_url().map(|u| u.to_string())),
                body,
                timestamp: ts,
                is_own,
                reply_to_event_id: None,
                is_edited: false,
                is_redacted: false,
            });
        }

        // API returns backward - reverse to chronological
        msgs.reverse();
        Ok(msgs)
    }

    // -----------------------------------------------------------------------
    // Sync loops
    // -----------------------------------------------------------------------

    /// Start the sync loop with message, typing, and IoT status callbacks.
    ///
    /// Also registers auto-join. Blocks until cancelled.
    pub async fn sync_with_all_callbacks<F, T, I, R>(
        &self,
        on_message: F,
        on_typing: T,
        on_iot_status: I,
        on_reaction: R,
    ) -> Result<(), SgxError>
    where
        F: Fn(IncomingMessage) + Send + Sync + 'static,
        T: Fn(TypingPayload) + Send + Sync + 'static,
        I: Fn(IotStatusPayload) + Send + Sync + 'static,
        R: Fn(IncomingReaction) + Send + Sync + 'static,
    {
        use matrix_sdk::ruma::events::{
            reaction::OriginalSyncReactionEvent, typing::SyncTypingEvent,
        };

        info!("Starting sync loop (with all callbacks)...");

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
        let own_uid = self.inner.user_id().map(|u| u.to_owned());
        self.inner
            .add_event_handler(move |ev: OriginalSyncRoomMessageEvent, room: Room| {
                let cb = msg_cb.clone();
                let own = own_uid.clone();
                async move {
                    // Skip replacement (edit) events - they update existing messages
                    if let Some(ref rel) = ev.content.relates_to {
                        use matrix_sdk::ruma::events::room::message::Relation;
                        if matches!(rel, Relation::Replacement(_)) {
                            return;
                        }
                    }

                    let body = match &ev.content.msgtype {
                        MessageType::Text(t) => t.body.clone(),
                        MessageType::Notice(n) => n.body.clone(),
                        _ => return,
                    };
                    let room_name = room
                        .display_name()
                        .await
                        .map(|n| n.to_string())
                        .unwrap_or_else(|_| "unknown".to_string());
                    let ts: u64 = ev.origin_server_ts.0.into();
                    let is_own = own.as_ref().is_some_and(|u| *u == ev.sender);
                    let member = room.get_member_no_sync(&ev.sender).await.ok().flatten();
                    let sender_display_name = member
                        .as_ref()
                        .and_then(|m| m.display_name().map(|n| n.to_string()));
                    let sender_avatar_url = member
                        .as_ref()
                        .and_then(|m| m.avatar_url().map(|u| u.to_string()));
                    // Extract reply-to info
                    let reply_to_event_id = ev.content.relates_to.as_ref().and_then(|r| {
                        use matrix_sdk::ruma::events::room::message::Relation;
                        match r {
                            Relation::Reply { in_reply_to } => {
                                Some(in_reply_to.event_id.to_string())
                            }
                            _ => None,
                        }
                    });

                    cb(IncomingMessage {
                        event_id: ev.event_id.to_string(),
                        room_id: room.room_id().to_string(),
                        room_name,
                        sender: ev.sender.to_string(),
                        sender_display_name,
                        sender_avatar_url,
                        body,
                        timestamp: ts,
                        is_own,
                        reply_to_event_id,
                        is_edited: false,
                        is_redacted: false,
                    });
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

        // IoT status handler - catches all timeline events and filters for custom type
        let iot_cb = std::sync::Arc::new(on_iot_status);
        self.inner
            .add_event_handler(move |raw: Raw<AnySyncTimelineEvent>, room: Room| {
                let cb = iot_cb.clone();
                async move {
                    if let Ok(json) = raw.deserialize_as::<serde_json::Value>() {
                        let event_type = json.get("type").and_then(|t| t.as_str());
                        if event_type == Some("dev.simplego.iot.status") {
                            if let Some(content) = json.get("content") {
                                let device_id = content
                                    .get("device_id")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or_default()
                                    .to_string();
                                let state = content.get("state").and_then(|v| v.as_bool());
                                let value = content.get("value").and_then(|v| v.as_f64());
                                let unit = content
                                    .get("unit")
                                    .and_then(|v| v.as_str())
                                    .map(String::from);
                                let timestamp = content
                                    .get("timestamp")
                                    .and_then(|v| v.as_u64())
                                    .unwrap_or(0);

                                cb(IotStatusPayload {
                                    room_id: room.room_id().to_string(),
                                    device_id,
                                    state,
                                    value,
                                    unit,
                                    timestamp,
                                });
                            }
                        }
                    }
                }
            });

        // Reaction handler
        let rx_cb = std::sync::Arc::new(on_reaction);
        let rx_own = self.inner.user_id().map(|u| u.to_owned());
        self.inner
            .add_event_handler(move |ev: OriginalSyncReactionEvent, room: Room| {
                let cb = rx_cb.clone();
                let own = rx_own.clone();
                async move {
                    let ann = &ev.content.relates_to;
                    let is_own = own.as_ref().is_some_and(|u| *u == ev.sender);
                    let ts: u64 = ev.origin_server_ts.0.into();
                    cb(IncomingReaction {
                        event_id: ev.event_id.to_string(),
                        room_id: room.room_id().to_string(),
                        sender: ev.sender.to_string(),
                        target_event_id: ann.event_id.to_string(),
                        key: ann.key.clone(),
                        is_own,
                        timestamp: ts,
                    });
                }
            });

        self.inner.sync(SyncSettings::default()).await?;
        Ok(())
    }

    /// Start the sync loop with message and typing callbacks.
    pub async fn sync_with_callbacks<F, T>(
        &self,
        on_message: F,
        on_typing: T,
    ) -> Result<(), SgxError>
    where
        F: Fn(IncomingMessage) + Send + Sync + 'static,
        T: Fn(TypingPayload) + Send + Sync + 'static,
    {
        self.sync_with_all_callbacks(on_message, on_typing, |_| {}, |_| {})
            .await
    }

    /// Start the sync loop with only a message callback.
    pub async fn sync_with_callback<F>(&self, on_message: F) -> Result<(), SgxError>
    where
        F: Fn(IncomingMessage) + Send + Sync + 'static,
    {
        self.sync_with_all_callbacks(on_message, |_| {}, |_| {}, |_| {})
            .await
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
