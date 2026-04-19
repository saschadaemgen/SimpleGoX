//! Per-contact session control plumbing.
//!
//! Each established SimpleX contact runs a long-lived background task that
//! owns the SMP receive stream and Double Ratchet state for that contact.
//! [`ContactCommand`] is the channel-injected control message that lets
//! gRPC handlers reach into a running task to request outbound operations
//! such as sending a chat text without taking ownership of the task's
//! state.
//!
//! Lifecycle: a [`ContactSessionHandle`] is inserted into
//! `SimplexService::contact_sessions` immediately before the contact's
//! background task is spawned. The matching [`mpsc::Receiver`] is moved
//! into the task. On task exit (stream end, error, or sender drop) the
//! task removes its own entry from the map.

use tokio::sync::{mpsc, oneshot};

/// Bounded capacity for the per-contact command channel. Picked small so
/// that bugs in the producer side surface as backpressure rather than
/// unbounded memory growth.
pub const CONTACT_COMMAND_CHANNEL_CAPACITY: usize = 32;

/// Commands injected into a running contact session loop from outside.
#[derive(Debug)]
pub enum ContactCommand {
    /// Send a plaintext chat message on this contact's reply queue. The
    /// result is delivered through `reply` once the SMP server has
    /// acknowledged the SEND or an error path has been taken.
    SendText {
        body: String,
        reply: oneshot::Sender<Result<SendTextResult, ContactSendError>>,
    },
}

/// Successful outcome of a [`ContactCommand::SendText`] request.
#[derive(Debug, Clone)]
pub struct SendTextResult {
    /// Monotonic per-contact send id (APrivHeader.sndMsgId after advance).
    pub msg_id: u64,
    /// Wall-clock time the SMP server returned Ok, in epoch milliseconds.
    pub timestamp_ms: i64,
}

/// Failure mode for [`ContactCommand::SendText`].
#[derive(Debug, Clone)]
pub enum ContactSendError {
    /// No running session for this contact id (handshake never completed
    /// or task already exited).
    ContactNotFound,
    /// The contact session task exists but its in-memory ratchet state
    /// is missing - typically a logic error.
    RatchetStateMissing,
    /// Encryption pipeline failed before SEND was attempted. Safe to retry.
    EncryptionFailed(String),
    /// SMP server did not return Ok. The ratchet has NOT been advanced;
    /// the same call can be retried with the same chain key.
    SmpSendFailed(String),
    /// SQLite write failed after a successful SEND. The send did happen
    /// but the local message log may be inconsistent.
    PersistenceFailed(String),
}

impl std::fmt::Display for ContactSendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ContactNotFound => write!(f, "contact session not found"),
            Self::RatchetStateMissing => write!(f, "ratchet state missing"),
            Self::EncryptionFailed(m) => write!(f, "encryption failed: {m}"),
            Self::SmpSendFailed(m) => write!(f, "SMP send failed: {m}"),
            Self::PersistenceFailed(m) => write!(f, "persistence failed: {m}"),
        }
    }
}

impl std::error::Error for ContactSendError {}

/// Handle stored in `SimplexService::contact_sessions` for one running
/// contact session. Holding the `tx` keeps the session task's command
/// receiver alive; dropping it signals the task to shut down.
#[derive(Debug, Clone)]
pub struct ContactSessionHandle {
    pub tx: mpsc::Sender<ContactCommand>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// End-to-end plumbing: create a channel pair, send a SendText command,
    /// have the receiver reply with a stub error, and confirm the caller
    /// observes that exact error through the oneshot. Mirrors the shape the
    /// gRPC handler will use in Phase 4.
    #[tokio::test]
    async fn send_text_command_round_trips_through_oneshot() {
        let (tx, mut rx) = mpsc::channel::<ContactCommand>(CONTACT_COMMAND_CHANNEL_CAPACITY);
        let handle = ContactSessionHandle { tx };

        let (reply_tx, reply_rx) = oneshot::channel();
        handle
            .tx
            .send(ContactCommand::SendText {
                body: "hallo".to_string(),
                reply: reply_tx,
            })
            .await
            .expect("channel send");

        // Stand-in for the contact session loop: pop the command and reply
        // with the Phase 1 stub error.
        let cmd = rx.recv().await.expect("cmd present");
        match cmd {
            ContactCommand::SendText { body, reply } => {
                assert_eq!(body, "hallo");
                let _ = reply.send(Err(ContactSendError::EncryptionFailed(
                    "not yet implemented".into(),
                )));
            }
        }

        let observed = reply_rx.await.expect("reply oneshot");
        match observed {
            Err(ContactSendError::EncryptionFailed(msg)) => {
                assert!(msg.contains("not yet implemented"));
            }
            other => panic!("unexpected reply: {other:?}"),
        }
    }

    /// Dropping the handle closes the channel; the receiver should observe
    /// `None`, which the contact session loop uses as its shutdown signal.
    #[tokio::test]
    async fn dropping_handle_closes_command_channel() {
        let (tx, mut rx) = mpsc::channel::<ContactCommand>(CONTACT_COMMAND_CHANNEL_CAPACITY);
        let handle = ContactSessionHandle { tx };
        drop(handle);
        assert!(rx.recv().await.is_none(), "rx should observe channel close");
    }
}
