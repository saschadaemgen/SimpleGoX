//! SimpleX agent message wire format encoders.
//!
//! AgentConfirmation ('K' tag), HELLO ('H' tag), A_MSG ('M' tag).
//! Wire formats from SimpleGo C implementation (verified).

/// X25519 SPKI header for queue address DH key.
const X25519_SPKI_HEADER: [u8; 12] = [
    0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x6e, 0x03, 0x21, 0x00,
];

/// Encode the full AgentInvitation message (PrivHeader + body).
/// This is the complete ClientMessage content for CONF SEND.
/// Includes '_' PrivHeader, connReq URI, and ConnInfo JSON.
pub fn encode_agent_invitation(
    server_host: &str,
    server_port: u16,
    server_key_hash: &[u8; 32],
    our_snd_id: &[u8; 24],
    our_rcv_dh_public: &[u8; 32],
    our_key1_spki: &[u8; 68],
    our_key2_spki: &[u8; 68],
    display_name: &str,
) -> Vec<u8> {
    use base64::Engine;

    let mut buf = Vec::new();

    // PrivHeader = '_' (PHEmpty)
    buf.push(b'_');

    // agentVersion = 7 (Word16 BE)
    buf.extend_from_slice(&[0x00, 0x07]);

    // Tag 'I' (Initiator/Invitation)
    buf.push(b'I');

    // Build connReq URI
    let mut dh_spki = [0u8; 44];
    dh_spki[..12].copy_from_slice(&X25519_SPKI_HEADER);
    dh_spki[12..].copy_from_slice(our_rcv_dh_public);
    let dh_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&dh_spki);

    let key1_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(our_key1_spki);
    let key2_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(our_key2_spki);

    // kh_b64 WITH padding (URL_SAFE) - '=' gets encoded as %3D in the URI
    let kh_b64 = base64::engine::general_purpose::URL_SAFE.encode(server_key_hash);
    let snd_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(our_snd_id);

    tracing::debug!("key1_spki hex: {}", hex::encode(our_key1_spki));
    tracing::debug!("key1_b64: {}", key1_b64);
    tracing::debug!("key2_b64: {}", key2_b64);
    tracing::debug!("kh_b64: {}", kh_b64);
    tracing::debug!("snd_b64: {}", snd_b64);
    tracing::debug!("dh_b64: {}", dh_b64);

    let conn_req = format!(
        "simplex:/invitation#/?v=2-7&smp=smp%3A%2F%2F{}%40{}%3A{}%2F{}%23%2F%3Fv%3D1-4%26dh%3D{}&e2e=v%3D2-3%26x3dh%3D{}%2C{}",
        kh_b64.replace('=', "%3D"),
        server_host,
        server_port,
        snd_b64,
        dh_b64,
        key1_b64,
        key2_b64,
    );

    tracing::info!("Contact: built connReq URI: {}", conn_req);
    tracing::info!("Contact: connReq length: {} chars", conn_req.len());
    // Our URI format has no explicit invitation_id parameter. Peer identification
    // is keyed on the snd_id (queue_id) in the smp= section. The Desktop-side
    // `SEInvitationNotFound` log suggests the peer agent may expect a separate
    // id (e.g. from a LinkId-style short link); logged for Mausi's diagnosis.
    tracing::info!(
        "Contact: no explicit invitation_id in URI (queue_id={} is the only identifier)",
        snd_b64
    );
    let decoded_uri = conn_req
        .replace("%3A", ":")
        .replace("%2F", "/")
        .replace("%40", "@")
        .replace("%23", "#")
        .replace("%3F", "?")
        .replace("%3D", "=")
        .replace("%26", "&")
        .replace("%2C", ",");
    tracing::debug!("connReq URI decoded: {}", decoded_uri);

    // connReq URI with plain Word16 BE length prefix
    let conn_req_bytes = conn_req.as_bytes();
    let uri_len = conn_req_bytes.len() as u16;
    tracing::debug!(
        "URI bytes len: {} (0x{:04x})",
        conn_req_bytes.len(),
        conn_req_bytes.len()
    );
    tracing::debug!(
        "URI len prefix: {:02x} {:02x}",
        (uri_len >> 8) as u8,
        uri_len as u8
    );
    tracing::debug!("buf offset before URI prefix: {}", buf.len());
    buf.push((uri_len >> 8) as u8);
    buf.push(uri_len as u8);
    buf.extend_from_slice(conn_req_bytes);

    // ConnInfo JSON (Tail - no length prefix)
    let conn_info = serde_json::json!({
        "v": "1-16",
        "event": "x.info",
        "params": {
            "profile": {
                "displayName": display_name,
                "fullName": ""
            }
        }
    });
    buf.extend_from_slice(conn_info.to_string().as_bytes());

    buf
}

/// Encode AgentConfirmation body (without PrivHeader).
/// PrivHeader ('K' + snd_auth SPKI) is added by caller.
pub fn encode_agent_confirmation(
    our_key1_spki: &[u8],
    our_key2_spki: &[u8],
    conn_info_reply: &[u8],
) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(&[0x00, 0x07]); // agentVersion = 7
    buf.push(b'C'); // Confirmation tag
    buf.push(0x31); // Maybe Just
    buf.extend_from_slice(&[0x00, 0x03]); // e2eVersion = 3 (single uint16)
    buf.push(our_key1_spki.len() as u8);
    buf.extend_from_slice(our_key1_spki);
    buf.push(our_key2_spki.len() as u8);
    buf.extend_from_slice(our_key2_spki);
    buf.push(0x30); // KEM Nothing
    buf.extend_from_slice(conn_info_reply); // Tail
    buf
}

