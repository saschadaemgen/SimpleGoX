//! Queue-level E2E encryption (NaCl crypto_box) - separate from Double Ratchet.
//! This wraps the agent message before sending to the SMP queue.

use crypto_box::{
    aead::{Aead, AeadCore, OsRng},
    SalsaBox, PublicKey, SecretKey,
};

/// E2E padded length for SMP client messages.
pub const E2E_PADDED_LENGTH: usize = 15904;

/// X25519 SPKI header for inline key in PubHeader.
const X25519_SPKI_HEADER: [u8; 12] = [
    0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x6e, 0x03, 0x21, 0x00,
];

/// Encrypt an agent message envelope with NaCl crypto_box for queue-level E2E.
///
/// For the first message (AgentConfirmation), `is_first_message = true` includes
/// our X25519 DH public key inline in the PubHeader so the peer can decrypt.
/// Subsequent messages use `is_first_message = false` (key already known).
///
/// Returns the complete SMP client_msg ready for the SEND command.
pub fn e2e_encrypt_agent_msg(
    body: &[u8],
    peer_dh_public: &[u8; 32],
    our_dh_private: &[u8; 32],
    our_dh_public: &[u8; 32],
    is_first_message: bool,
    priv_header: &[u8],
) -> Vec<u8> {
    // 1. ClientMessage = priv_header + body
    //    AgentConfirmation: priv_header = 'K' + snd_auth SPKI (45 bytes)
    //    HELLO/A_MSG:       priv_header = '_' (1 byte, PHEmpty)
    let mut client_message = Vec::with_capacity(priv_header.len() + body.len());
    client_message.extend_from_slice(priv_header);
    client_message.extend_from_slice(body);

    // 2. E2E pad to 16000 bytes: [Word16 BE len][content][0x23 padding]
    let mut padded = vec![0x00u8; E2E_PADDED_LENGTH];
    let content_len = client_message.len() as u16;
    padded[0] = (content_len >> 8) as u8;
    padded[1] = content_len as u8;
    padded[2..2 + client_message.len()].copy_from_slice(&client_message);

    // 3. NaCl crypto_box encrypt (XSalsa20-Poly1305)
    let peer_pub = PublicKey::from(*peer_dh_public);
    let our_priv = SecretKey::from(*our_dh_private);
    let nacl_box = SalsaBox::new(&peer_pub, &our_priv);
    let nonce = SalsaBox::generate_nonce(&mut OsRng);
    let encrypted = nacl_box
        .encrypt(&nonce, padded.as_ref())
        .expect("crypto_box encrypt failed");

    // 4. Build SMP client_msg with PubHeader
    let mut client_msg = Vec::new();
    client_msg.push(0x00);
    client_msg.push(0x04); // SMP client version 4

    if is_first_message {
        // '1' = Just + comma separator + X25519 SPKI key (44 bytes)
        client_msg.push(b'1');
        client_msg.push(b','); // Haskell smpEncoding separator
        client_msg.extend_from_slice(&X25519_SPKI_HEADER);
        client_msg.extend_from_slice(our_dh_public);
    } else {
        // '0' = Nothing (no comma)
        client_msg.push(b'0');
    }

    client_msg.extend_from_slice(nonce.as_ref());
    client_msg.extend_from_slice(&encrypted);

    client_msg
}

/// Decrypt an incoming SMP message - TWO layers of crypto_box.
///
/// Layer 3 (server-to-recipient): DH(server_dh_pub, our_rcv_dh_priv) + nonce=msgId
/// Layer 2 (per-queue E2E):       DH(peer_ephemeral_pub, our_rcv_dh_priv) + cmNonce
pub fn e2e_decrypt_incoming(
    msg_id: &[u8; 24],
    msg_body: &[u8],
    server_dh_public: &[u8; 32],
    our_rcv_dh_private: &[u8; 32],
) -> Result<Vec<u8>, String> {
    if msg_body.len() < 32 {
        return Err(format!("MSG body too short: {} bytes", msg_body.len()));
    }

    let our_priv = SecretKey::from(*our_rcv_dh_private);

    // === LAYER 3: Server-to-recipient decrypt ===
    // Nonce = msgId (already 24 bytes)
    let nonce3 = crypto_box::Nonce::from_slice(msg_id);
    let srv_pub = PublicKey::from(*server_dh_public);
    let srv_box = SalsaBox::new(&srv_pub, &our_priv);

    let rcv_msg_body = srv_box
        .decrypt(nonce3, msg_body)
        .map_err(|_| "Layer 3 (server) decrypt failed".to_string())?;

    tracing::debug!("Layer 3 decrypted: {} bytes", rcv_msg_body.len());

    // rcvMsgBody: [8B timestamp][1B flags][1B space][ClientMsgEnvelope...]
    if rcv_msg_body.len() < 10 {
        return Err(format!("rcvMsgBody too short after L3: {} bytes", rcv_msg_body.len()));
    }
    let envelope = &rcv_msg_body[10..]; // skip timestamp(8) + flags(1) + space(1)

    // === LAYER 2: Per-queue E2E decrypt ===
    // PubHeader: [2B version][Maybe DH key][24B nonce][encrypted...]
    if envelope.len() < 27 {
        return Err(format!("Envelope too short for L2: {} bytes", envelope.len()));
    }

    let mut pos = 0;
    let _version = u16::from_be_bytes([envelope[pos], envelope[pos + 1]]);
    pos += 2;

    // Maybe DH key: '1' = Just + SPKI, '0' = Nothing
    let peer_ephemeral: [u8; 32] = if envelope[pos] == b'1' {
        pos += 1;
        if envelope.len() < pos + 44 {
            return Err("Envelope truncated at inline DH key".into());
        }
        let mut raw = [0u8; 32];
        raw.copy_from_slice(&envelope[pos + 12..pos + 44]);
        pos += 44;
        raw
    } else if envelope[pos] == b'0' {
        return Err("No inline DH key in L2 (pre-shared mode not yet supported)".into());
    } else {
        return Err(format!("Unknown L2 DH tag: 0x{:02x}", envelope[pos]));
    };

    // cmNonce: 24 bytes
    if envelope.len() < pos + 24 {
        return Err("Envelope truncated at L2 nonce".into());
    }
    let cm_nonce = crypto_box::Nonce::from_slice(&envelope[pos..pos + 24]);
    pos += 24;

    let cm_enc_body = &envelope[pos..];

    // DH(peer_ephemeral, our_rcv_dh_private)
    let peer_pub = PublicKey::from(peer_ephemeral);
    let e2e_box = SalsaBox::new(&peer_pub, &our_priv);

    let padded = e2e_box
        .decrypt(cm_nonce, cm_enc_body)
        .map_err(|_| "Layer 2 (E2E) decrypt failed".to_string())?;

    tracing::debug!("Layer 2 decrypted: {} bytes", padded.len());

    // Unpad: [2B len][content][# padding]
    if padded.len() < 2 {
        return Err("Padded content too short".into());
    }
    let content_len = u16::from_be_bytes([padded[0], padded[1]]) as usize;
    if padded.len() < 2 + content_len {
        return Err(format!(
            "Content length {} exceeds padded size {}",
            content_len,
            padded.len()
        ));
    }

    Ok(padded[2..2 + content_len].to_vec())
}

