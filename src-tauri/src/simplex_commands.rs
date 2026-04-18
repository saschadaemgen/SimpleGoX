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
            }
        }
        tracing::info!(">>> sx_subscribe_updates: stream ended");
    });

    Ok(())
}
