//! SMP wire protocol - the layer between raw TLS bytes and handshake logic.
//!
//! Handles 16KB block framing, ServerHello/ClientHello, transmission parsing,
//! and signed/unsigned transmission building.

use ed25519_dalek::{Signer, SigningKey};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;

pub const SMP_BLOCK_SIZE: usize = 16384;
pub const PADDING_BYTE: u8 = 0x23; // '#'

pub const ED25519_SPKI_HEADER: [u8; 12] = [
    0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00,
];

pub const X25519_SPKI_HEADER: [u8; 12] = [
    0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x6e, 0x03, 0x21, 0x00,
];

/// Active SMP connection with session state.
pub struct SmpConnection {
    pub stream: TlsStream<TcpStream>,
    pub session_id: [u8; 32],
    pub server_key_hash: [u8; 32],
    pub version: u16,
    /// v9 session auth keypair (X25519) - private + public raw bytes
    pub session_auth_private: [u8; 32],
    pub session_auth_public: [u8; 32],
    /// v9 server's session public key (from ServerHello) - for DH
    pub server_session_public: [u8; 32],
    /// v9 shared secret: DH(session_auth_private, server_session_public)
    pub session_shared_secret: [u8; 32],
    /// v9 queue auth keypair - used for DH-based command authorization
    /// Public key goes into NEW as sndAuthKey, private stays here for signing.
    pub queue_auth_private: [u8; 32],
    pub queue_auth_public: [u8; 32],
}

