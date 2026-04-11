#![recursion_limit = "256"]

mod commands;
mod sidecar;
mod telegram_commands;

use commands::AppState;
use sidecar::SidecarManager;
use std::sync::Arc;
use tokio::sync::Mutex;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,matrix_sdk=warn")),
        )
        .init();

    let sidecar_manager = Arc::new(SidecarManager::new());

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState {
            client: Arc::new(Mutex::new(None)),
            sync_cancel: Arc::new(Mutex::new(tokio_util::sync::CancellationToken::new())),
        })
        .manage(sidecar_manager.clone())
        .setup(move |app| {
            // Auto-start Telegram sidecar if a previous session exists
            let tdlib_dir = telegram_commands::tdlib_data_dir();
            let has_session = tdlib_dir.join("td.binlog").exists();
            tracing::info!(
                "Checking TDLib session at {:?}: exists={}",
                tdlib_dir,
                has_session
            );

            if has_session {
                tracing::info!("Auto-starting Telegram sidecar");
                let handle = app.handle().clone();
                let sidecar = sidecar_manager.clone();
                let data_dir_str = tdlib_dir.to_string_lossy().to_string();

                tauri::async_runtime::spawn(async move {
                    use tauri_plugin_shell::ShellExt;

                    let api_id =
                        std::env::var("TG_API_ID").unwrap_or_else(|_| "34883771".to_string());
                    let api_hash = std::env::var("TG_API_HASH")
                        .unwrap_or_else(|_| "18be2f35cff67932d69d661faefe8fc3".to_string());

                    let cmd = handle.shell().command("sgx-telegram").args([
                        "--api-id",
                        &api_id,
                        "--api-hash",
                        &api_hash,
                        "--port",
                        "50051",
                        "--data-dir",
                        &data_dir_str,
                    ]);

                    match cmd.spawn() {
                        Ok(_) => tracing::info!("Telegram sidecar spawned"),
                        Err(e) => {
                            tracing::warn!("Failed to spawn Telegram sidecar: {e}");
                            return;
                        }
                    }

                    // Wait for sidecar to start
                    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

                    // Connect gRPC client
                    match sidecar.connect("telegram", 50051).await {
                        Ok(_) => {
                            tracing::info!("Telegram sidecar connected, emitting tg-ready");
                            use tauri::Emitter;
                            let _ = handle.emit("tg-ready", ());
                        }
                        Err(e) => tracing::warn!("Telegram sidecar connect failed: {e}"),
                    }
                });
            } else {
                tracing::info!("No TDLib session found - skipping Telegram auto-start");
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Matrix commands
            commands::login,
            commands::try_restore_session,
            commands::get_rooms,
            commands::send_message,
            commands::send_typing,
            commands::mark_as_read,
            commands::get_settings,
            commands::get_recovery_key,
            commands::send_iot_command,
            commands::get_iot_devices,
            commands::create_room,
            commands::create_dm,
            commands::join_room,
            commands::leave_room,
            commands::invite_user,
            commands::kick_user,
            commands::ban_user,
            commands::unban_user,
            commands::get_room_members,
            commands::get_room_info,
            commands::set_room_name,
            commands::set_room_topic,
            commands::set_room_tag,
            commands::remove_room_tag,
            commands::redact_event,
            commands::get_room_settings,
            commands::set_join_rule,
            commands::set_history_visibility,
            commands::resolve_mxc_url,
            commands::get_avatar_base64,
            commands::get_own_profile,
            commands::set_display_name,
            commands::upload_avatar,
            commands::remove_avatar,
            commands::set_room_avatar,
            commands::remove_room_avatar,
            commands::get_room_messages,
            commands::send_reply,
            commands::send_reaction,
            commands::edit_message,
            commands::upload_avatar_from_path,
            commands::upload_room_avatar_from_path,
            commands::logout,
            // Telegram / Multi-Messenger commands
            telegram_commands::tg_start_sidecar,
            telegram_commands::tg_connect,
            telegram_commands::tg_get_auth_state,
            telegram_commands::tg_submit_phone,
            telegram_commands::tg_submit_code,
            telegram_commands::tg_submit_password,
            telegram_commands::tg_list_chats,
            telegram_commands::tg_get_messages,
            telegram_commands::tg_send_message,
            telegram_commands::tg_logout,
            telegram_commands::tg_remove_account,
            telegram_commands::tg_download_avatar,
            telegram_commands::tg_subscribe_updates,
            telegram_commands::get_all_chats,
            telegram_commands::get_backends,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
