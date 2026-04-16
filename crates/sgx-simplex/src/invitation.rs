//! Parse SimpleX invitation and contact links.
//!
//! Supported formats:
//! - `simplex:/invitation#/?v=2-7&smp=...&e2e=...`
//! - `simplex:/contact#/?v=2-7&smp=...&e2e=...`
//! - `https://simplex.chat/invitation#/?v=2-7&smp=...&e2e=...`
//! - `https://simplex.chat/contact#/?v=2-7&smp=...&e2e=...`
//!
//! All sensitive data is in the fragment (after #) and never sent to a server.

use anyhow::{anyhow, Result};

/// Resolve a short SimpleX link (https://<server>/a#<key>) to the full invitation link.
pub async fn resolve_short_link(short_link: &str) -> Result<String> {
    let resolve_url = short_link.replace("#", "?key=");
    let response = reqwest::Client::new()
        .get(&resolve_url)
        .header("Accept", "application/json")
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| anyhow!("Short link resolution failed: {e}"))?;

    let full_link = response
        .text()
        .await
        .map_err(|e| anyhow!("Short link response read failed: {e}"))?;

    tracing::info!("Resolved short link to {} chars", full_link.len());
    Ok(full_link.trim().to_string())
}

/// Parsed invitation link data.
#[derive(Debug, Clone)]
#[allow(dead_code)] // Fields used in Phase 2
pub struct ParsedInvitation {
    pub server_host: String,
    pub server_port: u16,
    pub server_fingerprint: String,
    pub queue_id: String,
    pub sender_key: String,
    pub version_range: (u16, u16),
    pub is_one_time: bool,
}

/// Parse a SimpleX invitation or contact link.
pub fn parse_invitation_link(link: &str) -> Result<ParsedInvitation> {
    let is_one_time = link.contains("/invitation");

    // Extract fragment (everything after #)
    let fragment = link
        .split_once('#')
        .map(|(_, f)| f)
        .ok_or_else(|| anyhow!("No fragment in link"))?;

    // Strip leading /? if present
    let query = fragment.strip_prefix("/?").unwrap_or(fragment);

    // Parse query parameters
    let params = parse_query(query);

    // Version range
    let version_str = params
        .iter()
        .find(|(k, _)| k == "v")
        .map(|(_, v)| v.as_str())
        .unwrap_or("1-1");
    let version_range = parse_version_range(version_str)?;

    // SMP queue URI (URL-encoded)
    let smp_encoded = params
        .iter()
        .find(|(k, _)| k == "smp")
        .map(|(_, v)| v.as_str())
        .ok_or_else(|| anyhow!("Missing smp parameter"))?;

    let smp_decoded = percent_encoding::percent_decode_str(smp_encoded)
        .decode_utf8()
        .map_err(|e| anyhow!("Invalid smp encoding: {e}"))?;

    let (server_host, server_port, server_fingerprint, queue_id, sender_key) =
        parse_smp_uri(&smp_decoded)?;

    Ok(ParsedInvitation {
        server_host,
        server_port,
        server_fingerprint,
        queue_id,
        sender_key,
        version_range,
        is_one_time,
    })
}

/// Parse a query string into key-value pairs.
fn parse_query(query: &str) -> Vec<(String, String)> {
    query
        .split('&')
        .filter_map(|pair| {
            let (k, v) = pair.split_once('=')?;
            Some((k.to_string(), v.to_string()))
        })
        .collect()
}

/// Parse version range like "2-7" into (2, 7).
fn parse_version_range(s: &str) -> Result<(u16, u16)> {
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 2 {
        return Err(anyhow!("Invalid version range: {s}"));
    }
    let min: u16 = parts[0].parse().map_err(|_| anyhow!("Invalid min version"))?;
    let max: u16 = parts[1].parse().map_err(|_| anyhow!("Invalid max version"))?;
    Ok((min, max))
}

/// Parse an SMP queue URI: smp://fingerprint@host:port/queueId#/?dh=key
fn parse_smp_uri(uri: &str) -> Result<(String, u16, String, String, String)> {
    // Format: smp://FINGERPRINT@HOST:PORT/QUEUE_ID#/?dh=KEY
    let stripped = uri
        .strip_prefix("smp://")
        .ok_or_else(|| anyhow!("SMP URI must start with smp://"))?;

    // Split fingerprint@rest
    let (fingerprint, rest) = stripped
        .split_once('@')
        .ok_or_else(|| anyhow!("Missing @ in SMP URI"))?;

    // Split host:port/path#fragment
    let (host_port_path, fragment) = rest.split_once('#').unwrap_or((rest, ""));

    // Split host:port from /path
    let (host_port, path) = if let Some(idx) = host_port_path.find('/') {
        (&host_port_path[..idx], &host_port_path[idx + 1..])
    } else {
        (host_port_path, "")
    };

    // Parse host and port
    let (host, port) = if let Some((h, p)) = host_port.split_once(':') {
        (h.to_string(), p.parse::<u16>().unwrap_or(5223))
    } else {
        (host_port.to_string(), 5223)
    };

    let queue_id = path.to_string();

    // Extract dh key from fragment query
    let frag_query = fragment.strip_prefix("/?").unwrap_or(fragment);
    let frag_params = parse_query(frag_query);
    let sender_key_raw = frag_params
        .iter()
        .find(|(k, _)| k == "dh")
        .map(|(_, v)| v.clone())
        .unwrap_or_default();
    // Percent-decode the DH key (may contain %3D for '=' padding)
    let sender_key = percent_encoding::percent_decode_str(&sender_key_raw)
        .decode_utf8()
        .map(|s| s.to_string())
        .unwrap_or(sender_key_raw);

    Ok((host, port, fingerprint.to_string(), queue_id, sender_key))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_https_invitation() {
        let link = "https://simplex.chat/invitation#/?v=2-7&smp=smp%3A%2F%2FXfTKGkd9rBkebeTnXMKSnLMAh82tHjNubpJRylz7KXg%3D%40smp.simplego.dev%3A5223%2FtestQueueId%23%2F%3Fdh%3DtestKey";
        let parsed = parse_invitation_link(link).unwrap();
        assert_eq!(parsed.server_host, "smp.simplego.dev");
        assert_eq!(parsed.server_port, 5223);
        assert!(parsed.server_fingerprint.contains("XfTKGkd9"));
        assert_eq!(parsed.version_range, (2, 7));
        assert!(parsed.is_one_time);
    }

    #[test]
    fn test_parse_simplex_contact() {
        let link = "simplex:/contact#/?v=1-4&smp=smp%3A%2F%2Ffp123%40example.com%3A5223%2FqueueABC%23%2F%3Fdh%3DdhKey456";
        let parsed = parse_invitation_link(link).unwrap();
        assert_eq!(parsed.server_host, "example.com");
        assert_eq!(parsed.server_port, 5223);
        assert_eq!(parsed.server_fingerprint, "fp123");
        assert_eq!(parsed.queue_id, "queueABC");
        assert_eq!(parsed.sender_key, "dhKey456");
        assert!(!parsed.is_one_time);
    }

    #[test]
    fn test_parse_version_range() {
        assert_eq!(parse_version_range("2-7").unwrap(), (2, 7));
        assert_eq!(parse_version_range("1-4").unwrap(), (1, 4));
        assert!(parse_version_range("invalid").is_err());
    }

    #[test]
    fn test_missing_fragment() {
        assert!(parse_invitation_link("https://simplex.chat/contact").is_err());
    }
}