/// Parse peer's AgentConfirmation to extract snd_auth key and E2E X448 keys.
pub struct PeerConfirmation {
    pub snd_auth_public: [u8; 32],
    pub e2e_key1: Vec<u8>, // Raw key bytes (56 for X448, 32 for X25519)
    pub e2e_key2: Vec<u8>,
}

pub fn parse_peer_confirmation(plaintext: &[u8]) -> Result<PeerConfirmation, String> {
    if plaintext.len() < 50 {
        return Err(format!("Confirmation too short: {} bytes", plaintext.len()));
    }

    let mut pos = 0;

    // PrivHeader: 'K' (0x4B) + Ed25519 SPKI (44B, no length prefix)
    if plaintext[pos] != b'K' {
        return Err(format!(
            "Expected PrivHeader 'K' (0x4B), got 0x{:02x}",
            plaintext[pos]
        ));
    }
    pos += 1;

    // Ed25519 SPKI: 12B header + 32B raw key = 44 bytes
    if plaintext.len() < pos + 44 {
        return Err("Truncated at snd_auth SPKI".into());
    }
    let mut snd_auth_public = [0u8; 32];
    snd_auth_public.copy_from_slice(&plaintext[pos + 12..pos + 44]);
    pos += 44;

    // AgentConfirmation body: [0x00][0x07] 'C' [0x31] [0x00][0x03]
    if plaintext.len() < pos + 6 {
        return Err("Truncated at confirmation header".into());
    }
    let agent_ver = u16::from_be_bytes([plaintext[pos], plaintext[pos + 1]]);
    pos += 2;
    if plaintext[pos] != b'C' {
        return Err(format!("Expected 'C' tag, got 0x{:02x}", plaintext[pos]));
    }
    pos += 1;
    if plaintext[pos] != 0x31 {
        return Err(format!("Expected Maybe Just (0x31), got 0x{:02x}", plaintext[pos]));
    }
    pos += 1;
    let e2e_ver = u16::from_be_bytes([plaintext[pos], plaintext[pos + 1]]);
    pos += 2;

    // key1: [1B length][SPKI bytes]
    if pos >= plaintext.len() {
        return Err("Truncated at key1 length".into());
    }
    let key1_len = plaintext[pos] as usize;
    pos += 1;
    if plaintext.len() < pos + key1_len {
        return Err(format!("Truncated at key1 data (need {key1_len})"));
    }
    // Skip SPKI header (12 bytes), take raw key
    let key1_raw = plaintext[pos + 12..pos + key1_len].to_vec();
    pos += key1_len;

    // key2: [1B length][SPKI bytes]
    if pos >= plaintext.len() {
        return Err("Truncated at key2 length".into());
    }
    let key2_len = plaintext[pos] as usize;
    pos += 1;
    if plaintext.len() < pos + key2_len {
        return Err(format!("Truncated at key2 data (need {key2_len})"));
    }
    let key2_raw = plaintext[pos + 12..pos + key2_len].to_vec();

    tracing::info!(
        "Peer confirmation: agent_v={agent_ver}, e2e_v={e2e_ver}, key1={}B, key2={}B, snd_auth={}...",
        key1_raw.len(),
        key2_raw.len(),
        hex::encode(&snd_auth_public[..4])
    );

    Ok(PeerConfirmation {
        snd_auth_public,
        e2e_key1: key1_raw,
        e2e_key2: key2_raw,
    })
}

/// Build AgentMsgEnvelope wrapping ratchet-encrypted bytes.
/// Format: [0x00][0x07 agentVersion] + 'M' + ratchet_bytes
pub fn build_agent_msg_envelope(ratchet_encrypted: &[u8]) -> Vec<u8> {
    let mut envelope = Vec::new();
    envelope.push(0x00);
    envelope.push(0x07); // agentVersion = 7
    envelope.push(b'M');
    envelope.extend_from_slice(ratchet_encrypted);
    envelope
}