impl SmpConnection {
    /// Perform the SMP-level handshake after TLS is established.
    /// Reads ServerHello, extracts session_id, sends ClientHello.
    pub async fn smp_handshake(
        mut stream: TlsStream<TcpStream>,
        server_key_hash: [u8; 32],
    ) -> Result<Self, SmpError> {
        // Read ServerHello - this is a HANDSHAKE block, NOT a command block.
        // Format: [Word16 BE content_len][content][zero padding to 16384]
        // No txCount framing - different from command response blocks.
        let mut block = vec![0u8; SMP_BLOCK_SIZE];
        stream.read_exact(&mut block).await?;

        let content_len = u16::from_be_bytes([block[0], block[1]]) as usize;
        if content_len > SMP_BLOCK_SIZE - 2 {
            return Err(SmpError::Protocol("ServerHello too large".into()));
        }
        let hello = &block[2..2 + content_len];

        if hello.len() < 37 {
            return Err(SmpError::Protocol(format!(
                "ServerHello content too short: {} bytes", hello.len()
            )));
        }

        // hello[0..4] = version range: [minVersion Word16][maxVersion Word16]
        let min_ver = u16::from_be_bytes([hello[0], hello[1]]);
        let max_ver = u16::from_be_bytes([hello[2], hello[3]]);
        let negotiated_version = std::cmp::min(9u16, max_ver);
        tracing::info!("SMP ServerHello: server v{min_ver}-v{max_ver}, negotiated=v{negotiated_version}");

        // hello[4] = sessionId length (must be 32)
        let sess_id_len = hello[4] as usize;
        tracing::info!("SMP ServerHello: sess_id_len={}", sess_id_len);
        if sess_id_len != 32 {
            return Err(SmpError::Protocol(format!(
                "Bad sessionId length: {sess_id_len} (expected 32)"
            )));
        }
        let mut session_id = [0u8; 32];
        session_id.copy_from_slice(&hello[5..37]);

        tracing::info!(
            "SMP ServerHello: session_id={}...",
            hex::encode(&session_id[..4])
        );

        // v9 ServerHello: after sessionId follows Large-encoded cert chain + signedKeyDer.
        // Scan for X25519 OID (06 03 2b 65 6e) to locate server session public key.
        let mut server_session_public = [0u8; 32];
        if negotiated_version >= 9 {
            // Always log the post-sessionId structure for inspection
            tracing::debug!("ServerHello after sessId ({} bytes, showing 300B): {}",
                hello[37..].len(),
                hex::encode(&hello[37..hello.len().min(337)]));

            if let Some(key) = extract_x25519_key(&hello[37..]) {
                server_session_public = key;
                tracing::info!(
                    "SMP ServerHello: server_session_pub={}... (via X25519 OID scan)",
                    hex::encode(&server_session_public[..8])
                );
            } else {
                tracing::warn!("SMP ServerHello: X25519 OID not found in cert chain!");
            }
        }

        // v9 session auth keypair (X25519 raw)
        use x25519_dalek::{PublicKey as X25519Public, StaticSecret};
        use rand::rngs::OsRng;
        let sess_auth_priv = StaticSecret::random_from_rng(OsRng);
        let sess_auth_pub = X25519Public::from(&sess_auth_priv);
        let session_auth_private = sess_auth_priv.to_bytes();
        let session_auth_public = *sess_auth_pub.as_bytes();

        // v9: compute shared secret for auth MAC (session-level)
        let session_shared_secret: [u8; 32] = if negotiated_version >= 9 {
            let srv_pub = X25519Public::from(server_session_public);
            *sess_auth_priv.diffie_hellman(&srv_pub).as_bytes()
        } else {
            [0u8; 32]
        };

        // v9: generate queue auth keypair (used as sndAuthKey in NEW, and for DH auth)
        let queue_auth_priv = StaticSecret::random_from_rng(OsRng);
        let queue_auth_pub = X25519Public::from(&queue_auth_priv);
        let queue_auth_private = queue_auth_priv.to_bytes();
        let queue_auth_public = *queue_auth_pub.as_bytes();

        // v9 ClientHello: [version Word16][keyHash shortString][sessionAuthSPKI shortString]
        // keyHash = [1B len=32][32B hash]
        // sessionAuthSPKI = [1B len=44][12B X25519 SPKI header][32B raw key]
        let mut client_hello = Vec::with_capacity(80);
        client_hello.extend_from_slice(&[0x00, 0x09]); // version Word16
        client_hello.push(32u8);
        client_hello.extend_from_slice(&server_key_hash);
        client_hello.push(44u8);
        client_hello.extend_from_slice(&X25519_SPKI_HEADER);
        client_hello.extend_from_slice(&session_auth_public);

        // Handshake block: [Word16 BE len][content][zero padding to 16384]
        let mut hblock = vec![0u8; SMP_BLOCK_SIZE];
        let len = client_hello.len() as u16;
        hblock[0] = (len >> 8) as u8;
        hblock[1] = len as u8;
        hblock[2..2 + client_hello.len()].copy_from_slice(&client_hello);
        stream.write_all(&hblock).await?;

        tracing::info!(
            "SMP ClientHello sent (version 9, key_hash={}...)",
            hex::encode(&server_key_hash[..4])
        );

        Ok(Self {
            stream,
            session_id,
            server_key_hash,
            version: negotiated_version,
            session_auth_private,
            session_auth_public,
            server_session_public,
            session_shared_secret,
            queue_auth_private,
            queue_auth_public,
        })
    }

    /// Write a command block (all SMP commands after handshake).
    /// Format: [Word16 content_len][1B txCount][2B txLen][transmission][padding]
    pub async fn write_command_block(&mut self, transmission: &[u8]) -> Result<(), SmpError> {
        let mut block = vec![PADDING_BYTE; SMP_BLOCK_SIZE];

        let tx_len = transmission.len() as u16;
        let content_len = (1 + 2 + transmission.len()) as u16;

        // [Word16 BE content_len] - same framing as server responses
        block[0] = (content_len >> 8) as u8;
        block[1] = content_len as u8;
        // [1B txCount]
        block[2] = 1;
        // [2B txLen BE]
        block[3] = (tx_len >> 8) as u8;
        block[4] = tx_len as u8;
        // [transmission]
        block[5..5 + transmission.len()].copy_from_slice(transmission);

        self.stream.write_all(&block).await?;
        Ok(())
    }

