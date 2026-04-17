//! Double Ratchet state and operations for the Bob-side flow
//! (party that initiated the connection, receiving the first Alice message).
//!
//! Scope: first-message decrypt only. Skipped keys, out-of-order messages,
//! and KEM integration are deferred.

use crate::crypto::x3dh::X3dhBobResult;

/// Minimal Bob-side Double Ratchet state.
#[derive(Debug, Clone)]
pub struct BobRatchet {
    /// Root Key (rcRK).
    pub root_key: [u8; 32],
    /// Our X448 sending ratchet private key (rcDHRs).
    pub our_dhrs_priv: [u8; 56],
    /// Receiving chain key (rcCKr), set after first DHRatchet step.
    pub receiving_chain_key: Option<[u8; 32]>,
    /// Sending chain key (rcCKs), set after first DHRatchet step.
    pub sending_chain_key: Option<[u8; 32]>,
    /// Current next-header-key for receiving direction (rcNHKr).
    pub next_header_key_receive: [u8; 32],
    /// Current next-header-key for sending direction (rcNHKs).
    pub next_header_key_send: [u8; 32],
    /// Associated data from X3DH (112 bytes: peer_pub1 || our_pub1).
    pub assoc_data: Vec<u8>,
    /// Message counter - sending.
    pub ns: u32,
    /// Message counter - receiving.
    pub nr: u32,
    /// Previous-chain length (PN).
    pub pn: u32,
}

/// Bob-side initialization from X3DH result and our persisted second private key.
///
/// Mirrors simplexmq Ratchet.hs:674-699 `initRcvRatchet`.
///
/// Key mapping (note the swap):
/// - `rcRK`    = `x3dh.root_key`
/// - `rcDHRs`  = `our_priv2` (passed in separately, not via X3DH result)
/// - `rcNHKs`  = `x3dh.receiving_next_header_key`  (swapped!)
/// - `rcNHKr`  = `x3dh.sending_header_key`         (swapped!)
/// - `rcSnd`   = None (no sending chain yet)
/// - `rcRcv`   = None (no receiving chain yet)
///
/// The swap reflects that `sndHK` is named from Alice's perspective
/// (her outgoing key) but represents our incoming direction.
pub fn init_bob_ratchet(x3dh: &X3dhBobResult, our_priv2: [u8; 56]) -> BobRatchet {
    BobRatchet {
        root_key: x3dh.root_key,
        our_dhrs_priv: our_priv2,
        receiving_chain_key: None,
        sending_chain_key: None,
        next_header_key_receive: x3dh.sending_header_key, // swap
        next_header_key_send: x3dh.receiving_next_header_key, // swap
        assoc_data: x3dh.assoc_data.clone(),
        ns: 0,
        nr: 0,
        pn: 0,
    }
}
