//! SMP command builders - each returns a transmission ready for write_command_block().

use crate::smp_protocol::*;
use ed25519_dalek::SigningKey;

/// NEW command - create a recipient queue.
///
/// v9 format: "NEW " + X25519_SPKI(rcvDhKey) + X25519_SPKI(sndAuthKey) + "0ST"
/// v<9 format: "NEW " + Ed25519_SPKI(auth) + X25519_SPKI(dh) + "S"
pub fn cmd_new(
    conn: &SmpConnection,
    rcv_auth: &SigningKey,
    _rcv_auth_x25519_public: &[u8; 32], // unused in v9 (uses conn.queue_auth_public)
    rcv_dh_public: &[u8; 32],
) -> Vec<u8> {
    let mut cmd = Vec::new();
    cmd.extend_from_slice(b"NEW ");

    if conn.version >= 9 {
        // v9: rcvAuthKey FIRST, then rcvDhKey (both X25519)
        // rcvAuthKey = queue_auth_public (whose private is used for DH auth)
        cmd.push(0x2C); // 44 = X25519 SPKI length
        cmd.extend_from_slice(&X25519_SPKI_HEADER);
        cmd.extend_from_slice(&conn.queue_auth_public);
        cmd.push(0x2C);
        cmd.extend_from_slice(&X25519_SPKI_HEADER);
        cmd.extend_from_slice(rcv_dh_public);
        cmd.push(b'0'); // basicAuth Nothing
        cmd.push(b'S'); // subscribeMode
        cmd.push(b'T'); // sndSecure = True
    } else {
        // v<9: Ed25519 auth first, then X25519 dh
        cmd.push(44);
        cmd.extend_from_slice(&ED25519_SPKI_HEADER);
        cmd.extend_from_slice(rcv_auth.verifying_key().as_bytes());
        cmd.push(44);
        cmd.extend_from_slice(&X25519_SPKI_HEADER);
        cmd.extend_from_slice(rcv_dh_public);
        cmd.push(b'S');
    }

    tracing::debug!("NEW cmd raw ({} bytes, v{}): {}", cmd.len(), conn.version, hex::encode(&cmd[..cmd.len().min(20)]));
    let tx = conn.build_signed_transmission(rcv_auth, b"1", b"", &cmd);
    tracing::debug!("NEW tx ({} bytes) first 140: {}", tx.len(), hex::encode(&tx[..tx.len().min(140)]));
    tx
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

/// SKEY command on OUR queue - register sender auth key so peer can send to us.
/// v9 format: "SKEY " + [0x2C][X25519 SPKI 44B]
/// entityId = rcv_id (our queue)
pub fn cmd_skey_signed(
    conn: &SmpConnection,
    rcv_auth: &SigningKey,
    rcv_id: &[u8; 24],
    snd_auth_public: &[u8; 32],
) -> Vec<u8> {
    let mut cmd = Vec::new();
    cmd.extend_from_slice(b"SKEY ");
    cmd.push(0x2C); // 44 = X25519 SPKI length
    if conn.version >= 9 {
        // v9: X25519 SPKI
        cmd.extend_from_slice(&X25519_SPKI_HEADER);
    } else {
        // v<9: Ed25519 SPKI
        cmd.extend_from_slice(&ED25519_SPKI_HEADER);
    }
    cmd.extend_from_slice(snd_auth_public);
    conn.build_signed_transmission(rcv_auth, b"K", rcv_id, &cmd)
}

/// KEY command - register peer's sender auth key on our queue.
/// v9: "SKEY " + [0x2C][X25519 SPKI 44B], entityId = rcv_id
/// v<9: "KEY " + [0x2C][Ed25519 SPKI 44B], entityId = rcv_id
pub fn cmd_key(
    conn: &SmpConnection,
    rcv_auth: &SigningKey,
    rcv_id: &[u8; 24],
    peer_snd_auth_pub: &[u8; 32],
) -> Vec<u8> {
    let mut cmd = Vec::new();
    if conn.version >= 9 {
        cmd.extend_from_slice(b"SKEY ");
        cmd.push(0x2C); // 44 = X25519 SPKI length
        cmd.extend_from_slice(&X25519_SPKI_HEADER);
    } else {
        cmd.extend_from_slice(b"KEY ");
        cmd.push(0x2C);
        cmd.extend_from_slice(&ED25519_SPKI_HEADER);
    }
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

/// SEND command (signed) - send a message to a secured queue.
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

/// SEND command (unsigned) - send to an unsecured queue (e.g. AgentInvitation).
pub fn cmd_send_unsigned(
    conn: &SmpConnection,
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
    conn.build_unsigned_transmission(&[corr_id], peer_snd_id, &cmd)
}
