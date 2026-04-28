//! SMPQueueInfo wire format parser.
//!
//! Appears in AgentConnInfoReply (tag 'D') as a NonEmpty list, typically
//! representing the peer's reply queue address.
//!
//! Wire format per simplexmq Protocol.hs:1306-1320 for clientVersion >= 4
//! (shortLinksSMPClientVersion):
//! ```text
//! [Word16 BE clientVersion]
//! [NonEmpty TransportHost:
//!    [1B count]
//!    per count: [shortString host: 1B len + ASCII]]
//! [shortString port: 1B len + ASCII digits]
//! [shortString keyHash: 1B len=32 + 32B]
//! [shortString senderId: 1B len=24 + 24B]
//! [X25519 SPKI dhPublicKey: 1B len=44 + 44B]
//! [Optional QueueMode: empty OR 1 byte ('M' messaging | 'C' contact)]
//! ```
//!
//! Note: `Optional QueueMode` is NOT Maybe-prefixed - it is either absent
//! or exactly one byte. We try-parse it heuristically.

use crate::smp_protocol::SmpError;
use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

/// Parsed single SMP queue info.
///
/// Briefing 044g.1b: gained `Serialize, Deserialize, PartialEq` derives so
/// the post-handshake peer reply queue can be postcard-encoded into a
/// `connections.peer_queue_blob` column. The two `[u8; 32]` fields use
/// `serde-big-array` because serde's default array support tops at N=32
/// (`[u8; 24]` for `queue_id` works without it).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SmpQueueInfo {
    pub smp_client_version: u16,
    /// First host in the NonEmpty host list; additional hosts are stored raw.
    pub server_host: String,
    /// Additional hosts if the queue had more than one (rare).
    pub extra_hosts: Vec<String>,
    pub server_port: String,
    #[serde(with = "BigArray")]
    pub server_fingerprint: [u8; 32],
    pub queue_id: [u8; 24],
    /// X25519 raw public key (32B, extracted from 44B SPKI).
    #[serde(with = "BigArray")]
    pub sender_dh_public: [u8; 32],
    /// Optional queue mode: 'M' = Messaging, 'C' = Contact.
    pub queue_mode: Option<char>,
}

/// Parse a NonEmpty list of SMPQueueInfo from bytes.
///
/// Returns the parsed queues and the number of bytes consumed (so the caller
/// can slice the trailing JSON-ConnInfo bytes).
pub fn parse_smp_queue_info_list(bytes: &[u8]) -> Result<(Vec<SmpQueueInfo>, usize), SmpError> {
    if bytes.is_empty() {
        return Err(SmpError::TooShort("SMPQueueInfo list count"));
    }
    let count = bytes[0] as usize;
    if count == 0 {
        return Err(SmpError::InvalidLength {
            declared: 0,
            available: bytes.len() - 1,
        });
    }
    let mut pos = 1;
    let mut queues = Vec::with_capacity(count);
    for i in 0..count {
        let (queue, consumed) = parse_single(&bytes[pos..], i)?;
        queues.push(queue);
        pos += consumed;
    }
    Ok((queues, pos))
}

