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
use tauri::State;

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
