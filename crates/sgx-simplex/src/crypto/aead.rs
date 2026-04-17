//! AES-256-GCM implementation compatible with cryptonite's 16-byte IV handling.
//!
//! Standard Rust aes-gcm hardcodes 12-byte nonces. SimpleX protocol uses
//! 16-byte IVs throughout the Double Ratchet, which triggers the J_0
//! derivation path per NIST SP 800-38D Algorithm 4.

use aes::cipher::{BlockEncrypt, KeyInit};
use aes::Aes256;
use ctr::cipher::{KeyIvInit, StreamCipher};
use ghash::universal_hash::UniversalHash;
use ghash::GHash;

type Aes256Ctr = ctr::Ctr32BE<Aes256>;

const GCM_TAG_LEN: usize = 16;

/// AES-256-GCM encrypt with any-length IV (supports the SimpleX 16-byte IV).
///
/// Returns (tag, ciphertext). Tag is 16 bytes (GMAC).
pub fn aes256_gcm_encrypt(
    key: &[u8; 32],
    iv: &[u8],
    aad: &[u8],
    plaintext: &[u8],
) -> ([u8; GCM_TAG_LEN], Vec<u8>) {
    let cipher = Aes256::new(key.into());
    let h_block = hash_subkey(&cipher);
    let j0 = derive_j0(&h_block, iv);

    // Ciphertext: AES-CTR with counter = inc32(J_0).
    let mut ctr_iv = j0;
    inc32(&mut ctr_iv);
    let mut ct = plaintext.to_vec();
    let mut ctr = Aes256Ctr::new(key.into(), (&ctr_iv).into());
    ctr.apply_keystream(&mut ct);

    // GHASH over AAD || 0-pad || CT || 0-pad || len(AAD)_64 || len(CT)_64.
    let s = ghash_block(&h_block, aad, &ct);

    // tag = AES(J_0) XOR S
    let mut encrypted_j0 = j0;
    cipher.encrypt_block((&mut encrypted_j0).into());
    let mut tag = [0u8; GCM_TAG_LEN];
    for i in 0..GCM_TAG_LEN {
        tag[i] = encrypted_j0[i] ^ s[i];
    }

    (tag, ct)
}

/// AES-256-GCM decrypt with any-length IV.
///
/// Returns plaintext or `Err` on authentication failure.
pub fn aes256_gcm_decrypt(
    key: &[u8; 32],
    iv: &[u8],
    aad: &[u8],
    ciphertext: &[u8],
    tag: &[u8; GCM_TAG_LEN],
) -> Result<Vec<u8>, &'static str> {
    let cipher = Aes256::new(key.into());
    let h_block = hash_subkey(&cipher);
    let j0 = derive_j0(&h_block, iv);

    // Verify tag BEFORE decrypting.
    let s = ghash_block(&h_block, aad, ciphertext);
    let mut encrypted_j0 = j0;
    cipher.encrypt_block((&mut encrypted_j0).into());
    let mut expected_tag = [0u8; GCM_TAG_LEN];
    for i in 0..GCM_TAG_LEN {
        expected_tag[i] = encrypted_j0[i] ^ s[i];
    }
    if !constant_time_eq(&expected_tag, tag) {
        return Err("AES-GCM authentication failed");
    }

    // Decrypt: AES-CTR with counter starting at J_0 + 1.
    let mut ctr_iv = j0;
    inc32(&mut ctr_iv);
    let mut pt = ciphertext.to_vec();
    let mut ctr = Aes256Ctr::new(key.into(), (&ctr_iv).into());
    ctr.apply_keystream(&mut pt);

    Ok(pt)
}

fn hash_subkey(cipher: &Aes256) -> [u8; 16] {
    let mut h_block = [0u8; 16];
    cipher.encrypt_block((&mut h_block).into());
    h_block
}

fn derive_j0(h_block: &[u8; 16], iv: &[u8]) -> [u8; 16] {
    if iv.len() == 12 {
        // Standard: J_0 = IV || 0x00000001
        let mut j0 = [0u8; 16];
        j0[..12].copy_from_slice(iv);
        j0[15] = 1;
        j0
    } else {
        // J_0 = GHASH_H(IV || 0^(s+64) || [len(IV)_bits]_64)
        let iv_bit_len_be = ((iv.len() as u64) * 8).to_be_bytes();
        let mut buf = iv.to_vec();
        while buf.len() % 16 != 0 {
            buf.push(0);
        }
        // Append 8 zero bytes (padding to 16) + 8 bytes IV bit length.
        buf.extend_from_slice(&[0u8; 8]);
        buf.extend_from_slice(&iv_bit_len_be);

        let mut ghash = GHash::new(h_block.into());
        for chunk in buf.chunks_exact(16) {
            let arr: [u8; 16] = chunk.try_into().unwrap();
            ghash.update(&[arr.into()]);
        }
        let tag_bytes = ghash.finalize();
        let mut j0 = [0u8; 16];
        j0.copy_from_slice(tag_bytes.as_slice());
        j0
    }
}