/// Encode AgentConnInfoReply - our queue address + profile.
pub fn encode_agent_conn_info_reply(
    server_host: &str,
    server_port: u16,
    server_key_hash: &[u8; 32],
    our_snd_id: &[u8; 24],
    our_rcv_dh_public: &[u8; 32],
    profile_name: &str,
) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.push(b'D');
    buf.extend_from_slice(&[0x00, 0x01]); // queue count = 1
    buf.extend_from_slice(&[0x00, 0x04]); // clientVersion = 4
    buf.push(1); // hostCount = 1
    let host_bytes = server_host.as_bytes();
    buf.push(host_bytes.len() as u8);
    buf.extend_from_slice(host_bytes);
    let port_str = server_port.to_string();
    buf.push(port_str.len() as u8);
    buf.extend_from_slice(port_str.as_bytes());
    buf.push(32);
    buf.extend_from_slice(server_key_hash);
    buf.push(24);
    buf.extend_from_slice(our_snd_id);
    buf.push(44);
    buf.extend_from_slice(&X25519_SPKI_HEADER);
    buf.extend_from_slice(our_rcv_dh_public);
    let conn_info = serde_json::json!({
        "v": "1-16",
        "event": "x.info",
        "params": {
            "profile": {
                "displayName": profile_name,
                "fullName": ""
            }
        }
    });
    buf.extend_from_slice(conn_info.to_string().as_bytes());
    buf
}

/// Encode HELLO message.
///
/// Wire: [0x00 no priv header][agent_ver 0x0001][smp_ver 0x0004]
///       [prevMsgHash 0x00][0x48 'H' tag][features JSON]
pub fn encode_hello(features_json: &[u8]) -> Vec<u8> {
    let mut buf = Vec::new();

    buf.push(0x00); // No PrivHeader
    buf.extend_from_slice(&[0x00, 0x01]); // agentVersion = 1
    buf.extend_from_slice(&[0x00, 0x04]); // smpVersion = 4
    buf.push(0x00); // prevMsgHash = Nothing (first message)
    buf.push(b'H'); // HELLO tag
    buf.extend_from_slice(features_json);

    buf
}

/// Encode A_MSG (chat message).
///
/// Wire: [0x4D 'M' tag][sndMsgId Int64 BE][prevHash][0x4D inner 'M'][ChatMessage JSON]
pub fn encode_chat_message(msg_id: u64, text: &str, prev_hash: Option<&[u8; 32]>) -> Vec<u8> {
    let mut buf = Vec::new();

    buf.push(b'M'); // Outer tag

    // sndMsgId (Int64 BE)
    buf.extend_from_slice(&msg_id.to_be_bytes());

    // prevHash
    match prev_hash {
        None => buf.push(0x00),
        Some(h) => {
            buf.push(32);
            buf.extend_from_slice(h);
        }
    }

    // Inner 'M' tag
    buf.push(b'M');

    // ChatMessage JSON
    let chat_msg = serde_json::json!({
        "v": "1",
        "event": "x.msg.new",
        "params": {
            "content": {
                "type": "text",
                "text": text
            }
        }
    });
    buf.extend_from_slice(chat_msg.to_string().as_bytes());

    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hello_encoding() {
        let hello = encode_hello(b"{}");
        assert_eq!(hello[0], 0x00); // no priv header
        assert_eq!(hello[1..3], [0x00, 0x01]); // agent version 1
        assert_eq!(hello[3..5], [0x00, 0x04]); // smp version 4
        assert_eq!(hello[5], 0x00); // no prev hash
        assert_eq!(hello[6], b'H'); // HELLO tag
        assert_eq!(&hello[7..], b"{}");
    }

    #[test]
    fn test_chat_message_encoding() {
        let msg = encode_chat_message(1, "test", None);
        assert_eq!(msg[0], b'M'); // outer tag
        assert_eq!(u64::from_be_bytes(msg[1..9].try_into().unwrap()), 1); // msg_id
        assert_eq!(msg[9], 0x00); // no prev hash
        assert_eq!(msg[10], b'M'); // inner tag
        let json: serde_json::Value = serde_json::from_slice(&msg[11..]).unwrap();
        assert_eq!(json["event"], "x.msg.new");
    }

    #[test]
    fn test_agent_invitation_structure() {
        let kh = [0u8; 32];
        let sid = [1u8; 24];
        let dh = [2u8; 32];
        let k1 = [3u8; 68];
        let k2 = [4u8; 68];
        let inv = encode_agent_invitation("smp.test.dev", 5223, &kh, &sid, &dh, &k1, &k2, "Test");
        assert_eq!(inv[0], b'_'); // PHEmpty
        assert_eq!(inv[1..3], [0x00, 0x07]); // agentVersion 7
        assert_eq!(inv[3], b'I'); // Invitation tag
                                  // connReq URI follows with Word16 BE length prefix
        let uri_len = u16::from_be_bytes([inv[4], inv[5]]) as usize;
        assert!(uri_len > 100);
        let uri = std::str::from_utf8(&inv[6..6 + uri_len]).unwrap();
        assert!(uri.starts_with("simplex:/invitation#"));
    }
}
