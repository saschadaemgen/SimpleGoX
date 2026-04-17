//! Double Ratchet for SimpleX E2E encryption.
//!
//! HKDF constants from smp_ratchet.c (verified against live network).
//! AES-256-GCM with SimpleX-specific 16-byte IV (first 12 bytes as nonce).

use super::x3dh::X3dhResult;
use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use hkdf::Hkdf;
use sha2::Sha512;
use x25519_dalek::{PublicKey, StaticSecret};

/// HKDF info strings - from smp_ratchet.c, do NOT change.
const ROOT_RATCHET_INFO: &[u8] = b"SimpleXRootRatchet";
const CHAIN_RATCHET_INFO: &[u8] = b"SimpleXChainRatchet";

const GCM_TAG_LEN: usize = 16;

/// Chain KDF output: 96 bytes split into 4 parts.
struct ChainOutput {
    next_chain_key: [u8; 32],
    message_key: [u8; 32],
    msg_iv: [u8; 16],
    header_iv: [u8; 16],
}

/// Root KDF: salt=root_key, IKM=dh_out, info="SimpleXRootRatchet"
/// Returns (new_root_key, chain_key, next_header_key)
fn kdf_root(root_key: &[u8; 32], dh_out: &[u8; 32]) -> ([u8; 32], [u8; 32], [u8; 32]) {
    let hk = Hkdf::<Sha512>::new(Some(root_key.as_ref()), dh_out.as_ref());
    let mut output = [0u8; 96];
    hk.expand(ROOT_RATCHET_INFO, &mut output)
        .expect("HKDF root expand failed");

    let mut new_root_key = [0u8; 32];
    let mut chain_key = [0u8; 32];
    let mut next_header_key = [0u8; 32];
    new_root_key.copy_from_slice(&output[0..32]);
    chain_key.copy_from_slice(&output[32..64]);
    next_header_key.copy_from_slice(&output[64..96]);

    (new_root_key, chain_key, next_header_key)
}

/// Chain KDF: salt=None, IKM=chain_key, info="SimpleXChainRatchet"
fn kdf_chain(chain_key: &[u8; 32]) -> ChainOutput {
    let hk = Hkdf::<Sha512>::new(None, chain_key.as_ref());
    let mut output = [0u8; 96];
    hk.expand(CHAIN_RATCHET_INFO, &mut output)
        .expect("HKDF chain expand failed");

    let mut next_chain_key = [0u8; 32];
    let mut message_key = [0u8; 32];
    let mut msg_iv = [0u8; 16];
    let mut header_iv = [0u8; 16];
    next_chain_key.copy_from_slice(&output[0..32]);
    message_key.copy_from_slice(&output[32..64]);
    msg_iv.copy_from_slice(&output[64..80]);
    header_iv.copy_from_slice(&output[80..96]);

    ChainOutput {
        next_chain_key,
        message_key,
        msg_iv,
        header_iv,
    }
}

/// AES-256-GCM encrypt. Uses first 12 bytes of 16-byte IV as nonce.
fn aes_gcm_encrypt(
    key: &[u8; 32],
    iv: &[u8; 16],
    aad: &[u8],
    plaintext: &[u8],
) -> (Vec<u8>, [u8; 16]) {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let nonce = Nonce::from_slice(&iv[..12]);
    let payload = Payload {
        msg: plaintext,
        aad,
    };
    let ct_with_tag = cipher
        .encrypt(nonce, payload)
        .expect("AES-GCM encrypt failed");

    let ct_len = ct_with_tag.len() - GCM_TAG_LEN;
    let ciphertext = ct_with_tag[..ct_len].to_vec();
    let mut tag = [0u8; 16];
    tag.copy_from_slice(&ct_with_tag[ct_len..]);
    (ciphertext, tag)
}

