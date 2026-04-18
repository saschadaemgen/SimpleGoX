//! Double Ratchet state and operations for the Bob-side flow
//! (party that initiated the connection, receiving the first Alice message).
//!
//! Scope: first-message decrypt only. Skipped keys, out-of-order messages,
//! and KEM integration are deferred.

use crate::crypto::aead;
use crate::crypto::x3dh::{parse_x448_spki, X3dhBobResult};
use crate::smp_protocol::{unpad, SmpError};
use hkdf::Hkdf;
use sha2::Sha512;

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

    // Maybe KEM: '0' Nothing or '1' Just (ARKEMParams).
    //
    // If Just, the KEM is one of:
    //   'P' + Large(kem_pub)              RKParamsProposed
    //   'A' + Large(ct) + Large(kem_pub)  RKParamsAccepted
    //
    // For our Bob flow (pqEnc=false), the KEM is parsed and stored but
    // its shared secret is NOT folded into rootKdf - see
    // simplexmq Ratchet.hs:1092-1095 `pqRatchetStep` otherwise branch.
    if pos >= bytes.len() {
        return Err(SmpError::TooShort("MsgHeader Maybe KEM"));
    }
    let kem = match bytes[pos] {
        b'0' => {
            pos += 1;
            None
        }
        b'1' => {
            pos += 1;
            if pos >= bytes.len() {
                return Err(SmpError::TooShort("MsgHeader RKParams tag"));
            }
            let rk_tag = bytes[pos];
            pos += 1;
            match rk_tag {
                b'P' => {
                    // Large(kem_pub)
                    if pos + 2 > bytes.len() {
                        return Err(SmpError::TooShort("MsgHeader KEMPublicKey length"));
                    }
                    let kem_len =
                        u16::from_be_bytes([bytes[pos], bytes[pos + 1]]) as usize;
                    pos += 2;
                    if pos + kem_len > bytes.len() {
                        return Err(SmpError::InvalidLength {
                            declared: kem_len,
                            available: bytes.len() - pos,
                        });
                    }
                    let kem_pub = bytes[pos..pos + kem_len].to_vec();
                    pos += kem_len;
                    Some(kem_pub)
                }
                b'A' => {
                    // Large(ct) + Large(kem_pub): skip ct, keep kem_pub.
                    if pos + 2 > bytes.len() {
                        return Err(SmpError::TooShort("MsgHeader KEMCiphertext length"));
                    }
                    let ct_len = u16::from_be_bytes([bytes[pos], bytes[pos + 1]]) as usize;
                    pos += 2;
                    if pos + ct_len > bytes.len() {
                        return Err(SmpError::InvalidLength {
                            declared: ct_len,
                            available: bytes.len() - pos,
                        });
                    }
                    pos += ct_len;
                    if pos + 2 > bytes.len() {
                        return Err(SmpError::TooShort("MsgHeader KEMPublicKey length"));
                    }
                    let kem_len =
                        u16::from_be_bytes([bytes[pos], bytes[pos + 1]]) as usize;
                    pos += 2;
                    if pos + kem_len > bytes.len() {
                        return Err(SmpError::InvalidLength {
                            declared: kem_len,
                            available: bytes.len() - pos,
                        });
                    }
                    let kem_pub = bytes[pos..pos + kem_len].to_vec();
                    pos += kem_len;
                    Some(kem_pub)
                }
                other => {
                    return Err(SmpError::UnexpectedByte {
                        expected: b'P',
                        got: other,
                        ctx: "MsgHeader RKParams tag ('P' or 'A')",
                    });
                }
            }
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

/// Perform the DHRatchet step on receive and decrypt the first message body.
///
/// Mirrors simplexmq Ratchet.hs:1043-1070 (`ratchetStep`) for the receive
/// half followed by Ratchet.hs:1025-1031 (body decrypt with chainKdf).
///
/// Receive-only variant: we do NOT generate a new rcDHRs' or advance the
/// sending chain; for the first message that is handled when we send our
/// HELLO back (a later briefing).
pub fn dh_ratchet_and_decrypt_message(
    rc: &mut BobRatchet,
    header: &MessageHeader,
    enc_msg: &EncRatchetMessage<'_>,
) -> Result<Vec<u8>, SmpError> {
    // Parse peer's new X448 ratchet public key from SPKI.
    let peer_ratchet_pub = parse_x448_spki(&header.ratchet_pub_spki)?;

    // DH(our_dhrs_priv, peer_ratchet_pub) per Haskell `dh' k pk`.
    let dh_out = x448::x448(rc.our_dhrs_priv, peer_ratchet_pub).ok_or_else(|| {
        SmpError::Layer2DecryptFailed("ratchet DH produced low-order point".into())
    })?;

    // rootKdf (Ratchet.hs:1159-1166): (rcRK', rcCKr', rcNHKr') = hkdf3(rcRK, dh_out, "SimpleXRootRatchet")
    let (new_rk, new_ckr, new_nhkr) = hkdf3(&rc.root_key, &dh_out, b"SimpleXRootRatchet")?;

    rc.root_key = new_rk;
    rc.receiving_chain_key = Some(new_ckr);
    rc.next_header_key_receive = new_nhkr;
    rc.pn = rc.ns;
    rc.ns = 0;
    rc.nr = 0;

    // chainKdf on new_ckr (Ratchet.hs:1168-1172):
    // (ckr', mk, ivs) = hkdf3("", ckr, "SimpleXChainRatchet"); iv1 = ivs[..16]
    let (new_ckr2, message_key, ivs) = hkdf3(b"", &new_ckr, b"SimpleXChainRatchet")?;
    rc.receiving_chain_key = Some(new_ckr2);
    rc.nr += 1;

    let mut msg_iv = [0u8; 16];
    msg_iv.copy_from_slice(&ivs[..16]);

    // Body AAD per Ratchet.hs:1154-1157: `rcAD <> emHeader`.
    // `emHeader` here refers to the ENCODED EncRatchetMessage.emHeader field
    // (i.e. the inner EncMessageHeader bytes), WITHOUT the outer
    // Large-length prefix. Haskell passes encMsg.emHeader straight through.
    let mut aad = rc.assoc_data.clone();
    aad.extend_from_slice(enc_msg.enc_header);

    let plaintext_padded = aead::aes256_gcm_decrypt(
        &message_key,
        &msg_iv,
        &aad,
        enc_msg.body,
        &enc_msg.auth_tag,
    )
    .map_err(|e| SmpError::Layer2DecryptFailed(format!("message AEAD: {e}")))?;

    unpad(&plaintext_padded)
}

/// Minimal AgentConnInfoReply representation for the first-message path.
///
/// Full NonEmpty SMPQueueInfo parsing is deferred - for Briefing 035b we
/// just need to verify the 'D' tag and expose the remaining bytes so the
/// peer profile is visible as readable substring in the log.
#[derive(Debug)]
pub struct AgentConnInfoReply {
    /// Bytes after the 'D' tag: NonEmpty SMPQueueInfo followed by ConnInfo.
    pub body: Vec<u8>,
}

/// Verify the AgentMessage 'D' tag and return the remaining bytes.
///
/// Per simplexmq Protocol.hs:869-881, AgentMessage encodes with a single
/// ASCII tag followed by payload:
/// ```text
/// 'I' -> AgentConnInfo      (Tail ConnInfo)
/// 'D' -> AgentConnInfoReply (NonEmpty SMPQueueInfo, Tail ConnInfo)
/// 'R' -> AgentRatchetInfo   (Tail ByteString)
/// 'M' -> AgentMessage       (APrivHeader, AMessage)
/// ```
pub fn parse_agent_conn_info_reply(plaintext: &[u8]) -> Result<AgentConnInfoReply, SmpError> {
    if plaintext.is_empty() {
        return Err(SmpError::TooShort("AgentMessage tag"));
    }
    if plaintext[0] != b'D' {
        return Err(SmpError::UnexpectedByte {
            expected: b'D',
            got: plaintext[0],
            ctx: "AgentMessage tag (expected 'D' AgentConnInfoReply)",
        });
    }
    Ok(AgentConnInfoReply {
        body: plaintext[1..].to_vec(),
    })
}

/// Parsed AgentConnInfoReply with structured SMP queue list and JSON bytes.
///
/// This is the successor to [`parse_agent_conn_info_reply`]; the older raw
/// variant is kept for backwards-compatibility with the ASCII preview log.
#[derive(Debug)]
pub struct AgentConnInfoReplyParsed {
    pub smp_queues: Vec<crate::protocol::smp_queue_info::SmpQueueInfo>,
    pub conn_info_json: Vec<u8>,
}

/// Full parse of an AgentConnInfoReply payload:
/// - verifies the 'D' tag
/// - parses the NonEmpty SMPQueueInfo list
/// - exposes the remaining bytes (the x.info JSON envelope) as tail
pub fn parse_agent_conn_info_reply_full(
    plaintext: &[u8],
) -> Result<AgentConnInfoReplyParsed, SmpError> {
    if plaintext.is_empty() {
        return Err(SmpError::TooShort("AgentMessage tag"));
    }
    if plaintext[0] != b'D' {
        return Err(SmpError::UnexpectedByte {
            expected: b'D',
            got: plaintext[0],
            ctx: "AgentMessage tag (expected 'D' AgentConnInfoReply)",
        });
    }

    let after_tag = &plaintext[1..];
    let (smp_queues, consumed) =
        crate::protocol::smp_queue_info::parse_smp_queue_info_list(after_tag)?;

    Ok(AgentConnInfoReplyParsed {
        smp_queues,
        conn_info_json: after_tag[consumed..].to_vec(),
    })
}

/// HKDF-SHA512 with 96-byte output split into three 32-byte slices.
fn hkdf3(
    salt: &[u8],
    ikm: &[u8],
    info: &[u8],
) -> Result<([u8; 32], [u8; 32], [u8; 32]), SmpError> {
    let hk = Hkdf::<Sha512>::new(Some(salt), ikm);
    let mut okm = [0u8; 96];
    hk.expand(info, &mut okm)
        .map_err(|e| SmpError::Layer2DecryptFailed(format!("HKDF expand: {e}")))?;
    let mut a = [0u8; 32];
    let mut b = [0u8; 32];
    let mut c = [0u8; 32];
    a.copy_from_slice(&okm[0..32]);
    b.copy_from_slice(&okm[32..64]);
    c.copy_from_slice(&okm[64..96]);
    Ok((a, b, c))
}
