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
