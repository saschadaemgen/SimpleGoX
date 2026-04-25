//! Tauri commands for the SimpleX sidecar (sgx-simplex).
//!
//! Mirrors the dispatch pattern used in `telegram_commands.rs`: each
//! command fetches the gRPC client for backend "simplex" from the
//! SidecarManager and forwards the call. Errors are returned as plain
//! Strings so the frontend can surface them directly.

use crate::sidecar::SidecarManager;
use serde::{Deserialize, Serialize};
use sgx_proto::messenger::v1::*;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

/// Frontend-friendly profile shape. Returned by `sx_get_profile`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimplexProfile {
    pub has_profile: bool,
    pub display_name: String,
    pub full_name: String,
    pub bio: String,
}

/// Set (or update) the local user profile stored in the sgx-simplex
/// singleton `user_profile` row. Empty `display_name` is rejected by the
/// sidecar and surfaces here as an error string.
#[tauri::command]
pub async fn sx_set_profile(
    sidecar: State<'_, Arc<SidecarManager>>,
    display_name: String,
    full_name: String,
    bio: String,
) -> Result<(), String> {
    let mut client = sidecar
        .get_client("simplex")
        .await
        .ok_or("SimpleX sidecar not connected")?;

    let response = client
        .set_profile(SetProfileRequest {
            display_name,
            full_name,
            bio,
        })
        .await
        .map_err(|e| format!("gRPC error: {e}"))?
        .into_inner();

    if response.success {
        Ok(())
    } else {
        Err(response.error)
    }
}

/// Load the current user profile. Returns a SimplexProfile with
/// `has_profile=false` and empty strings when no profile has been set yet.
#[tauri::command]
pub async fn sx_get_profile(
    sidecar: State<'_, Arc<SidecarManager>>,
) -> Result<SimplexProfile, String> {
    let mut client = sidecar
        .get_client("simplex")
        .await
        .ok_or("SimpleX sidecar not connected")?;

    let response = client
        .get_profile(GetProfileRequest {})
        .await
        .map_err(|e| format!("gRPC error: {e}"))?
        .into_inner();

    Ok(SimplexProfile {
        has_profile: response.has_profile,
        display_name: response.display_name,
        full_name: response.full_name,
        bio: response.bio,
    })
}

/// Submit a SimpleX invitation or contact-address URL. Triggers the
/// SubmitAuthCode endpoint, which dispatches to the invitation-handshake
/// or contact-handshake background task depending on the link type.
#[tauri::command]
pub async fn sx_submit_invitation(
    sidecar: State<'_, Arc<SidecarManager>>,
    code: String,
) -> Result<(), String> {
    let mut client = sidecar
        .get_client("simplex")
        .await
        .ok_or("SimpleX sidecar not connected")?;

    client
        .submit_auth_code(SubmitAuthCodeRequest { code })
        .await
        .map_err(|e| format!("gRPC error: {e}"))?;

    Ok(())
}

/// Reset the entire SimpleX sidecar state: profile, contacts, ratchets,
/// messages. Analogous to the Matrix logout / Telegram remove-account
/// flows. The sidecar stays alive; a fresh profile can be set via
/// sx_set_profile afterwards.
#[tauri::command]
pub async fn sx_disconnect(
    sidecar: State<'_, Arc<SidecarManager>>,
) -> Result<(), String> {
    let mut client = sidecar
        .get_client("simplex")
        .await
        .ok_or("SimpleX sidecar not connected")?;

    let response = client
        .reset_simplex(ResetSimplexRequest {})
        .await
        .map_err(|e| format!("gRPC error: {e}"))?
        .into_inner();

    if response.success {
        Ok(())
    } else {
        Err(response.error)
    }
}

/// Frontend-friendly result for `sx_send_message`. Mirrors the shape of
/// `SendSimplexMessageResponse` but with serde-friendly snake_case.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SxSendResult {
    pub msg_id: u64,
    pub timestamp_ms: i64,
}

