//! X3DH key agreement for SimpleX.
//!
//! SimpleX uses a modified X3DH with 3 DH operations (not 4 like Signal).
//! HKDF-SHA512 with info="SimpleXX3DH" and 64-byte zero salt.
//!
//! DH primitive: X448 (Curve448) per simplexmq wire format. Ratchet keys
//! are serialised as 68-byte SPKI (X448 OID 1.3.101.111 + 56-byte raw).

use crate::smp_protocol::SmpError;
use hkdf::Hkdf;
use sha2::Sha512;

/// HKDF info string - must match simplexmq `Ratchet.hs` pqX3dh.
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
    /// Associated data: our_key1_raw(56) || peer_key1_raw(56) = 112 bytes (X448).
    pub assoc_data: Vec<u8>,
}

/// X3DH Alice-path: used by the party ACCEPTING a connection invitation.
///
/// In SimpleX terminology this matches `pqX3dhSnd` in simplexmq
/// `Crypto/Ratchet.hs:467-483` - the "peer joining the connection".
///
/// DH ordering (Alice-specific, see Haskell source):
///   dh1 = DH(our_priv2, peer_pub1)
///   dh2 = DH(our_priv1, peer_pub2)
///   dh3 = DH(our_priv2, peer_pub2)
///
/// The Bob-path (initiator that receives the reply) uses a different
/// DH ordering - see [`x3dh_bob_shared_secret`] for the pqX3dhRcv equivalent.
///
/// IKM = dh1 || dh2 || dh3
/// HKDF-SHA512(salt=64 zero bytes, IKM, info="SimpleXX3DH") -> 96 bytes
/// Output: [0..32]=HKs, [32..64]=NHKr, [64..96]=root_key
///
/// Returns an error if any DH operation produces a low-order point (shouldn't
/// happen with honest peer inputs).
pub fn x3dh_alice_shared_secret(
    peer_pub1: &[u8; 56],
    peer_pub2: &[u8; 56],
    our_priv1: &[u8; 56],
    our_pub1: &[u8; 56],
    our_priv2: &[u8; 56],
) -> Result<X3dhResult, SmpError> {
    // 3 X448 DH operations.
    let dh1 = x448::x448(*our_priv2, *peer_pub1)
        .ok_or(SmpError::Layer2DecryptFailed("X3DH(alice) dh1 low-order point".into()))?;
    let dh2 = x448::x448(*our_priv1, *peer_pub2)
        .ok_or(SmpError::Layer2DecryptFailed("X3DH(alice) dh2 low-order point".into()))?;
    let dh3 = x448::x448(*our_priv2, *peer_pub2)
        .ok_or(SmpError::Layer2DecryptFailed("X3DH(alice) dh3 low-order point".into()))?;

    // IKM = dh1 || dh2 || dh3 (KEM empty in the initial flow).
    let mut ikm = Vec::with_capacity(56 * 3);
    ikm.extend_from_slice(&dh1);
    ikm.extend_from_slice(&dh2);
    ikm.extend_from_slice(&dh3);

    // HKDF-SHA512.
    let salt = [0u8; X3DH_SALT_LEN];
    let hk = Hkdf::<Sha512>::new(Some(&salt), &ikm);
    let mut output = [0u8; 96];
    hk.expand(X3DH_INFO, &mut output)
        .map_err(|e| SmpError::Layer2DecryptFailed(format!("HKDF expand: {e}")))?;

    // Parse output (Signal spec RatchetInitAliceHE).
    let mut header_key_send = [0u8; 32];
    let mut next_header_key_recv = [0u8; 32];
    let mut root_key = [0u8; 32];
    header_key_send.copy_from_slice(&output[0..32]);
    next_header_key_recv.copy_from_slice(&output[32..64]);
    root_key.copy_from_slice(&output[64..96]);

    // assoc_data = our_pub1 || peer_pub1 (raw bytes, no SPKI). 56 + 56 = 112.
    let mut assoc_data = Vec::with_capacity(112);
    assoc_data.extend_from_slice(our_pub1);
    assoc_data.extend_from_slice(peer_pub1);

    Ok(X3dhResult {
        root_key,
        header_key_send,
        next_header_key_recv,
        assoc_data,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::OsRng;

    fn gen_x448_pair() -> ([u8; 56], [u8; 56]) {
        let sk = x448::Secret::new(&mut OsRng);
        let pk = x448::PublicKey::from(&sk);
        (*sk.as_bytes(), *pk.as_bytes())
    }

    #[test]
    fn test_x3dh_output_sizes() {
        let (_, peer_pub1) = gen_x448_pair();
        let (_, peer_pub2) = gen_x448_pair();
        let (our_priv1, our_pub1) = gen_x448_pair();
        let (our_priv2, _) = gen_x448_pair();

        let result = x3dh_alice_shared_secret(
            &peer_pub1, &peer_pub2, &our_priv1, &our_pub1, &our_priv2,
        )
        .expect("X3DH should not fail on honest inputs");

        assert_eq!(result.root_key.len(), 32);
        assert_eq!(result.header_key_send.len(), 32);
        assert_eq!(result.next_header_key_recv.len(), 32);
        assert_eq!(result.assoc_data.len(), 112); // 56 + 56 for X448

        // Root key must not be all zeros
        assert_ne!(result.root_key, [0u8; 32]);

        // assoc_data starts with our_pub1
        assert_eq!(&result.assoc_data[..56], &our_pub1);
    }

    #[test]
    fn test_x3dh_deterministic() {
        // Same inputs produce same outputs
        let (_, peer_pub1) = gen_x448_pair();
        let (_, peer_pub2) = gen_x448_pair();

        let our_priv1: [u8; 56] = {
            let mut b = [0u8; 56];
            rand::RngCore::fill_bytes(&mut OsRng, &mut b);
            b
        };
        let our_priv2: [u8; 56] = {
            let mut b = [0u8; 56];
            rand::RngCore::fill_bytes(&mut OsRng, &mut b);
            b
        };
        let our_pub1 = *x448::PublicKey::from(&x448::Secret::from(our_priv1)).as_bytes();

        let r1 = x3dh_alice_shared_secret(
            &peer_pub1, &peer_pub2, &our_priv1, &our_pub1, &our_priv2,
        )
        .unwrap();
        let r2 = x3dh_alice_shared_secret(
            &peer_pub1, &peer_pub2, &our_priv1, &our_pub1, &our_priv2,
        )
        .unwrap();

        assert_eq!(r1.root_key, r2.root_key);
        assert_eq!(r1.header_key_send, r2.header_key_send);
    }
}
