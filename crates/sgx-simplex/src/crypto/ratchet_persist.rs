//! Versioned persistence wrapper for the BobRatchet state.
//!
//! Briefing 044d: every BobRatchet mutation (send-side advance, recv-side
//! advance, DH-ratchet step, init) gets postcard-encoded into the
//! `ratchet_states.state_blob` column with a `format_version` discriminator.
//! Future field changes add a new `PersistedRatchetV` variant; the loader
//! picks the right decode path from the persisted version.
//!
//! Why postcard, not bincode: varint encoding keeps the on-disk size
//! tighter for the small u32/u64 counters; the codec is also no_std-
//! friendly which preserves a future ESP32 reuse path from the sgx-iot
//! crate.

use crate::crypto::bob_ratchet::BobRatchet;
use serde::{Deserialize, Serialize};

/// Schema generation marker for the on-disk `ratchet_states.format_version`
/// column. Increment on any breaking change to a `PersistedRatchetV`
/// variant payload (i.e., adding a new variant).
pub const FORMAT_VERSION_V1: i64 = 1;

/// Versioned wrapper around the in-memory ratchet state. Postcard
/// encodes the variant tag along with the payload, which would already
/// distinguish on its own; the SQL `format_version` column is a
/// belt-and-braces selector that lets the loader reject an unknown
/// version without attempting decode.
#[derive(Serialize, Deserialize)]
pub enum PersistedRatchetV {
    V1(BobRatchet),
}

impl PersistedRatchetV {
    /// Schema generation that fresh saves should use.
    pub fn current_version() -> i64 {
        FORMAT_VERSION_V1
    }

    /// Postcard-encode a current-version snapshot of the ratchet state.
    /// Clones the BobRatchet because postcard owns the data through the
    /// V1 variant; the BG loop's `&mut BobRatchet` cannot be moved.
    pub fn encode(ratchet: &BobRatchet) -> Result<Vec<u8>, RatchetPersistError> {
        let wrapper = PersistedRatchetV::V1(ratchet.clone());
        postcard::to_allocvec(&wrapper).map_err(RatchetPersistError::Codec)
    }

    /// Decode a persisted blob back to an in-memory BobRatchet. The
    /// `format_version` argument is the value of the SQL column on the
    /// loaded row; mismatched versions return an error rather than
    /// attempting an undefined decode.
    ///
    /// Used only on the 044g boot-time respawn path; 044d itself ships
    /// the encode side and the SQL columns. Tests in this module exercise
    /// the function so it does not become rot.
    #[allow(dead_code)]
    pub fn decode(blob: &[u8], format_version: i64) -> Result<BobRatchet, RatchetPersistError> {
        match format_version {
            FORMAT_VERSION_V1 => {
                let wrapper: PersistedRatchetV =
                    postcard::from_bytes(blob).map_err(RatchetPersistError::Codec)?;
                match wrapper {
                    PersistedRatchetV::V1(r) => Ok(r),
                }
            }
            other => Err(RatchetPersistError::UnknownFormatVersion(other)),
        }
    }
}

/// Failure modes for ratchet (de)serialisation. `thiserror` is not in
/// the crate's dependency set, so a manual `Display` + `std::error::Error`
/// impl matches the pattern used elsewhere in sgx-simplex.
#[derive(Debug)]
pub enum RatchetPersistError {
    Codec(postcard::Error),
    // Only constructed on the 044g boot-time decode path. Tests in this
    // module cover the variant so the dead-code lint stays quiet without
    // dragging in a wider allow.
    #[allow(dead_code)]
    UnknownFormatVersion(i64),
}

impl std::fmt::Display for RatchetPersistError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Codec(e) => write!(f, "postcard codec error: {e}"),
            Self::UnknownFormatVersion(v) => write!(f, "unknown ratchet format_version: {v}"),
        }
    }
}