    /// Read and parse all responses from the next block.
    pub async fn read_responses(&mut self) -> Result<Vec<ServerResponse>, SmpError> {
        let mut block = vec![0u8; SMP_BLOCK_SIZE];
        self.stream.read_exact(&mut block).await?;
        let content_len = u16::from_be_bytes([block[0], block[1]]) as usize;
        tracing::debug!("SMP raw block content_len={}, first 150 bytes: {}",
            content_len, hex::encode(&block[..block.len().min(150)]));
        parse_block_responses(&block, self.version)
    }

    /// Build a signed transmission for recipient commands (SUB, KEY, ACK, NEW).
    /// v9: corrId = 24 random bytes, auth = crypto_box(SHA512(sessionId||corrId||entityId||cmd))
    /// v<9: Ed25519 sign corrId+entityId+cmd
    pub fn build_signed_transmission(
        &self,
        auth_key: &SigningKey,
        _corr_id: &[u8],
        entity_id: &[u8],
        command: &[u8],
    ) -> Vec<u8> {
        use rand::RngCore;

        // v9: generate 24 random bytes for corrId
        let corr_id: Vec<u8> = if self.version >= 9 {
            let mut c = vec![0u8; 24];
            rand::rngs::OsRng.fill_bytes(&mut c);
            c
        } else {
            _corr_id.to_vec()
        };

        // Wire body = [corrIdLen][corrId][entityIdLen][entityId][command]
        let mut trans_body = Vec::new();
        trans_body.push(corr_id.len() as u8);
        trans_body.extend_from_slice(&corr_id);
        trans_body.push(entity_id.len() as u8);
        trans_body.extend_from_slice(entity_id);
        trans_body.extend_from_slice(command);

        if self.version >= 9 {
            // v9 auth: crypto_box of SHA512(sessionId || trans_body)
            // tToSend (on wire) = trans_body (NO sessionId)
            // tForAuth (in SHA512) = sessionId || trans_body
            tracing::debug!(
                "AUTH input: sessId={}... corr_id(len={})={} entity_id(len={})={} cmd(len={})={}",
                hex::encode(&self.session_id[..4]),
                corr_id.len(), hex::encode(&corr_id),
                entity_id.len(), hex::encode(entity_id),
                command.len(), hex::encode(&command[..8.min(command.len())])
            );
            tracing::debug!(
                "AUTH trans_body ({} bytes): {}",
                trans_body.len(),
                hex::encode(&trans_body[..trans_body.len().min(40)])
            );
            use sha2::{Digest, Sha512};
            let mut hasher = Sha512::new();
            hasher.update([self.session_id.len() as u8]); // length prefix (0x20 = 32)
            hasher.update(self.session_id);
            hasher.update(&trans_body);
            let hash = hasher.finalize(); // 64 bytes
            tracing::debug!("AUTH hash (64B): {}", hex::encode(&hash[..16]));

            // NaCl crypto_box: X25519 DH + HSalsa20 + XSalsa20-Poly1305
            // DH = queue_auth_private × server_session_public (queue-specific key!)
            use crypto_box::{aead::Aead, SalsaBox, PublicKey, SecretKey};
            let our_priv = SecretKey::from(self.queue_auth_private);
            let srv_pub = PublicKey::from(self.server_session_public);
            let nacl_box = SalsaBox::new(&srv_pub, &our_priv);
            let nonce = crypto_box::Nonce::from_slice(&corr_id);
            let auth_data = nacl_box.encrypt(nonce, hash.as_ref())
                .expect("crypto_box auth failed");
            // auth_data = 64 (hash) + 16 (tag) = 80 bytes

            let mut tx = Vec::new();
            tx.push(auth_data.len() as u8); // 80
            tx.extend_from_slice(&auth_data);
            tx.extend_from_slice(&trans_body);
            tx
        } else {
            // v<9: Ed25519 signature
            let mut to_sign = Vec::new();
            to_sign.extend_from_slice(&corr_id);
            to_sign.extend_from_slice(entity_id);
            to_sign.extend_from_slice(command);
            let signature = auth_key.sign(&to_sign);

            let mut tx = Vec::new();
            tx.push(64u8);
            tx.extend_from_slice(&signature.to_bytes());
            tx.extend_from_slice(&trans_body);
            tx
        }
    }

