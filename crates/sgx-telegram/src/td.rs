//! TDLib event pump.
//!
//! tdlib_rs::receive() must be called continuously to pump TDLib's event queue.
//! Internally, receive() checks each response for an @extra field:
//! - If @extra is present, the response is routed to the function caller
//!   via the internal Observer (so functions::* get their answers).
//! - If no @extra, it's an update and returned to us.
//!
//! Without this pump running, function calls hang because nothing
//! is calling td_receive to pull responses off the wire.

use tdlib_rs::enums::Update;
use tokio::sync::broadcast;
use tracing::warn;

/// Start the TDLib receive pump on a dedicated OS thread.
/// Returns a broadcast sender for updates (messages, auth changes, etc).
pub fn start_pump() -> broadcast::Sender<Update> {
    let (tx, _) = broadcast::channel::<Update>(512);
    let tx_clone = tx.clone();

    std::thread::spawn(move || {
        loop {
            // receive() blocks up to 2s internally.
            // It routes @extra responses to function callers automatically.
            // Only pure updates (no @extra) are returned to us here.
            match tdlib_rs::receive() {
                Some((update, _client_id)) => {
                    // Forward updates to any subscribers (for future streaming)
                    let _ = tx_clone.send(update);
                }
                None => {
                    // No events, loop continues (receive already waited ~2s)
                }
            }
        }
    });

    tx
}
