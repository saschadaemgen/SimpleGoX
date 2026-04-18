//! Double Ratchet state and operations for the Bob-side flow
//! (party that initiated the connection, receiving the first Alice message).
//!
//! Scope: first-message decrypt only. Skipped keys, out-of-order messages,
//! and KEM integration are deferred.

use crate::crypto::aead;
use crate::crypto::keys::X448Keypair;
use crate::crypto::x3dh::{parse_x448_spki, X3dhBobResult};
use crate::smp_protocol::{pad, unpad, SmpError};
use hkdf::Hkdf;
use rand::RngCore;
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
    /// Current header key for the active receiving chain (rcRcv.rcHKr).
    ///
    /// Used to decrypt incoming EncMessageHeader bytes in the SameRatchet
    /// branch per Ratchet.hs:1142-1150. Set during the first DH ratchet
    /// step (Ratchet.hs:1065 `rcSnd = Just SndRatchet {rcHKr = rcNHKr}`)
    /// to the pre-advance value of `next_header_key_receive`.
    pub receiving_header_key: Option<[u8; 32]>,
    /// Next-header-key for receiving direction (rcNHKr).
    ///
    /// Consumed by the AdvanceRatchet branch when a new peer DH ratchet
    /// step arrives. Initialised from the X3DH sending_header_key (swap).
    pub next_header_key_receive: [u8; 32],
    /// Current next-header-key for sending direction (rcNHKs).
    ///
    /// For the first outgoing message after the DH ratchet step this also
    /// serves as rcSnd.rcHKs (the header key used to encrypt the outgoing
    /// EncMessageHeader). The Haskell reference splits these into two
    /// fields; we collapse them while only a single send is performed
    /// before the next DH step.
    pub next_header_key_send: [u8; 32],
    /// Associated data from X3DH (112 bytes: peer_pub1 || our_pub1).
    pub assoc_data: Vec<u8>,
    /// Message counter - sending.
    pub ns: u32,
    /// Message counter - receiving.
    pub nr: u32,
    /// Previous-chain length (PN).
    pub pn: u32,
    /// New X448 ratchet private key (rcDHRs') generated during the first
    /// DH ratchet step. Used for future DH operations and published to the
    /// peer as msgDHRs in outgoing MsgHeaders.
    pub our_new_ratchet_priv: Option<[u8; 56]>,
    /// Raw 56-byte public component of `our_new_ratchet_priv`.
    pub our_new_ratchet_pub: Option<[u8; 56]>,
    /// SHA-256 of the last outgoing AgentMessage plaintext, used as
    /// `prevMsgHash` in APrivHeader of the next send.
    ///
    /// Empty for the very first send (matches the SQLite schema default
    /// `last_snd_msg_hash BLOB NOT NULL DEFAULT x''`).
    pub last_snd_msg_hash: Vec<u8>,
    /// Counter for outgoing APrivHeader.sndMsgId, monotonically increasing.
    /// Starts at 1 (HELLO), next regular message is 2.
    pub next_snd_msg_id: u64,
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
        receiving_header_key: None,
        next_header_key_receive: x3dh.sending_header_key, // swap
        next_header_key_send: x3dh.receiving_next_header_key, // swap
        assoc_data: x3dh.assoc_data.clone(),
        ns: 0,
        nr: 0,
        pn: 0,
        our_new_ratchet_priv: None,
        our_new_ratchet_pub: None,
        last_snd_msg_hash: Vec::new(),
        next_snd_msg_id: 1,
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

