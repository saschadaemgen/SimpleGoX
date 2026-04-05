#![recursion_limit = "256"]

mod commands;

use commands::AppState;
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

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(AppState {
            client: Arc::new(Mutex::new(None)),
        })
        .invoke_handler(tauri::generate_handler![
            commands::login,
            commands::get_rooms,
            commands::send_message,
            commands::send_typing,
            commands::mark_as_read,
            commands::get_settings,
            commands::logout,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
