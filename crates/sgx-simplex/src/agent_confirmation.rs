//! AgentConfirmation wire format parser (agentVersion = 7).
//!
//! Wire format:
//! ```text
//! [ClientMessage Header]        '_' PHEmpty  or  'K' + [0x2C][44B SPKI]
//! [agentVersion Word16 BE]      must be 7
//! 'C'                           AgentConfirmation tag
//! [Maybe e2eEncryption]         '0' Nothing  or  '1' Just
//!   if Just:
//!   [e2eVersion Word16 BE]      typically 3
//!   [0x44][X448 SPKI 68B]       ratchet key 1
//!   [0x44][X448 SPKI 68B]       ratchet key 2
//!   [Maybe KEM]                 '0' Nothing  or  '1' Just
//!     if Just:
//!     [Word16 BE kem_len][kem_bytes]   SNTRUP761 public key, 1158 bytes
//! [encConnInfo tail]            remaining bytes - encrypted ConnInfo blob
//! ```
//!
//! CRITICAL: The KEM length uses a DIFFERENT encoding than regular Large strings.
//! Regular Large: `0xFF + Word16 BE`. KEM: `Word16 BE` directly, no 0xFF prefix.
//! See Season 9 PR #70/#71 bug history.

use crate::smp_protocol::SmpError;

/// Parsed AgentConfirmation content (agentVersion 7, with optional PQ KEM).
#[derive(Debug, Clone)]
pub struct AgentConfirmation {
    pub agent_version: u16,
    pub e2e_version: Option<u16>,
    /// X448 SPKI (68B, OID 2b 65 6f) - first ratchet key.
    pub ratchet_key1_spki: Option<Vec<u8>>,
    /// X448 SPKI (68B, OID 2b 65 6f) - second ratchet key.
    pub ratchet_key2_spki: Option<Vec<u8>>,
    /// SNTRUP761 public key, 1158B. None if KEM Nothing.
    pub kem_public: Option<Vec<u8>>,
    /// Encrypted connection info blob - tail after parser.
    pub enc_conn_info: Vec<u8>,
}