    /// Build an unsigned transmission (for SKEY - sender command).
    pub fn build_unsigned_transmission(
        &self,
        corr_id: &[u8],
        entity_id: &[u8],
        command: &[u8],
    ) -> Vec<u8> {
        let mut tx = Vec::new();
        tx.push(0u8); // no signature (v9: no session_id either)
        tx.push(corr_id.len() as u8);
        tx.extend_from_slice(corr_id);
        tx.push(entity_id.len() as u8);
        tx.extend_from_slice(entity_id);
        tx.extend_from_slice(command);
        tx
    }
}

/// Compute SHA-256 of a certificate's DER bytes (for fingerprint matching).
pub fn cert_der_sha256(der: &[u8]) -> [u8; 32] {
    Sha256::digest(der).into()
}

// ---------------------------------------------------------------------------
// Response parsing
// ---------------------------------------------------------------------------

/// SMP server response types.
#[derive(Debug)]
pub enum ServerResponse {
    Ok,
    Ids {
        rcv_id: [u8; 24],
        snd_id: [u8; 24],
        srv_dh_public: [u8; 32],
    },
    Msg {
        msg_id: [u8; 24],
        body: Vec<u8>,
    },
    End,
    Err(String),
    Unknown(Vec<u8>),
}

/// Scan DER bytes for X25519 OID and extract the 32-byte raw public key.
/// X25519 SPKI: [30 2a 30 05 06 03 2b 65 6e 03 21 00][32B raw key]
/// We scan for the OID bytes (06 03 2b 65 6e), skip to the BIT STRING,
/// then take 32 bytes.
fn extract_x25519_key(der: &[u8]) -> Option<[u8; 32]> {
    let oid: [u8; 5] = [0x06, 0x03, 0x2b, 0x65, 0x6e];
    for i in 0..der.len().saturating_sub(45) {
        if der[i..i + 5] == oid {
            // After OID (5 bytes): BIT STRING tag (03) + length (21 = 33) + unused bits (00)
            // Then 32 bytes of raw key
            let key_start = i + 5 + 3; // OID(5) + BIT STRING header(3)
            if key_start + 32 <= der.len() {
                let mut key = [0u8; 32];
                key.copy_from_slice(&der[key_start..key_start + 32]);
                return Some(key);
            }
        }
    }
    None
}

fn parse_first_transmission(block: &[u8]) -> Result<&[u8], SmpError> {
    if block.len() < 3 {
        return Err(SmpError::Protocol("Block too short".into()));
    }
    let tx_len = u16::from_be_bytes([block[1], block[2]]) as usize;
    if 3 + tx_len > block.len() {
        return Err(SmpError::Protocol("Transmission exceeds block".into()));
    }
    Ok(&block[3..3 + tx_len])
}

fn parse_block_responses(block: &[u8], version: u16) -> Result<Vec<ServerResponse>, SmpError> {
    // Server response blocks: [Word16 content_len][content][padding]
    let content_len = u16::from_be_bytes([block[0], block[1]]) as usize;
    if content_len == 0 || content_len > SMP_BLOCK_SIZE - 2 {
        return Err(SmpError::Protocol(format!("Bad content_len: {content_len}")));
    }
    let content = &block[2..2 + content_len];

    let tx_count = content[0] as usize;
    if tx_count == 0 {
        return Err(SmpError::Protocol("txCount is 0".into()));
    }

    let mut responses = Vec::new();
    let mut offset = 1;

    for _ in 0..tx_count {
        if offset + 2 > content.len() {
            break;
        }
        let tx_len = u16::from_be_bytes([content[offset], content[offset + 1]]) as usize;
        offset += 2;
        if tx_len == 0 || offset + tx_len > content.len() {
            break;
        }
        let tx = &content[offset..offset + tx_len];
        offset += tx_len;
        responses.push(parse_single_response(tx, version));
    }

    Ok(responses)
}