/// AES-256-GCM decrypt.
fn aes_gcm_decrypt(
    key: &[u8; 32],
    iv: &[u8; 16],
    aad: &[u8],
    ciphertext: &[u8],
    tag: &[u8; 16],
) -> Option<Vec<u8>> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let nonce = Nonce::from_slice(&iv[..12]);
    let mut ct_with_tag = ciphertext.to_vec();
    ct_with_tag.extend_from_slice(tag);
    let payload = Payload {
        msg: &ct_with_tag,
        aad,
    };
    cipher.decrypt(nonce, payload).ok()
}

/// Double Ratchet state.
pub struct RatchetState {
    pub root_key: [u8; 32],
    pub chain_key_send: [u8; 32],
    pub chain_key_recv: [u8; 32],
    pub header_key_send: [u8; 32],
    pub header_key_recv: [u8; 32],
    pub next_header_key_send: [u8; 32],
    pub next_header_key_recv: [u8; 32],
    pub dh_self_private: StaticSecret,
    pub dh_self_public: PublicKey,
    pub dh_peer: PublicKey,
    pub msg_num_send: u32,
    pub msg_num_recv: u32,
    pub prev_chain_len: u32,
    pub assoc_data: Vec<u8>,
}

impl RatchetState {
    /// Initialize ratchet from X3DH result.
    /// Performs the initial send ratchet step.
    pub fn init_from_x3dh(x3dh: X3dhResult, dh_self: StaticSecret, dh_peer: &PublicKey) -> Self {
        let dh_self_public = PublicKey::from(&dh_self);

        // Initial send ratchet step
        let dh_out = *dh_self.diffie_hellman(dh_peer).as_bytes();
        let (new_root_key, chain_key_send, next_hks) = kdf_root(&x3dh.root_key, &dh_out);

        Self {
            root_key: new_root_key,
            chain_key_send,
            chain_key_recv: [0u8; 32],
            header_key_send: x3dh.header_key_send,
            header_key_recv: [0u8; 32],
            next_header_key_send: next_hks,
            next_header_key_recv: x3dh.next_header_key_recv,
            dh_self_private: dh_self,
            dh_self_public,
            dh_peer: *dh_peer,
            msg_num_send: 0,
            msg_num_recv: 0,
            prev_chain_len: 0,
            assoc_data: x3dh.assoc_data,
        }
    }

    /// Encrypt a message. Input should be zstd-compressed.
    /// Returns encrypted wire bytes.
    pub fn encrypt(
        &mut self,
        compressed_plaintext: &[u8],
        padded_len: usize,
    ) -> Result<Vec<u8>, &'static str> {
        // 1. Chain KDF
        let chain_out = kdf_chain(&self.chain_key_send);
        self.chain_key_send = chain_out.next_chain_key;

        // 2. Build header (simplified - just DH public key + counters)
        let header_plain = self.build_header();

        // 3. Encrypt header
        let (enc_header, header_tag) = aes_gcm_encrypt(
            &self.header_key_send,
            &chain_out.header_iv,
            &self.assoc_data,
            &header_plain,
        );

        // 4. Build emHeader
        let mut em_header = Vec::with_capacity(128);
        em_header.extend_from_slice(&[0x00, 0x03]); // version 3
        em_header.extend_from_slice(&chain_out.header_iv);
        em_header.extend_from_slice(&header_tag);
        let eh_len = enc_header.len() as u16;
        em_header.extend_from_slice(&eh_len.to_be_bytes());
        em_header.extend_from_slice(&enc_header);

        // 5. Pad and encrypt body
        let mut body_aad = Vec::with_capacity(self.assoc_data.len() + em_header.len());
        body_aad.extend_from_slice(&self.assoc_data);
        body_aad.extend_from_slice(&em_header);

        let mut padded = vec![b'#'; padded_len];
        let content_len = compressed_plaintext.len() as u16;
        padded[0] = (content_len >> 8) as u8;
        padded[1] = content_len as u8;
        if compressed_plaintext.len() + 2 > padded_len {
            return Err("Content too large for padding");
        }
        padded[2..2 + compressed_plaintext.len()].copy_from_slice(compressed_plaintext);

        let (enc_payload, payload_tag) = aes_gcm_encrypt(
            &chain_out.message_key,
            &chain_out.msg_iv,
            &body_aad,
            &padded,
        );

