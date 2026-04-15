//! SimpleX agent message wire format encoders.
//!
//! AgentConfirmation ('K' tag), HELLO ('H' tag), A_MSG ('M' tag).
//! Wire formats from SimpleGo C implementation (verified).

/// Encode AgentConfirmation with sender key.
///
/// Wire: [0x4B 'K' tag][agent_ver 0x0007][0x43 'C' tag]
///       [Maybe E2E params][sender auth key][Tail: encrypted ConnInfo]
pub fn encode_agent_confirmation(
    our_key1_spki: &[u8],
    our_key2_spki: &[u8],
    snd_auth_public: &[u8; 32],
    conn_info_bytes: &[u8],
) -> Vec<u8> {
    let mut buf = Vec::new();

    // PrivHeader: PHConfirmation 'K' (0x4B)
    buf.push(0x4B);

    // agentVersion = 7 (Word16 BE)
    buf.extend_from_slice(&[0x00, 0x07]);

    // 'C' tag
    buf.push(b'C');

    // e2eEncryption_ = Just (0x31)
    buf.push(0x31);

    // version range (Word16 min=3, Word16 max=3)
    buf.extend_from_slice(&[0x00, 0x03]);
    buf.extend_from_slice(&[0x00, 0x03]);

    // key1: 1-byte length prefix + SPKI
    buf.push(our_key1_spki.len() as u8);
    buf.extend_from_slice(our_key1_spki);

    // key2: 1-byte length prefix + SPKI
    buf.push(our_key2_spki.len() as u8);
    buf.extend_from_slice(our_key2_spki);

    // KEM Nothing = '0' (0x30)
    buf.push(0x30);

    // sndAuthPublicKey: 1-byte length + Ed25519 public key
    buf.push(32);
    buf.extend_from_slice(snd_auth_public);

    // Tail: ConnInfo (rest of buffer, no length prefix)
    buf.extend_from_slice(conn_info_bytes);

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
    fn test_agent_confirmation_structure() {
        let key1 = [0u8; 44]; // fake SPKI
        let key2 = [1u8; 44];
        let auth = [2u8; 32];
        let info = b"test_conn_info";
        let conf = encode_agent_confirmation(&key1, &key2, &auth, info);
        assert_eq!(conf[0], 0x4B); // K tag
        assert_eq!(conf[1..3], [0x00, 0x07]); // version 7
        assert_eq!(conf[3], b'C'); // C tag
        assert_eq!(conf[4], 0x31); // Just
    }
}
