//! Configuration management for SimpleGoX clients.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// TODO(security): Encrypt access_token at rest. Consider OS keychain
// integration (Linux: libsecret, macOS: Keychain, Windows: Credential Manager)
// Reference: Element X stores tokens in platform keystore

/// Configuration for a SimpleGoX Matrix client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SgxConfig {
    /// Matrix homeserver URL (e.g. "https://matrix.org")
    pub homeserver_url: String,

    /// Username (without @user:server format, just the localpart)
    pub username: String,

    /// Path to the local data directory for SQLite stores
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,

    /// Whether to enable E2E encryption (default: true)
    #[serde(default = "default_true")]
    pub encryption: bool,

    /// Full Matrix user ID (e.g. "@alice:matrix.org") - set after login
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,

    /// Device ID assigned by the homeserver - set after login
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,

    /// Access token for authenticated requests - set after login
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access_token: Option<String>,

    /// Refresh token (if the homeserver supports token refresh)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,

    /// Recovery key generated during cross-signing setup
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery_key: Option<String>,
}

fn default_data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("simplego-x")
}

fn default_true() -> bool {
    true
}

impl SgxConfig {
    /// Load configuration from a TOML file.
    pub fn from_file(path: &std::path::Path) -> Result<Self, crate::SgxError> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| crate::SgxError::Config(format!("Failed to read config: {e}")))?;
        toml::from_str(&content)
            .map_err(|e| crate::SgxError::Config(format!("Failed to parse config: {e}")))
    }

    /// Return the default config file path: `~/.config/simplego-x/config.toml`
    pub fn default_config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("simplego-x")
            .join("config.toml")
    }

    /// Save configuration to a TOML file.
    ///
    /// Creates parent directories if they do not exist.
    pub fn save_to_file(&self, path: &std::path::Path) -> Result<(), crate::SgxError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)
            .map_err(|e| crate::SgxError::Config(format!("Failed to serialize config: {e}")))?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Check whether this config contains a persisted session.
    pub fn has_session(&self) -> bool {
        self.access_token.is_some() && self.user_id.is_some() && self.device_id.is_some()
    }

    /// Ensure the data directory exists.
    pub fn ensure_data_dir(&self) -> Result<(), crate::SgxError> {
        std::fs::create_dir_all(&self.data_dir)?;
        Ok(())
    }
}
