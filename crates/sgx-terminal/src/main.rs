#![recursion_limit = "256"]
//! # SimpleGoX Terminal
//!
//! The world's first dedicated Matrix hardware communication terminal.
//!
//! ## Usage
//!
//! ```bash
//! # Login and start syncing
//! sgx-terminal login --homeserver https://matrix.org --user alice
//!
//! # Start the terminal (after initial login)
//! sgx-terminal run
//!
//! # Send a message
//! sgx-terminal send --room '#test:matrix.org' --message "Hello!"
//!
//! # Verify a device interactively
//! sgx-terminal verify
//!
//! # Logout and clean up
//! sgx-terminal logout
//! ```

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use sgx_core::{SgxClient, SgxConfig};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(
    name = "sgx-terminal",
    about = "SimpleGoX Terminal - Dedicated Matrix communication terminal",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Login to a Matrix homeserver
    Login {
        /// Homeserver URL (e.g. https://matrix.org)
        #[arg(short = 's', long)]
        homeserver: String,

        /// Username (localpart only, e.g. 'alice')
        #[arg(short, long)]
        user: String,

        /// Password (will prompt if not provided)
        #[arg(short, long)]
        password: Option<String>,
    },

    /// Start the terminal and sync with the homeserver
    Run,

    /// Send a message to a room
    Send {
        /// Room ID (!xxx:server) or alias (#xxx:server)
        #[arg(short, long)]
        room: String,

        /// Message text to send
        #[arg(short, long)]
        message: String,
    },

    /// Start interactive SAS emoji device verification
    Verify,

    /// Logout and remove local data
    Logout,
}