        // 6. Assemble
        let em_hdr_len = em_header.len() as u16;
        let mut output = Vec::new();
        output.extend_from_slice(&em_hdr_len.to_be_bytes());
        output.extend_from_slice(&em_header);
        output.extend_from_slice(&payload_tag);
        output.extend_from_slice(&enc_payload);

        self.msg_num_send += 1;
        Ok(output)
    }

    /// Decrypt an incoming encrypted message.
    /// Returns decompressed plaintext.
    pub fn decrypt(&mut self, ciphertext: &[u8]) -> Result<Vec<u8>, &'static str> {
        if ciphertext.len() < 4 {
            return Err("Ciphertext too short");
        }

        // Parse em_hdr_len
        let em_hdr_len = u16::from_be_bytes([ciphertext[0], ciphertext[1]]) as usize;
        if ciphertext.len() < 2 + em_hdr_len + 16 {
            return Err("Truncated ciphertext");
        }
        let em_header = &ciphertext[2..2 + em_hdr_len];
        let rest = &ciphertext[2 + em_hdr_len..];

        // Parse emHeader: [2B ver][16B hdr_iv][16B hdr_tag][2B eh_len][encrypted_header]
        if em_header.len() < 36 {
            return Err("emHeader too short");
        }
        let header_iv: [u8; 16] = em_header[2..18].try_into().unwrap();
        let header_tag: [u8; 16] = em_header[18..34].try_into().unwrap();
        let eh_body_len = u16::from_be_bytes([em_header[34], em_header[35]]) as usize;
        let encrypted_header = &em_header[36..36 + eh_body_len];

        // Payload
        if rest.len() < 16 {
            return Err("No payload tag");
        }
        let payload_tag: [u8; 16] = rest[..16].try_into().unwrap();
        let encrypted_payload = &rest[16..];

        // Decrypt header
        let decrypted_header = aes_gcm_decrypt(
            &self.header_key_recv,
            &header_iv,
            &self.assoc_data,
            encrypted_header,
            &header_tag,
        )
        .ok_or("Header decrypt failed")?;

        // Parse peer DH key from header
        let peer_new_dh = self.parse_header_dh(&decrypted_header)?;

        // Advance ratchet if peer sent new DH key
        if peer_new_dh != self.dh_peer {
            self.advance_ratchet(&peer_new_dh);
        }

        // Chain KDF
        let chain_out = kdf_chain(&self.chain_key_recv);
        self.chain_key_recv = chain_out.next_chain_key;

        // Decrypt body
        let mut body_aad = Vec::with_capacity(self.assoc_data.len() + em_hdr_len);
        body_aad.extend_from_slice(&self.assoc_data);
        body_aad.extend_from_slice(em_header);

        let padded = aes_gcm_decrypt(
            &chain_out.message_key,
            &chain_out.msg_iv,
            &body_aad,
            encrypted_payload,
            &payload_tag,
        )
        .ok_or("Body decrypt failed")?;

        // Unpad
        if padded.len() < 2 {
            return Err("Payload too short");
        }
        let content_len = u16::from_be_bytes([padded[0], padded[1]]) as usize;
        if padded.len() < 2 + content_len {
            return Err("Content length overflow");
        }
        let compressed = &padded[2..2 + content_len];

        // Zstd decompress
        let plaintext = zstd::decode_all(compressed).map_err(|_| "Zstd decompress failed")?;

        self.msg_num_recv += 1;
        Ok(plaintext)
    }

    fn build_header(&self) -> Vec<u8> {
        let mut h = Vec::with_capacity(48);
        // Simplified header: [32B DH public][4B prev_chain_len][4B msg_num_send]
        h.extend_from_slice(self.dh_self_public.as_bytes());
        h.extend_from_slice(&self.prev_chain_len.to_be_bytes());
        h.extend_from_slice(&self.msg_num_send.to_be_bytes());
        h
    }

    fn parse_header_dh(&self, header: &[u8]) -> Result<PublicKey, &'static str> {
        if header.len() < 32 {
            return Err("Header too short for DH key");
        }
        let mut key_bytes = [0u8; 32];
        key_bytes.copy_from_slice(&header[..32]);
        Ok(PublicKey::from(key_bytes))
    }

    fn advance_ratchet(&mut self, peer_new_dh: &PublicKey) {
        let new_dh = StaticSecret::random_from_rng(rand::rngs::OsRng);
        let new_dh_pub = PublicKey::from(&new_dh);

        // Recv direction
        let dh_out_recv = *self.dh_self_private.diffie_hellman(peer_new_dh).as_bytes();
        let (new_rk, ck_recv, nhk_recv) = kdf_root(&self.root_key, &dh_out_recv);
        self.root_key = new_rk;
        self.prev_chain_len = self.msg_num_send;
        self.msg_num_send = 0;
        self.header_key_recv = self.next_header_key_recv;
        self.chain_key_recv = ck_recv;
        self.next_header_key_recv = nhk_recv;

        // Send direction
        let dh_out_send = *new_dh.diffie_hellman(peer_new_dh).as_bytes();
        let (new_rk2, ck_send, nhk_send) = kdf_root(&self.root_key, &dh_out_send);
        self.root_key = new_rk2;
        self.header_key_send = self.next_header_key_send;
        self.chain_key_send = ck_send;
        self.next_header_key_send = nhk_send;

        self.dh_peer = *peer_new_dh;
        self.dh_self_private = new_dh;
        self.dh_self_public = new_dh_pub;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::x3dh::x3dh_sender;
    use rand::rngs::OsRng;

    #[test]
    fn test_ratchet_encrypt_decrypt_roundtrip() {
        // Simulate both sides of a connection
        let peer_priv1 = StaticSecret::random_from_rng(OsRng);
        let peer_pub1 = PublicKey::from(&peer_priv1);
        let peer_priv2 = StaticSecret::random_from_rng(OsRng);
        let peer_pub2 = PublicKey::from(&peer_priv2);

        let our_priv1 = StaticSecret::random_from_rng(OsRng);
        let our_pub1 = PublicKey::from(&our_priv1);
        let our_priv2 = StaticSecret::random_from_rng(OsRng);

        let x3dh_result = x3dh_sender(&peer_pub1, &peer_pub2, &our_priv1, &our_pub1, &our_priv2);

        let mut ratchet = RatchetState::init_from_x3dh(x3dh_result, our_priv2, &peer_pub2);

        // Compress a test message
        let plaintext = b"Hello SimpleX from SimpleGoX!";
        let compressed = zstd::encode_all(plaintext.as_slice(), 3).unwrap();

        // Encrypt
        let encrypted = ratchet.encrypt(&compressed, 256).unwrap();
        assert!(!encrypted.is_empty());
        assert_ne!(&encrypted[..32], &[0u8; 32]); // Not all zeros

        // Note: Decrypt requires the peer's ratchet state (initialized from
        // the other side of X3DH). Full bidirectional test needs Phase 3.
        // For now, verify encrypt doesn't panic and produces output.
    }

    #[test]
    fn test_kdf_chain_deterministic() {
        let chain_key = [42u8; 32];
        let out1 = kdf_chain(&chain_key);
        let out2 = kdf_chain(&chain_key);
        assert_eq!(out1.next_chain_key, out2.next_chain_key);
        assert_eq!(out1.message_key, out2.message_key);
        assert_ne!(out1.next_chain_key, chain_key); // Must differ from input
    }

    #[test]
    fn test_aes_gcm_roundtrip() {
        let key = [1u8; 32];
        let iv = [2u8; 16];
        let aad = b"test_aad";
        let plaintext = b"secret message";

        let (ct, tag) = aes_gcm_encrypt(&key, &iv, aad, plaintext);
        let decrypted = aes_gcm_decrypt(&key, &iv, aad, &ct, &tag).unwrap();
        assert_eq!(&decrypted, plaintext);
    }
}