impl std::error::Error for RatchetPersistError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Codec(e) => Some(e),
            Self::UnknownFormatVersion(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::bob_ratchet::BobRatchet;

    fn fixture_ratchet() -> BobRatchet {
        BobRatchet {
            root_key: [1u8; 32],
            our_dhrs_priv: [2u8; 56],
            receiving_chain_key: Some([3u8; 32]),
            sending_chain_key: Some([4u8; 32]),
            receiving_header_key: Some([5u8; 32]),
            next_header_key_receive: [6u8; 32],
            sending_header_key: [7u8; 32],
            next_sending_header_key: [8u8; 32],
            assoc_data: vec![9u8; 112],
            ns: 42,
            nr: 17,
            pn: 3,
            our_new_ratchet_priv: Some([10u8; 56]),
            our_new_ratchet_pub: Some([11u8; 56]),
            last_snd_msg_hash: vec![12u8; 32],
            next_snd_msg_id: 99,
        }
    }

    #[test]
    fn encode_decode_roundtrip_full() {
        let original = fixture_ratchet();
        let blob = PersistedRatchetV::encode(&original).expect("encode");
        let decoded = PersistedRatchetV::decode(&blob, FORMAT_VERSION_V1).expect("decode");

        assert_eq!(decoded.root_key, original.root_key);
        assert_eq!(decoded.our_dhrs_priv, original.our_dhrs_priv);
        assert_eq!(decoded.receiving_chain_key, original.receiving_chain_key);
        assert_eq!(decoded.sending_chain_key, original.sending_chain_key);
        assert_eq!(decoded.receiving_header_key, original.receiving_header_key);
        assert_eq!(
            decoded.next_header_key_receive,
            original.next_header_key_receive
        );
        assert_eq!(decoded.sending_header_key, original.sending_header_key);
        assert_eq!(
            decoded.next_sending_header_key,
            original.next_sending_header_key
        );
        assert_eq!(decoded.assoc_data, original.assoc_data);
        assert_eq!(decoded.ns, original.ns);
        assert_eq!(decoded.nr, original.nr);
        assert_eq!(decoded.pn, original.pn);
        assert_eq!(decoded.our_new_ratchet_priv, original.our_new_ratchet_priv);
        assert_eq!(decoded.our_new_ratchet_pub, original.our_new_ratchet_pub);
        assert_eq!(decoded.last_snd_msg_hash, original.last_snd_msg_hash);
        assert_eq!(decoded.next_snd_msg_id, original.next_snd_msg_id);
    }

    #[test]
    fn encode_decode_roundtrip_with_none_fields() {
        let mut ratchet = fixture_ratchet();
        ratchet.receiving_chain_key = None;
        ratchet.sending_chain_key = None;
        ratchet.receiving_header_key = None;
        ratchet.our_new_ratchet_priv = None;
        ratchet.our_new_ratchet_pub = None;
        ratchet.last_snd_msg_hash = Vec::new();

        let blob = PersistedRatchetV::encode(&ratchet).expect("encode");
        let decoded = PersistedRatchetV::decode(&blob, FORMAT_VERSION_V1).expect("decode");

        assert_eq!(decoded.receiving_chain_key, None);
        assert_eq!(decoded.sending_chain_key, None);
        assert_eq!(decoded.receiving_header_key, None);
        assert_eq!(decoded.our_new_ratchet_priv, None);
        assert_eq!(decoded.our_new_ratchet_pub, None);
        assert!(decoded.last_snd_msg_hash.is_empty());
    }

    #[test]
    fn decode_rejects_unknown_format_version() {
        let blob = PersistedRatchetV::encode(&fixture_ratchet()).expect("encode");
        let err = PersistedRatchetV::decode(&blob, 99).unwrap_err();
        match err {
            RatchetPersistError::UnknownFormatVersion(v) => assert_eq!(v, 99),
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn decode_rejects_garbage_blob() {
        let garbage = vec![0xff; 16];
        let err = PersistedRatchetV::decode(&garbage, FORMAT_VERSION_V1).unwrap_err();
        match err {
            RatchetPersistError::Codec(_) => {}
            other => panic!("expected Codec error, got: {other}"),
        }
    }
}