/// Encode a plaintext MsgHeader for outgoing ratchet messages.
///
/// Mirrors [`parse_message_header`]. Wire layout (v>=3, KEM always Nothing
/// in our flow):
///
/// ```text
/// [Word16 BE maxVersion]
/// [0x44 shortString prefix][68B X448 SPKI]   (= smpEncode ByteString for the pub key)
/// ['0' Maybe KEM = Nothing]
/// [Word32 BE PN]
/// [Word32 BE Ns]
/// ```
///
/// Total: 80 bytes. `spki` is the full 68-byte X448 SPKI (12B ASN.1 header
/// + 56B raw key) as produced by `X448Keypair::encode_spki`.
pub fn encode_msg_header(max_version: u16, spki: &[u8; 68], pn: u32, ns: u32) -> Vec<u8> {
    let mut buf = Vec::with_capacity(2 + 1 + 68 + 1 + 4 + 4);
    buf.extend_from_slice(&max_version.to_be_bytes());
    buf.push(0x44); // smpEncode ByteString length prefix = 68
    buf.extend_from_slice(spki);
    buf.push(b'0'); // Maybe KEM = Nothing
    buf.extend_from_slice(&pn.to_be_bytes());
    buf.extend_from_slice(&ns.to_be_bytes());
    buf
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
/// Which ratchet step applies to an incoming message, per simplexmq
/// Ratchet.hs:982 `data RatchetStep = AdvanceRatchet | SameRatchet`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RatchetStep {
    /// Message belongs to the active receiving chain - decrypt with `rcHKr`,
    /// run chainKdf on the existing `rcCKr`, no new DH.
    SameRatchet,
    /// Message announces a new peer DH key - decrypt with `rcNHKr`,
    /// perform a full DH ratchet step before decrypting the body.
    AdvanceRatchet,
}

/// Try to decrypt an incoming EncMessageHeader and classify the ratchet step,
/// per simplexmq Ratchet.hs:1142-1150 `decryptRcHeader`.
///
/// First attempts SameRatchet using `rc.receiving_header_key` (set on the
/// last DH advance). Falls back to AdvanceRatchet using
/// `rc.next_header_key_receive` if the first attempt fails with an AEAD
/// authentication error.
pub fn decrypt_header_and_classify_step(
    enc_header: &[u8],
    rc: &BobRatchet,
) -> Result<(MessageHeader, RatchetStep), SmpError> {
    if let Some(rc_hkr) = rc.receiving_header_key {
        if let Ok(header) = decrypt_message_header(enc_header, &rc_hkr, &rc.assoc_data) {
            return Ok((header, RatchetStep::SameRatchet));
        }
    }
    let header = decrypt_message_header(
        enc_header,
        &rc.next_header_key_receive,
        &rc.assoc_data,
    )?;
    Ok((header, RatchetStep::AdvanceRatchet))
}

/// Decrypt an incoming message body on the current receiving chain without
/// performing a DH ratchet step (SameRatchet branch per Ratchet.hs:1015-1016
/// and :1025-1031). Advances `receiving_chain_key` via chainKdf and
/// increments `nr`.
pub fn same_ratchet_decrypt_body(
    rc: &mut BobRatchet,
    enc_msg: &EncRatchetMessage<'_>,
) -> Result<Vec<u8>, SmpError> {
    let rc_ckr = rc.receiving_chain_key.ok_or_else(|| {
        SmpError::Layer2DecryptFailed("SameRatchet: receiving_chain_key missing".into())
    })?;

    let (new_ckr, mk, body_iv, _header_iv) = chain_kdf(&rc_ckr)?;
    rc.receiving_chain_key = Some(new_ckr);
    rc.nr += 1;

    let mut aad = rc.assoc_data.clone();
    aad.extend_from_slice(enc_msg.enc_header);

    let plaintext_padded = aead::aes256_gcm_decrypt(
        &mk,
        &body_iv,
        &aad,
        enc_msg.body,
        &enc_msg.auth_tag,
    )
    .map_err(|e| SmpError::Layer2DecryptFailed(format!("SameRatchet body AEAD: {e}")))?;

    unpad(&plaintext_padded)
}

