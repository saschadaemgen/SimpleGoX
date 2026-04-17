//! Double Ratchet state and operations for the Bob-side flow
//! (party that initiated the connection, receiving the first Alice message).
//!
//! Scope: first-message decrypt only. Skipped keys, out-of-order messages,
//! and KEM integration are deferred.

use crate::crypto::x3dh::X3dhBobResult;
use crate::smp_protocol::SmpError;

/// Minimal Bob-side Double Ratchet state.
#[derive(Debug, Clone)]
pub struct BobRatchet {
    /// Root Key (rcRK).
    pub root_key: [u8; 32],
    /// Our X448 sending ratchet private key (rcDHRs).
    pub our_dhrs_priv: [u8; 56],
    /// Receiving chain key (rcCKr), set after first DHRatchet step.
    pub receiving_chain_key: Option<[u8; 32]>,
    /// Sending chain key (rcCKs), set after first DHRatchet step.
    pub sending_chain_key: Option<[u8; 32]>,
    /// Current next-header-key for receiving direction (rcNHKr).
    pub next_header_key_receive: [u8; 32],
    /// Current next-header-key for sending direction (rcNHKs).
    pub next_header_key_send: [u8; 32],
    /// Associated data from X3DH (112 bytes: peer_pub1 || our_pub1).
    pub assoc_data: Vec<u8>,
    /// Message counter - sending.
    pub ns: u32,
    /// Message counter - receiving.
    pub nr: u32,
    /// Previous-chain length (PN).
    pub pn: u32,
}

/// Bob-side initialization from X3DH result and our persisted second private key.
///
/// Mirrors simplexmq Ratchet.hs:674-699 `initRcvRatchet`.
///
/// Key mapping (note the swap):
/// - `rcRK`    = `x3dh.root_key`
/// - `rcDHRs`  = `our_priv2` (passed in separately, not via X3DH result)
/// - `rcNHKs`  = `x3dh.receiving_next_header_key`  (swapped!)
/// - `rcNHKr`  = `x3dh.sending_header_key`         (swapped!)
/// - `rcSnd`   = None (no sending chain yet)
/// - `rcRcv`   = None (no receiving chain yet)
///
/// The swap reflects that `sndHK` is named from Alice's perspective
/// (her outgoing key) but represents our incoming direction.
pub fn init_bob_ratchet(x3dh: &X3dhBobResult, our_priv2: [u8; 56]) -> BobRatchet {
    BobRatchet {
        root_key: x3dh.root_key,
        our_dhrs_priv: our_priv2,
        receiving_chain_key: None,
        sending_chain_key: None,
        next_header_key_receive: x3dh.sending_header_key, // swap
        next_header_key_send: x3dh.receiving_next_header_key, // swap
        assoc_data: x3dh.assoc_data.clone(),
        ns: 0,
        nr: 0,
        pn: 0,
    }
}

/// Outer encrypted ratchet message wire layout.
///
/// Per simplexmq Ratchet.hs:772-787 `EncRatchetMessage`:
/// ```text
/// [Word16 BE enc_header_len] [enc_header bytes]
/// [16B auth_tag]
/// [Tail body]
/// ```
#[derive(Debug)]
pub struct EncRatchetMessage<'a> {
    /// Encrypted message header (EncMessageHeader bytes, to be decrypted
    /// separately with rcNHKr).
    pub enc_header: &'a [u8],
    /// GMAC tag over rcAD || enc_header || body.
    pub auth_tag: [u8; 16],
    /// Encrypted message body (AES-CTR ciphertext, AEAD-authenticated).
    pub body: &'a [u8],
}

/// Parse the outer EncRatchetMessage wire bytes.
pub fn parse_enc_ratchet_message(bytes: &[u8]) -> Result<EncRatchetMessage<'_>, SmpError> {
    if bytes.len() < 2 {
        return Err(SmpError::TooShort("EncRatchetMessage header length"));
    }
    let header_len = u16::from_be_bytes([bytes[0], bytes[1]]) as usize;
    let mut pos = 2;

    if pos + header_len > bytes.len() {
        return Err(SmpError::InvalidLength {
            declared: header_len,
            available: bytes.len() - pos,
        });
    }
    let enc_header = &bytes[pos..pos + header_len];
    pos += header_len;

    if pos + 16 > bytes.len() {
        return Err(SmpError::TooShort("EncRatchetMessage authTag"));
    }
    let mut auth_tag = [0u8; 16];
    auth_tag.copy_from_slice(&bytes[pos..pos + 16]);
    pos += 16;

    let body = &bytes[pos..];

    Ok(EncRatchetMessage {
        enc_header,
        auth_tag,
        body,
    })
}