/// Send a chat text on a SimpleX contact's reply queue. Routes the call
/// through the sgx-simplex sidecar; the sidecar in turn injects a
/// ContactCommand::SendText into the per-contact session loop, which
/// runs the Double Ratchet send and returns the SMP-acknowledged
/// sndMsgId plus the wall-clock timestamp.
///
/// Returns the result struct on success; gRPC-side errors come back as
/// formatted strings the frontend can show in a toast or inline. The
/// frontend does NOT need to optimistically render the own message:
/// the sidecar emits an SimplexNewMessage event with `is_own = true`
/// over the existing sx-new-message subscription on success.
#[tauri::command]
pub async fn sx_send_message(
    sidecar: State<'_, Arc<SidecarManager>>,
    contact_id: String,
    body: String,
) -> Result<SxSendResult, String> {
    let mut client = sidecar
        .get_client("simplex")
        .await
        .ok_or("SimpleX sidecar not connected")?;

    let response = client
        .send_simplex_message(SendSimplexMessageRequest { contact_id, body })
        .await
        .map_err(|e| format!("gRPC error: {e}"))?
        .into_inner();

    Ok(SxSendResult {
        msg_id: response.msg_id,
        timestamp_ms: response.timestamp_ms,
    })
}

// ==================== Briefing 045: Contact Listing + Wipe ====================

/// Frontend-shaped contact summary for the `sx_list_contacts` Tauri
/// command. Classified as Tier::PublicMetadata in Briefing 045 D4
/// (contact_id included for RPC correlation, technically Tier 2 but
/// unavoidable on any wire that needs a handle to the contact).
///
/// `#[derive(Zeroize, ZeroizeOnDrop)]` causes the Rust-side memory to
/// be actively zeroed when the Vec is dropped after Tauri has
/// JSON-serialized it for the frontend. JavaScript-side strings are
/// GC-managed and opaque to us; the frontend zeroize guarantee is
/// best-effort (drop all references, let GC reclaim) per Risk R5.
#[derive(Debug, Clone, Serialize, zeroize::Zeroize, zeroize::ZeroizeOnDrop)]
pub struct ContactSummaryDto {
    pub contact_id: String,
    pub display_name: String,
    pub full_name: String,
    // Numeric fields are Copy; zeroize derive handles them as no-ops,
    // which is fine - there is no secret material in an epoch-seconds
    // integer. Kept here for field uniformity.
    #[zeroize(skip)]
    pub established_at_unix: i64,
    #[zeroize(skip)]
    pub last_message_at_unix: i64,
    #[zeroize(skip)]
    pub unread_count: i32,
    // Frontend-friendly string form of ConnStateProto: "connected",
    // "reconnecting", "dead", or "unspecified" (should never happen).
    pub conn_state: String,
}

/// Fetch the current contact list from the SimpleX sidecar. Called
/// once by the frontend startup sequence (`simplexContacts.loadFromBackend`)
/// and re-callable via `refresh()` for post-event updates.
///
/// Frontend receives a JSON array; Rust-side Vec<ContactSummaryDto>
/// memory is zeroed when it leaves this scope.
#[tauri::command]
pub async fn sx_list_contacts(
    sidecar: State<'_, Arc<SidecarManager>>,
) -> Result<Vec<ContactSummaryDto>, String> {
    let mut client = sidecar
        .get_client("simplex")
        .await
        .ok_or("SimpleX sidecar not connected")?;

    let response = client
        .list_simplex_contacts(ListSimplexContactsRequest {})
        .await
        .map_err(|e| format!("list_simplex_contacts gRPC error: {e}"))?
        .into_inner();

    let contacts: Vec<ContactSummaryDto> = response
        .contacts
        .into_iter()
        .map(|c| {
            let conn_state = match ConnStateProto::try_from(c.conn_state)
                .unwrap_or(ConnStateProto::ConnStateUnspecified)
            {
                ConnStateProto::ConnStateConnected => "connected",
                ConnStateProto::ConnStateReconnecting => "reconnecting",
                ConnStateProto::ConnStateDead => "dead",
                ConnStateProto::ConnStateUnspecified => "unspecified",
            }
            .to_string();
            ContactSummaryDto {
                contact_id: c.contact_id,
                display_name: c.display_name,
                full_name: c.full_name,
                established_at_unix: c.established_at_unix,
                last_message_at_unix: c.last_message_at_unix,
                unread_count: c.unread_count,
                conn_state,
            }
        })
        .collect();

    tracing::info!(
        "sx_list_contacts: returning {} contact(s) to frontend",
        contacts.len()
    );
    Ok(contacts)
}