/// Parse AgentConfirmation from a Layer 2 unpadded plaintext.
pub fn parse_agent_confirmation(plaintext: &[u8]) -> Result<AgentConfirmation, SmpError> {
    if plaintext.is_empty() {
        return Err(SmpError::TooShort("AgentConfirmation empty"));
    }

    let mut off = 0;

    // 1. ClientMessage header tag
    let ph_tag = plaintext[off];
    off += 1;
    match ph_tag {
        b'_' => {
            // PHEmpty - nothing else
        }
        b'K' => {
            // PHConfirmation: shortString length prefix + X25519 SPKI
            if off >= plaintext.len() {
                return Err(SmpError::TooShort("PHConfirmation length"));
            }
            let len = plaintext[off] as usize;
            if len != 44 {
                return Err(SmpError::UnexpectedByte {
                    expected: 44,
                    got: plaintext[off],
                    ctx: "PHConfirmation SPKI length",
                });
            }
            off += 1;
            if off + len > plaintext.len() {
                return Err(SmpError::TooShort("PHConfirmation SPKI body"));
            }
            off += len;
        }
        _ => {
            return Err(SmpError::UnexpectedByte {
                expected: b'_',
                got: ph_tag,
                ctx: "ClientMessage header tag",
            });
        }
    }

    // 2. agentVersion (Word16 BE) - must be 7
    if off + 2 > plaintext.len() {
        return Err(SmpError::TooShort("agentVersion"));
    }
    let agent_version = u16::from_be_bytes([plaintext[off], plaintext[off + 1]]);
    off += 2;
    if agent_version != 7 {
        return Err(SmpError::WrongAgentVersion {
            expected: 7,
            got: agent_version,
        });
    }

    // 3. Tag 'C'
    if off >= plaintext.len() {
        return Err(SmpError::TooShort("AgentConfirmation tag"));
    }
    if plaintext[off] != b'C' {
        return Err(SmpError::UnexpectedByte {
            expected: b'C',
            got: plaintext[off],
            ctx: "AgentConfirmation tag",
        });
    }
    off += 1;

    // 4. Maybe e2eEncryption
    if off >= plaintext.len() {
        return Err(SmpError::TooShort("Maybe e2eEncryption"));
    }
    let (e2e_version, key1, key2, kem) = match plaintext[off] {
        b'0' => {
            off += 1;
            (None, None, None, None)
        }
        b'1' => {
            off += 1;

            // e2eVersion Word16 BE
            if off + 2 > plaintext.len() {
                return Err(SmpError::TooShort("e2eVersion"));
            }
            let v = u16::from_be_bytes([plaintext[off], plaintext[off + 1]]);
            off += 2;

            // key1: [len=0x44][68B SPKI]
            if off >= plaintext.len() {
                return Err(SmpError::TooShort("ratchet key1 length"));
            }
            if plaintext[off] != 0x44 {
                return Err(SmpError::UnexpectedByte {
                    expected: 0x44,
                    got: plaintext[off],
                    ctx: "ratchet key1 length",
                });
            }
            off += 1;
            if off + 68 > plaintext.len() {
                return Err(SmpError::TooShort("ratchet key1 body"));
            }
            let k1 = plaintext[off..off + 68].to_vec();
            off += 68;

            // key2: [len=0x44][68B SPKI]
            if off >= plaintext.len() {
                return Err(SmpError::TooShort("ratchet key2 length"));
            }
            if plaintext[off] != 0x44 {
                return Err(SmpError::UnexpectedByte {
                    expected: 0x44,
                    got: plaintext[off],
                    ctx: "ratchet key2 length",
                });
            }
            off += 1;
            if off + 68 > plaintext.len() {
                return Err(SmpError::TooShort("ratchet key2 body"));
            }
            let k2 = plaintext[off..off + 68].to_vec();
            off += 68;

            // Maybe KEM (ARKEMParams, verified against simplexmq
            // Crypto/Ratchet.hs:208-214: RKParamsProposed or RKParamsAccepted tag
            // 'P' + Large(kem_pub)            [RKParamsProposed]
            // 'A' + Large(ciphertext) + Large(kem_pub)  [RKParamsAccepted]
            // where Large = Word16 BE length + bytes (Encoding.hs:137-143).
            if off >= plaintext.len() {
                return Err(SmpError::TooShort("Maybe KEM"));
            }
            let kem = match plaintext[off] {
                b'0' => {
                    off += 1;
                    None
                }
                b'1' => {
                    off += 1;
                    // RKParams tag: 'P' (Proposed) or 'A' (Accepted).
                    if off >= plaintext.len() {
                        return Err(SmpError::TooShort("RKParams tag"));
                    }
                    let rk_tag = plaintext[off];
                    off += 1;
                    match rk_tag {
                        b'P' => {
                            // Large KEMPublicKey: Word16 BE len + bytes.
                            if off + 2 > plaintext.len() {
                                return Err(SmpError::TooShort("KEMPublicKey length"));
                            }
                            let kem_len =
                                u16::from_be_bytes([plaintext[off], plaintext[off + 1]])
                                    as usize;
                            off += 2;
                            if off + kem_len > plaintext.len() {
                                return Err(SmpError::InvalidLength {
                                    declared: kem_len,
                                    available: plaintext.len() - off,
                                });
                            }
                            let kem_bytes = plaintext[off..off + kem_len].to_vec();
                            off += kem_len;
                            Some(kem_bytes)
                        }
                        b'A' => {
                            // Large ciphertext + Large kem_pub. We only keep the public key
                            // for now; ciphertext is irrelevant until we implement ratchet step.
                            if off + 2 > plaintext.len() {
                                return Err(SmpError::TooShort("KEMCiphertext length"));
                            }
                            let ct_len =
                                u16::from_be_bytes([plaintext[off], plaintext[off + 1]])
                                    as usize;
                            off += 2;
                            if off + ct_len > plaintext.len() {
                                return Err(SmpError::InvalidLength {
                                    declared: ct_len,
                                    available: plaintext.len() - off,
                                });
                            }
                            off += ct_len;
                            if off + 2 > plaintext.len() {
                                return Err(SmpError::TooShort("KEMPublicKey length"));
                            }
                            let kem_len =
                                u16::from_be_bytes([plaintext[off], plaintext[off + 1]])
                                    as usize;
                            off += 2;
                            if off + kem_len > plaintext.len() {
                                return Err(SmpError::InvalidLength {
                                    declared: kem_len,
                                    available: plaintext.len() - off,
                                });
                            }
                            let kem_bytes = plaintext[off..off + kem_len].to_vec();
                            off += kem_len;
                            Some(kem_bytes)
                        }
                        other => {
                            return Err(SmpError::UnexpectedByte {
                                expected: b'P',
                                got: other,
                                ctx: "RKParams tag ('P' or 'A')",
                            });
                        }
                    }
                }
                other => {
                    return Err(SmpError::UnexpectedByte {
                        expected: b'0',
                        got: other,
                        ctx: "Maybe KEM",
                    });
                }
            };

            (Some(v), Some(k1), Some(k2), kem)
        }
        other => {
            return Err(SmpError::UnexpectedByte {
                expected: b'0',
                got: other,
                ctx: "Maybe e2eEncryption",
            });
        }
    };

    // 5. encConnInfo tail
    let enc_conn_info = plaintext[off..].to_vec();

    Ok(AgentConfirmation {
        agent_version,
        e2e_version,
        ratchet_key1_spki: key1,
        ratchet_key2_spki: key2,
        kem_public: kem,
        enc_conn_info,
    })
}
