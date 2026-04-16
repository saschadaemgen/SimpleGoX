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
    let mut padded = vec![0x23u8; E2E_PADDED_LENGTH];
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
        // '1' = Just - inline X25519 SPKI key (44 bytes)
        // Peer needs this to derive the shared secret for crypto_box
        client_msg.push(b'1');
        client_msg.extend_from_slice(&X25519_SPKI_HEADER);
        client_msg.extend_from_slice(our_dh_public);
    } else {
        // '0' = Nothing - key already known from first message
        client_msg.push(b'0');
    }

    client_msg.extend_from_slice(nonce.as_ref());
    client_msg.extend_from_slice(&encrypted);

    client_msg
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
