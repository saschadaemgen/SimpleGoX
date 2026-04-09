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
        })
        .manage(sidecar_manager)
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
            telegram_commands::get_all_chats,
            telegram_commands::get_backends,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