/// Restore a session from the saved config, returning the client and config.
async fn restore_client() -> Result<(SgxClient, SgxConfig)> {
    let config_path = SgxConfig::default_config_path();
    let config = SgxConfig::from_file(&config_path).with_context(|| {
        format!(
            "No config found at {}. Run 'sgx-terminal login' first.",
            config_path.display()
        )
    })?;

    if !config.has_session() {
        anyhow::bail!(
            "No session in {}. Run 'sgx-terminal login' first.",
            config_path.display()
        );
    }

    let client = SgxClient::new(config.clone()).await?;
    client.restore_session().await?;
    Ok((client, config))
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,matrix_sdk=warn")),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Login {
            homeserver,
            user,
            password,
        } => {
            let password = match password {
                Some(p) => p,
                None => rpassword::prompt_password_stderr("Password: ")
                    .context("Failed to read password")?,
            };

            let mut config = SgxConfig {
                homeserver_url: homeserver,
                username: user,
                data_dir: dirs::data_local_dir()
                    .unwrap_or_else(|| std::path::PathBuf::from("."))
                    .join("simplego-x"),
                encryption: true,
                user_id: None,
                device_id: None,
                access_token: None,
                refresh_token: None,
            };

            let client = SgxClient::new(config.clone()).await?;
            client.login(&password).await?;
            println!("Login successful!");

            let (user_id, device_id, access_token, refresh_token) = client.session_credentials()?;
            config.user_id = Some(user_id);
            config.device_id = Some(device_id);
            config.access_token = Some(access_token);
            config.refresh_token = refresh_token;

            let config_path = SgxConfig::default_config_path();
            config.save_to_file(&config_path)?;

            // Cross-signing bootstrap (needs password for UIA)
            println!("Setting up cross-signing...");
            client.bootstrap_cross_signing(&password).await?;
            println!("Cross-signing keys created and uploaded.");

            // Recovery: key backup + recovery key
            match client.enable_recovery().await {
                Ok(recovery_key) => {
                    println!("Recovery key: {recovery_key}");
                    println!("IMPORTANT: Save this recovery key in a safe place!");
                }
                Err(e) => {
                    eprintln!("Warning: Could not enable recovery: {e}");
                    eprintln!("You can set up recovery later from another client.");
                }
            }

            println!("Config saved to {}", config_path.display());

            // Drop client explicitly before process exit so the SQLite
            // connection pool shuts down cleanly (avoids deadpool panic).
            drop(client);
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }

        Commands::Run => {
            println!("SimpleGoX Terminal starting...");

            let (client, config) = restore_client().await?;

            // Display cross-signing status
            if let Some(status) = client.cross_signing_status().await {
                println!(
                    "Cross-signing status: Master key: {}, Self-signing: {}, User-signing: {}",
                    if status.has_master { "yes" } else { "no" },
                    if status.has_self_signing { "yes" } else { "no" },
                    if status.has_user_signing { "yes" } else { "no" },
                );
            } else {
                println!("Cross-signing status: not available");
            }

            let user_id = config.user_id.as_deref().unwrap_or("unknown");
            println!("Syncing with {} as {}", config.homeserver_url, user_id);

            tokio::select! {
                result = client.sync() => {
                    result?;
                }
                _ = tokio::signal::ctrl_c() => {
                    println!("\nSimpleGoX Terminal shutting down...");
                }
            }
        }

        Commands::Send { room, message } => {
            let (client, _config) = restore_client().await?;

            // Initial sync to populate room list and crypto sessions
            client.sync_once().await?;

            let target_room = client.resolve_room(&room).await?;
            client.send_message(&target_room, &message).await?;

            println!("Message sent to {room}");

            // Clean shutdown
            drop(client);
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }

        Commands::Verify => {
            println!("SimpleGoX Terminal - Verification Mode");
            println!("Waiting for verification request...");

            let (client, _config) = restore_client().await?;
            register_verification_handlers(client.inner());

            println!("Syncing...");
            tokio::select! {
                result = client.sync() => {
                    result?;
                }
                _ = tokio::signal::ctrl_c() => {
                    println!("\nVerification mode cancelled.");
                }
            }
        }

        Commands::Logout => {
            let config_path = SgxConfig::default_config_path();
            let config = SgxConfig::from_file(&config_path).with_context(|| {
                format!(
                    "No config found at {}. Nothing to log out.",
                    config_path.display()
                )
            })?;

            if config.has_session() {
                let client = SgxClient::new(config.clone()).await?;
                client.restore_session().await?;
                client.logout().await?;
                println!("Logged out from server.");
                drop(client);
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }

            // Remove local data
            if config.data_dir.exists() {
                std::fs::remove_dir_all(&config.data_dir)?;
            }
            if config_path.exists() {
                std::fs::remove_file(&config_path)?;
            }

            println!("Local data and config removed.");
            println!("Goodbye!");
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Verification helpers
// ---------------------------------------------------------------------------

use futures_util::StreamExt;
use matrix_sdk::{
    encryption::verification::{SasState, SasVerification, Verification, VerificationRequestState},
    ruma::events::key::verification::request::ToDeviceKeyVerificationRequestEvent,
    Client,
};

/// Register event handlers that accept incoming verification requests and
/// drive the SAS emoji flow.
fn register_verification_handlers(client: &Client) {
    client.add_event_handler({
        |ev: ToDeviceKeyVerificationRequestEvent, client: Client| async move {
            let Some(request) = client
                .encryption()
                .get_verification_request(&ev.sender, &ev.content.transaction_id)
                .await
            else {
                return;
            };

            println!(
                "\nVerification request from {} ({})",
                request.other_user_id(),
                request.flow_id()
            );
            println!("Accepting request...");

            if let Err(e) = request.accept().await {
                eprintln!("Failed to accept verification request: {e}");
                return;
            }

            // Wait until the request transitions to Ready, then start SAS
            let mut changes = request.changes();
            while let Some(state) = changes.next().await {
                match state {
                    VerificationRequestState::Ready { .. } => {
                        println!("Starting SAS verification...");
                        match request.start_sas().await {
                            Ok(Some(sas)) => {
                                handle_sas(sas).await;
                                return;
                            }
                            Ok(None) => {
                                eprintln!("Could not start SAS (unsupported by other device).");
                                return;
                            }
                            Err(e) => {
                                eprintln!("Failed to start SAS: {e}");
                                return;
                            }
                        }
                    }
                    VerificationRequestState::Transitioned { verification } => {
                        // The other side started SAS before we could
                        if let Verification::SasV1(sas) = verification {
                            handle_sas(sas).await;
                        }
                        return;
                    }
                    VerificationRequestState::Done | VerificationRequestState::Cancelled(_) => {
                        return;
                    }
                    _ => {}
                }
            }
        }
    });
}

/// Drive a single SAS emoji verification to completion, interacting with the
/// user on stdin/stdout.
async fn handle_sas(sas: SasVerification) {
    use matrix_sdk::encryption::verification::format_emojis;
    use std::io::Write;

    let mut changes = sas.changes();
    while let Some(state) = changes.next().await {
        match state {
            SasState::KeysExchanged { emojis, .. } => {
                let Some(emoji_sas) = emojis else {
                    eprintln!("Emoji verification not supported by the other device.");
                    if let Err(e) = sas.cancel().await {
                        eprintln!("Cancel failed: {e}");
                    }
                    return;
                };

                println!("\nDo the emojis match?\n");
                println!("{}", format_emojis(emoji_sas.emojis));
                print!("\nConfirm with 'yes' or cancel with 'no': ");
                let _ = std::io::stdout().flush();

                let mut input = String::new();
                if std::io::stdin().read_line(&mut input).is_err() {
                    eprintln!("Failed to read input, cancelling.");
                    let _ = sas.cancel().await;
                    return;
                }

                match input.trim().to_lowercase().as_str() {
                    "yes" | "y" => {
                        if let Err(e) = sas.confirm().await {
                            eprintln!("Confirm failed: {e}");
                        }
                    }
                    _ => {
                        println!("Emojis did not match, cancelling.");
                        if let Err(e) = sas.mismatch().await {
                            eprintln!("Mismatch signal failed: {e}");
                        }
                        return;
                    }
                }
            }
            SasState::Done { .. } => {
                let device = sas.other_device();
                println!(
                    "Successfully verified device {} {}",
                    device.user_id(),
                    device.device_id()
                );
                println!("Verification complete!");
                return;
            }
            SasState::Cancelled(info) => {
                println!("Verification cancelled: {}", info.reason());
                return;
            }
            _ => {}
        }
    }
}
