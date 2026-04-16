//! SMP command builders - each returns a transmission ready for write_command_block().

use crate::smp_protocol::*;
use ed25519_dalek::SigningKey;

/// NEW command - create a recipient queue.
/// Format: "NEW " + Ed25519_SPKI(auth) + X25519_SPKI(dh) + 'S'
pub fn cmd_new(
    conn: &SmpConnection,
    rcv_auth: &SigningKey,
    rcv_dh_public: &[u8; 32],
) -> Vec<u8> {
    let mut cmd = Vec::new();
    cmd.extend_from_slice(b"NEW ");
    cmd.push(44);
    cmd.extend_from_slice(&ED25519_SPKI_HEADER);
    cmd.extend_from_slice(rcv_auth.verifying_key().as_bytes());
    cmd.push(44);
    cmd.extend_from_slice(&X25519_SPKI_HEADER);
    cmd.extend_from_slice(rcv_dh_public);
    cmd.push(b'S'); // subscribe immediately

    conn.build_signed_transmission(rcv_auth, b"1", b"", &cmd)
}

/// SUB command - subscribe to a recipient queue.
pub fn cmd_sub(conn: &SmpConnection, rcv_auth: &SigningKey, rcv_id: &[u8; 24]) -> Vec<u8> {
    conn.build_signed_transmission(rcv_auth, b"S", rcv_id, b"S")
}

/// SKEY command - secure peer's queue with our sender auth key.
/// Signed with snd_auth key (sender authenticates to the queue).
pub fn cmd_skey(
    conn: &SmpConnection,
    snd_auth: &SigningKey,
    peer_snd_id: &[u8],
) -> Vec<u8> {
    let mut cmd = Vec::new();
    cmd.extend_from_slice(b"SKEY ");
    cmd.push(44);
    cmd.extend_from_slice(&ED25519_SPKI_HEADER);
    cmd.extend_from_slice(snd_auth.verifying_key().as_bytes());
    conn.build_signed_transmission(snd_auth, b"K", peer_snd_id, &cmd)
}

/// KEY command - register peer's sender auth key on our queue.
pub fn cmd_key(
    conn: &SmpConnection,
    rcv_auth: &SigningKey,
    rcv_id: &[u8; 24],
    peer_snd_auth_pub: &[u8; 32],
) -> Vec<u8> {
    let mut cmd = Vec::new();
    cmd.extend_from_slice(b"KEY ");
    cmd.push(0x2C);
    cmd.push(44);
    cmd.extend_from_slice(&ED25519_SPKI_HEADER);
    cmd.extend_from_slice(peer_snd_auth_pub);
    conn.build_signed_transmission(rcv_auth, b"E", rcv_id, &cmd)
}

/// ACK command - acknowledge message delivery.
pub fn cmd_ack(
    conn: &SmpConnection,
    rcv_auth: &SigningKey,
    rcv_id: &[u8; 24],
    msg_id: &[u8; 24],
) -> Vec<u8> {
    let mut cmd = Vec::new();
    cmd.extend_from_slice(b"ACK ");
    cmd.extend_from_slice(msg_id);
    conn.build_signed_transmission(rcv_auth, b"A", rcv_id, &cmd)
}

/// SEND command - send a message to a queue.
pub fn cmd_send(
    conn: &SmpConnection,
    snd_auth: &SigningKey,
    peer_snd_id: &[u8],
    client_msg: &[u8],
    corr_id: u8,
    notify: bool,
) -> Vec<u8> {
    let mut cmd = Vec::new();
    cmd.extend_from_slice(b"SEND ");
    cmd.push(if notify { b'T' } else { b'F' });
    cmd.push(b' ');
    cmd.extend_from_slice(client_msg);
    conn.build_signed_transmission(snd_auth, &[corr_id], peer_snd_id, &cmd)
}
