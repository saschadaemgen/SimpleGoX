#![recursion_limit = "256"]

mod commands;
mod i2p;
mod sidecar;
mod simplex_commands;
mod telegram_commands;
mod tor;
mod routing_commands;
mod tor_logging;

use commands::AppState;
use sidecar::SidecarManager;
use std::sync::Arc;
use tokio::sync::Mutex;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Layered tracing subscriber: fmt output + Tor log forwarder.
    // The TorLogForwarder uses a global OnceLock for the AppHandle,
    // which is set in .setup(). Events before setup are silently dropped.
    use tracing_subscriber::prelude::*;
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer().with_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(
                        "info,matrix_sdk=warn,matrix_sdk::encryption::recovery=off,matrix_sdk::encryption=off"
                    )),
            ),
        )
        .with(tor_logging::TorLogForwarder)
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
        .manage(Mutex::new(tor::TorManager::new()))
        .manage(Arc::new(Mutex::new(i2p::I2PManager::new())))
        .setup(move |app| {
            // Enable Tor log forwarding to frontend
            tor_logging::set_app_handle(app.handle().clone());
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

            // --- SimpleX Sidecar Auto-Start ---
            // SimpleX has no "session" concept: the sidecar always starts.
            // Profile setup is gRPC-initiated (SetProfile) after spawn, so
            // an unconfigured profile just means the startup log warns
            // "no profile configured" - the sidecar still serves gRPC.
            {
                tracing::info!("Auto-starting SimpleX sidecar");
                let handle = app.handle().clone();
                let sidecar = sidecar_manager.clone();
                let data_dir_simplex = dirs::data_local_dir()
                    .unwrap_or_else(|| std::path::PathBuf::from("."))
                    .join("simplego-x")
                    .join("simplex-data");
                let data_dir_simplex_str = data_dir_simplex.to_string_lossy().to_string();

                tauri::async_runtime::spawn(async move {
                    use tauri_plugin_shell::ShellExt;

                    let cmd = handle.shell().command("sgx-simplex").args([
                        "--port",
                        "50053",
                        "--data-dir",
                        &data_dir_simplex_str,
                    ]);

                    match cmd.spawn() {
                        Ok(_) => tracing::info!("SimpleX sidecar spawned"),
                        Err(e) => {
                            tracing::warn!("Failed to spawn SimpleX sidecar: {e}");
                            return;
                        }
                    }

                    // Wait for sidecar to start
                    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

                    // Connect gRPC client
                    match sidecar.connect("simplex", 50053).await {
                        Ok(_) => {
                            tracing::info!(
                                "SimpleX sidecar connected, emitting sx-ready"
                            );
                            use tauri::Emitter;
                            let _ = handle.emit("sx-ready", ());
                        }
                        Err(e) => {
                            tracing::warn!("SimpleX sidecar connect failed: {e}")
                        }
                    }
                });
            }

            // --- Routing Auto-Restore ---
            let tor_data_dir = dirs::data_local_dir()
                .unwrap_or_default()
                .join("simplego-x");

            // Migrate old config filename
            let old_file = tor_data_dir.join("tor-routing.json");
            let routing_file = tor_data_dir.join("routing-config.json");
            if old_file.exists() && !routing_file.exists() {
                let _ = std::fs::rename(&old_file, &routing_file);
                tracing::info!("Routing: migrated tor-routing.json to routing-config.json");
            }
            if routing_file.exists() {
                match std::fs::read_to_string(&routing_file) {
                    Ok(json) => {
                        match serde_json::from_str::<tor::TorRouting>(&json) {
                            Ok(routing) => {
                                if routing.any_enabled() {
                                    tracing::info!(
                                        "Routing: saved config found, auto-restoring: {:?}",
                                        routing
                                    );
                                    let tor_data = tor_data_dir.clone();
                                    let app_handle = app.handle().clone();
                                    tauri::async_runtime::spawn(async move {
                                        use tauri::Manager;

                                        let needs_tor = routing.any_tor_enabled();
                                        let needs_i2p = routing.any_i2p_enabled();

                                        // Bootstrap Arti only if any protocol uses Tor
                                        if needs_tor {
                                            tor::emit_tor_status(&app_handle, "bootstrapping", "Auto-restoring Tor connection...");
                                            let tor_state: tauri::State<
                                                '_,
                                                Mutex<tor::TorManager>,
                                            > = app_handle.state();
                                            let mut t = tor_state.lock().await;
                                            if let Err(e) =
                                                t.bootstrap(tor_data.clone()).await
                                            {
                                                tracing::error!(
                                                    "Tor: auto-bootstrap failed: {e}"
                                                );
                                                tor::emit_tor_status(&app_handle, "error", &format!("Tor bootstrap failed: {e}"));
                                            } else {
                                                // Apply Tor routing
                                                for (proto, mode) in [
                                                    ("matrix", &routing.matrix),
                                                    ("telegram", &routing.telegram),
                                                ] {
                                                    if matches!(
                                                        mode,
                                                        tor::RoutingMode::Tor
                                                            | tor::RoutingMode::TorOnion { .. }
                                                    ) {
                                                        let _ = t
                                                            .set_routing(
                                                                proto,
                                                                mode.clone(),
                                                                tor_data.clone(),
                                                            )
                                                            .await;
                                                    }
                                                }
                                                tor::emit_tor_status(&app_handle, "connected", "Tor restored from previous session");
                                            }
                                        }

                                        // Bootstrap I2P only if any protocol uses I2P
                                        if needs_i2p {
                                            let i2p_state: tauri::State<
                                                '_,
                                                Arc<Mutex<i2p::I2PManager>>,
                                            > = app_handle.state();
                                            let mut i = i2p_state.lock().await;
                                            if let Err(e) =
                                                i.bootstrap(tor_data.clone(), &app_handle).await
                                            {
                                                tracing::error!(
                                                    "I2P: auto-bootstrap failed: {e}"
                                                );
                                                // bootstrap() already emitted error status
                                            } else {
                                                // bootstrap() leaves status as "bootstrapping"
                                                // - connected comes when Matrix sync works
                                                let i2p_arc = i2p_state.inner().clone();
                                                let wd_app = app_handle.clone();
                                                tokio::spawn(async move {
                                                    i2p::start_watchdog(i2p_arc, wd_app).await;
                                                });
                                            }
                                        }

                                        tracing::info!("Auto-restore complete (tor={needs_tor}, i2p={needs_i2p})");
                                    });
                                } else {
                                    tracing::info!("Routing: all direct, skipping auto-restore");
                                }
                            }
                            Err(e) => tracing::warn!("Routing: parse config error: {e}"),
                        }
                    }
                    Err(e) => tracing::warn!("Routing: read config error: {e}"),
                }
            } else {
                tracing::info!("Routing: no saved config found");
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Destroyed = event {
                use tauri::Manager;
                // 1. Cancel sync loop first (stops network requests)
                let app = window.app_handle();
                if let Some(state) = app.try_state::<AppState>() {
                    if let Ok(cancel) = state.sync_cancel.try_lock() {
                        cancel.cancel();
                    }
                }
                // 2. Kill sidecar processes
                i2p::kill_all_i2pd();
                #[cfg(windows)]
                {
                    use std::os::windows::process::CommandExt;
                    for target in ["sgx-telegram.exe", "sgx-simplex.exe"] {
                        let mut cmd = std::process::Command::new("taskkill");
                        cmd.args(["/IM", target, "/F"]);
                        cmd.creation_flags(0x08000000);
                        let _ = cmd.output();
                    }
                }
                #[cfg(unix)]
                {
                    for target in ["sgx-telegram", "sgx-simplex"] {
                        let _ = std::process::Command::new("killall")
                            .args(["-q", target])
                            .output();
                    }
                }
            }
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
            // SimpleX commands
            simplex_commands::sx_set_profile,
            simplex_commands::sx_get_profile,
            simplex_commands::sx_submit_invitation,
            // Tor routing
            routing_commands::tor_set_protocol,
            routing_commands::tor_get_routing,
            routing_commands::tor_get_status,
            routing_commands::tor_check_ip,
            routing_commands::tor_get_saved_routing,
            routing_commands::tor_save_routing,
            routing_commands::tor_start_stats,
            routing_commands::tor_get_connections,
            routing_commands::get_i2p_stats,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
