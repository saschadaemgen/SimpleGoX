//! X3DH key agreement for SimpleX.
//!
//! SimpleX uses a modified X3DH with 3 DH operations (not 4 like Signal).
//! HKDF-SHA512 with info="SimpleXX3DH" and 64-byte zero salt.
//!
//! Note: The briefing specifies X448 for the DH operations. Until a stable
//! X448 crate is available, this implementation uses X25519 as a structural
//! placeholder. The HKDF and key derivation logic is identical regardless
//! of the DH primitive.

use crate::smp_protocol::SmpError;
use hkdf::Hkdf;
use sha2::Sha512;
use x25519_dalek::{PublicKey, StaticSecret};

/// HKDF info string - from smp_ratchet.c, do NOT change.
const X3DH_INFO: &[u8] = b"SimpleXX3DH";

/// Salt length - 64 zero bytes.
const X3DH_SALT_LEN: usize = 64;

/// X448 SPKI header: ASN.1 DER wrapper around the raw 56-byte key.
///
/// Structure: SEQUENCE {
///   AlgorithmIdentifier (OID 1.3.101.111 = X448),
///   BIT STRING raw_key
/// }
///
/// Total: 12 bytes header + 56 bytes raw = 68 bytes.
pub const X448_SPKI_HEADER: [u8; 12] = [
    0x30, 0x42, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x6f, 0x03, 0x39, 0x00,
];

/// Parse an X448 SPKI (68 bytes) into a raw 56-byte public key.
///
/// The AgentConfirmation delivers peer ratchet keys as 68-byte SPKI;
/// this helper extracts the raw 56-byte key suitable for `x448::x448`.
pub fn parse_x448_spki(spki: &[u8]) -> Result<[u8; 56], SmpError> {
    if spki.len() != 68 {
        return Err(SmpError::InvalidLength {
            declared: 68,
            available: spki.len(),
        });
    }
    if spki[..12] != X448_SPKI_HEADER {
        return Err(SmpError::UnexpectedByte {
            expected: 0x2b,
            got: spki[6],
            ctx: "X448 SPKI OID",
        });
    }
    let mut raw = [0u8; 56];
    raw.copy_from_slice(&spki[12..68]);
    Ok(raw)
}

/// X3DH result - feeds into ratchet initialization.
pub struct X3dhResult {
    /// Root key for the Double Ratchet (32 bytes).
    pub root_key: [u8; 32],
    /// Initial send header key HKs (32 bytes).
    pub header_key_send: [u8; 32],
    /// Next receive header key NHKr (32 bytes).
    pub next_header_key_recv: [u8; 32],
    /// Associated data: our_key1_raw(32) || peer_key1_raw(32) = 64 bytes.
    pub assoc_data: Vec<u8>,
}

