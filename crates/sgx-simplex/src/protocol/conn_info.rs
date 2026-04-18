//! ConnInfo JSON - SimpleX profile and features.

use serde::{Deserialize, Serialize};

/// Connection info sent in AgentConfirmation.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ConnInfo {
    pub profile: Profile,
    pub features: Features,
}

/// User profile.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Profile {
    #[serde(rename = "displayName")]
    pub display_name: String,
    #[serde(rename = "fullName")]
    pub full_name: String,
}

/// Feature flags (empty for Phase 2).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct Features {}

impl ConnInfo {
    /// Create a new ConnInfo with display name.
    pub fn new(display_name: &str) -> Self {
        Self {
            profile: Profile {
                display_name: display_name.to_string(),
                full_name: String::new(),
            },
            features: Features::default(),
        }
    }

    /// Serialize to JSON bytes.
    pub fn to_json(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("ConnInfo serialize failed")
    }
}

// ---------------------------------------------------------------------------
// Receive-side envelope (briefing 036a): ConnInfo embedded in
// AgentConnInfoReply arrives wrapped in an `x.info` event envelope with
// a richer profile than the one we construct ourselves. Keep this additive
// to the outbound ConnInfo above.
// ---------------------------------------------------------------------------

/// Full `x.info` event envelope as seen in peer-sent AgentConnInfoReply.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnInfoEnvelope {
    pub v: String,
    pub event: String,
    pub params: ConnInfoParams,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnInfoParams {
    pub profile: PeerProfile,
}

/// Peer profile received from the other side. All fields are optional so
/// schema drift on the sender side does not break parsing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerProfile {
    #[serde(rename = "displayName", default)]
    pub display_name: Option<String>,
    #[serde(rename = "fullName", default)]
    pub full_name: Option<String>,
    #[serde(rename = "contactLink", default)]
    pub contact_link: Option<String>,
    #[serde(default)]
    pub preferences: Option<serde_json::Value>,
    #[serde(default)]
    pub image: Option<String>,
}

/// Parse a peer `x.info` envelope from JSON bytes.
pub fn parse_conn_info_json(bytes: &[u8]) -> Result<ConnInfoEnvelope, String> {
    serde_json::from_slice(bytes).map_err(|e| format!("ConnInfo JSON parse: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conn_info_json() {
        let info = ConnInfo::new("Sascha");
        let json = String::from_utf8(info.to_json()).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["profile"]["displayName"], "Sascha");
        assert_eq!(parsed["profile"]["fullName"], "");
    }
}