/// Briefing 045 W6: Season-8-ready logout stub.
///
/// For Season 7 this is metadata-only: emits `app-logged-out` so the
/// frontend clears its in-memory stores, then disconnects via the
/// existing `sx_disconnect` (= reset_simplex RPC) to tear down
/// profile + contacts + ratchets on the sidecar side. Season 8 will
/// additionally zero the master key in the OS keyring and lock the
/// SQLCipher DB.
///
/// Intentionally separate from `sx_disconnect` so the logout semantic
/// can grow without breaking disconnect callers. Frontend uses
/// sx_logout from a future lock / logout button; sx_disconnect remains
/// the "undo setup entirely" path in AccountsTab.
#[tauri::command]
pub async fn sx_logout(
    app: AppHandle,
    sidecar: State<'_, Arc<SidecarManager>>,
) -> Result<(), String> {
    tracing::info!("sx_logout: initiating logout");
    let mut client = sidecar
        .get_client("simplex")
        .await
        .ok_or("SimpleX sidecar not connected")?;
    // Season 7 logout == Season 6 disconnect at the RPC level.
    // Season 8 will add keyring + SQLCipher lock here.
    let _ = client
        .reset_simplex(ResetSimplexRequest {})
        .await
        .map_err(|e| format!("logout reset RPC failed: {e}"))?;
    let _ = app.emit("app-logged-out", ());
    tracing::info!("sx_logout: app-logged-out emitted");
    Ok(())
}

/// Destructive: wipe every SimpleX contact and its crypto state. Only
/// callable with `wizard_intent: true` to guard against accidental
/// invocation from other frontend paths, RPC debuggers or dev consoles
/// (explicit-intent parameter, not a security boundary - Briefing 045
/// D3 / Risk R4).
///
/// Returns the number of contact rows that were removed from the
/// sidecar DB.
#[tauri::command]
pub async fn sx_wipe_all_contacts(
    sidecar: State<'_, Arc<SidecarManager>>,
    wizard_intent: bool,
) -> Result<u32, String> {
    if !wizard_intent {
        return Err(
            "sx_wipe_all_contacts requires wizard_intent=true (refusing to wipe without explicit intent flag)".to_string()
        );
    }
    let mut client = sidecar
        .get_client("simplex")
        .await
        .ok_or("SimpleX sidecar not connected")?;
    let response = client
        .wipe_all_simplex_contacts(WipeAllSimplexContactsRequest {})
        .await
        .map_err(|e| format!("wipe_all_simplex_contacts gRPC error: {e}"))?
        .into_inner();
    tracing::warn!(
        "sx_wipe_all_contacts: {} contact(s) deleted (wizard_intent confirmed)",
        response.deleted_count
    );
    Ok(response.deleted_count)
}

// ==================== Stream Subscription ====================

/// Frontend-friendly shape for the SimplexContactEstablished update.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SxContactEstablishedEvent {
    pub contact_id: String,
    pub display_name: String,
    pub full_name: String,
    pub bio: String,
    pub established_at: i64,
}

/// Frontend-friendly shape for the SimplexNewMessage update.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SxNewMessageEvent {
    pub contact_id: String,
    pub msg_id: i64,
    pub timestamp: i64,
    pub body: String,
    pub is_own: bool,
}

/// Frontend-friendly shape for the SimplexContactUpdated update.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SxContactUpdatedEvent {
    pub contact_id: String,
    pub display_name: String,
    pub full_name: String,
    pub bio: String,
}

/// StatusBanner-compatible payload for SimpleX handshake progress.
/// Intentionally shaped like the tor-status / i2p-status events so the
/// existing StatusBanner component only needs a second listener line.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SxStatusEvent {
    pub state: String,  // "bootstrapping" | "connected" | "error"
    pub detail: String, // human-readable message shown in the banner
    pub stage: String,  // protocol stage identifier for log rows
}

