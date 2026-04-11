//! Telegram authentication state machine powered by TDLib.
//!
//! Auth state is queried from TDLib via get_authorization_state().
//! The td::start_pump() receive loop runs on a background thread and
//! routes function responses internally - functions::* work normally.

use tdlib_rs::enums::{AuthorizationState, User};
use tokio::sync::Mutex;
use tracing::{info, warn};

/// Simplified auth state exposed to the gRPC layer.
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum AuthStatus {
    WaitPhone,
    WaitCode {
        phone_hint: String,
        /// "telegram_message", "sms", "sms_word", "sms_phrase", "call", "fragment", "unknown"
        code_type: String,
    },
    WaitPassword {
        hint: String,
    },
    Ready {
        user_id: String,
        display_name: String,
    },
    LoggedOut,
    Error(String),
}

impl Default for AuthStatus {
    fn default() -> Self {
        Self::WaitPhone
    }
}

/// Manages TDLib auth state.
pub struct AuthManager {
    client_id: i32,
    status: Mutex<AuthStatus>,
}

impl AuthManager {
    pub fn new(client_id: i32) -> Self {
        Self {
            client_id,
            status: Mutex::new(AuthStatus::default()),
        }
    }

    pub async fn status(&self) -> AuthStatus {
        self.status.lock().await.clone()
    }

    /// Run initial TDLib setup: set parameters, then query actual auth state.
    pub async fn initialize(
        &self,
        api_id: i32,
        api_hash: &str,
        data_dir: &str,
    ) -> Result<(), String> {
        let abs_path = std::path::Path::new(data_dir)
            .canonicalize()
            .unwrap_or_else(|_| std::env::current_dir().unwrap_or_default().join(data_dir));
        info!("=== TDLib database_directory: {:?}", data_dir);
        info!("=== TDLib database_directory ABSOLUTE: {:?}", abs_path);
        tdlib_rs::functions::set_tdlib_parameters(
            false,
            data_dir.to_string(),
            format!("{data_dir}/files"),
            String::new(),
            true,
            true,
            true,
            false,
            api_id,
            api_hash.to_string(),
            "en".to_string(),
            "SimpleGoX".to_string(),
            "1.0".to_string(),
            "0.1.0".to_string(),
            self.client_id,
        )
        .await
        .map_err(|e| format!("set_tdlib_parameters: {e:?}"))?;
        info!("TDLib parameters set OK");

        self.query_auth_state().await;
        Ok(())
    }

    /// Ask TDLib for the current authorization state and update our status.
    async fn query_auth_state(&self) {
        info!("Querying TDLib authorization state...");
        match tdlib_rs::functions::get_authorization_state(self.client_id).await {
            Ok(state) => {
                info!("TDLib auth state: {state:?}");
                self.apply_auth_state(state).await;
            }
            Err(e) => {
                warn!("get_authorization_state failed: {e:?}");
                *self.status.lock().await = AuthStatus::WaitPhone;
            }
        }
    }

    /// Map a TDLib AuthorizationState to our AuthStatus.
    async fn apply_auth_state(&self, state: AuthorizationState) {
        let new_status = match state {
            AuthorizationState::Ready => {
                // Pre-load chat list immediately so it's ready when frontend asks
                let cid = self.client_id;
                tokio::spawn(async move {
                    info!("=== Pre-loading chat list...");
                    match tdlib_rs::functions::load_chats(
                        Some(tdlib_rs::enums::ChatList::Main),
                        100,
                        cid,
                    )
                    .await
                    {
                        Ok(_) => info!("=== Chat list pre-loaded"),
                        Err(e) => info!("=== Chat list pre-load: {e:?} (may already be loaded)"),
                    }
                });

                match tdlib_rs::functions::get_me(self.client_id).await {
                    Ok(User::User(user)) => {
                        let name = format!("{} {}", user.first_name, user.last_name)
                            .trim()
                            .to_string();
                        info!("Session restored: {} ({})", name, user.id);
                        AuthStatus::Ready {
                            user_id: user.id.to_string(),
                            display_name: name,
                        }
                    }
                    Err(e) => {
                        warn!("Ready state but get_me failed: {e:?}");
                        AuthStatus::Ready {
                            user_id: "unknown".into(),
                            display_name: "Unknown".into(),
                        }
                    }
                }
            }
            AuthorizationState::WaitPhoneNumber => {
                info!("Fresh start, waiting for phone number");
                AuthStatus::WaitPhone
            }
            AuthorizationState::WaitCode(wait) => {
                use tdlib_rs::enums::AuthenticationCodeType;
                let code_type = match &wait.code_info.r#type {
                    AuthenticationCodeType::TelegramMessage(_) => "telegram_message",
                    AuthenticationCodeType::Sms(_) => "sms",
                    AuthenticationCodeType::SmsWord(_) => "sms_word",
                    AuthenticationCodeType::SmsPhrase(_) => "sms_phrase",
                    AuthenticationCodeType::Call(_) => "call",
                    AuthenticationCodeType::FlashCall(_) => "flash_call",
                    AuthenticationCodeType::Fragment(_) => "fragment",
                    _ => "unknown",
                };
                info!("=== AUTH WaitCode: code_type={code_type}");
                AuthStatus::WaitCode {
                    phone_hint: String::new(),
                    code_type: code_type.to_string(),
                }
            }
            AuthorizationState::WaitPassword(info) => {
                info!("Waiting for 2FA password");
                AuthStatus::WaitPassword {
                    hint: info.password_hint,
                }
            }
            AuthorizationState::LoggingOut
            | AuthorizationState::Closing
            | AuthorizationState::Closed => {
                info!("TDLib is logged out");
                AuthStatus::LoggedOut
            }
            other => {
                warn!("Unexpected auth state: {other:?}");
                AuthStatus::WaitPhone
            }
        };
        *self.status.lock().await = new_status;
    }

    /// Submit phone number. Queries actual state after.
    pub async fn submit_phone(&self, phone: &str) -> Result<(), String> {
        info!("Submitting phone number...");
        tdlib_rs::functions::set_authentication_phone_number(
            phone.to_string(),
            None,
            self.client_id,
        )
        .await
        .map_err(|e| format!("submit_phone: {e:?}"))?;

        info!("Phone submitted OK");
        self.query_auth_state().await;
        Ok(())
    }

    /// Submit auth code. Queries actual state after.
    pub async fn submit_code(&self, code: &str) -> Result<(), String> {
        info!("Submitting auth code...");
        tdlib_rs::functions::check_authentication_code(code.to_string(), self.client_id)
            .await
            .map_err(|e| format!("submit_code: {e:?}"))?;

        info!("Code accepted");
        self.query_auth_state().await;
        Ok(())
    }

    /// Submit 2FA password. Queries actual state after.
    pub async fn submit_password(&self, password: &str) -> Result<(), String> {
        info!("Submitting 2FA password...");
        tdlib_rs::functions::check_authentication_password(password.to_string(), self.client_id)
            .await
            .map_err(|e| format!("submit_password: {e:?}"))?;

        info!("Password accepted");
        self.query_auth_state().await;
        Ok(())
    }
}
