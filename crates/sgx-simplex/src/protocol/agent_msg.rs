//! SimpleX agent message wire format encoders.
//!
//! AgentConfirmation ('K' tag), HELLO ('H' tag), A_MSG ('M' tag).
//! Wire formats from SimpleGo C implementation (verified).

use serde::Serialize;

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

    // Inner SMP URI ends with `&q=m` (URL-encoded %26q%3Dm) to signal that
    // our queue is a QMMessaging queue (not a contact queue). Required by
    // the receiving client per GoChat invitation.ts:83-88. Without this the
    // peer's agent raises SEInvitationNotFound on acceptContact.
    let conn_req = format!(
        "simplex:/invitation#/?v=2-7&smp=smp%3A%2F%2F{}%40{}%3A{}%2F{}%23%2F%3Fv%3D1-4%26dh%3D{}%26q%3Dm&e2e=v%3D2-3%26x3dh%3D{}%2C{}",
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
/// Encode an AgentConnInfo payload for the invitation handshake response.
///
/// This is the inner ratchet plaintext for the AgentConfirmation we send
/// back to the peer's reply queue after processing their AgentConfirmation.
/// It is NOT wrapped in an APrivHeader - AgentConnInfo has no sndMsgId or
/// prevMsgHash (those only apply to AgentMessage variant tag 'M').
///
/// Wire format per simplexmq Agent/Protocol.hs AgentConnInfo encoding and
/// GoChat connection.ts:1142-1145:
///
/// ```text
/// 'I'                   AgentConnInfo tag (0x49)
/// [Tail profile_json]   raw bytes to end of buffer, no length prefix
/// ```
///
/// The profile JSON follows the x.info envelope format:
/// ```json
/// {
///   "v":"1-17",
///   "event":"x.info",
///   "params":{"profile":{"displayName":"...","fullName":"","preferences":{}}}
/// }
/// ```
pub fn encode_agent_conn_info(display_name: &str) -> Vec<u8> {
    encode_agent_conn_info_full(display_name, "", "")
}

/// Full x.info profile mirroring GoChat connection.ts:1123-1140. Desktop
/// renders the contact with a display name only when the profile carries
/// both `displayName` AND a populated `preferences` tree; a minimal
/// profile with an empty preferences object is accepted structurally but
/// surfaces as "Your contact" without a name.
///
/// Preferences match SimpleX Desktop defaults today; these will become
/// user-configurable in a later briefing.
pub fn encode_agent_conn_info_full(
    display_name: &str,
    full_name: &str,
    bio: &str,
) -> Vec<u8> {
    let mut profile = serde_json::json!({
        "displayName": display_name,
        "fullName": full_name,
        "preferences": {
            "calls":         { "allow": "no"  },
            "files":         { "allow": "no"  },
            "voice":         { "allow": "no"  },
            "reactions":     { "allow": "yes" },
            "fullDelete":    { "allow": "no"  },
            "timedMessages": { "allow": "yes" }
        }
    });
    if !bio.is_empty() {
        profile["bio"] = serde_json::Value::String(bio.to_string());
    }

    let envelope = serde_json::json!({
        "v": "1-17",
        "event": "x.info",
        "params": { "profile": profile }
    });
    let json_bytes = envelope.to_string().into_bytes();
    let mut buf = Vec::with_capacity(1 + json_bytes.len());
    buf.push(b'I');
    buf.extend_from_slice(&json_bytes);
    buf
}

/// Encode an AgentMessage HELLO in the correct SimpleX wire format.
///
/// Per simplexmq Agent/Protocol.hs:779 `type MsgHash = ByteString`, the
/// prevMsgHash is serialised with a 1-byte length prefix followed by the
/// hash bytes. The initial value stored per connection (`last_snd_msg_hash`
/// default, Store/SQLite/Migrations/M20220101_initial.hs:26) is the empty
/// string, so for the very first outgoing message `prev_msg_hash` is `&[]`
/// (encoded on the wire as the single byte `0x00`).
///
/// Wire:
/// ```text
/// 'M'                               AgentMessage APrivHeader AMessage tag
/// [Int64 BE sndMsgId]               APrivHeader.sndMsgId
/// [lenPrefix][prevMsgHash bytes]    APrivHeader.prevMsgHash (ByteString)
/// 'H'                               AMessage HELLO tag
/// ```
pub fn encode_agent_message_hello(snd_msg_id: u64, prev_msg_hash: &[u8]) -> Vec<u8> {
    assert!(
        prev_msg_hash.len() <= u8::MAX as usize,
        "prev_msg_hash too long for 1-byte length prefix"
    );
    let mut buf = Vec::with_capacity(1 + 8 + 1 + prev_msg_hash.len() + 1);
    buf.push(b'M');
    buf.extend_from_slice(&snd_msg_id.to_be_bytes());
    buf.push(prev_msg_hash.len() as u8);
    buf.extend_from_slice(prev_msg_hash);
    buf.push(b'H');
    buf
}

/// Decoded AgentMessage content returned by [`parse_agent_message_content`].
///
/// Mirror of simplexmq Agent/Protocol.hs:859-881 variants that we care about
/// in the current scope. QADD/QKEY/QUSE/QTEST/EREADY/A_QCONT are reported as
/// `Other` for logging without further parsing.
#[derive(Debug)]
pub enum AgentMessageContent {
    /// AMessage HELLO (tag 'H').
    Hello {
        snd_msg_id: u64,
        prev_msg_hash: Vec<u8>,
    },
    /// AMessage A_MSG with text body (tag 'M').
    Message {
        snd_msg_id: u64,
        prev_msg_hash: Vec<u8>,
        body: Vec<u8>,
    },
    /// AMessage A_RCVD delivery receipts (tag 'V'). Content not parsed yet.
    Receipt {
        snd_msg_id: u64,
        prev_msg_hash: Vec<u8>,
        raw: Vec<u8>,
    },
    /// Other AMessage variant; raw post-APrivHeader bytes preserved for
    /// diagnostic logging.
    Other {
        snd_msg_id: u64,
        prev_msg_hash: Vec<u8>,
        tag: u8,
        raw: Vec<u8>,
    },
}

/// Parse an AgentMessage plaintext (the ratchet-decrypted bytes) and dispatch
/// on the AMessage tag.
///
/// Expects the wire layout produced by simplexmq Agent/Protocol.hs AgentMessage:
/// ```text
/// 'M' [Int64 BE sndMsgId] [lenPrefix][prevMsgHash] <AMessage>
/// ```
/// The outer 'M' is the `AgentMessage APrivHeader AMessage` wrapper tag.
pub fn parse_agent_message_content(plaintext: &[u8]) -> Result<AgentMessageContent, crate::smp_protocol::SmpError> {
    use crate::smp_protocol::SmpError;

    if plaintext.is_empty() {
        return Err(SmpError::TooShort("AgentMessage outer tag"));
    }
    if plaintext[0] != b'M' {
        return Err(SmpError::UnexpectedByte {
            expected: b'M',
            got: plaintext[0],
            ctx: "AgentMessage outer tag (expected 'M' AgentMessage wrapper)",
        });
    }
    let mut pos = 1;

    if pos + 8 > plaintext.len() {
        return Err(SmpError::TooShort("AgentMessage sndMsgId"));
    }
    let snd_msg_id =
        u64::from_be_bytes(plaintext[pos..pos + 8].try_into().unwrap());
    pos += 8;

    if pos >= plaintext.len() {
        return Err(SmpError::TooShort("AgentMessage prevMsgHash length"));
    }
    let hash_len = plaintext[pos] as usize;
    pos += 1;
    if pos + hash_len > plaintext.len() {
        return Err(SmpError::InvalidLength {
            declared: hash_len,
            available: plaintext.len() - pos,
        });
    }
    let prev_msg_hash = plaintext[pos..pos + hash_len].to_vec();
    pos += hash_len;

    if pos >= plaintext.len() {
        return Err(SmpError::TooShort("AgentMessage AMessage tag"));
    }
    let tag = plaintext[pos];
    pos += 1;
    let remainder = &plaintext[pos..];

    Ok(match tag {
        b'H' => AgentMessageContent::Hello {
            snd_msg_id,
            prev_msg_hash,
        },
        b'M' => AgentMessageContent::Message {
            snd_msg_id,
            prev_msg_hash,
            body: remainder.to_vec(),
        },
        b'V' => AgentMessageContent::Receipt {
            snd_msg_id,
            prev_msg_hash,
            raw: remainder.to_vec(),
        },
        other => AgentMessageContent::Other {
            snd_msg_id,
            prev_msg_hash,
            tag: other,
            raw: remainder.to_vec(),
        },
    })
}

/// Chat message envelope serialised as the A_MSG body.
///
/// Matches the goChatX outbound wire format from
/// `smp-web/src/connection.ts::sendChatMessage` (lines 1218-1233):
///
/// ```json
/// {"event":"x.msg.new","params":{"content":{"text":"...","type":"text"}}}
/// ```
///
/// No `v` protocol version, no `msgId` - the peer correlates via the
/// APrivHeader `sndMsgId` in the enclosing AgentMessage. Verified against
/// goChatX production code AND its `__tests__/send-message.test.ts` for
/// exact field set.
///
/// Field order (event, params, then inside content: text, type) mirrors
/// goChatX's `JSON.stringify` output byte-for-byte; Rust's serde honours
/// struct declaration order so we produce the same on-wire bytes.
#[derive(Serialize, Debug)]
pub struct ChatMessageEnvelope<'a> {
    pub event: &'a str,
    pub params: ChatMessageParams<'a>,
}

