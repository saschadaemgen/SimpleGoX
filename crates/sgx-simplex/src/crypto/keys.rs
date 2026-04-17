//! Key generation and SPKI encoding for SimpleX crypto.
//!
//! SimpleX uses X25519 for queue-level crypto_box and Ed25519 for command
//! signing. X448 is used for Double Ratchet DH - we use raw Curve25519
//! extended DH as a placeholder until a stable X448 crate is available.
//!
//! SPKI headers are DER-encoded ASN.1 per RFC 5958.

use ed25519_dalek::SigningKey;
use rand::rngs::OsRng;
use x25519_dalek::{PublicKey as X25519Public, StaticSecret};

// X25519 SPKI header (12 bytes)
// SEQUENCE(42) { SEQUENCE(5) { OID(1.3.101.110) } BITSTRING(33) }
pub const X25519_SPKI_HEADER: [u8; 12] = [
    0x30, 0x2a, // SEQUENCE, 42 bytes
    0x30, 0x05, // SEQUENCE, 5 bytes
    0x06, 0x03, // OID, 3 bytes
    0x2b, 0x65, 0x6e, // 1.3.101.110 (X25519)
    0x03, 0x21, // BIT STRING, 33 bytes
    0x00, // no unused bits
];

pub const X25519_SPKI_SIZE: usize = 44; // 12 header + 32 raw key

// X448 SPKI header (12 bytes)
// SEQUENCE(66) { SEQUENCE(5) { OID(1.3.101.111) } BITSTRING(57) }
pub const X448_SPKI_HEADER: [u8; 12] = [
    0x30, 0x42, // SEQUENCE, 66 bytes
    0x30, 0x05, // SEQUENCE, 5 bytes
    0x06, 0x03, // OID, 3 bytes
    0x2b, 0x65, 0x6f, // 1.3.101.111 (X448)
    0x03, 0x39, // BIT STRING, 57 bytes
    0x00, // no unused bits
];

pub const X448_SPKI_SIZE: usize = 68; // 12 header + 56 raw key

/// X448 keypair (56-byte keys). The public key is derived from the private
/// scalar via the X448 basepoint so `(private, public)` forms a valid DH pair.
pub struct X448Keypair {
    pub private: [u8; 56],
    pub public: [u8; 56],
}

impl X448Keypair {
    /// Generate a new X448 keypair.
    pub fn generate() -> Self {
        use rand::RngCore;
        let mut private = [0u8; 56];
        OsRng.fill_bytes(&mut private);
        // Derive the public key: X448 basepoint × clamped private scalar.
        // x448::Secret::from applies RFC 7748 clamping internally.
        let secret = x448::Secret::from(private);
        let public = *x448::PublicKey::from(&secret).as_bytes();
        // Store the clamped private so `private` and `public` form a
        // canonical DH pair that round-trips through (Secret::from, PublicKey::from).
        let private = *secret.as_bytes();
        Self { private, public }
    }

    /// Encode public key as X448 SPKI DER (68 bytes).
    pub fn encode_spki(&self) -> [u8; X448_SPKI_SIZE] {
        let mut out = [0u8; X448_SPKI_SIZE];
        out[..12].copy_from_slice(&X448_SPKI_HEADER);
        out[12..].copy_from_slice(&self.public);
        out
    }

    /// Raw 56-byte public key.
    pub fn public_bytes(&self) -> [u8; 56] {
        self.public
    }
}

// Ed25519 SPKI header (12 bytes)
pub const ED25519_SPKI_HEADER: [u8; 12] = [
    0x30, 0x2a, // SEQUENCE, 42 bytes
    0x30, 0x05, // SEQUENCE, 5 bytes
    0x06, 0x03, // OID, 3 bytes
    0x2b, 0x65, 0x70, // 1.3.101.112 (Ed25519)
    0x03, 0x21, // BIT STRING, 33 bytes
    0x00, // no unused bits
];

pub const ED25519_SPKI_SIZE: usize = 44; // 12 header + 32 raw key

/// Generate a new X25519 keypair.
pub fn generate_x25519() -> (StaticSecret, X25519Public) {
    let private = StaticSecret::random_from_rng(OsRng);
    let public = X25519Public::from(&private);
    (private, public)
}

/// Generate a new Ed25519 signing keypair.
pub fn generate_ed25519() -> SigningKey {
    SigningKey::generate(&mut OsRng)
}

/// Encode an X25519 public key as SPKI DER.
pub fn encode_x25519_spki(public: &X25519Public) -> [u8; X25519_SPKI_SIZE] {
    let mut out = [0u8; X25519_SPKI_SIZE];
    out[..12].copy_from_slice(&X25519_SPKI_HEADER);
    out[12..].copy_from_slice(public.as_bytes());
    out
}

/// Decode raw 32-byte X25519 key from 44-byte SPKI encoding.
pub fn decode_x25519_spki(spki: &[u8]) -> Option<[u8; 32]> {
    if spki.len() != X25519_SPKI_SIZE {
        return None;
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(&spki[12..]);
    Some(key)
}

/// Encode an Ed25519 verifying key as SPKI DER.
pub fn encode_ed25519_spki(public: &ed25519_dalek::VerifyingKey) -> [u8; ED25519_SPKI_SIZE] {
    let mut out = [0u8; ED25519_SPKI_SIZE];
    out[..12].copy_from_slice(&ED25519_SPKI_HEADER);
    out[12..].copy_from_slice(public.as_bytes());
    out
}

/// X25519 Diffie-Hellman.
pub fn x25519_dh(our_private: &StaticSecret, their_public: &X25519Public) -> [u8; 32] {
    *our_private.diffie_hellman(their_public).as_bytes()
}

/// Generate 24 random bytes for a correlation ID.
pub fn generate_corr_id() -> [u8; 24] {
    let mut id = [0u8; 24];
    use rand::RngCore;
    OsRng.fill_bytes(&mut id);
    id
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_x25519_spki_roundtrip() {
        let (_, public) = generate_x25519();
        let spki = encode_x25519_spki(&public);
        assert_eq!(spki.len(), X25519_SPKI_SIZE);
        let decoded = decode_x25519_spki(&spki).unwrap();
        assert_eq!(&decoded, public.as_bytes());
    }

    #[test]
    fn test_ed25519_spki_encoding() {
        let key = generate_ed25519();
        let spki = encode_ed25519_spki(&key.verifying_key());
        assert_eq!(spki.len(), ED25519_SPKI_SIZE);
        assert_eq!(&spki[..12], &ED25519_SPKI_HEADER);
        assert_eq!(&spki[12..], key.verifying_key().as_bytes());
    }

    #[test]
    fn test_x25519_dh_shared_secret() {
        let (priv_a, pub_a) = generate_x25519();
        let (priv_b, pub_b) = generate_x25519();
        let shared_ab = x25519_dh(&priv_a, &pub_b);
        let shared_ba = x25519_dh(&priv_b, &pub_a);
        assert_eq!(shared_ab, shared_ba);
        assert_ne!(shared_ab, [0u8; 32]);
    }

    #[test]
    fn test_corr_id_random() {
        let id1 = generate_corr_id();
        let id2 = generate_corr_id();
        assert_ne!(id1, id2);
        assert_eq!(id1.len(), 24);
    }
}
