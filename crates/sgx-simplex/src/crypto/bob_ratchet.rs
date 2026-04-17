//! Double Ratchet state and operations for the Bob-side flow
//! (party that initiated the connection, receiving the first Alice message).
//!
//! Scope: first-message decrypt only. Skipped keys, out-of-order messages,
//! and KEM integration are deferred.

use crate::crypto::aead;
use crate::crypto::x3dh::X3dhBobResult;
use crate::smp_protocol::{unpad, SmpError};

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

/// Parsed plaintext MsgHeader.
///
/// Per simplexmq Ratchet.hs:703-711 and encoding at Ratchet.hs:727-740.
#[derive(Debug, Clone)]
pub struct MessageHeader {
    pub max_version: u16,
    /// X448 SPKI (68 bytes) - the peer's new ratchet public key (msgDHRs).
    pub ratchet_pub_spki: Vec<u8>,
    /// KEM params present; None when Maybe KEM is Nothing.
    pub kem: Option<Vec<u8>>,
    /// Previous-chain message count.
    pub pn: u32,
    /// Sending counter in current chain.
    pub ns: u32,
}

/// Decrypt the inner EncMessageHeader with rcNHKr (receive next header key),
/// then parse the resulting MsgHeader.
///
/// Wire layout of enc_header bytes (from EncRatchetMessage):
/// ```text
/// [Word16 BE ehVersion] [16B ehIV] [16B ehAuthTag]
/// [Word16 BE ehBody_len] [ehBody encrypted]
/// ```
///
/// After AES-256-GCM decrypt of ehBody (AAD = rcAD), the plaintext is
/// SMP-padded (`[Word16 BE content_len][content][# padding]`) to
/// `paddedHeaderLen` (2310 bytes for v>=3 with PQSupportOn, 88 otherwise).
/// The content then parses into MsgHeader.
pub fn decrypt_message_header(
    enc_header: &[u8],
    rc_nhkr: &[u8; 32],
    assoc_data: &[u8],
) -> Result<MessageHeader, SmpError> {
    // Minimum: 2 (version) + 16 (iv) + 16 (tag) + 2 (ehBody_len) = 36.
    if enc_header.len() < 36 {
        return Err(SmpError::TooShort("EncMessageHeader"));
    }
    let version = u16::from_be_bytes([enc_header[0], enc_header[1]]);
    let mut pos = 2;

    let mut iv = [0u8; 16];
    iv.copy_from_slice(&enc_header[pos..pos + 16]);
    pos += 16;

    let mut tag = [0u8; 16];
    tag.copy_from_slice(&enc_header[pos..pos + 16]);
    pos += 16;

    let body_len = u16::from_be_bytes([enc_header[pos], enc_header[pos + 1]]) as usize;
    pos += 2;

    if pos + body_len > enc_header.len() {
        return Err(SmpError::InvalidLength {
            declared: body_len,
            available: enc_header.len() - pos,
        });
    }
    let enc_body = &enc_header[pos..pos + body_len];

    let plaintext_padded = aead::aes256_gcm_decrypt(rc_nhkr, &iv, assoc_data, enc_body, &tag)
        .map_err(|e| SmpError::Layer2DecryptFailed(format!("header AEAD: {e}")))?;

    let plaintext = unpad(&plaintext_padded)?;
    parse_message_header(&plaintext, version)
}

/// Parse the plaintext MsgHeader bytes into structured fields.
///
/// For v>=3 with PQSupportOn (our case):
/// `[Word16 maxVersion] [X448 SPKI 68B] [Maybe KEM] [Word32 PN] [Word32 Ns]`
fn parse_message_header(bytes: &[u8], _version: u16) -> Result<MessageHeader, SmpError> {
    if bytes.len() < 2 {
        return Err(SmpError::TooShort("MsgHeader version"));
    }
    let max_version = u16::from_be_bytes([bytes[0], bytes[1]]);
    let mut pos = 2;

    // X448 SPKI: Haskell smpEncode for a PublicKey wraps the 68-byte SPKI
    // with a shortString length prefix (0x44 = 68), so we expect that here.
    if pos >= bytes.len() {
        return Err(SmpError::TooShort("MsgHeader SPKI length"));
    }
    if bytes[pos] == 0x44 {
        pos += 1;
    }
    if pos + 68 > bytes.len() {
        return Err(SmpError::TooShort("MsgHeader X448 SPKI body"));
    }
    let ratchet_pub_spki = bytes[pos..pos + 68].to_vec();
    // Sanity check the X448 OID (1.3.101.111 = 2b 65 6f).
    if ratchet_pub_spki[6] != 0x2b || ratchet_pub_spki[7] != 0x65 || ratchet_pub_spki[8] != 0x6f {
        return Err(SmpError::UnexpectedByte {
            expected: 0x2b,
            got: ratchet_pub_spki[6],
            ctx: "MsgHeader X448 SPKI OID",
        });
    }
    pos += 68;

    // Maybe KEM: '0' Nothing or '1' Just ... (we expect Nothing in our flow).
    if pos >= bytes.len() {
        return Err(SmpError::TooShort("MsgHeader Maybe KEM"));
    }
    let kem = match bytes[pos] {
        b'0' => {
            pos += 1;
            None
        }
        b'1' => {
            return Err(SmpError::UnexpectedByte {
                expected: b'0',
                got: b'1',
                ctx: "MsgHeader Maybe KEM (unexpected Just - KEM decode not implemented)",
            });
        }
        other => {
            return Err(SmpError::UnexpectedByte {
                expected: b'0',
                got: other,
                ctx: "MsgHeader Maybe KEM tag",
            });
        }
    };

    if pos + 8 > bytes.len() {
        return Err(SmpError::TooShort("MsgHeader PN/Ns"));
    }
    let pn = u32::from_be_bytes(bytes[pos..pos + 4].try_into().unwrap());
    pos += 4;
    let ns = u32::from_be_bytes(bytes[pos..pos + 4].try_into().unwrap());

    Ok(MessageHeader {
        max_version,
        ratchet_pub_spki,
        kem,
        pn,
        ns,
    })
}