/// Compute S = GHASH_H(AAD || 0-pad || C || 0-pad || [len(AAD)_bits]_64 || [len(C)_bits]_64).
fn ghash_block(h_block: &[u8; 16], aad: &[u8], ct: &[u8]) -> [u8; 16] {
    let mut ghash = GHash::new(h_block.into());

    // AAD chunks (zero-padded to 16).
    let mut a_buf = aad.to_vec();
    while a_buf.len() % 16 != 0 {
        a_buf.push(0);
    }
    for chunk in a_buf.chunks_exact(16) {
        let arr: [u8; 16] = chunk.try_into().unwrap();
        ghash.update(&[arr.into()]);
    }

    // Ciphertext chunks (zero-padded to 16).
    let mut c_buf = ct.to_vec();
    while c_buf.len() % 16 != 0 {
        c_buf.push(0);
    }
    for chunk in c_buf.chunks_exact(16) {
        let arr: [u8; 16] = chunk.try_into().unwrap();
        ghash.update(&[arr.into()]);
    }

    // Lengths block: [len(AAD)_bits]_64 || [len(CT)_bits]_64.
    let mut lengths = [0u8; 16];
    lengths[..8].copy_from_slice(&((aad.len() as u64) * 8).to_be_bytes());
    lengths[8..].copy_from_slice(&((ct.len() as u64) * 8).to_be_bytes());
    ghash.update(&[lengths.into()]);

    let s_bytes = ghash.finalize();
    let mut s = [0u8; 16];
    s.copy_from_slice(s_bytes.as_slice());
    s
}

fn inc32(block: &mut [u8; 16]) {
    let counter = u32::from_be_bytes([block[12], block[13], block[14], block[15]]);
    let new_counter = counter.wrapping_add(1);
    block[12..].copy_from_slice(&new_counter.to_be_bytes());
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Spec-compliance check: for a 12-byte IV our output must match the
    /// RustCrypto aes-gcm reference (which is NIST-validated).
    #[test]
    fn aes256_gcm_matches_reference_for_12_byte_iv() {
        use aes_gcm::aead::{Aead, KeyInit, Payload};
        use aes_gcm::{Aes256Gcm, Key, Nonce};

        let key = [0x42u8; 32];
        let iv = [0x17u8; 12];
        let aad = b"associated-data";
        let pt = b"This is a plaintext message for GCM testing purposes.";

        // Reference
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key));
        let ct_ref = cipher
            .encrypt(Nonce::from_slice(&iv), Payload { msg: pt, aad })
            .unwrap();
        let ct_ref_len = ct_ref.len() - 16;
        let ct_ref_bytes = &ct_ref[..ct_ref_len];
        let tag_ref: [u8; 16] = ct_ref[ct_ref_len..].try_into().unwrap();

        // Our impl
        let (tag, ct) = aes256_gcm_encrypt(&key, &iv, aad, pt);

        assert_eq!(ct, ct_ref_bytes, "ciphertext must match reference");
        assert_eq!(tag, tag_ref, "tag must match reference");

        // Roundtrip decrypt
        let recovered = aes256_gcm_decrypt(&key, &iv, aad, &ct, &tag).unwrap();
        assert_eq!(recovered, pt);
    }

    /// Roundtrip test with 16-byte IV (the SimpleX case).
    #[test]
    fn aes256_gcm_16_byte_iv_roundtrip() {
        let key = [0xAAu8; 32];
        let iv = [
            0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xAA, 0xBB, 0xCC, 0xDD,
            0xEE, 0xFF,
        ];
        let aad = b"rcAD-plus-enc-header";
        let pt = b"SimpleX Bob ratchet first message";

        let (tag, ct) = aes256_gcm_encrypt(&key, &iv, aad, pt);
        assert_eq!(tag.len(), 16);
        assert_eq!(ct.len(), pt.len());

        let recovered = aes256_gcm_decrypt(&key, &iv, aad, &ct, &tag).unwrap();
        assert_eq!(recovered, pt);
    }

    /// Wrong tag must fail.
    #[test]
    fn aes256_gcm_16_byte_iv_wrong_tag_fails() {
        let key = [0x11u8; 32];
        let iv = [0x22u8; 16];
        let (tag, ct) = aes256_gcm_encrypt(&key, &iv, b"", b"payload");
        let mut bad_tag = tag;
        bad_tag[0] ^= 0xFF;
        let r = aes256_gcm_decrypt(&key, &iv, b"", &ct, &bad_tag);
        assert!(r.is_err());
    }

    /// Empty plaintext + empty AAD - the edge case for J_0 derivation and tag computation.
    #[test]
    fn aes256_gcm_16_byte_iv_empty() {
        let key = [0u8; 32];
        let iv = [0u8; 16];
        let (tag, ct) = aes256_gcm_encrypt(&key, &iv, b"", b"");
        assert_eq!(ct.len(), 0);
        let recovered = aes256_gcm_decrypt(&key, &iv, b"", &ct, &tag).unwrap();
        assert_eq!(recovered, b"");
    }
}