fn parse_single_response(tx: &[u8], version: u16) -> ServerResponse {
    tracing::debug!("SMP response raw ({} bytes): {}", tx.len(),
        hex::encode(&tx[..tx.len().min(80)]));

    // Skip response header to find the command/response part
    // v9:  [sigLen][sig?][corrIdLen][corrId][entityIdLen][entityId][response]
    // v<9: [sigLen][sig?][sessIdLen][sessId][corrIdLen][corrId][entityIdLen][entityId][response]
    let mut offset = 0;
    // Skip sigLen + signature (server responses usually have sigLen=0)
    if !tx.is_empty() {
        let sig_len = tx[0] as usize;
        tracing::debug!("Response parse: sig_len={}", sig_len);
        offset = 1 + sig_len;
    }
    // v<9: skip session_id
    if version < 9 && offset < tx.len() {
        let sess_len = tx[offset] as usize;
        tracing::debug!("Response parse: sess_len={} at offset {}", sess_len, offset);
        offset += 1 + sess_len;
    }
    // Parse corrId (variable length: 24 bytes for v9)
    if offset < tx.len() {
        let corr_len = tx[offset] as usize;
        tracing::debug!("Response parse: corr_len={} at offset {}", corr_len, offset);
        offset += 1 + corr_len;
    }
    // Parse entityId
    if offset < tx.len() {
        let eid_len = tx[offset] as usize;
        tracing::debug!("Response parse: eid_len={} at offset {}", eid_len, offset);
        offset += 1 + eid_len;
    }

    let cmd_data = if offset < tx.len() { &tx[offset..] } else { tx };
    tracing::debug!("SMP response cmd at offset {}: {}",
        offset, hex::encode(&cmd_data[..cmd_data.len().min(40)]));

    // Match response tags from the command data
    if let Some(pos) = find_response_tag(cmd_data) {
        let abs_pos = pos; // position within cmd_data
        match cmd_data[abs_pos] {
            b'O' if cmd_data.get(abs_pos + 1) == Some(&b'K') => ServerResponse::Ok,
            b'I' if cmd_data.get(abs_pos + 1) == Some(&b'D') && cmd_data.get(abs_pos + 2) == Some(&b'S') => {
                parse_ids_response(&cmd_data[abs_pos..])
            }
            b'M' if cmd_data.get(abs_pos + 1) == Some(&b'S') && cmd_data.get(abs_pos + 2) == Some(&b'G') => {
                parse_msg_response(&cmd_data[abs_pos..], version)
            }
            b'E' if cmd_data.get(abs_pos + 1) == Some(&b'N') && cmd_data.get(abs_pos + 2) == Some(&b'D') => {
                ServerResponse::End
            }
            b'E' if cmd_data.get(abs_pos + 1) == Some(&b'R') && cmd_data.get(abs_pos + 2) == Some(&b'R') => {
                ServerResponse::Err(String::from_utf8_lossy(&cmd_data[abs_pos..]).to_string())
            }
            _ => ServerResponse::Unknown(tx.to_vec()),
        }
    } else {
        ServerResponse::Unknown(tx.to_vec())
    }
}

fn find_response_tag(tx: &[u8]) -> Option<usize> {
    if tx.len() < 2 {
        return None;
    }
    for i in 0..tx.len() - 1 {
        match (tx[i], tx.get(i + 1), tx.get(i + 2)) {
            (b'O', Some(&b'K'), _) => return Some(i),
            (b'I', Some(&b'D'), Some(&b'S')) => return Some(i),
            (b'M', Some(&b'S'), Some(&b'G')) => return Some(i),
            (b'E', Some(&b'N'), Some(&b'D')) => return Some(i),
            (b'E', Some(&b'R'), Some(&b'R')) => return Some(i),
            _ => {}
        }
    }
    None
}

