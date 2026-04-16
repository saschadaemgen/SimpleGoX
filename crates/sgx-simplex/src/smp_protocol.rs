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

        // hello[0..4] = version + other fields
        // hello[4] = sessionId length (must be 32)
        // hello[5..37] = sessionId
        let sess_id_len = hello[4] as usize;
        if sess_id_len != 32 {
            return Err(SmpError::Protocol(format!(
                "Bad sessionId length: {sess_id_len}"
            )));
        }
        let mut session_id = [0u8; 32];
        session_id.copy_from_slice(&hello[5..37]);

        tracing::info!(
            "SMP ServerHello: session_id={}...",
            hex::encode(&session_id[..4])
        );

        // Send ClientHello: [version=6][key_hash_len=32][key_hash]
        let mut client_hello = Vec::with_capacity(35);
        client_hello.push(0x00);
        client_hello.push(0x06); // version 6
        client_hello.push(32);
        client_hello.extend_from_slice(&server_key_hash);

        // Handshake block: [Word16 BE len][content][zero padding to 16384]
        let mut hblock = vec![0u8; SMP_BLOCK_SIZE];
        let len = client_hello.len() as u16;
        hblock[0] = (len >> 8) as u8;
        hblock[1] = len as u8;
        hblock[2..2 + client_hello.len()].copy_from_slice(&client_hello);
        stream.write_all(&hblock).await?;

        tracing::info!(
            "SMP ClientHello sent (version 6, key_hash={}...)",
            hex::encode(&server_key_hash[..4])
        );

        Ok(Self {
            stream,
            session_id,
            server_key_hash,
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
        parse_block_responses(&block)
    }

    /// Build a signed transmission for recipient commands (SUB, KEY, ACK, NEW).
    pub fn build_signed_transmission(
        &self,
        auth_key: &SigningKey,
        corr_id: &[u8],
        entity_id: &[u8],
        command: &[u8],
    ) -> Vec<u8> {
        // trans_body = [corrIdLen][corrId][entityIdLen][entityId][command]
        let mut trans_body = Vec::new();
        trans_body.push(corr_id.len() as u8);
        trans_body.extend_from_slice(corr_id);
        trans_body.push(entity_id.len() as u8);
        trans_body.extend_from_slice(entity_id);
        trans_body.extend_from_slice(command);

        // Sign: [0x20][sessionId][trans_body]
        let mut to_sign = Vec::new();
        to_sign.push(32u8);
        to_sign.extend_from_slice(&self.session_id);
        to_sign.extend_from_slice(&trans_body);

        let signature = auth_key.sign(&to_sign);

        // Final: [sigLen=64][sig][sessLen=32][sessionId][trans_body]
        let mut tx = Vec::new();
        tx.push(64u8);
        tx.extend_from_slice(&signature.to_bytes());
        tx.push(32u8);
        tx.extend_from_slice(&self.session_id);
        tx.extend_from_slice(&trans_body);
        tx
    }

    /// Build an unsigned transmission (for SKEY - sender command).
    pub fn build_unsigned_transmission(
        &self,
        corr_id: &[u8],
        entity_id: &[u8],
        command: &[u8],
    ) -> Vec<u8> {
        let mut tx = Vec::new();
        tx.push(0u8); // no signature
        tx.push(32u8);
        tx.extend_from_slice(&self.session_id);
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

fn parse_block_responses(block: &[u8]) -> Result<Vec<ServerResponse>, SmpError> {
    // Server response blocks: [Word16 content_len][content][padding]
    // Content starts with txCount, then individual transmissions.
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
    let mut offset = 1; // skip txCount byte

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
        responses.push(parse_single_response(tx));
    }

    Ok(responses)
}

fn parse_single_response(tx: &[u8]) -> ServerResponse {
    // Scan for known response tags
    if let Some(pos) = find_response_tag(tx) {
        match tx[pos] {
            b'O' if tx.get(pos + 1) == Some(&b'K') => ServerResponse::Ok,
            b'I' if tx.get(pos + 1) == Some(&b'D') && tx.get(pos + 2) == Some(&b'S') => {
                parse_ids_response(&tx[pos..])
            }
            b'M' if tx.get(pos + 1) == Some(&b'S') && tx.get(pos + 2) == Some(&b'G') => {
                parse_msg_response(&tx[pos..])
            }
            b'E' if tx.get(pos + 1) == Some(&b'N') && tx.get(pos + 2) == Some(&b'D') => {
                ServerResponse::End
            }
            b'E' if tx.get(pos + 1) == Some(&b'R') && tx.get(pos + 2) == Some(&b'R') => {
                ServerResponse::Err(String::from_utf8_lossy(&tx[pos..]).to_string())
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
    if data.len() < 4 + 24 + 1 + 24 {
        return ServerResponse::Unknown(data.to_vec());
    }
    let mut pos = 4; // skip "IDS "
    let mut rcv_id = [0u8; 24];
    rcv_id.copy_from_slice(&data[pos..pos + 24]);
    pos += 25; // +24 data +1 space
    let mut snd_id = [0u8; 24];
    snd_id.copy_from_slice(&data[pos..pos + 24]);
    pos += 24;
    pos += 2; // snd_secure flag + space
    let mut srv_dh_public = [0u8; 32];
    if data.len() >= pos + 44 {
        srv_dh_public.copy_from_slice(&data[pos + 12..pos + 44]);
    }
    ServerResponse::Ids {
        rcv_id,
        snd_id,
        srv_dh_public,
    }
}

fn parse_msg_response(data: &[u8]) -> ServerResponse {
    if data.len() < 4 + 24 {
        return ServerResponse::Unknown(data.to_vec());
    }
    let mut msg_id = [0u8; 24];
    msg_id.copy_from_slice(&data[4..28]);
    let body = if data.len() > 29 {
        data[29..].to_vec()
    } else {
        Vec::new()
    };
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