#[derive(Serialize, Debug)]
pub struct ChatMessageParams<'a> {
    pub content: ChatContent<'a>,
}

#[derive(Serialize, Debug)]
pub struct ChatContent<'a> {
    pub text: &'a str,
    #[serde(rename = "type")]
    pub content_type: &'a str,
}

/// Build the canonical chat text envelope JSON string that the peer's
/// `parse.event === "x.msg.new"` branch expects.
///
/// Infallible: `serde_json` cannot fail for these borrowed string fields
/// (no user-controlled keys, no custom Serialize impls), so we unwrap.
pub fn build_chat_text_envelope(text: &str) -> String {
    let env = ChatMessageEnvelope {
        event: "x.msg.new",
        params: ChatMessageParams {
            content: ChatContent {
                text,
                content_type: "text",
            },
        },
    };
    serde_json::to_string(&env)
        .expect("ChatMessageEnvelope serialisation is infallible for &str fields")
}

/// Encode an AgentMessage with a text body (A_MSG) in SimpleX wire format.
///
/// Per simplexmq Agent/Protocol.hs:1074 `A_MSG body -> smpEncode (A_MSG_, Tail body)`
/// the body is encoded as `Tail` (raw bytes until end of buffer), with no
/// length prefix. The outer APrivHeader carries the snd id and prev hash.
///
/// Wire:
/// ```text
/// 'M'                               AgentMessage APrivHeader AMessage tag
/// [Int64 BE sndMsgId]               APrivHeader.sndMsgId
/// [lenPrefix][prevMsgHash bytes]    APrivHeader.prevMsgHash (ByteString)
/// 'M'                               AMessage A_MSG tag
/// [body bytes]                      Tail body (raw, no length prefix)
/// ```
pub fn encode_agent_message_text(
    snd_msg_id: u64,
    prev_msg_hash: &[u8],
    body: &[u8],
) -> Vec<u8> {
    assert!(
        prev_msg_hash.len() <= u8::MAX as usize,
        "prev_msg_hash too long for 1-byte length prefix"
    );
    let mut buf = Vec::with_capacity(1 + 8 + 1 + prev_msg_hash.len() + 1 + body.len());
    buf.push(b'M');
    buf.extend_from_slice(&snd_msg_id.to_be_bytes());
    buf.push(prev_msg_hash.len() as u8);
    buf.extend_from_slice(prev_msg_hash);
    buf.push(b'M');
    buf.extend_from_slice(body);
    buf
}

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

    /// Exact byte layout for the very first outgoing chat message
    /// (empty `prev_msg_hash` since no previous send has happened).
    #[test]
    fn test_agent_message_text_first_message_byte_layout() {
        let encoded = encode_agent_message_text(1, &[], b"Hi");
        let expected: Vec<u8> = vec![
            b'M', // outer AgentMessage wrapper tag
            0, 0, 0, 0, 0, 0, 0, 1, // sndMsgId = 1, Int64 BE
            0,    // prevMsgHash length = 0 (empty ByteString)
            b'M', // inner AMessage A_MSG variant tag
            b'H', b'i', // body, raw Tail
        ];
        assert_eq!(encoded, expected);
    }

    /// Exact byte layout for a follow-up chat message with a 32-byte SHA-256
    /// prev_msg_hash. Verifies the 1-byte length prefix is exactly 0x20 = 32
    /// and that the hash bytes are emitted unchanged.
    #[test]
    fn test_agent_message_text_with_prev_hash_byte_layout() {
        let prev = [0xABu8; 32];
        let encoded = encode_agent_message_text(0x0102_0304_0506_0708, &prev, b"x");
        // 1 (tag) + 8 (sndMsgId) + 1 (len) + 32 (hash) + 1 (inner tag) + 1 (body)
        assert_eq!(encoded.len(), 44);
        assert_eq!(encoded[0], b'M');
        assert_eq!(&encoded[1..9], &[0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08]);
        assert_eq!(encoded[9], 0x20); // hashLen = 32
        assert_eq!(&encoded[10..42], &prev);
        assert_eq!(encoded[42], b'M');
        assert_eq!(encoded[43], b'x');
    }

    /// Round-trip: anything `encode_agent_message_text` produces must parse
    /// back via `parse_agent_message_content` to the matching
    /// `AgentMessageContent::Message` with byte-for-byte field equality.
    /// This is the single contract that has to hold for the Phase 3 send
    /// path to be decryptable as a `Message` by an unmodified SimpleX peer.
    #[test]
    fn test_agent_message_text_roundtrip_via_parser() {
        let cases: Vec<(u64, Vec<u8>, &[u8])> = vec![
            (1, vec![], b"hallo"),
            (42, vec![0xAB; 32], b"Hello, SimpleX!"),
            (
                u64::MAX,
                (0..255u8).collect(),
                "emoji \u{1F389} and umlauts \u{00E4}\u{00F6}\u{00FC}".as_bytes(),
            ),
            (1234, vec![0x00; 32], &[]),
        ];

        for (snd_msg_id, prev_msg_hash, body) in cases {
            let encoded = encode_agent_message_text(snd_msg_id, &prev_msg_hash, body);
            let parsed = parse_agent_message_content(&encoded)
                .expect("encoded message must parse back");
            match parsed {
                AgentMessageContent::Message {
                    snd_msg_id: id,
                    prev_msg_hash: hash,
                    body: parsed_body,
                } => {
                    assert_eq!(id, snd_msg_id, "sndMsgId mismatch");
                    assert_eq!(hash, prev_msg_hash, "prevMsgHash mismatch");
                    assert_eq!(parsed_body, body, "body mismatch");
                }
                other => panic!("expected Message variant, got {other:?}"),
            }
        }
    }

    /// Canonical outbound shape must match goChatX byte-for-byte (aside
    /// from object field order tolerance on the peer side). No `v`, no
    /// `msgId`, just `event` and `params.content.{text, type}`.
    #[test]
    fn test_chat_text_envelope_shape_matches_gochat() {
        let json = build_chat_text_envelope("hallo");
        // Exact string match on the minimal shape.
        assert_eq!(
            json,
            r#"{"event":"x.msg.new","params":{"content":{"text":"hallo","type":"text"}}}"#
        );
    }

    /// Quotes and backslashes in the user text must be JSON-escaped, not
    /// passed through raw. Protects against the "I forgot to use
    /// serde_json and used format!() instead" class of injection bug.
    #[test]
    fn test_chat_text_envelope_escapes_special_chars() {
        let json = build_chat_text_envelope(r#"he said "hi" and \ escaped"#);
        assert!(
            json.contains(r#""text":"he said \"hi\" and \\ escaped""#),
            "quotes and backslash must be escaped, got: {json}"
        );
        // The envelope is still parseable as JSON: round-trip through
        // serde_json::Value to confirm.
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["event"], "x.msg.new");
        assert_eq!(parsed["params"]["content"]["type"], "text");
        assert_eq!(
            parsed["params"]["content"]["text"],
            r#"he said "hi" and \ escaped"#
        );
    }

    /// Exact byte shape of the outbound HELLO AgentMessage, matching
    /// GoChat `smp-web/src/connection.ts:1187-1193 sendHello`. No body
    /// after the 'H' inner tag. Verifies the first-HELLO case (empty
    /// prev_msg_hash, sndMsgId=1) which is the ONLY case we hit in
    /// practice (HELLO is sent exactly once per contact direction).
    #[test]
    fn test_agent_message_hello_first_wire_shape() {
        let hello = encode_agent_message_hello(1, &[]);
        let expected: Vec<u8> = vec![
            b'M',                              // 0x4D outer AgentMessage
            0, 0, 0, 0, 0, 0, 0, 1,            // sndMsgId=1, Int64 BE
            0,                                 // prevMsgHash length = 0
            b'H',                              // 0x48 inner HELLO tag
        ];
        assert_eq!(hello, expected);
        assert_eq!(hello.len(), 11);
    }

    /// A HELLO with a non-empty prev_msg_hash (not our current wire
    /// pattern but must still round-trip through the inbound parser as
    /// a Hello variant).
    #[test]
    fn test_agent_message_hello_roundtrip_via_parser() {
        let prev = [0xCDu8; 32];
        let hello = encode_agent_message_hello(42, &prev);
        let parsed = parse_agent_message_content(&hello).expect("parse");
        match parsed {
            AgentMessageContent::Hello { snd_msg_id, prev_msg_hash } => {
                assert_eq!(snd_msg_id, 42);
                assert_eq!(prev_msg_hash, prev);
            }
            other => panic!("expected Hello, got {other:?}"),
        }
    }

    /// Unicode / multibyte UTF-8 must survive serde_json's default output
    /// (non-ASCII is left as UTF-8 unless a caller sets escape options,
    /// which we do not).
    #[test]
    fn test_chat_text_envelope_preserves_unicode() {
        let text = "hallo \u{1F319} mein Prinz \u{1F451} \u{00E4}\u{00F6}\u{00FC}\u{00DF}";
        let json = build_chat_text_envelope(text);
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["params"]["content"]["text"], text);
    }
}