/// Full ratchet step per simplexmq Ratchet.hs:1043-1070: generates a new
/// X448 keypair, runs two chained rootKdfs (first for the receive chain with
/// the existing rcDHRs, second for the send chain with the new rcDHRs'),
/// populates both receive and send state, then decrypts the message body.
pub fn dh_ratchet_and_decrypt_message(
    rc: &mut BobRatchet,
    header: &MessageHeader,
    enc_msg: &EncRatchetMessage<'_>,
) -> Result<Vec<u8>, SmpError> {
    // Parse peer's new X448 ratchet public key from SPKI.
    let peer_ratchet_pub = parse_x448_spki(&header.ratchet_pub_spki)?;

    // First DH: DH(our_dhrs_priv, peer_ratchet_pub) per Haskell `dh' k pk`.
    let dh_out = x448::x448(rc.our_dhrs_priv, peer_ratchet_pub).ok_or_else(|| {
        SmpError::Layer2DecryptFailed("ratchet DH produced low-order point".into())
    })?;

    // First rootKdf (Ratchet.hs:1049):
    //   (rcRK', rcCKr', rcNHKr') = rootKdf rcRK msgDHRs rcDHRs kemSS
    let (new_rk, new_ckr, new_nhkr) = hkdf3(&rc.root_key, &dh_out, b"SimpleXRootRatchet")?;

    // Generate a fresh rcDHRs' for the sending side.
    let new_ratchet = X448Keypair::generate();

    // Second DH: DH(rcDHRs', peer_ratchet_pub) feeds the sending rootKdf.
    let dh_out2 = x448::x448(new_ratchet.private, peer_ratchet_pub).ok_or_else(|| {
        SmpError::Layer2DecryptFailed("ratchet DH' produced low-order point".into())
    })?;

    // Second rootKdf (Ratchet.hs:1051):
    //   (rcRK'', rcCKs', rcNHKs') = rootKdf rcRK' msgDHRs rcDHRs' kemSS'
    //
    // We only consume (rcRK'', rcCKs'); rcNHKs' (the next sending header key)
    // is not tracked here because the first HELLO uses the pre-step rcNHKs
    // as rcHKs. A future commit introducing a second DH step will add that.
    let (new_rk2, new_cks, _new_nhks) = hkdf3(&new_rk, &dh_out2, b"SimpleXRootRatchet")?;

    // Promote the pre-advance rcNHKr to rcHKr for the new receiving chain,
    // mirroring Ratchet.hs:1065 `rcRcv = Just RcvRatchet {rcHKr = rcNHKr}`
    // where the RHS is the value BEFORE the rcNHKr := rcNHKr' assignment on
    // line 1070. Must happen before we overwrite next_header_key_receive.
    rc.receiving_header_key = Some(rc.next_header_key_receive);

    rc.root_key = new_rk2;
    rc.receiving_chain_key = Some(new_ckr);
    rc.next_header_key_receive = new_nhkr;
    rc.sending_chain_key = Some(new_cks);
    rc.our_new_ratchet_priv = Some(new_ratchet.private);
    rc.our_new_ratchet_pub = Some(new_ratchet.public);
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

/// Advance a ratchet chain key and derive message keys.
///
/// Per simplexmq Ratchet.hs:1168-1172 `chainKdf`:
///
/// ```text
/// (ck', mk, ivs) = hkdf3 "" ck "SimpleXChainRatchet"   -- 96 bytes total
/// iv1 = ivs[..16]   (body AEAD IV)
/// iv2 = ivs[16..32] (header AEAD IV)
/// ```
///
/// Returns `(new_chain_key, message_key, body_iv, header_iv)`.
pub fn chain_kdf(
    chain_key: &[u8; 32],
) -> Result<([u8; 32], [u8; 32], [u8; 16], [u8; 16]), SmpError> {
    let hk = Hkdf::<Sha512>::new(Some(&[]), chain_key);
    let mut okm = [0u8; 96];
    hk.expand(b"SimpleXChainRatchet", &mut okm)
        .map_err(|e| SmpError::Layer2DecryptFailed(format!("chainKdf: {e}")))?;

    let mut new_ck = [0u8; 32];
    let mut mk = [0u8; 32];
    let mut body_iv = [0u8; 16];
    let mut header_iv = [0u8; 16];
    new_ck.copy_from_slice(&okm[0..32]);
    mk.copy_from_slice(&okm[32..64]);
    body_iv.copy_from_slice(&okm[64..80]);
    header_iv.copy_from_slice(&okm[80..96]);

    Ok((new_ck, mk, body_iv, header_iv))
}

/// Padded length of the plaintext MsgHeader for E2E v>=3 with PQSupportOn,
/// per simplexmq Ratchet.hs:716-719 `paddedHeaderLen`.
pub const PADDED_HEADER_LEN: usize = 2310;

/// Padded length of the ratchet-encrypted AgentMessage body per
/// simplexmq Agent/Protocol.hs:332-336 `e2eEncAgentMsgLength`
/// with PQSupportOn + v >= pqdrSMPAgentVersion (agent v5).
pub const PADDED_AGENT_MSG_LEN: usize = 13618;

/// Encrypt a MsgHeader plaintext and produce the EncMessageHeader wire bytes.
///
/// Steps per simplexmq Ratchet.hs:917-920 and :750-752:
///
/// 1. Pad plaintext to `PADDED_HEADER_LEN` with Word16-length-prefixed `#`.
/// 2. Generate a random 16-byte header IV (ehIV).
/// 3. AES-256-GCM encrypt with `rc_nhks` as key, `rc_ad` as AAD.
/// 4. Assemble wire bytes:
///    `[Word16 BE ehVersion][16B ehIV][16B ehAuthTag][Word16 BE body_len][body]`
pub fn encrypt_message_header(
    msg_header_plain: &[u8],
    rc_nhks: &[u8; 32],
    rc_ad: &[u8],
    eh_version: u16,
) -> Result<Vec<u8>, SmpError> {
    let padded = pad(msg_header_plain, PADDED_HEADER_LEN)?;

    let mut eh_iv = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut eh_iv);

    let (tag, ct) = aead::aes256_gcm_encrypt(rc_nhks, &eh_iv, rc_ad, &padded);

    let mut out = Vec::with_capacity(2 + 16 + 16 + 2 + ct.len());
    out.extend_from_slice(&eh_version.to_be_bytes());
    out.extend_from_slice(&eh_iv);
    out.extend_from_slice(&tag);
    out.extend_from_slice(&(ct.len() as u16).to_be_bytes());
    out.extend_from_slice(&ct);

    Ok(out)
}