/// Briefing 044 W6: per-contact SMP connection lifecycle. The sidecar
/// emits these whenever ConnState flips in a BG loop; the frontend
/// maps them to a coloured dot next to the contact entry in the
/// sidebar and chat header.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SxContactDisconnectedEvent {
    pub contact_id: String,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SxContactReconnectingEvent {
    pub contact_id: String,
    pub attempt: u32,
    pub max_attempts: u32,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SxContactReconnectedEvent {
    pub contact_id: String,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SxContactDeadEvent {
    pub contact_id: String,
    pub timestamp: i64,
}

/// Subscribe to the SimpleX sidecar update stream and re-emit every
/// variant as a typed Tauri event. Mirrors `tg_subscribe_updates`.
///
/// The frontend should call this exactly once after `sx-ready`. The
/// spawned background task lives as long as the gRPC stream: it exits
/// when the stream ends or the sidecar goes away, at which point the
/// frontend can call this again to resubscribe.
#[tauri::command]
pub async fn sx_subscribe_updates(
    app: AppHandle,
    sidecar: State<'_, Arc<SidecarManager>>,
) -> Result<(), String> {
    let mut client = sidecar
        .get_client("simplex")
        .await
        .ok_or("SimpleX sidecar not connected")?;

    tracing::info!(">>> sx_subscribe_updates: connecting to stream...");

    let response = client
        .stream_simplex_updates(StreamSimplexUpdatesRequest {})
        .await
        .map_err(|e| format!("stream_simplex_updates gRPC error: {e}"))?;

    let mut stream = response.into_inner();
    tracing::info!(">>> sx_subscribe_updates: stream connected!");

    tokio::spawn(async move {
        use sgx_proto::messenger::v1::simplex_update::Update as U;

        while let Ok(Some(proto_update)) = stream.message().await {
            let Some(u) = proto_update.update else { continue };
            match u {
                U::ContactEstablished(e) => {
                    let event = SxContactEstablishedEvent {
                        contact_id: e.contact_id,
                        display_name: e.display_name,
                        full_name: e.full_name,
                        bio: e.bio,
                        established_at: e.established_at,
                    };
                    tracing::info!(
                        ">>> sx event: contact-established contact={} name={}",
                        event.contact_id,
                        event.display_name
                    );
                    let _ = app.emit("sx-contact-established", &event);
                }
                U::NewMessage(m) => {
                    let event = SxNewMessageEvent {
                        contact_id: m.contact_id,
                        msg_id: m.msg_id,
                        timestamp: m.timestamp,
                        body: m.body,
                        is_own: m.is_own,
                    };
                    tracing::info!(
                        ">>> sx event: new-message contact={} msg_id={}",
                        event.contact_id,
                        event.msg_id
                    );
                    let _ = app.emit("sx-new-message", &event);
                }
                U::ContactUpdated(u) => {
                    let event = SxContactUpdatedEvent {
                        contact_id: u.contact_id,
                        display_name: u.display_name,
                        full_name: u.full_name,
                        bio: u.bio,
                    };
                    let _ = app.emit("sx-contact-updated", &event);
                }
                U::HandshakeProgress(p) => {
                    tracing::info!(
                        ">>> sx progress: stage={} state={} msg={}",
                        p.stage,
                        p.state,
                        p.message
                    );
                    let event = SxStatusEvent {
                        state: p.state,
                        detail: p.message,
                        stage: p.stage,
                    };
                    let _ = app.emit("sx-status", &event);
                }
                // Briefing 044 W6: per-contact connection lifecycle.
                U::ContactDisconnected(d) => {
                    tracing::info!(">>> sx event: contact-disconnected contact={}", d.contact_id);
                    let event = SxContactDisconnectedEvent {
                        contact_id: d.contact_id,
                        timestamp: d.timestamp,
                    };
                    let _ = app.emit("sx-contact-disconnected", &event);
                }
                U::ContactReconnecting(r) => {
                    tracing::info!(
                        ">>> sx event: contact-reconnecting contact={} attempt={}/{}",
                        r.contact_id,
                        r.attempt,
                        r.max_attempts
                    );
                    let event = SxContactReconnectingEvent {
                        contact_id: r.contact_id,
                        attempt: r.attempt,
                        max_attempts: r.max_attempts,
                        timestamp: r.timestamp,
                    };
                    let _ = app.emit("sx-contact-reconnecting", &event);
                }
                U::ContactReconnected(r) => {
                    tracing::info!(">>> sx event: contact-reconnected contact={}", r.contact_id);
                    let event = SxContactReconnectedEvent {
                        contact_id: r.contact_id,
                        timestamp: r.timestamp,
                    };
                    let _ = app.emit("sx-contact-reconnected", &event);
                }
                U::ContactDead(d) => {
                    tracing::warn!(">>> sx event: contact-dead contact={}", d.contact_id);
                    let event = SxContactDeadEvent {
                        contact_id: d.contact_id,
                        timestamp: d.timestamp,
                    };
                    let _ = app.emit("sx-contact-dead", &event);
                }
            }
        }
        tracing::info!(">>> sx_subscribe_updates: stream ended");
    });

    Ok(())
}
