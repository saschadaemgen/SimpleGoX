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

use tokio::sync::{mpsc, oneshot, watch};

/// Bounded capacity for the per-contact command channel. Picked small so
/// that bugs in the producer side surface as backpressure rather than
/// unbounded memory growth.
pub const CONTACT_COMMAND_CHANNEL_CAPACITY: usize = 32;

/// Observable state of a contact's SMP connection (Briefing 044 W5).
///
/// The BG loop owns the `watch::Sender` and publishes transitions; the
/// gRPC send handler borrows the `watch::Receiver` to fail-fast when the
/// connection is not available, rather than letting a SendText pile up
/// in the mpsc behind a blocking reconnect call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnState {
    /// TCP + TLS + SMP session is alive, sends are safe to attempt.
    Connected,
    /// A recoverable IO / TLS error was observed; the BG loop is inside
    /// `reconnect_with_backoff`. Sends should fail-fast with a retry hint
    /// because the attempt would queue behind an in-progress reconnect
    /// (select! is blocked in the read arm until reconnect completes).
    Reconnecting,
    /// The reconnect loop exhausted its attempts. The session is gone and
    /// won't self-recover; user action or app restart required.
    Dead,
}

// Briefing 045 W1: cheap mapping to the proto-wire enum so the
// ListSimplexContacts RPC can embed conn_state in each ContactSummary.
// The proto sentinel UNSPECIFIED(=0) is intentionally unreachable from
// this conversion - every Rust variant has a direct target. If the set
// of ConnState variants grows, the compiler will force this match to
// stay exhaustive.
impl From<ConnState> for sgx_proto::messenger::v1::ConnStateProto {
    fn from(s: ConnState) -> Self {
        use sgx_proto::messenger::v1::ConnStateProto as P;
        // prost-build does not strip the `CONN_STATE_` prefix, so the
        // generated variant names retain it in PascalCase form.
        match s {
            ConnState::Connected => P::ConnStateConnected,
            ConnState::Reconnecting => P::ConnStateReconnecting,
            ConnState::Dead => P::ConnStateDead,
        }
    }
}

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
    /// Emit an app-layer PING frame (Briefing 044 W2). Enqueued by a
    /// sibling timer task every 30s to (a) keep NAT / proxy state alive
    /// and (b) prevent the server from expiring our SUB. The routing
    /// through the mpsc channel is mandatory: the PING timer MUST NOT
    /// write to the TLS stream directly - that would race the select!
    /// loop's receive arm and corrupt TLS framing. See W2 / Risk 1.
    ///
    /// No reply channel: failures are logged at warn level and the next
    /// read error triggers the reconnect path (W3) naturally.
    KeepAlive,
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
    /// Briefing 044 W5: connection is mid-reconnect. User should retry
    /// in a few seconds. The message has NOT entered any queue and the
    /// ratchet has not advanced, so retry with the same body is safe.
    Reconnecting(String),
    /// Briefing 044 W5: reconnect loop exhausted its attempts. The
    /// connection is dead until the app restarts or the user explicitly
    /// requests a reconnect.
    Dead(String),
}

impl std::fmt::Display for ContactSendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ContactNotFound => write!(f, "contact session not found"),
            Self::RatchetStateMissing => write!(f, "ratchet state missing"),
            Self::EncryptionFailed(m) => write!(f, "encryption failed: {m}"),
            Self::SmpSendFailed(m) => write!(f, "SMP send failed: {m}"),
            Self::PersistenceFailed(m) => write!(f, "persistence failed: {m}"),
            Self::Reconnecting(m) => write!(f, "connection is reconnecting: {m}"),
            Self::Dead(m) => write!(f, "connection is dead: {m}"),
        }
    }
}

impl std::error::Error for ContactSendError {}

/// Handle stored in `SimplexService::contact_sessions` for one running
/// contact session. Holding the `tx` keeps the session task's command
/// receiver alive; dropping it signals the task to shut down.
///
/// Briefing 044 W5: also carries a [`watch::Receiver<ConnState>`] so the
/// gRPC handler can fail-fast before enqueueing a SendText when the BG
/// loop is mid-reconnect. Without this check a SendText would pile up in
/// the mpsc behind the blocked select! read arm for the entire reconnect
/// window (up to ~3.5 minutes in the worst case).
#[derive(Debug, Clone)]
pub struct ContactSessionHandle {
    pub tx: mpsc::Sender<ContactCommand>,
    pub state_rx: watch::Receiver<ConnState>,
}

impl ContactSessionHandle {
    /// Non-blocking peek at the current connection state. Returns `Connected`
    /// by default if the sender has been dropped (session shutting down);
    /// the subsequent `tx.send` will then error and the caller maps that
    /// to `ContactNotFound` anyway.
    pub fn state(&self) -> ConnState {
        *self.state_rx.borrow()
    }
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
        let (_state_tx, state_rx) = watch::channel(ConnState::Connected);
        let handle = ContactSessionHandle { tx, state_rx };

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
            other => panic!("expected SendText, got {other:?}"),
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
        let (_state_tx, state_rx) = watch::channel(ConnState::Connected);
        let handle = ContactSessionHandle { tx, state_rx };
        drop(handle);
        assert!(rx.recv().await.is_none(), "rx should observe channel close");
    }

    /// Briefing 044 W5: the handle's state accessor reflects whatever
    /// the BG loop publishes through the watch::Sender. Tested in
    /// isolation here because the gRPC handler relies on this behaviour
    /// for fail-fast rejection during reconnect.
    #[tokio::test]
    async fn state_handle_reflects_sender_updates() {
        let (tx, _rx) = mpsc::channel::<ContactCommand>(CONTACT_COMMAND_CHANNEL_CAPACITY);
        let (state_tx, state_rx) = watch::channel(ConnState::Connected);
        let handle = ContactSessionHandle { tx, state_rx };

        assert_eq!(handle.state(), ConnState::Connected);
        state_tx.send(ConnState::Reconnecting).unwrap();
        assert_eq!(handle.state(), ConnState::Reconnecting);
        state_tx.send(ConnState::Dead).unwrap();
        assert_eq!(handle.state(), ConnState::Dead);
    }
}