/// X3DH Alice-path: used by the party ACCEPTING a connection invitation.
///
/// In SimpleX terminology this matches `pqX3dhSnd` in simplexmq
/// `Crypto/Ratchet.hs:467-483` - the "peer joining the connection".
///
/// DH ordering:
///   dh1 = DH(our_priv2, peer_pub1)
///   dh2 = DH(our_priv1, peer_pub2)
///   dh3 = DH(our_priv2, peer_pub2)
///
/// The Bob-path (initiator that receives the reply) uses a different
/// DH ordering - see `x3dh_bob_shared_secret` for the pqX3dhRcv equivalent.
///
/// IKM = dh1 || dh2 || dh3
/// HKDF-SHA512(salt=64 zero bytes, IKM, info="SimpleXX3DH") -> 96 bytes
/// Output: [0..32]=HKs, [32..64]=NHKr, [64..96]=root_key
pub fn x3dh_alice_shared_secret(
    peer_key1: &PublicKey,
    peer_key2: &PublicKey,
    our_key1_private: &StaticSecret,
    our_key1_public: &PublicKey,
    our_key2_private: &StaticSecret,
) -> X3dhResult {
    // 3 DH operations
    let dh1 = our_key2_private.diffie_hellman(peer_key1);
    let dh2 = our_key1_private.diffie_hellman(peer_key2);
    let dh3 = our_key2_private.diffie_hellman(peer_key2);

    // Concatenate: IKM = dh1 || dh2 || dh3
    let mut ikm = Vec::with_capacity(96);
    ikm.extend_from_slice(dh1.as_bytes());
    ikm.extend_from_slice(dh2.as_bytes());
    ikm.extend_from_slice(dh3.as_bytes());

    // HKDF-SHA512
    let salt = [0u8; X3DH_SALT_LEN];
    let hk = Hkdf::<Sha512>::new(Some(&salt), &ikm);
    let mut output = [0u8; 96];
    hk.expand(X3DH_INFO, &mut output)
        .expect("HKDF expand failed");

    // Parse output (Signal spec RatchetInitAliceHE)
    let mut header_key_send = [0u8; 32];
    let mut next_header_key_recv = [0u8; 32];
    let mut root_key = [0u8; 32];
    header_key_send.copy_from_slice(&output[0..32]);
    next_header_key_recv.copy_from_slice(&output[32..64]);
    root_key.copy_from_slice(&output[64..96]);

    // assoc_data = our_key1_public || peer_key1_public (raw bytes, no SPKI)
    let mut assoc_data = Vec::with_capacity(64);
    assoc_data.extend_from_slice(our_key1_public.as_bytes());
    assoc_data.extend_from_slice(peer_key1.as_bytes());

    X3dhResult {
        root_key,
        header_key_send,
        next_header_key_recv,
        assoc_data,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::OsRng;

    #[test]
    fn test_x3dh_output_sizes() {
        let peer_priv1 = StaticSecret::random_from_rng(OsRng);
        let peer_pub1 = PublicKey::from(&peer_priv1);
        let peer_priv2 = StaticSecret::random_from_rng(OsRng);
        let peer_pub2 = PublicKey::from(&peer_priv2);

        let our_priv1 = StaticSecret::random_from_rng(OsRng);
        let our_pub1 = PublicKey::from(&our_priv1);
        let our_priv2 = StaticSecret::random_from_rng(OsRng);

        let result = x3dh_alice_shared_secret(&peer_pub1, &peer_pub2, &our_priv1, &our_pub1, &our_priv2);

        assert_eq!(result.root_key.len(), 32);
        assert_eq!(result.header_key_send.len(), 32);
        assert_eq!(result.next_header_key_recv.len(), 32);
        assert_eq!(result.assoc_data.len(), 64);

        // Root key must not be all zeros
        assert_ne!(result.root_key, [0u8; 32]);

        // assoc_data starts with our_key1 public
        assert_eq!(&result.assoc_data[..32], our_pub1.as_bytes());
    }

    #[test]
    fn test_x3dh_deterministic() {
        // Same inputs produce same outputs
        let peer_priv1 = StaticSecret::random_from_rng(OsRng);
        let peer_pub1 = PublicKey::from(&peer_priv1);
        let peer_priv2 = StaticSecret::random_from_rng(OsRng);
        let peer_pub2 = PublicKey::from(&peer_priv2);

        let our_bytes1: [u8; 32] = rand::random();
        let our_bytes2: [u8; 32] = rand::random();

        let our_priv1a = StaticSecret::from(our_bytes1);
        let our_pub1a = PublicKey::from(&our_priv1a);
        let our_priv2a = StaticSecret::from(our_bytes2);

        let our_priv1b = StaticSecret::from(our_bytes1);
        let our_pub1b = PublicKey::from(&our_priv1b);
        let our_priv2b = StaticSecret::from(our_bytes2);

        let r1 = x3dh_alice_shared_secret(&peer_pub1, &peer_pub2, &our_priv1a, &our_pub1a, &our_priv2a);
        let r2 = x3dh_alice_shared_secret(&peer_pub1, &peer_pub2, &our_priv1b, &our_pub1b, &our_priv2b);

        assert_eq!(r1.root_key, r2.root_key);
        assert_eq!(r1.header_key_send, r2.header_key_send);
    }
}