/// Encrypt the message body and assemble the full EncRatchetMessage wire bytes.
///
/// Per simplexmq Ratchet.hs:970-975 `rcEncryptMsg`:
///
/// 1. Pad the plaintext to `padded_msg_len` (Word16 length prefix + `#` fill).
/// 2. AES-256-GCM encrypt with the message key, body IV from `chainKdf`,
///    AAD = `rcAD || enc_message_header_wire`.
/// 3. Assemble per Ratchet.hs:779-781 `encodeEncRatchetMessage`:
///    `[Word16 BE header_len][header bytes][16B authTag][Tail body]`
pub fn encrypt_and_assemble_ratchet_message(
    agent_msg_plaintext: &[u8],
    enc_message_header_wire: &[u8],
    message_key: &[u8; 32],
    body_iv: &[u8; 16],
    rc_ad: &[u8],
    padded_msg_len: usize,
) -> Result<Vec<u8>, SmpError> {
    let padded = pad(agent_msg_plaintext, padded_msg_len)?;

    let mut aad = Vec::with_capacity(rc_ad.len() + enc_message_header_wire.len());
    aad.extend_from_slice(rc_ad);
    aad.extend_from_slice(enc_message_header_wire);

    let (body_tag, body_ct) =
        aead::aes256_gcm_encrypt(message_key, body_iv, &aad, &padded);

    let mut out =
        Vec::with_capacity(2 + enc_message_header_wire.len() + 16 + body_ct.len());
    out.extend_from_slice(&(enc_message_header_wire.len() as u16).to_be_bytes());
    out.extend_from_slice(enc_message_header_wire);
    out.extend_from_slice(&body_tag);
    out.extend_from_slice(&body_ct);

    Ok(out)
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