fn parse_single(bytes: &[u8], idx: usize) -> Result<(SmpQueueInfo, usize), SmpError> {
    let mut pos = 0;

    // Word16 BE clientVersion
    if pos + 2 > bytes.len() {
        return Err(SmpError::TooShort("SMPQueueInfo version"));
    }
    let smp_client_version = u16::from_be_bytes([bytes[pos], bytes[pos + 1]]);
    pos += 2;

    // NonEmpty TransportHost
    if pos >= bytes.len() {
        return Err(SmpError::TooShort("SMPQueueInfo host count"));
    }
    let host_count = bytes[pos] as usize;
    pos += 1;
    if host_count == 0 {
        return Err(SmpError::InvalidLength {
            declared: 0,
            available: bytes.len() - pos,
        });
    }

    let mut hosts: Vec<String> = Vec::with_capacity(host_count);
    for _ in 0..host_count {
        if pos >= bytes.len() {
            return Err(SmpError::TooShort("SMPQueueInfo host length"));
        }
        let host_len = bytes[pos] as usize;
        pos += 1;
        if pos + host_len > bytes.len() {
            return Err(SmpError::InvalidLength {
                declared: host_len,
                available: bytes.len() - pos,
            });
        }
        let host = String::from_utf8_lossy(&bytes[pos..pos + host_len]).into_owned();
        pos += host_len;
        hosts.push(host);
    }

    // shortString port (ASCII digits)
    if pos >= bytes.len() {
        return Err(SmpError::TooShort("SMPQueueInfo port length"));
    }
    let port_len = bytes[pos] as usize;
    pos += 1;
    if pos + port_len > bytes.len() {
        return Err(SmpError::InvalidLength {
            declared: port_len,
            available: bytes.len() - pos,
        });
    }
    let server_port = String::from_utf8_lossy(&bytes[pos..pos + port_len]).into_owned();
    pos += port_len;

    // shortString keyHash (fingerprint)
    if pos >= bytes.len() {
        return Err(SmpError::TooShort("SMPQueueInfo keyHash length"));
    }
    let kh_len = bytes[pos] as usize;
    pos += 1;
    if kh_len != 32 {
        return Err(SmpError::UnexpectedByte {
            expected: 32,
            got: kh_len as u8,
            ctx: "SMPQueueInfo keyHash length (expected 32)",
        });
    }
    if pos + 32 > bytes.len() {
        return Err(SmpError::TooShort("SMPQueueInfo keyHash body"));
    }
    let mut server_fingerprint = [0u8; 32];
    server_fingerprint.copy_from_slice(&bytes[pos..pos + 32]);
    pos += 32;

    // shortString senderId (24B queue_id)
    if pos >= bytes.len() {
        return Err(SmpError::TooShort("SMPQueueInfo senderId length"));
    }
    let sid_len = bytes[pos] as usize;
    pos += 1;
    if sid_len != 24 {
        return Err(SmpError::UnexpectedByte {
            expected: 24,
            got: sid_len as u8,
            ctx: "SMPQueueInfo senderId length (expected 24)",
        });
    }
    if pos + 24 > bytes.len() {
        return Err(SmpError::TooShort("SMPQueueInfo senderId body"));
    }
    let mut queue_id = [0u8; 24];
    queue_id.copy_from_slice(&bytes[pos..pos + 24]);
    pos += 24;

    // X25519 SPKI dhPublicKey (44B = 12B header + 32B raw)
    if pos >= bytes.len() {
        return Err(SmpError::TooShort("SMPQueueInfo dhPublicKey length"));
    }
    let dh_len = bytes[pos] as usize;
    pos += 1;
    if dh_len != 44 {
        return Err(SmpError::UnexpectedByte {
            expected: 44,
            got: dh_len as u8,
            ctx: "SMPQueueInfo dhPublicKey SPKI length (expected 44)",
        });
    }
    if pos + 44 > bytes.len() {
        return Err(SmpError::TooShort("SMPQueueInfo dhPublicKey body"));
    }
    // Verify X25519 OID (1.3.101.110 = 2b 65 6e).
    if bytes[pos + 6] != 0x2b || bytes[pos + 7] != 0x65 || bytes[pos + 8] != 0x6e {
        return Err(SmpError::UnexpectedByte {
            expected: 0x2b,
            got: bytes[pos + 6],
            ctx: "SMPQueueInfo X25519 SPKI OID",
        });
    }
    let mut sender_dh_public = [0u8; 32];
    sender_dh_public.copy_from_slice(&bytes[pos + 12..pos + 44]);
    pos += 44;

    // Optional QueueMode (not Maybe-tagged, either empty or 1 byte 'M'/'C').
    // Strategic logging for now: we heuristically try-parse.
    let queue_mode = if pos < bytes.len() {
        match bytes[pos] {
            b'M' => {
                tracing::debug!("SMPQueueInfo[{}]: QueueMode present = 'M'", idx);
                pos += 1;
                Some('M')
            }
            b'C' => {
                tracing::debug!("SMPQueueInfo[{}]: QueueMode present = 'C'", idx);
                pos += 1;
                Some('C')
            }
            other => {
                tracing::debug!(
                    "SMPQueueInfo[{}]: byte after dhPublicKey = 0x{:02x} ('{}') - treating as start of next structure (no QueueMode here)",
                    idx,
                    other,
                    if other.is_ascii_graphic() { other as char } else { '.' }
                );
                None
            }
        }
    } else {
        tracing::debug!(
            "SMPQueueInfo[{}]: no bytes after dhPublicKey (end of buffer)",
            idx
        );
        None
    };

    Ok((
        SmpQueueInfo {
            smp_client_version,
            server_host: hosts[0].clone(),
            extra_hosts: hosts.into_iter().skip(1).collect(),
            server_port,
            server_fingerprint,
            queue_id,
            sender_dh_public,
            queue_mode,
        },
        pos,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smp_queue_info_postcard_roundtrip() {
        let original = SmpQueueInfo {
            smp_client_version: 4,
            server_host: "smp.simplego.dev".to_string(),
            extra_hosts: vec![],
            server_port: "5223".to_string(),
            server_fingerprint: [0xee; 32],
            queue_id: [0xab; 24],
            sender_dh_public: [0xcd; 32],
            queue_mode: Some('M'),
        };
        let encoded = postcard::to_allocvec(&original).expect("encode");
        let decoded: SmpQueueInfo = postcard::from_bytes(&encoded).expect("decode");
        assert_eq!(original, decoded);
    }

    #[test]
    fn smp_queue_info_postcard_roundtrip_with_extra_hosts() {
        let original = SmpQueueInfo {
            smp_client_version: 5,
            server_host: "primary.example.com".to_string(),
            extra_hosts: vec![
                "mirror1.example.com".to_string(),
                "mirror2.example.com".to_string(),
            ],
            server_port: "443".to_string(),
            server_fingerprint: [0x12; 32],
            queue_id: [0x34; 24],
            sender_dh_public: [0x56; 32],
            queue_mode: None,
        };
        let encoded = postcard::to_allocvec(&original).expect("encode");
        let decoded: SmpQueueInfo = postcard::from_bytes(&encoded).expect("decode");
        assert_eq!(original, decoded);
    }
}