fn parse_ids_response(data: &[u8]) -> ServerResponse {
    tracing::debug!("IDS raw response ({} bytes): {}", data.len(), hex::encode(data));

    if data.len() < 4 + 24 + 1 + 24 {
        return ServerResponse::Unknown(data.to_vec());
    }
    // Try v9 binary format first: "IDS " + [len=24][rcv_id] + [len=24][snd_id] + [len=44][srv_dh_spki]
    let mut pos = 4; // skip "IDS "
    tracing::debug!("IDS after 'IDS ': byte[4]=0x{:02x} (expected 24 for len prefix, or raw ID start)", data[pos]);

    // Check if there's a length prefix (v9) or raw bytes (v<9)
    let has_len_prefix = data[pos] == 24;
    if has_len_prefix {
        pos += 1;
    }
    let mut rcv_id = [0u8; 24];
    rcv_id.copy_from_slice(&data[pos..pos + 24]);
    tracing::debug!("IDS rcv_id (len_prefix={}): {}", has_len_prefix, hex::encode(&rcv_id[..8]));
    pos += 24;

    if !has_len_prefix {
        pos += 1; // space between IDs in v<9
    } else if pos < data.len() && data[pos] == 24 {
        pos += 1; // length prefix for snd_id in v9
    }

    let mut snd_id = [0u8; 24];
    if pos + 24 <= data.len() {
        snd_id.copy_from_slice(&data[pos..pos + 24]);
    }
    tracing::debug!("IDS snd_id: {}", hex::encode(&snd_id[..8]));

    // Scan forward for SPKI header (30 2a 30 05 06 03 2b 65 6e) of srv_dh_public
    let mut srv_dh_public = [0u8; 32];
    let spki_marker: [u8; 9] = [0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x6e];
    if let Some(spki_pos) = data.windows(9).position(|w| w == spki_marker) {
        tracing::debug!("IDS srv_dh SPKI found at offset {}", spki_pos);
        if spki_pos + 12 + 32 <= data.len() {
            srv_dh_public.copy_from_slice(&data[spki_pos + 12..spki_pos + 44]);
        }
    }
    tracing::debug!("IDS srv_dh_public: {}", hex::encode(&srv_dh_public[..8]));

    ServerResponse::Ids {
        rcv_id,
        snd_id,
        srv_dh_public,
    }
}

fn parse_msg_response(data: &[u8], version: u16) -> ServerResponse {
    if data.len() < 4 + 1 + 24 {
        tracing::warn!("parse_msg_response: too short ({})", data.len());
        return ServerResponse::Unknown(data.to_vec());
    }

    // Check "MSG " tag
    if &data[0..4] != b"MSG " {
        tracing::error!(
            "parse_msg_response: expected \"MSG \", got {:?}",
            &data[..4]
        );
        return ServerResponse::Unknown(data.to_vec());
    }

    // Skip "MSG "
    let mut pos = 4;

    // v9: shortString length prefix 0x18 for msgId
    let msg_id_byte = data[pos];
    let has_len_prefix = version >= 9 && msg_id_byte == 24;
    if has_len_prefix {
        pos += 1;
    } else {
        tracing::warn!(
            "msgId length byte unexpected: 0x{:02x} (v={version})",
            msg_id_byte
        );
    }

    if pos + 24 > data.len() {
        tracing::error!("parse_msg_response: truncated at msgId body");
        return ServerResponse::Unknown(data.to_vec());
    }
    let mut msg_id = [0u8; 24];
    msg_id.copy_from_slice(&data[pos..pos + 24]);
    pos += 24;

    // Body is Tail-encoded in SimpleX wire format - all remaining bytes of the
    // transmission, NO length prefix. Verified against simplexmq Protocol.hs:1839:
    //   MSG RcvMessage {msgId, msgBody = EncRcvMsgBody body} -> e (MSG_, ' ', msgId, Tail body)
    // and Encoding.hs:131: `smpP = Tail <$> A.takeByteString`.
    let body = if pos < data.len() {
        data[pos..].to_vec()
    } else {
        Vec::new()
    };
    tracing::debug!(
        "parse_msg_response v{}: msgId={}..., body={} bytes",
        version,
        hex::encode(&msg_id[..4]),
        body.len()
    );

    ServerResponse::Msg { msg_id, body }
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum SmpError {
    Io(std::io::Error),
    Protocol(String),
    Tls(String),
}

impl From<std::io::Error> for SmpError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl std::fmt::Display for SmpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "IO: {e}"),
            Self::Protocol(e) => write!(f, "Protocol: {e}"),
            Self::Tls(e) => write!(f, "TLS: {e}"),
        }
    }
}
