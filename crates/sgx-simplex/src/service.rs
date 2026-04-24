//! MessengerService gRPC implementation for SimpleX.
//!
//! Phase 1: Auth flow (profile setup + invitation parsing), ListChats, stubs for rest.
//! Phase 2 will add actual SMP message exchange.

use crate::contact_session::{
    ContactCommand, ContactSendError, ContactSessionHandle, SendTextResult,
};
use crate::invitation;
use crate::queue_store::QueueStore;
use sgx_proto::messenger::v1::messenger_service_server::MessengerService;
use sgx_proto::messenger::v1::*;
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tokio_stream::Stream;
use tonic::{Request, Response, Status};
use tracing::info;

/// gRPC service for SimpleX protocol.
pub struct SimplexService {
    store: Arc<QueueStore>,
    display_name: Mutex<Option<String>>,
    /// Broadcast sender for [`SimplexUpdate`] events. Background tasks
    /// (contact handshake, message receiver) publish here; gRPC stream
    /// subscribers consume via `StreamSimplexUpdates`. Channel capacity
    /// is intentionally generous: we would rather drop to Lagged than
    /// back-pressure the handshake.
    update_tx: tokio::sync::broadcast::Sender<SimplexUpdate>,
    /// Per-contact command channels. Populated by handshake-spawn paths
    /// immediately before the contact's background task starts running;
    /// each task removes its own entry when it exits. gRPC handlers (e.g.
    /// SendSimplexMessage) look up the [`ContactSessionHandle`] by
    /// contact id to inject [`ContactCommand`]s into the running session.
    contact_sessions: Arc<Mutex<HashMap<String, ContactSessionHandle>>>,
}

impl SimplexService {
    pub fn new(store: QueueStore) -> Self {
        // Prefer the structured user_profile singleton; fall back to the
        // legacy key-value profile.display_name for databases that were
        // created before user_profile existed.
        let name = match store.get_user_profile().ok().flatten() {
            Some(profile) => {
                info!(
                    "SimpleX: profile loaded: display_name='{}'",
                    profile.display_name
                );
                if !profile.full_name.is_empty() {
                    info!("  full_name='{}'", profile.full_name);
                }
                if !profile.bio.is_empty() {
                    info!("  bio='{}'", profile.bio);
                }
                Some(profile.display_name)
            }
            None => match store.get_profile_name().ok().flatten() {
                Some(legacy) => {
                    info!(
                        "SimpleX: legacy profile loaded: display_name='{legacy}' (migrate via SetProfile)"
                    );
                    Some(legacy)
                }
                None => {
                    tracing::warn!(
                        "SimpleX: no profile configured - call SetProfile to set your display name"
                    );
                    None
                }
            },
        };

        let (update_tx, _) = tokio::sync::broadcast::channel(256);

        Self {
            store: Arc::new(store),
            display_name: Mutex::new(name),
            update_tx,
            contact_sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

type UpdateStream = Pin<Box<dyn Stream<Item = Result<Update, Status>> + Send>>;

#[tonic::async_trait]
impl MessengerService for SimplexService {
    // ----- Backend Info -----

    async fn get_backend_info(
        &self,
        _request: Request<GetBackendInfoRequest>,
    ) -> Result<Response<BackendInfo>, Status> {
        let name = self.display_name.lock().await;
        Ok(Response::new(BackendInfo {
            backend_id: "simplex".into(),
            display_name: "SimpleX".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            is_authenticated: name.is_some(),
            badge_label: "SX".into(),
            badge_color: "#5cba48".into(),
        }))
    }

    // ----- Auth Flow -----
    // SimpleX has no phone/code/password. We repurpose:
    // - SubmitPhoneNumber -> set display name (SimpleX profile name)
    // - SubmitAuthCode -> paste invitation link to connect to someone

    async fn get_auth_state(
        &self,
        _request: Request<GetAuthStateRequest>,
    ) -> Result<Response<AuthState>, Status> {
        let name = self.display_name.lock().await;
        let state = if let Some(ref n) = *name {
            auth_state::State::Ready(Ready {
                user_id: "simplex-local".into(),
                display_name: n.clone(),
            })
        } else {
            auth_state::State::WaitPhone(WaitPhone {})
        };
        Ok(Response::new(AuthState { state: Some(state) }))
    }

    async fn submit_phone_number(
        &self,
        request: Request<SubmitPhoneNumberRequest>,
    ) -> Result<Response<SubmitPhoneNumberResponse>, Status> {
        let name = request.into_inner().phone_number;
        if name.trim().is_empty() {
            return Err(Status::invalid_argument("Display name cannot be empty"));
        }
        info!("SimpleX: profile name set to '{name}'");
        self.store
            .save_profile(&name)
            .map_err(|e| Status::internal(format!("Store error: {e}")))?;
        *self.display_name.lock().await = Some(name);
        Ok(Response::new(SubmitPhoneNumberResponse { success: true }))
    }

    async fn submit_auth_code(
        &self,
        request: Request<SubmitAuthCodeRequest>,
    ) -> Result<Response<SubmitAuthCodeResponse>, Status> {
        let code = request.into_inner().code;
        info!("SimpleX: received link: {code}");

        // Resolve short links (https://<server>/a#<key>)
        let link = if code.contains("/a#") {
            info!("SimpleX: resolving short link...");
            invitation::resolve_short_link(&code)
                .await
                .map_err(|e| Status::internal(format!("Short link resolution failed: {e}")))?
        } else {
            code
        };

        let store = self.store.clone();
        let profile_name = {
            let locked = self.display_name.lock().await.clone().unwrap_or_default();
            if locked.is_empty() {
                // Fallback when no profile has been set yet (e.g. fresh DB);
                // Desktop would otherwise show "Your contact" with no name.
                "SimpleGoX".to_string()
            } else {
                locked
            }
        };
        let contact_id = uuid_v4();

        if invitation::is_contact_address(&link) {
            // Contact Address -> AgentInvitation ('I'), no Double Ratchet
            info!("SimpleX: detected CONTACT ADDRESS");

            let contact = invitation::parse_contact_address(&link)
                .map_err(|e| Status::invalid_argument(format!("Contact parse: {e}")))?;

            self.store
                .save_contact(
                    &contact_id,
                    None,
                    &contact.server_host,
                    contact.server_port,
                    &contact.server_fingerprint,
                    &contact.queue_id,
                    &contact.sender_key,
                )
                .map_err(|e| Status::internal(format!("Store: {e}")))?;

            info!(
                "SimpleX: contact saved (id={contact_id}, server={}:{})",
                contact.server_host, contact.server_port
            );

            let update_tx = self.update_tx.clone();
            // First visible status line in the banner: sets the tone before the
            // more technical stages take over.
            emit_progress(
                &update_tx,
                "init",
                "Initiating encrypted contact handshake...",
                "bootstrapping",
            );
            let contact_sessions = self.contact_sessions.clone();
            tokio::spawn(async move {
                info!("SimpleX: contact handshake task started for {contact_id}");
                match execute_contact_handshake(
                    &contact,
                    &profile_name,
                    store,
                    &contact_id,
                    update_tx,
                    contact_sessions,
                )
                .await
                {
                    Ok(_) => info!("SimpleX: contact handshake COMPLETED for {contact_id}"),
                    Err(e) => tracing::error!("SimpleX: contact handshake FAILED: {e:?}"),
                }
            });
        } else {
            // One-Time Invitation -> AgentConfirmation ('C')
            info!("SimpleX: detected ONE-TIME INVITATION");

            let parsed = invitation::parse_invitation_link(&link)
                .map_err(|e| Status::invalid_argument(format!("Invitation parse: {e}")))?;

            self.store
                .save_contact(
                    &contact_id,
                    None,
                    &parsed.server_host,
                    parsed.server_port,
                    &parsed.server_fingerprint,
                    &parsed.queue_id,
                    &parsed.sender_key,
                )
                .map_err(|e| Status::internal(format!("Store: {e}")))?;

            info!(
                "SimpleX: contact saved (id={contact_id}, server={}:{})",
                parsed.server_host, parsed.server_port
            );

            tokio::spawn(async move {
                info!("SimpleX: invitation handshake task started for {contact_id}");
                match execute_handshake(&parsed, &profile_name, store, &contact_id).await {
                    Ok(_) => info!("SimpleX: invitation handshake COMPLETED for {contact_id}"),
                    Err(e) => tracing::error!("SimpleX: invitation handshake FAILED: {e:?}"),
                }
            });
        }

        Ok(Response::new(SubmitAuthCodeResponse { success: true }))
    }

    async fn submit_password(
        &self,
        _request: Request<SubmitPasswordRequest>,
    ) -> Result<Response<SubmitPasswordResponse>, Status> {
        Err(Status::unimplemented("SimpleX does not use passwords"))
    }

    async fn logout(
        &self,
        _request: Request<LogoutRequest>,
    ) -> Result<Response<LogoutResponse>, Status> {
        info!("SimpleX: logout - clearing all data");
        self.store
            .clear_all()
            .map_err(|e| Status::internal(format!("Store error: {e}")))?;
        *self.display_name.lock().await = None;
        Ok(Response::new(LogoutResponse { success: true }))
    }

    // ----- Chats -----

    async fn list_chats(
        &self,
        _request: Request<ListChatsRequest>,
    ) -> Result<Response<ListChatsResponse>, Status> {
        let contacts = self
            .store
            .list_contacts()
            .map_err(|e| Status::internal(format!("Store error: {e}")))?;

        let chats: Vec<Chat> = contacts
            .iter()
            .map(|c| Chat {
                chat_id: Some(ChatId {
                    backend: "simplex".into(),
                    id: c.id.clone(),
                }),
                title: c
                    .display_name
                    .clone()
                    .unwrap_or_else(|| "New contact".into()),
                chat_type: 1, // CHAT_TYPE_PRIVATE
                avatar_url: String::new(),
                last_message: None,
                unread_count: 0,
                is_encrypted: true,
                is_muted: false,
                is_pinned: false,
                last_activity: None,
            })
            .collect();

        Ok(Response::new(ListChatsResponse { chats }))
    }

    // ----- Messages (Phase 2) -----

    async fn get_messages(
        &self,
        _request: Request<GetMessagesRequest>,
    ) -> Result<Response<GetMessagesResponse>, Status> {
        // Phase 2: will query messages from queue_store
        Ok(Response::new(GetMessagesResponse { messages: vec![] }))
    }

    async fn send_message(
        &self,
        _request: Request<SendMessageRequest>,
    ) -> Result<Response<SendMessageResponse>, Status> {
        Err(Status::unimplemented(
            "SimpleX message sending requires Phase 2 (Double Ratchet encryption)",
        ))
    }

    // ----- Streaming -----

    type StreamUpdatesStream = UpdateStream;

    async fn stream_updates(
        &self,
        _request: Request<StreamUpdatesRequest>,
    ) -> Result<Response<Self::StreamUpdatesStream>, Status> {
        // Phase 2: will stream real-time messages from SMP subscriptions
        let stream = tokio_stream::pending::<Result<Update, Status>>();
        Ok(Response::new(Box::pin(stream)))
    }

    // ----- Avatar -----

    async fn download_avatar(
        &self,
        _request: Request<DownloadAvatarRequest>,
    ) -> Result<Response<DownloadAvatarResponse>, Status> {
        // SimpleX profile images come in Phase 2+ via vCard
        Ok(Response::new(DownloadAvatarResponse {
            data_url: String::new(),
        }))
    }

    // ----- Proxy -----

    async fn set_proxy(
        &self,
        request: Request<SetProxyRequest>,
    ) -> Result<Response<SetProxyResponse>, Status> {
        let req = request.into_inner();
        if req.enabled {
            info!("SimpleX: proxy set to {}:{}", req.server, req.port);
            self.store
                .save_proxy(&req.server, req.port)
                .map_err(|e| Status::internal(format!("Store error: {e}")))?;
        } else {
            info!("SimpleX: proxy disabled");
            self.store
                .clear_proxy()
                .map_err(|e| Status::internal(format!("Store error: {e}")))?;
        }
        Ok(Response::new(SetProxyResponse {
            success: true,
            message: String::new(),
        }))
    }

    async fn set_profile(
        &self,
        request: Request<SetProfileRequest>,
    ) -> Result<Response<SetProfileResponse>, Status> {
        let req = request.into_inner();
        if req.display_name.trim().is_empty() {
            return Ok(Response::new(SetProfileResponse {
                success: false,
                error: "display_name cannot be empty".into(),
            }));
        }

        // Preserve preferences JSON across updates if caller did not supply
        // a new one (they currently can not - the proto only carries the
        // three plain string fields). Falls back to "{}" on first insert.
        let existing = self.store.get_user_profile().ok().flatten();
        let preferences_json = existing
            .as_ref()
            .map(|p| p.preferences_json.clone())
            .unwrap_or_else(|| "{}".to_string());

        let profile = crate::queue_store::UserProfile {
            display_name: req.display_name.clone(),
            full_name: req.full_name.clone(),
            bio: req.bio.clone(),
            preferences_json,
            created_at: 0, // ignored by save_user_profile on conflict
            updated_at: 0,
        };

        if let Err(e) = self.store.save_user_profile(&profile) {
            return Ok(Response::new(SetProfileResponse {
                success: false,
                error: format!("Store error: {e}"),
            }));
        }

        // Keep the legacy key-value profile.display_name in sync so the
        // existing auth flow (submit_phone_number path) reads a consistent
        // value. Non-fatal if it fails.
        let _ = self.store.save_profile(&req.display_name);

        // Refresh in-memory cache used by the invitation handshake.
        *self.display_name.lock().await = Some(req.display_name.clone());

        info!(
            "SimpleX: profile updated: display_name='{}', full_name='{}', bio={}B",
            req.display_name,
            req.full_name,
            req.bio.len()
        );
        Ok(Response::new(SetProfileResponse {
            success: true,
            error: String::new(),
        }))
    }

    async fn get_profile(
        &self,
        _request: Request<GetProfileRequest>,
    ) -> Result<Response<GetProfileResponse>, Status> {
        match self.store.get_user_profile() {
            Ok(Some(p)) => Ok(Response::new(GetProfileResponse {
                has_profile: true,
                display_name: p.display_name,
                full_name: p.full_name,
                bio: p.bio,
            })),
            Ok(None) => Ok(Response::new(GetProfileResponse {
                has_profile: false,
                display_name: String::new(),
                full_name: String::new(),
                bio: String::new(),
            })),
            Err(e) => Err(Status::internal(format!("Store error: {e}"))),
        }
    }

    async fn reset_simplex(
        &self,
        _request: Request<ResetSimplexRequest>,
    ) -> Result<Response<ResetSimplexResponse>, Status> {
        match self.store.reset_all() {
            Ok(_) => {
                *self.display_name.lock().await = None;
                info!("SimpleX: local state reset (profile + contacts + messages + ratchets)");
                Ok(Response::new(ResetSimplexResponse {
                    success: true,
                    error: String::new(),
                }))
            }
            Err(e) => Ok(Response::new(ResetSimplexResponse {
                success: false,
                error: format!("Store error: {e}"),
            })),
        }
    }

    type StreamSimplexUpdatesStream =
        Pin<Box<dyn Stream<Item = Result<SimplexUpdate, Status>> + Send>>;

    async fn stream_simplex_updates(
        &self,
        _request: Request<StreamSimplexUpdatesRequest>,
    ) -> Result<Response<Self::StreamSimplexUpdatesStream>, Status> {
        info!("=== StreamSimplexUpdates: new subscriber connected");

        let mut rx = self.update_tx.subscribe();

        let stream = async_stream::stream! {
            loop {
                match rx.recv().await {
                    Ok(update) => yield Ok(update),
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(
                            "StreamSimplexUpdates subscriber lagged by {n} messages"
                        );
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        info!("StreamSimplexUpdates: broadcast channel closed");
                        break;
                    }
                }
            }
        };

        Ok(Response::new(Box::pin(stream)))
    }

    async fn send_simplex_message(
        &self,
        request: Request<SendSimplexMessageRequest>,
    ) -> Result<Response<SendSimplexMessageResponse>, Status> {
        let req = request.into_inner();
        let contact_id = req.contact_id;
        let body = req.body;

        if body.is_empty() {
            return Err(Status::invalid_argument("body cannot be empty"));
        }

        // Resolve the per-contact session handle. The handle is inserted
        // into `contact_sessions` immediately before the contact's
        // background task is spawned (see execute_contact_handshake), so
        // a SendSimplexMessage arriving the moment ContactEstablished
        // fires should already find it.
        let handle = {
            let sessions = self.contact_sessions.lock().await;
            sessions.get(&contact_id).cloned()
        };
        let handle = match handle {
            Some(h) => h,
            None => {
                return Err(Status::not_found(format!(
                    "no running SimpleX session for contact_id={contact_id}"
                )));
            }
        };

        // Briefing 044 W5: fail-fast before enqueueing the SendText. The
        // BG loop's select! is blocked inside `read_responses` during
        // steady-state operation, and inside `reconnect_with_backoff`
        // during a reconnect. Without this check a SendText would sit in
        // the mpsc behind the blocked select! read arm for the entire
        // reconnect window (up to ~3.5 minutes worst case) before
        // discovering the reply path is broken.
        use crate::contact_session::ConnState;
        match handle.state() {
            ConnState::Connected => {
                // Normal path, fall through.
            }
            ConnState::Reconnecting => {
                return Err(Status::unavailable(
                    "SimpleX connection is reconnecting, try again shortly",
                ));
            }
            ConnState::Dead => {
                return Err(Status::unavailable(
                    "SimpleX connection is dead, contact needs manual reconnect",
                ));
            }
        }

        // Inject the SendText command and await the in-task reply.
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        if handle
            .tx
            .send(ContactCommand::SendText {
                body,
                reply: reply_tx,
            })
            .await
            .is_err()
        {
            return Err(Status::unavailable(
                "contact session is shutting down (command channel closed)",
            ));
        }

        let result = match reply_rx.await {
            Ok(r) => r,
            Err(_) => {
                return Err(Status::internal(
                    "contact session dropped reply oneshot before responding",
                ));
            }
        };

        match result {
            Ok(SendTextResult {
                msg_id,
                timestamp_ms,
            }) => Ok(Response::new(SendSimplexMessageResponse {
                msg_id,
                timestamp_ms,
            })),
            Err(ContactSendError::ContactNotFound) => Err(Status::not_found(
                "contact session reported ContactNotFound",
            )),
            Err(ContactSendError::RatchetStateMissing) => Err(Status::failed_precondition(
                "contact ratchet state not yet initialised (handshake incomplete)",
            )),
            Err(e @ ContactSendError::EncryptionFailed(_)) => {
                Err(Status::internal(e.to_string()))
            }
            Err(e @ ContactSendError::SmpSendFailed(_)) => {
                Err(Status::internal(e.to_string()))
            }
            Err(e @ ContactSendError::PersistenceFailed(_)) => {
                Err(Status::internal(e.to_string()))
            }
            // Briefing 044 W5: map the reconnect-window errors to
            // Tonic's `Status::unavailable`. The frontend translates
            // those into the "try again shortly" / "needs manual
            // reconnect" toasts per W6's event plan. If we ever hit
            // this branch it means the state flipped between our
            // pre-send check above and the BG loop picking up the
            // command - rare but possible.
            Err(e @ ContactSendError::Reconnecting(_)) => {
                Err(Status::unavailable(e.to_string()))
            }
            Err(e @ ContactSendError::Dead(_)) => {
                Err(Status::unavailable(e.to_string()))
            }
        }
    }
}

/// Execute the SimpleX connection handshake in the background.
///
/// 7 steps: TLS+SMP -> NEW -> KEY -> SUB -> CONF -> wait -> HELLO
async fn execute_handshake(
    invitation: &invitation::ParsedInvitation,
    profile_name: &str,
    store: Arc<QueueStore>,
    contact_id: &str,
) -> Result<(), anyhow::Error> {
    use crate::crypto::keys::*;
    use crate::e2e_crypto::*;
    use crate::protocol::agent_msg::*;
    use crate::smp_client::{SmpClient, SmpServerAddr};
    use crate::smp_commands::*;
    use crate::smp_protocol::*;
    use base64::Engine;

    // ---- Step 1: TLS + SMP handshake ----
    tracing::info!(
        "Step 1: TLS + SMP handshake to {}:{}",
        invitation.server_host,
        invitation.server_port
    );

    let addr = SmpServerAddr {
        host: invitation.server_host.clone(),
        port: invitation.server_port,
        fingerprint: invitation.server_fingerprint.clone(),
    };
    let client = SmpClient::new(addr, None);
    let tls_stream = client
        .connect()
        .await
        .map_err(|e| anyhow::anyhow!("TLS connect: {e}"))?;

    // Compute server_key_hash from the fingerprint (already validated during TLS)
    let fp_stripped = invitation.server_fingerprint.trim_end_matches('=');
    let server_key_hash_vec = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(fp_stripped)
        .map_err(|e| anyhow::anyhow!("Fingerprint decode: {e}"))?;
    let mut server_key_hash = [0u8; 32];
    if server_key_hash_vec.len() == 32 {
        server_key_hash.copy_from_slice(&server_key_hash_vec);
    }

    let mut smp = SmpConnection::smp_handshake(tls_stream, server_key_hash)
        .await
        .map_err(|e| anyhow::anyhow!("SMP handshake: {e}"))?;

    tracing::info!(
        "Step 1: SMP handshake OK, session_id={}...",
        hex::encode(&smp.session_id[..4])
    );

    // ---- Step 2: Create receive queue ----
    tracing::info!("Step 2: Creating receive queue");

    let rcv_auth = generate_ed25519();
    let (rcv_dh_priv, rcv_dh_pub) = generate_x25519();
    // v9: separate X25519 auth key for NEW
    let (_rcv_auth_x25519_priv, rcv_auth_x25519_pub) = generate_x25519();

    let new_tx = cmd_new(
        &smp,
        &rcv_auth,
        rcv_auth_x25519_pub.as_bytes(),
        rcv_dh_pub.as_bytes(),
    );

    tracing::debug!("NEW tx {} bytes, sig_len={}", new_tx.len(), new_tx[0]);
    // After sig (65 bytes): corrIdLen, corrId, entityIdLen, entityId, cmd
    let after_sig = &new_tx[65..];
    tracing::debug!(
        "NEW after sig ({} bytes): {}",
        after_sig.len(),
        hex::encode(&after_sig[..after_sig.len().min(60)])
    );
    // Find cmd start: skip corrIdLen+corrId+entityIdLen+entityId
    if after_sig.len() > 2 {
        let cid_len = after_sig[0] as usize;
        let eid_offset = 1 + cid_len;
        if eid_offset < after_sig.len() {
            let eid_len = after_sig[eid_offset] as usize;
            let cmd_offset = eid_offset + 1 + eid_len;
            if cmd_offset + 4 < after_sig.len() {
                tracing::debug!(
                    "NEW cmd at +{}: '{}'",
                    cmd_offset,
                    String::from_utf8_lossy(
                        &after_sig[cmd_offset..after_sig.len().min(cmd_offset + 20)]
                    )
                );
            }
        }
    }
    tracing::debug!(
        "NEW full hex (first 140): {}",
        hex::encode(&new_tx[..new_tx.len().min(140)])
    );

    smp.write_command_block(&new_tx)
        .await
        .map_err(|e| anyhow::anyhow!("NEW send: {e}"))?;

    let responses = smp
        .read_responses()
        .await
        .map_err(|e| anyhow::anyhow!("NEW response: {e}"))?;

    tracing::info!("Step 2: NEW response count={}", responses.len());
    for (i, r) in responses.iter().enumerate() {
        tracing::info!(
            "Step 2: response[{i}]: {:?}",
            format!("{r:?}").chars().take(100).collect::<String>()
        );
    }

    // Extract IDS
    let mut rcv_id = [0u8; 24];
    let mut snd_id = [0u8; 24];
    let mut srv_dh = [0u8; 32];
    let mut got_ids = false;

    for resp in &responses {
        if let ServerResponse::Ids {
            rcv_id: r,
            snd_id: s,
            srv_dh_public: d,
        } = resp
        {
            rcv_id = *r;
            snd_id = *s;
            srv_dh = *d;
            got_ids = true;
            break;
        }
    }

    if !got_ids {
        return Err(anyhow::anyhow!("No IDS response from server"));
    }

    tracing::info!(
        "Step 2: Queue created rcv_id={}... snd_id={}...",
        hex::encode(&rcv_id[..4]),
        hex::encode(&snd_id[..4])
    );

    // Briefing 044c: persist the queue_auth_private whose public half
    // was just registered on the server as this queue's rcvAuthKey. The
    // Invitee path's BG loop does not currently reconnect (pre-existing
    // Phase 5 TODO), but we still persist so a future Phase 5 picks up
    // the right key with no further plumbing. Idempotent: overwriting
    // an already-stored value with the same Private is harmless.
    store
        .save_queue_auth_private(contact_id, &smp.queue_auth_private)
        .map_err(|e| anyhow::anyhow!("save queue_auth_private: {e}"))?;
    tracing::info!(
        "Step 2c: persisted queue_auth_private for contact={} (pub[..4]={})",
        contact_id,
        hex::encode(&smp.queue_auth_public[..4])
    );

    // ---- Step 3: Generate sender auth key ----
    tracing::info!("Step 3: Generating sender auth key");

    let snd_auth = generate_ed25519();
    tracing::info!("Step 3: Sender auth key ready");

    // ---- Step 4: KEY on OUR queue (register sender auth key, v6) ----
    tracing::info!(
        "Step 4: Sending KEY to OUR queue (rcv_id={}...)",
        hex::encode(&rcv_id[..4])
    );

    let key_tx = cmd_key(
        &smp,
        &rcv_auth,
        &rcv_id,
        snd_auth.verifying_key().as_bytes(),
    );
    smp.write_command_block(&key_tx)
        .await
        .map_err(|e| anyhow::anyhow!("KEY send: {e}"))?;
    let key_resp = smp
        .read_responses()
        .await
        .map_err(|e| anyhow::anyhow!("KEY response: {e}"))?;
    tracing::info!(
        "Step 4: KEY response: {:?}",
        key_resp
            .iter()
            .map(|x| format!("{x:?}").chars().take(60).collect::<String>())
            .collect::<Vec<_>>()
    );

    // Step 4b: SUB to our queue (subscribe for incoming messages)
    tracing::info!(
        "Step 4b: SUB to our queue (rcv_id={}...)",
        hex::encode(&rcv_id[..4])
    );
    let sub_tx = cmd_sub(&smp, &rcv_auth, &rcv_id);
    smp.write_command_block(&sub_tx)
        .await
        .map_err(|e| anyhow::anyhow!("SUB send: {e}"))?;
    let sub_resp = smp
        .read_responses()
        .await
        .map_err(|e| anyhow::anyhow!("SUB response: {e}"))?;
    tracing::info!(
        "Step 4b: SUB response: {:?}",
        sub_resp
            .iter()
            .map(|x| format!("{x:?}").chars().take(60).collect::<String>())
            .collect::<Vec<_>>()
    );

    let peer_snd_id = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(&invitation.queue_id)
        .unwrap_or_default();

    // ---- Step 5: Send AgentConfirmation ----
    tracing::info!("Step 5: Sending AgentConfirmation with profile '{profile_name}'");

    // E2E ratchet keys - X448 (68-byte SPKI with OID 2b656f)
    let our_key1 = crate::crypto::keys::X448Keypair::generate();
    let our_key2 = crate::crypto::keys::X448Keypair::generate();

    // PERSIST E2E keypairs BEFORE sending (needed for X3DH when peer responds)
    store
        .save_e2e_keypairs(
            contact_id,
            &our_key1.private,
            &our_key1.public,
            &our_key2.private,
            &our_key2.public,
        )
        .ok();

    // PrivHeader = 'K' + Ed25519 SPKI (44B, no length byte)
    let mut priv_header = vec![b'K'];
    priv_header.extend_from_slice(&crate::smp_protocol::ED25519_SPKI_HEADER);
    priv_header.extend_from_slice(snd_auth.verifying_key().as_bytes());

    // ConnInfoReply = 'D' + queue address + profile JSON
    let conn_info_reply = encode_agent_conn_info_reply(
        &invitation.server_host,
        invitation.server_port,
        &server_key_hash,
        &snd_id,
        rcv_dh_pub.as_bytes(),
        profile_name,
    );

    // AgentConfirmation body: [0x00][0x07] 'C' 0x31 [0x00][0x03] key1(68) key2(68) ConnInfoReply
    let conf_body = encode_agent_confirmation(
        &our_key1.encode_spki(),
        &our_key2.encode_spki(),
        &conn_info_reply,
    );

    // Decode peer's DH public key from invitation sender_key
    let peer_dh_bytes = base64::engine::general_purpose::URL_SAFE
        .decode(&invitation.sender_key)
        .map_err(|e| anyhow::anyhow!("peer DH key decode: {e}"))?;
    let mut peer_dh_pub = [0u8; 32];
    if peer_dh_bytes.len() == 44 {
        peer_dh_pub.copy_from_slice(&peer_dh_bytes[12..44]);
    } else if peer_dh_bytes.len() == 32 {
        peer_dh_pub.copy_from_slice(&peer_dh_bytes);
    } else {
        return Err(anyhow::anyhow!(
            "Unexpected peer DH key length: {}",
            peer_dh_bytes.len()
        ));
    }

    tracing::debug!("CONF priv_header: {} bytes", priv_header.len());
    tracing::debug!(
        "CONF body: {} bytes, first 20: {}",
        conf_body.len(),
        hex::encode(&conf_body[..20.min(conf_body.len())])
    );
    tracing::debug!("CONF peer_dh_pub: {}", hex::encode(&peer_dh_pub));

    let conf_client_msg = e2e_encrypt_agent_msg(
        &conf_body,
        &peer_dh_pub,
        rcv_dh_priv.as_bytes(),
        rcv_dh_pub.as_bytes(),
        true,         // is_first_message - inline our DH key
        &priv_header, // 'K' + snd_auth SPKI
    );

    let conf_tx = cmd_send_unsigned(&smp, &peer_snd_id, &conf_client_msg, b'D', true);
    smp.write_command_block(&conf_tx)
        .await
        .map_err(|e| anyhow::anyhow!("CONF send: {e}"))?;

    let conf_resp = smp
        .read_responses()
        .await
        .map_err(|e| anyhow::anyhow!("CONF response: {e}"))?;

    for (i, r) in conf_resp.iter().enumerate() {
        tracing::info!(
            "Step 5: CONF response[{i}]: {:?}",
            format!("{r:?}").chars().take(100).collect::<String>()
        );
    }

    tracing::info!("Step 5: AgentConfirmation sent");

    // ---- Steps 6-7 run in background (no timeout - wait as long as needed) ----
    let contact_id_bg = contact_id.to_string();
    let rcv_dh_priv_bytes = *rcv_dh_priv.as_bytes();
    let rcv_dh_pub_bytes = *rcv_dh_pub.as_bytes();
    let srv_dh_bytes = srv_dh; // server DH public from IDS response (for Layer 3)
    let store_bg = store.clone();

    tokio::spawn(async move {
        tracing::info!("Step 6: Background receive loop started, waiting for peer...");

        // Wait indefinitely for peer's AgentConfirmation MSG
        loop {
            match smp.read_responses().await {
                Ok(responses) => {
                    for resp in &responses {
                        if let ServerResponse::Msg { msg_id, body } = resp {
                            tracing::info!(
                                "BG: MSG received, msg_id={}, body={} bytes",
                                hex::encode(&msg_id[..4]),
                                body.len()
                            );

                            // ACK immediately
                            let ack = cmd_ack(&smp, &rcv_auth, &rcv_id, msg_id);
                            if let Err(e) = smp.write_command_block(&ack).await {
                                tracing::error!("BG: ACK failed: {e}");
                                return;
                            }
                            let _ = smp.read_responses().await;

                            // Decrypt peer's confirmation (Layer 3 server + Layer 2 E2E)
                            let conf_plaintext = match e2e_decrypt_incoming(
                                msg_id,
                                body,
                                &srv_dh_bytes,
                                &rcv_dh_priv_bytes,
                            ) {
                                Ok(p) => {
                                    tracing::info!("BG: Layer 3+2 decrypted OK, {} bytes", p.len());
                                    p
                                }
                                Err(e) => {
                                    tracing::error!("BG: E2E decrypt failed: {e}");
                                    tracing::debug!(
                                        "BG: srv_dh={}, rcv_dh_priv={}...",
                                        hex::encode(&srv_dh_bytes[..4]),
                                        hex::encode(&rcv_dh_priv_bytes[..4])
                                    );
                                    tracing::debug!(
                                        "BG: body first 20: {}",
                                        hex::encode(&body[..20.min(body.len())])
                                    );
                                    return;
                                }
                            };

                            tracing::info!(
                                "BG: PrivHeader = 0x{:02x} '{}'",
                                conf_plaintext[0],
                                conf_plaintext[0] as char
                            );
                            tracing::debug!(
                                "BG: Plaintext first 40: {}",
                                hex::encode(&conf_plaintext[..40.min(conf_plaintext.len())])
                            );

                            // Parse peer's keys
                            let peer_conf = match parse_peer_confirmation(&conf_plaintext) {
                                Ok(c) => {
                                    tracing::info!("BG: Peer conf parsed OK");
                                    c
                                }
                                Err(e) => {
                                    tracing::error!("BG: Parse peer conf failed: {e}");
                                    return;
                                }
                            };

                            // Log peer E2E key sizes
                            tracing::info!(
                                "BG: Peer e2e_key1={} bytes, e2e_key2={} bytes",
                                peer_conf.e2e_key1.len(),
                                peer_conf.e2e_key2.len()
                            );

                            // KEY command
                            tracing::info!(
                                "BG: Sending KEY (snd_auth={}...)",
                                hex::encode(&peer_conf.snd_auth_public[..4])
                            );
                            let key_tx =
                                cmd_key(&smp, &rcv_auth, &rcv_id, &peer_conf.snd_auth_public);
                            if let Err(e) = smp.write_command_block(&key_tx).await {
                                tracing::error!("BG: KEY send failed: {e}");
                                return;
                            }
                            match smp.read_responses().await {
                                Ok(r) => tracing::info!(
                                    "BG: KEY response: {:?}",
                                    r.iter()
                                        .map(|x| format!("{x:?}")
                                            .chars()
                                            .take(60)
                                            .collect::<String>())
                                        .collect::<Vec<_>>()
                                ),
                                Err(e) => tracing::warn!("BG: KEY response error: {e}"),
                            }

                            // Build HELLO plaintext
                            tracing::info!("BG: Building HELLO");
                            let mut hello_plain = Vec::new();
                            hello_plain.push(b'M');
                            hello_plain.extend_from_slice(&1u64.to_be_bytes());
                            hello_plain.push(0x00);
                            hello_plain.push(b'H');

                            // Wrap in AgentMsgEnvelope and crypto_box
                            // TODO: When real X448 DH is available, encrypt HELLO with Double Ratchet here
                            let hello_envelope = build_agent_msg_envelope(&hello_plain);
                            let hello_client_msg = e2e_encrypt_agent_msg(
                                &hello_envelope,
                                &peer_dh_pub,
                                &rcv_dh_priv_bytes,
                                &rcv_dh_pub_bytes,
                                false,
                                b"_",
                            );

                            let hello_tx = cmd_send(
                                &smp,
                                &snd_auth,
                                &peer_snd_id,
                                &hello_client_msg,
                                b'H',
                                false,
                            );
                            if let Err(e) = smp.write_command_block(&hello_tx).await {
                                tracing::error!("HELLO send failed: {e}");
                                return;
                            }
                            match smp.read_responses().await {
                                Ok(r) => tracing::info!(
                                    "BG: HELLO response: {:?}",
                                    r.iter()
                                        .map(|x| format!("{x:?}")
                                            .chars()
                                            .take(60)
                                            .collect::<String>())
                                        .collect::<Vec<_>>()
                                ),
                                Err(e) => tracing::warn!("BG: HELLO response error: {e}"),
                            }
                            tracing::info!("BG: HELLO sent, waiting for peer HELLO...");
                            // The next MSG on our queue should be peer's HELLO
                            // It will arrive in the next loop iteration or we handle it here
                            // For now, mark as connected after sending our HELLO
                            store_bg
                                .set_contact_status(&contact_id_bg, "connected")
                                .ok();
                            tracing::info!("*** CONNECTED! *** contact={}", &contact_id_bg);

                            // Continue loop to receive further messages (HELLO, chat msgs)
                            // For now return - full receive loop in Phase 5
                            return;
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("Background receive error: {e}");
                    return;
                }
            }
        }
    });

    tracing::info!("Steps 1-5 complete, Steps 6-7 running in background");
    Ok(())
}

/// Push a single handshake-progress update onto the broadcast so any
/// subscribed gRPC stream (and thereby the Tauri StatusBanner) sees it.
fn emit_progress(
    tx: &tokio::sync::broadcast::Sender<SimplexUpdate>,
    stage: &str,
    message: &str,
    state: &str,
) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let update = SimplexUpdate {
        update: Some(simplex_update::Update::HandshakeProgress(
            SimplexHandshakeProgress {
                stage: stage.to_string(),
                message: message.to_string(),
                state: state.to_string(),
                timestamp: now,
            },
        )),
    };
    let _ = tx.send(update);
}

/// Execute contact address handshake (AgentInvitation 'I', no Double Ratchet).
async fn execute_contact_handshake(
    contact: &invitation::ParsedContactAddress,
    profile_name: &str,
    store: Arc<QueueStore>,
    contact_id: &str,
    update_tx: tokio::sync::broadcast::Sender<SimplexUpdate>,
    contact_sessions: Arc<Mutex<HashMap<String, ContactSessionHandle>>>,
) -> Result<(), anyhow::Error> {
    use crate::crypto::keys::*;
    use crate::e2e_crypto::*;
    use crate::protocol::agent_msg::*;
    use crate::smp_client::{SmpClient, SmpServerAddr};
    use crate::smp_commands::*;
    use crate::smp_protocol::*;
    use base64::Engine;

    // Step 1: TLS + SMP handshake
    emit_progress(
        &update_tx,
        "tls_connect",
        &format!("Establishing TLS 1.3 channel to {}...", contact.server_host),
        "bootstrapping",
    );
    tracing::info!(
        "Contact Step 1: Connecting to {}:{}",
        contact.server_host,
        contact.server_port
    );
    let addr = SmpServerAddr {
        host: contact.server_host.clone(),
        port: contact.server_port,
        fingerprint: contact.server_fingerprint.clone(),
    };
    // Briefing 044 W3: clone into SmpClient so `addr` remains available
    // for the BG loop's reconnect helper. The SmpServerAddr is small
    // (two Strings + u16), clone cost is negligible.
    let client = SmpClient::new(addr.clone(), None);
    let tls_stream = match client.connect().await {
        Ok(s) => s,
        Err(e) => {
            emit_progress(
                &update_tx,
                "tls_error",
                &format!("TLS failed: {e}"),
                "error",
            );
            return Err(anyhow::anyhow!("TLS: {e}"));
        }
    };
    emit_progress(
        &update_tx,
        "tls_ok",
        "TLS channel verified via SHA-256 server fingerprint",
        "bootstrapping",
    );
    emit_progress(
        &update_tx,
        "smp_handshake",
        "Negotiating SMP v9 protocol version...",
        "bootstrapping",
    );

    let fp_clean = contact.server_fingerprint.replace("%3D", "=");
    let fp_bytes = base64::engine::general_purpose::URL_SAFE
        .decode(&fp_clean)
        .map_err(|e| {
            anyhow::anyhow!(
                "fingerprint decode: {e} (raw: '{}')",
                contact.server_fingerprint
            )
        })?;
    let mut server_key_hash = [0u8; 32];
    if fp_bytes.len() == 32 {
        server_key_hash.copy_from_slice(&fp_bytes);
    } else {
        return Err(anyhow::anyhow!(
            "bad fingerprint len: {} (expected 32)",
            fp_bytes.len()
        ));
    }

    let mut smp = SmpConnection::smp_handshake(tls_stream, server_key_hash)
        .await
        .map_err(|e| anyhow::anyhow!("SMP: {e}"))?;
    tracing::info!(
        "Contact Step 1: SMP OK, session_id={}...",
        hex::encode(&smp.session_id[..4])
    );

    // Step 2: Create reply queue
    emit_progress(
        &update_tx,
        "smp_session",
        "SMP session established (NaCl crypto_box transport)",
        "bootstrapping",
    );
    emit_progress(
        &update_tx,
        "queue_create",
        "Generating X25519 queue authentication keypair...",
        "bootstrapping",
    );
    tracing::info!("Contact Step 2: Creating reply queue");
    let rcv_auth = generate_ed25519();
    let (rcv_dh_priv, rcv_dh_pub) = generate_x25519();
    tracing::info!(
        "NEW generated rcv_dh_private (raw 32B): {}",
        hex::encode(rcv_dh_priv.as_bytes())
    );
    tracing::info!(
        "NEW generated rcv_dh_public  (raw 32B): {}",
        hex::encode(rcv_dh_pub.as_bytes())
    );
    // v9: separate X25519 auth key for NEW
    let (_rcv_auth_x25519_priv, rcv_auth_x25519_pub) = generate_x25519();

    let new_tx = cmd_new(
        &smp,
        &rcv_auth,
        rcv_auth_x25519_pub.as_bytes(),
        rcv_dh_pub.as_bytes(),
    );
    smp.write_command_block(&new_tx)
        .await
        .map_err(|e| anyhow::anyhow!("NEW: {e}"))?;
    let responses = smp
        .read_responses()
        .await
        .map_err(|e| anyhow::anyhow!("NEW resp: {e}"))?;

    let mut rcv_id = [0u8; 24];
    let mut snd_id = [0u8; 24];
    let mut srv_dh = [0u8; 32];
    for resp in &responses {
        if let ServerResponse::Ids {
            rcv_id: r,
            snd_id: s,
            srv_dh_public: d,
        } = resp
        {
            rcv_id = *r;
            snd_id = *s;
            srv_dh = *d;
            break;
        }
    }
    tracing::info!(
        "Contact Step 2: rcv_id={}... snd_id={}...",
        hex::encode(&rcv_id[..4]),
        hex::encode(&snd_id[..4])
    );

    // Briefing 044c: persist the queue_auth_private whose public half
    // was just registered on the server as this queue's rcvAuthKey via
    // NEW. Every TCP reconnect later generates a fresh SmpConnection
    // with a brand-new random queue_auth_* pair - without this save,
    // reconnect_with_backoff would re-SUB with a key that does not
    // match the server's registration and receive ERR AUTH (see
    // 044c diagnostics report).
    store
        .save_queue_auth_private(contact_id, &smp.queue_auth_private)
        .map_err(|e| anyhow::anyhow!("save queue_auth_private: {e}"))?;
    tracing::info!(
        "Contact Step 2c: persisted queue_auth_private for contact={} (pub[..4]={})",
        contact_id,
        hex::encode(&smp.queue_auth_public[..4])
    );

    emit_progress(
        &update_tx,
        "queue_ok",
        "Receive queue ready, DH public key published",
        "bootstrapping",
    );

    // Generate E2E ratchet keypairs for the reply queue.
    // sndSecure=True in NEW means the queue is already secured; no separate
    // KEY/SUB calls are required in SMP v9 (subscription is active via the
    // '0ST' flag tuple in NEW command options).
    // snd_auth (for sending back to peer) will be generated on demand in
    // Briefing 036 when the HELLO response is built.
    let our_key1 = X448Keypair::generate();
    let our_key2 = X448Keypair::generate();
    store
        .save_e2e_keypairs(
            contact_id,
            &our_key1.private,
            &our_key1.public,
            &our_key2.private,
            &our_key2.public,
        )
        .ok();

    // Step 3: Build and send AgentInvitation
    emit_progress(
        &update_tx,
        "invitation_build",
        "Building AgentInvitation (X448 X3DH keys, post-quantum KEM slot)...",
        "bootstrapping",
    );
    emit_progress(
        &update_tx,
        "invitation_send",
        "Invitation encrypted with per-message ephemeral key, dispatching...",
        "bootstrapping",
    );
    tracing::info!("Contact Step 3: Sending AgentInvitation");

    let peer_dh_bytes = base64::engine::general_purpose::URL_SAFE
        .decode(&contact.sender_key)
        .map_err(|e| anyhow::anyhow!("peer DH: {e}"))?;
    let mut peer_dh_pub = [0u8; 32];
    if peer_dh_bytes.len() == 44 {
        peer_dh_pub.copy_from_slice(&peer_dh_bytes[12..44]);
    } else if peer_dh_bytes.len() == 32 {
        peer_dh_pub.copy_from_slice(&peer_dh_bytes);
    }

    let inv_body = encode_agent_invitation(
        &contact.server_host,
        contact.server_port,
        &server_key_hash,
        &snd_id,
        rcv_dh_pub.as_bytes(),
        &our_key1.encode_spki(),
        &our_key2.encode_spki(),
        profile_name,
    );

    // encode_agent_invitation already includes '_' at the start
    let inv_client_msg = e2e_encrypt_agent_msg(
        &inv_body,
        &peer_dh_pub,
        rcv_dh_priv.as_bytes(),
        rcv_dh_pub.as_bytes(),
        true,
        &[], // empty - '_' is already in inv_body
    );

    let peer_snd_id = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(&contact.queue_id)
        .unwrap_or_default();

    let send_tx = cmd_send_unsigned(&smp, &peer_snd_id, &inv_client_msg, b'S', false);
    smp.write_command_block(&send_tx)
        .await
        .map_err(|e| anyhow::anyhow!("SEND: {e}"))?;
    let send_resp = smp
        .read_responses()
        .await
        .map_err(|e| anyhow::anyhow!("SEND resp: {e}"))?;
    tracing::info!(
        "Contact Step 3: SEND: {:?}",
        send_resp
            .iter()
            .map(|x| format!("{x:?}").chars().take(60).collect::<String>())
            .collect::<Vec<_>>()
    );
    emit_progress(
        &update_tx,
        "invitation_ok",
        "Invitation delivered. Waiting for peer to accept the contact request...",
        "bootstrapping",
    );

    tracing::info!("Contact: AgentInvitation sent! Background loop starting...");

    // Step 6: Background - wait for contact's AgentConfirmation
    let contact_id_bg = contact_id.to_string();
    let rcv_dh_priv_bytes = *rcv_dh_priv.as_bytes();
    let srv_dh_bytes = srv_dh;
    let store_bg = store.clone();
    let profile_name_bg = profile_name.to_string();
    let update_tx_bg = update_tx.clone();

    // Per-contact command channel: gRPC handlers (e.g. SendSimplexMessage)
    // post ContactCommand variants here, the background select! loop below
    // races them against incoming SMP messages. Inserting the handle
    // BEFORE spawn means a SendSimplexMessage RPC arriving the moment the
    // ContactEstablished event fires will already find a valid handle.
    let (cmd_tx, mut cmd_rx) =
        mpsc::channel::<ContactCommand>(crate::contact_session::CONTACT_COMMAND_CHANNEL_CAPACITY);

    // Briefing 044 W5: watch channel for the observable connection state.
    // The BG loop owns the sender; the gRPC SendText handler consults the
    // receiver via `ContactSessionHandle::state()` to fail-fast during
    // reconnect windows instead of piling up commands behind a blocked
    // select! read arm.
    let (state_tx, state_rx) = tokio::sync::watch::channel(
        crate::contact_session::ConnState::Connected,
    );

    // Briefing 044 W2: spawn a sibling timer task that enqueues a
    // `ContactCommand::KeepAlive` every 30 s. The actual TLS write is
    // performed by the select! loop below - routing through the mpsc is
    // the only way to keep reader and writer on the same task and avoid
    // racing `read_exact` (Risk 1). Aborting this task on BG-loop exit
    // happens inside the spawn closure near the cleanup block.
    let ping_cmd_tx = cmd_tx.clone();
    let ping_contact_id = contact_id_bg.clone();
    // Briefing 044a D2.1: high-visibility spawn marker - if this line never
    // appears in the live log we know the keep-alive scaffolding never even
    // started (e.g. wrong code path entered, this function never reached).
    tracing::info!(
        "PING: timer task spawned for contact={ping_contact_id} interval=30s"
    );
    let ping_handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        // `tokio::time::interval` fires an immediate first tick on the
        // same instant it is created; skip it so the initial PING lands
        // ~30 s into the session, not at t=0 when SUB is still in flight.
        interval.tick().await;
        loop {
            interval.tick().await;
            // Briefing 044a D2.2: per-tick visibility BEFORE the mpsc send.
            // If this line stops appearing while the BG loop is still alive
            // we know the timer task itself died.
            tracing::info!(
                "PING: tick for contact={ping_contact_id} about to enqueue KeepAlive"
            );
            if ping_cmd_tx.send(ContactCommand::KeepAlive).await.is_err() {
                tracing::debug!(
                    "PING timer: command channel closed for contact={}, exiting",
                    ping_contact_id
                );
                break;
            }
        }
    });

    {
        let mut sessions = contact_sessions.lock().await;
        sessions.insert(
            contact_id_bg.clone(),
            ContactSessionHandle {
                tx: cmd_tx,
                state_rx,
            },
        );
    }
    let contact_sessions_for_cleanup = contact_sessions.clone();

    tokio::spawn(async move {
        use crate::agent_confirmation::parse_agent_confirmation;
        tracing::info!("Contact BG: Waiting for AgentConfirmation...");
        let mut msg_counter: u32 = 0;
        // Persist ratchet state across messages.
        // Initialised during MSG #1 (AgentConfirmation) processing.
        let mut ratchet_opt: Option<crate::crypto::bob_ratchet::BobRatchet> = None;
        // Peer's reply queue is captured during MSG #1's Stage 16 from the
        // AgentConnInfoReply payload; chat sends require it across all
        // subsequent loop iterations (Phase 3 / Briefing 041b).
        let mut peer_queue: Option<crate::protocol::smp_queue_info::SmpQueueInfo> = None;
        // Briefing 041b-fix: the per-contact X25519 sender_auth_private is
        // now loaded fresh from the store on each SendText command
        // (contact_id-keyed) instead of a dummy Ed25519 generated per
        // session. See `load_sender_auth_private` in queue_store.rs.
        'outer: loop {
            tokio::select! {
                read_result = smp.read_responses() => {
                match read_result {
                Ok(responses) => {
                    for resp in &responses {
                        if let ServerResponse::Msg { msg_id, body } = resp {
                            msg_counter += 1;
                            tracing::info!("=== BG MSG #{} received ===", msg_counter);
                            tracing::info!(
                                "Contact BG: MSG received, {} bytes, msg_id={}...",
                                body.len(),
                                hex::encode(&msg_id[..4])
                            );

                            tracing::debug!(
                                "KEY VERIFICATION: srv_dh_public={}, rcv_dh_private={}, msgId={}",
                                hex::encode(srv_dh_bytes),
                                hex::encode(rcv_dh_priv_bytes),
                                hex::encode(msg_id)
                            );

                            'msg_proc: {
                            // ---- Stage 1: Layer 3 decrypt (server NaCl, queue-level) ----
                            let layer3_plaintext = match decrypt_layer3(
                                msg_id,
                                body,
                                &srv_dh_bytes,
                                &rcv_dh_priv_bytes,
                            ) {
                                Ok(c) => {
                                    tracing::info!(
                                        "Contact BG: Layer 3 decrypt OK, {} bytes",
                                        c.len()
                                    );
                                    c
                                }
                                Err(e) => {
                                    tracing::error!("Contact BG: Layer 3 decrypt FAILED: {}", e);
                                    tracing::warn!(
                                        "Contact BG: MSG #{} - skipping ACK and further stages (continuing to listen)",
                                        msg_counter
                                    );
                                    break 'msg_proc;
                                }
                            };

                            // ---- Stage 2: ACK (only after successful L3 decrypt) ----
                            let ack = cmd_ack(&smp, &rcv_auth, &rcv_id, msg_id);
                            if let Err(e) = smp.write_command_block(&ack).await {
                                tracing::error!("Contact BG: ACK write failed: {e}");
                                break 'outer;
                            }
                            match smp.read_responses().await {
                                Ok(rs) => {
                                    let ok = rs.iter().any(|r| matches!(r, ServerResponse::Ok));
                                    if ok {
                                        tracing::info!(
                                            "Contact BG: ACK OK for msg {:02x?}",
                                            &msg_id[..4]
                                        );
                                    } else {
                                        tracing::warn!(
                                            "Contact BG: ACK unexpected response: {:?}",
                                            rs.iter()
                                                .map(|r| format!("{r:?}")
                                                    .chars()
                                                    .take(60)
                                                    .collect::<String>())
                                                .collect::<Vec<_>>()
                                        );
                                    }
                                }
                                Err(e) => tracing::warn!("Contact BG: ACK read error: {e}"),
                            }

                            // ---- Stage 1.5: unpad + rcvMeta parse ----
                            let (rcv_meta, client_msg_envelope) =
                                match unpad_and_parse_rcv_meta(&layer3_plaintext) {
                                    Ok(pair) => pair,
                                    Err(e) => {
                                        tracing::error!(
                                            "Contact BG: unpad+rcvMeta FAILED: {}",
                                            e
                                        );
                                        tracing::debug!(
                                            "Contact BG: layer3_plaintext[..32]={}",
                                            hex::encode(
                                                &layer3_plaintext
                                                    [..32.min(layer3_plaintext.len())]
                                            )
                                        );
                                        break 'msg_proc;
                                    }
                                };
                            tracing::info!(
                                "Contact BG: unpad+rcvMeta OK: msgTs={}, notification={}, envelope={} bytes",
                                rcv_meta.msg_ts,
                                rcv_meta.notification_flag,
                                client_msg_envelope.len()
                            );

                            // Sanity-Check: PubHeader signature should be at offset 0 of envelope now.
                            const PUB_HEADER_SIGNATURE: &[u8] = &[
                                0x00, 0x04, // smpClientVersion = 4 (Word16 BE)
                                0x31, // Maybe Just ('1')
                                0x2c, // SPKI length = 44
                                0x30, 0x2a, 0x30, 0x05, // X25519 SPKI OID start
                                0x06, 0x03, 0x2b, 0x65, 0x6e, // X25519 OID bytes
                            ];
                            match client_msg_envelope
                                .windows(PUB_HEADER_SIGNATURE.len())
                                .position(|w| w == PUB_HEADER_SIGNATURE)
                            {
                                Some(0) => tracing::debug!(
                                    "Contact BG: PubHeader signature at envelope offset 0 (expected)"
                                ),
                                Some(other) => tracing::warn!(
                                    "Contact BG: PubHeader signature at envelope offset {} (expected 0)",
                                    other
                                ),
                                None => tracing::error!(
                                    "Contact BG: PubHeader signature NOT found in envelope!"
                                ),
                            }

                            // ---- Stage 3: PubHeader parse + Layer 2 decrypt + unpad ----
                            let (pub_header, consumed) =
                                match parse_pub_header(&client_msg_envelope) {
                                    Ok(pair) => pair,
                                    Err(e) => {
                                        tracing::error!(
                                            "Contact BG: parse_pub_header FAILED: {}",
                                            e
                                        );
                                        tracing::debug!(
                                            "Contact BG: envelope[..80]={}",
                                            hex::encode(
                                                &client_msg_envelope
                                                    [..80.min(client_msg_envelope.len())]
                                            )
                                        );
                                        break 'msg_proc;
                                    }
                                };
                            tracing::info!(
                                "Contact BG: PubHeader parsed: version={}, ephemeral_pub={}, nonce={}..., consumed={}B",
                                pub_header.smp_client_version,
                                pub_header
                                    .ephemeral_pub
                                    .as_ref()
                                    .map(hex::encode)
                                    .unwrap_or_else(|| "None".into()),
                                hex::encode(pub_header.nonce),
                                consumed
                            );

                            // If PubHeader carries an inline ephemeral key, persist it so
                            // subsequent `Maybe Nothing` messages can reuse it; if it is
                            // Nothing, fall back to the stored key for this contact.
                            let pub_header = match pub_header.ephemeral_pub {
                                Some(k) => {
                                    if let Err(e) =
                                        store_bg.save_peer_e2e_pub(&contact_id_bg, &k)
                                    {
                                        tracing::warn!(
                                            "Contact BG: save_peer_e2e_pub failed: {e}"
                                        );
                                    }
                                    pub_header
                                }
                                None => match store_bg.load_peer_e2e_pub(&contact_id_bg) {
                                    Ok(Some(stored)) => {
                                        tracing::info!(
                                            "Contact BG: reusing stored peer_e2e_pub[..4]={} for Layer 2 decrypt",
                                            hex::encode(&stored[..4])
                                        );
                                        crate::smp_protocol::PubHeader {
                                            smp_client_version: pub_header
                                                .smp_client_version,
                                            ephemeral_pub: Some(stored),
                                            nonce: pub_header.nonce,
                                        }
                                    }
                                    Ok(None) => {
                                        tracing::error!(
                                            "Contact BG: PubHeader Nothing but no stored peer_e2e_pub for contact"
                                        );
                                        break 'msg_proc;
                                    }
                                    Err(e) => {
                                        tracing::error!(
                                            "Contact BG: load_peer_e2e_pub failed: {e}"
                                        );
                                        break 'msg_proc;
                                    }
                                },
                            };

                            let inner_cipher = &client_msg_envelope[consumed..];
                            let layer2_padded =
                                match decrypt_layer2(&pub_header, inner_cipher, &rcv_dh_priv_bytes)
                                {
                                    Ok(p) => {
                                        tracing::info!(
                                            "Contact BG: Layer 2 decrypt OK, {} bytes (padded)",
                                            p.len()
                                        );
                                        p
                                    }
                                    Err(e) => {
                                        tracing::error!(
                                            "Contact BG: Layer 2 decrypt FAILED: {}",
                                            e
                                        );
                                        break 'msg_proc;
                                    }
                                };

                            let plaintext = match unpad(&layer2_padded) {
                                Ok(p) => {
                                    tracing::info!(
                                        "Contact BG: Layer 2 unpad OK, {} bytes plaintext",
                                        p.len()
                                    );
                                    p
                                }
                                Err(e) => {
                                    tracing::error!("Contact BG: unpad FAILED: {}", e);
                                    break 'msg_proc;
                                }
                            };
                            if plaintext.len() >= 4 {
                                tracing::info!(
                                    "Contact BG: ClientMessage header bytes: {:02x} {:02x} {:02x} {:02x}",
                                    plaintext[0],
                                    plaintext[1],
                                    plaintext[2],
                                    plaintext[3]
                                );
                            }
                            tracing::debug!(
                                "Contact BG: plaintext[..16]={}",
                                hex::encode(&plaintext[..16.min(plaintext.len())])
                            );

                            if msg_counter == 1 {
                            // ---- Stage 4: Parse AgentConfirmation ----
                            let conf = match parse_agent_confirmation(&plaintext) {
                                Ok(c) => c,
                                Err(e) => {
                                    tracing::error!(
                                        "Contact BG: AgentConfirmation parse FAILED: {}",
                                        e
                                    );
                                    tracing::debug!(
                                        "Contact BG: plaintext[..32]={}",
                                        hex::encode(&plaintext[..32.min(plaintext.len())])
                                    );
                                    break 'msg_proc;
                                }
                            };
                            tracing::info!("Contact BG: AgentConfirmation parsed:");
                            tracing::info!("  agent_version={}", conf.agent_version);
                            tracing::info!(
                                "  e2e_version={}",
                                conf.e2e_version
                                    .map(|v| v.to_string())
                                    .unwrap_or_else(|| "None".into())
                            );
                            tracing::info!(
                                "  key1={}",
                                conf.ratchet_key1_spki
                                    .as_ref()
                                    .map(|k| format!(
                                        "{}B SPKI (OID {})",
                                        k.len(),
                                        hex::encode(&k[6..9])
                                    ))
                                    .unwrap_or_else(|| "None".into())
                            );
                            tracing::info!(
                                "  key2={}",
                                conf.ratchet_key2_spki
                                    .as_ref()
                                    .map(|k| format!(
                                        "{}B SPKI (OID {})",
                                        k.len(),
                                        hex::encode(&k[6..9])
                                    ))
                                    .unwrap_or_else(|| "None".into())
                            );
                            tracing::info!(
                                "  kem={}",
                                conf.kem_public
                                    .as_ref()
                                    .map(|k| format!("Just {}B", k.len()))
                                    .unwrap_or_else(|| "Nothing".into())
                            );
                            tracing::info!("  encConnInfo={}B", conf.enc_conn_info.len());

                            store_bg
                                .set_contact_status(&contact_id_bg, "pending_hello")
                                .ok();
                            tracing::info!(
                                "Contact BG: Stage 4 complete - AgentConfirmation ready for X3DH (Briefing 035)"
                            );
                            emit_progress(
                                &update_tx_bg,
                                "confirmation_received",
                                &format!(
                                    "Peer accepted. Receiving AgentConfirmation ({} B encrypted payload)...",
                                    body.len()
                                ),
                                "bootstrapping",
                            );
                            emit_progress(
                                &update_tx_bg,
                                "layer3_decrypt",
                                "Layer 3 decrypt OK (NaCl crypto_box, queue-level)",
                                "bootstrapping",
                            );
                            emit_progress(
                                &update_tx_bg,
                                "layer2_decrypt",
                                "Layer 2 decrypt OK (per-queue ephemeral NaCl)",
                                "bootstrapping",
                            );
                            emit_progress(
                                &update_tx_bg,
                                "confirmation_parsed",
                                "AgentConfirmation parsed: X448 SPKI + SNTRUP761 KEM slot",
                                "bootstrapping",
                            );
                            emit_progress(
                                &update_tx_bg,
                                "x3dh_compute",
                                "Computing X3DH key agreement (3 DH + HKDF-SHA512)...",
                                "bootstrapping",
                            );

                            // ---- Stage 5: Parse peer X448 SPKI keys ----
                            let peer_key1_spki = match conf.ratchet_key1_spki.as_ref() {
                                Some(k) => k,
                                None => {
                                    tracing::error!(
                                        "Contact BG: AgentConfirmation missing ratchet_key1"
                                    );
                                    break 'msg_proc;
                                }
                            };
                            let peer_key2_spki = match conf.ratchet_key2_spki.as_ref() {
                                Some(k) => k,
                                None => {
                                    tracing::error!(
                                        "Contact BG: AgentConfirmation missing ratchet_key2"
                                    );
                                    break 'msg_proc;
                                }
                            };

                            let peer_pub1 = match crate::crypto::x3dh::parse_x448_spki(
                                peer_key1_spki,
                            ) {
                                Ok(k) => k,
                                Err(e) => {
                                    tracing::error!(
                                        "Contact BG: parse peer_key1 SPKI FAILED: {e}"
                                    );
                                    break 'msg_proc;
                                }
                            };
                            let peer_pub2 = match crate::crypto::x3dh::parse_x448_spki(
                                peer_key2_spki,
                            ) {
                                Ok(k) => k,
                                Err(e) => {
                                    tracing::error!(
                                        "Contact BG: parse peer_key2 SPKI FAILED: {e}"
                                    );
                                    break 'msg_proc;
                                }
                            };
                            tracing::info!(
                                "Contact BG: Peer X448 keys parsed: key1_pub[..4]={}, key2_pub[..4]={}",
                                hex::encode(&peer_pub1[..4]),
                                hex::encode(&peer_pub2[..4])
                            );

                            // ---- Stage 6: Load our persisted X448 keypairs ----
                            let (our_priv1_vec, our_pub1_vec, our_priv2_vec, _our_pub2_vec) =
                                match store_bg.load_e2e_keypairs(&contact_id_bg) {
                                    Ok(tuple) => tuple,
                                    Err(e) => {
                                        tracing::error!(
                                            "Contact BG: load_e2e_keypairs FAILED: {e}"
                                        );
                                        break 'msg_proc;
                                    }
                                };

                            if our_priv1_vec.len() != 56
                                || our_pub1_vec.len() != 56
                                || our_priv2_vec.len() != 56
                            {
                                tracing::error!(
                                    "Contact BG: our X448 keypair lengths unexpected: priv1={}, pub1={}, priv2={}",
                                    our_priv1_vec.len(),
                                    our_pub1_vec.len(),
                                    our_priv2_vec.len()
                                );
                                break 'msg_proc;
                            }

                            let mut our_priv1 = [0u8; 56];
                            let mut our_pub1 = [0u8; 56];
                            let mut our_priv2 = [0u8; 56];
                            our_priv1.copy_from_slice(&our_priv1_vec);
                            our_pub1.copy_from_slice(&our_pub1_vec);
                            our_priv2.copy_from_slice(&our_priv2_vec);

                            // ---- Stage 7: X3DH Bob-path ----
                            let x3dh = match crate::crypto::x3dh::x3dh_bob_shared_secret(
                                &our_priv1, &our_pub1, &our_priv2, &peer_pub1, &peer_pub2,
                            ) {
                                Ok(r) => r,
                                Err(e) => {
                                    tracing::error!("Contact BG: X3DH FAILED: {e}");
                                    break 'msg_proc;
                                }
                            };

                            tracing::info!("Contact BG: X3DH output:");
                            tracing::info!(
                                "  root_key[..4]                   = {}",
                                hex::encode(&x3dh.root_key[..4])
                            );
                            tracing::info!(
                                "  sending_header_key[..4]         = {}",
                                hex::encode(&x3dh.sending_header_key[..4])
                            );
                            tracing::info!(
                                "  receiving_next_header_key[..4]  = {}",
                                hex::encode(&x3dh.receiving_next_header_key[..4])
                            );
                            tracing::info!(
                                "  assoc_data_len                  = {}",
                                x3dh.assoc_data.len()
                            );
                            tracing::info!(
                                "  assoc_data[..8]                 = {}",
                                hex::encode(&x3dh.assoc_data[..8])
                            );

                            tracing::info!(
                                "Contact BG: Stage 7 complete - Root Key ready for Double Ratchet (Briefing 035b)"
                            );
                            emit_progress(
                                &update_tx_bg,
                                "x3dh_complete",
                                "X3DH complete. Root key derived, Double Ratchet seeded.",
                                "bootstrapping",
                            );
                            emit_progress(
                                &update_tx_bg,
                                "ratchet_init",
                                "Bob Double Ratchet initialized (post-quantum-aware)",
                                "bootstrapping",
                            );
                            emit_progress(
                                &update_tx_bg,
                                "reply_decrypt",
                                "Reply message decrypted (AES-256-GCM, 16-byte IV)",
                                "bootstrapping",
                            );

                            // ---- Stage 8: Init BobRatchet ----
                            ratchet_opt = Some(
                                crate::crypto::bob_ratchet::init_bob_ratchet(&x3dh, our_priv2),
                            );
                            let ratchet = ratchet_opt.as_mut().expect("just set");
                            tracing::info!("Contact BG: BobRatchet initialized (rcSnd=None, rcRcv=None)");

                            // ---- Stage 9: Parse EncRatchetMessage outer ----
                            let enc_msg = match crate::crypto::bob_ratchet::parse_enc_ratchet_message(
                                &conf.enc_conn_info,
                            ) {
                                Ok(m) => {
                                    tracing::info!(
                                        "Contact BG: EncRatchetMessage parsed: header={}B, body={}B",
                                        m.enc_header.len(),
                                        m.body.len()
                                    );
                                    m
                                }
                                Err(e) => {
                                    tracing::error!(
                                        "Contact BG: parse EncRatchetMessage FAILED: {}",
                                        e
                                    );
                                    break 'msg_proc;
                                }
                            };

                            // ---- Stage 10: Decrypt + parse EncMessageHeader ----
                            let header = match crate::crypto::bob_ratchet::decrypt_message_header(
                                enc_msg.enc_header,
                                &ratchet.next_header_key_receive,
                                &ratchet.assoc_data,
                            ) {
                                Ok(h) => {
                                    tracing::info!(
                                        "Contact BG: Header decrypted: max_version={}, PN={}, Ns={}, ratchet_pub[..4]={}",
                                        h.max_version,
                                        h.pn,
                                        h.ns,
                                        hex::encode(&h.ratchet_pub_spki[12..16])
                                    );
                                    h
                                }
                                Err(e) => {
                                    tracing::error!(
                                        "Contact BG: header decrypt FAILED: {}",
                                        e
                                    );
                                    tracing::debug!(
                                        "Contact BG: enc_header[..32]={}",
                                        hex::encode(&enc_msg.enc_header[..32.min(enc_msg.enc_header.len())])
                                    );
                                    tracing::debug!(
                                        "Contact BG: rcNHKr={}",
                                        hex::encode(&ratchet.next_header_key_receive)
                                    );
                                    break 'msg_proc;
                                }
                            };

                            // ---- Stage 11: DHRatchet + body decrypt ----
                            let plaintext =
                                match crate::crypto::bob_ratchet::dh_ratchet_and_decrypt_message(
                                    ratchet,
                                    &header,
                                    &enc_msg,
                                ) {
                                    Ok(p) => {
                                        tracing::info!(
                                            "Contact BG: Body decrypted, plaintext={}B (new root_key[..4]={})",
                                            p.len(),
                                            hex::encode(&ratchet.root_key[..4])
                                        );
                                        p
                                    }
                                    Err(e) => {
                                        tracing::error!(
                                            "Contact BG: body decrypt FAILED: {}",
                                            e
                                        );
                                        break 'msg_proc;
                                    }
                                };

                            // ---- Stage 12: Parse AgentMessage ----
                            match crate::crypto::bob_ratchet::parse_agent_conn_info_reply(
                                &plaintext,
                            ) {
                                Ok(_reply) => {
                                    tracing::info!(
                                        "Contact BG: AgentConnInfoReply parsed ('D' tag OK)"
                                    );
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        "Contact BG: AgentMessage parse failed: {} (first byte 0x{:02x})",
                                        e,
                                        plaintext.first().copied().unwrap_or(0)
                                    );
                                }
                            }

                            // ---- Stage 13: ASCII preview of plaintext ----
                            let ascii_preview: String = plaintext
                                .iter()
                                .take(200)
                                .map(|&b| {
                                    if (0x20..0x7e).contains(&b) {
                                        b as char
                                    } else {
                                        '.'
                                    }
                                })
                                .collect();
                            tracing::info!(
                                "Contact BG: plaintext preview (first 200 bytes as ASCII): {}",
                                ascii_preview
                            );
                            tracing::info!(
                                "*** CONNECTED *** Peer ConnInfo received and decrypted ***"
                            );

                            // ---- Stage 14: Structured AgentConnInfoReply parse ----
                            let reply_parsed = match crate::crypto::bob_ratchet::parse_agent_conn_info_reply_full(
                                &plaintext,
                            ) {
                                Ok(r) => {
                                    tracing::info!(
                                        "Contact BG: Reply parsed: {} SMP queue(s), {} bytes ConnInfo JSON",
                                        r.smp_queues.len(),
                                        r.conn_info_json.len()
                                    );
                                    r
                                }
                                Err(e) => {
                                    tracing::error!(
                                        "Contact BG: full AgentConnInfoReply parse FAILED: {e}"
                                    );
                                    break 'msg_proc;
                                }
                            };

                            for (i, q) in reply_parsed.smp_queues.iter().enumerate() {
                                tracing::info!(
                                    "Contact BG:   Queue[{}]: v={} {}:{} queue_id={} fingerprint[..4]={} dh[..4]={} mode={:?}",
                                    i,
                                    q.smp_client_version,
                                    q.server_host,
                                    q.server_port,
                                    hex::encode(&q.queue_id[..4]),
                                    hex::encode(&q.server_fingerprint[..4]),
                                    hex::encode(&q.sender_dh_public[..4]),
                                    q.queue_mode
                                );
                            }

                            // ---- Stage 15: Parse profile JSON ----
                            //
                            // Briefing 041b-hotfix: decouple the frontend
                            // ContactEstablished event from profile parse
                            // success. The match computes a (display_name,
                            // full_name) pair from the parsed profile OR
                            // falls back to the contact id prefix when
                            // parsing fails. ContactEstablished fires
                            // unconditionally AFTER the match.
                            let (peer_display_name, peer_full_name) =
                                match crate::protocol::conn_info::parse_conn_info_json(
                                    &reply_parsed.conn_info_json,
                                ) {
                                    Ok(envelope) => {
                                        tracing::info!(
                                            "Contact BG: ConnInfo envelope v={}, event={}",
                                            envelope.v,
                                            envelope.event
                                        );
                                        let profile = &envelope.params.profile;
                                        tracing::info!("Contact BG: *** PEER PROFILE ***");
                                        tracing::info!(
                                            "  displayName : {:?}",
                                            profile.display_name
                                        );
                                        tracing::info!(
                                            "  fullName    : {:?}",
                                            profile.full_name
                                        );
                                        tracing::info!(
                                            "  contactLink : {:?}",
                                            profile.contact_link
                                        );
                                        tracing::info!(
                                            "  image       : {}",
                                            profile
                                                .image
                                                .as_ref()
                                                .map(|s| format!("<{} chars>", s.len()))
                                                .unwrap_or_else(|| "None".into())
                                        );
                                        tracing::info!(
                                            "  preferences : {}",
                                            if profile.preferences.is_some() {
                                                "<present>"
                                            } else {
                                                "None"
                                            }
                                        );

                                        let dn = profile
                                            .display_name
                                            .clone()
                                            .unwrap_or_default();
                                        let fn_ = profile
                                            .full_name
                                            .clone()
                                            .unwrap_or_default();
                                        emit_progress(
                                            &update_tx_bg,
                                            "profile_parsed",
                                            &format!(
                                                "Peer profile received: {}",
                                                if dn.is_empty() {
                                                    "(no display name)"
                                                } else {
                                                    dn.as_str()
                                                }
                                            ),
                                            "bootstrapping",
                                        );
                                        (dn, fn_)
                                    }
                                    Err(e) => {
                                        // Briefing 041b-hotfix Phase H3:
                                        // richer diagnostic logging so we can
                                        // reverse-engineer the bytes offline.
                                        let total_len = reply_parsed.conn_info_json.len();
                                        let preview_end = total_len.min(256);
                                        let hex_preview: String = reply_parsed
                                            .conn_info_json[..preview_end]
                                            .iter()
                                            .map(|b| format!("{:02x}", b))
                                            .collect();
                                        let ascii_preview: String = reply_parsed
                                            .conn_info_json[..preview_end]
                                            .iter()
                                            .map(|&b| {
                                                if (32..=126).contains(&b) {
                                                    b as char
                                                } else {
                                                    '.'
                                                }
                                            })
                                            .collect();
                                        tracing::error!(
                                            target: "sgx_simplex::profile_debug",
                                            "Profile parse FAILED. Total ConnInfo length: {} B. First 256 B hex: {}",
                                            total_len, hex_preview
                                        );
                                        tracing::error!(
                                            target: "sgx_simplex::profile_debug",
                                            "Profile parse FAILED. First 256 B ASCII: {}",
                                            ascii_preview
                                        );
                                        tracing::error!(
                                            target: "sgx_simplex::profile_debug",
                                            "Profile parse FAILED. Error kind: {}",
                                            e
                                        );
                                        if total_len > 256 {
                                            let tail_start =
                                                total_len.saturating_sub(64);
                                            let tail_hex: String = reply_parsed
                                                .conn_info_json[tail_start..]
                                                .iter()
                                                .map(|b| format!("{:02x}", b))
                                                .collect();
                                            tracing::error!(
                                                target: "sgx_simplex::profile_debug",
                                                "Profile parse FAILED. Last 64 B hex: {}",
                                                tail_hex
                                            );
                                        }
                                        // Fallback display name: first 8 chars
                                        // of the local contact UUID. The
                                        // contact is still established on our
                                        // side; the user can rename later.
                                        let fallback = contact_id_bg
                                            .chars()
                                            .take(8)
                                            .collect::<String>();
                                        tracing::warn!(
                                            "Contact BG: profile parse failed; using fallback display_name='{}'. Contact is still established.",
                                            fallback
                                        );
                                        (fallback, String::new())
                                    }
                                };

                            // Hoisted: ALWAYS emit ContactEstablished, even
                            // when the profile JSON parse failed. Without
                            // this, the frontend never learns about the
                            // contact, never shows it in the sidebar, and
                            // the user cannot send to it - which blocks all
                            // further 041b live testing.
                            let now = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .map(|d| d.as_secs() as i64)
                                .unwrap_or(0);
                            let update = SimplexUpdate {
                                update: Some(simplex_update::Update::ContactEstablished(
                                    SimplexContactEstablished {
                                        contact_id: contact_id_bg.clone(),
                                        display_name: peer_display_name.clone(),
                                        full_name: peer_full_name,
                                        bio: String::new(),
                                        established_at: now,
                                    },
                                )),
                            };
                            let _ = update_tx_bg.send(update);
                            tracing::info!(
                                "Contact BG: emitted contact-established event for {} (display_name='{}')",
                                contact_id_bg,
                                peer_display_name
                            );

                            tracing::info!(
                                "Contact BG: Stage 15 complete - peer identity established"
                            );

                            // ---- Stage 16: Send AgentConfirmation to peer reply queue ----
                            //
                            // Mirrors GoChat connection.ts:1110-1175 sendHandshake. This
                            // replaces the previous HELLO-as-first-message implementation
                            // which caused SEInvitationNotFound on the Desktop because
                            // Desktop expects an AgentConfirmation (tag 'C') with
                            // PHConfirmation header on its fresh reply queue, not an
                            // AgentMsgEnvelope (tag 'M') with PHEmpty.
                            emit_progress(
                                &update_tx_bg,
                                "sending_ratchet",
                                "Initializing sending ratchet (new X448 keypair + chainKdf)...",
                                "bootstrapping",
                            );
                            emit_progress(
                                &update_tx_bg,
                                "confirmation_send",
                                "Sending AgentConfirmation with our profile to peer reply queue...",
                                "bootstrapping",
                            );
                            let peer_queue_owned = match reply_parsed.smp_queues.first() {
                                Some(q) => q.clone(),
                                None => {
                                    tracing::error!(
                                        "Contact BG: no peer reply queue in AgentConnInfoReply"
                                    );
                                    break 'msg_proc;
                                }
                            };
                            // Capture the peer's reply queue for chat sends in
                            // later loop iterations. Set BEFORE Stage 16's SEND
                            // because if 16.7 itself fails the contact is broken
                            // anyway and the value is harmless to retain.
                            peer_queue = Some(peer_queue_owned.clone());

                            // 16.1: Generate sender auth keypair (X25519) and persist.
                            let (sender_auth_priv, sender_auth_pub) =
                                crate::crypto::keys::generate_x25519();
                            let sender_auth_spki =
                                crate::crypto::keys::encode_x25519_spki(&sender_auth_pub);
                            if let Err(e) = store_bg.save_sender_auth_keypair(
                                &contact_id_bg,
                                sender_auth_priv.as_bytes(),
                                &sender_auth_spki,
                            ) {
                                tracing::warn!(
                                    "Contact BG: save_sender_auth_keypair failed: {e}"
                                );
                            }
                            tracing::info!(
                                "Contact BG: sender auth key generated, pub[..4]={}",
                                hex::encode(&sender_auth_spki[12..16])
                            );

                            // 16.2: Build AgentConnInfo ('I' + profile JSON).
                            //
                            // Load the stored user profile (full_name, bio) directly
                            // from the store. If nothing is configured yet we fall
                            // back to the hoisted profile_name_bg (possibly itself a
                            // "SimpleGoX" fallback) and empty full_name / bio.
                            let (pn, fn_, bio) = match store_bg.get_user_profile() {
                                Ok(Some(p)) => (p.display_name, p.full_name, p.bio),
                                _ => (profile_name_bg.clone(), String::new(), String::new()),
                            };
                            let conn_info_plain =
                                crate::protocol::agent_msg::encode_agent_conn_info_full(
                                    &pn, &fn_, &bio,
                                );
                            tracing::info!(
                                "Contact BG: AgentConnInfo plaintext {} bytes (display='{}', full='{}', bio={}B)",
                                conn_info_plain.len(),
                                pn,
                                fn_,
                                bio.len()
                            );

                            // 16.3: Ratchet encrypt the AgentConnInfo body.
                            let our_new_pub_raw = match ratchet.our_new_ratchet_pub {
                                Some(p) => p,
                                None => {
                                    tracing::error!(
                                        "Contact BG: our_new_ratchet_pub missing after DH step"
                                    );
                                    break 'msg_proc;
                                }
                            };
                            let sending_ck = match ratchet.sending_chain_key {
                                Some(c) => c,
                                None => {
                                    tracing::error!(
                                        "Contact BG: sending_chain_key missing"
                                    );
                                    break 'msg_proc;
                                }
                            };
                            let (new_ck, mk, body_iv, _header_iv) =
                                match crate::crypto::bob_ratchet::chain_kdf(&sending_ck) {
                                    Ok(x) => x,
                                    Err(e) => {
                                        tracing::error!("Contact BG: chainKdf failed: {e}");
                                        break 'msg_proc;
                                    }
                                };
                            let mut our_new_pub_spki = [0u8; 68];
                            our_new_pub_spki[..12].copy_from_slice(
                                &crate::crypto::x3dh::X448_SPKI_HEADER,
                            );
                            our_new_pub_spki[12..].copy_from_slice(&our_new_pub_raw);
                            let msg_header_plain =
                                crate::crypto::bob_ratchet::encode_msg_header(
                                    3,
                                    &our_new_pub_spki,
                                    ratchet.pn,
                                    ratchet.ns,
                                );
                            let enc_message_header =
                                match crate::crypto::bob_ratchet::encrypt_message_header(
                                    &msg_header_plain,
                                    // 041b-ratchet-fix RF4: read CURRENT
                                    // sending_header_key (hks analog), which
                                    // after MSG #1's AdvanceRatchet promotion
                                    // equals the X3DH init value - exactly
                                    // what Stage 16's pre-fix path used via
                                    // the frozen `next_header_key_send`.
                                    // Byte-for-byte identical behaviour.
                                    &ratchet.sending_header_key,
                                    &ratchet.assoc_data,
                                    3,
                                    // Stage 16 AgentConfirmation reply: keep
                                    // simplexmq PQSupportOn value (2310) that
                                    // was in production prior to 041b-crypto-fix
                                    // and produces the wire shape the peer's
                                    // handshake path accepts today.
                                    crate::crypto::bob_ratchet::PADDED_HEADER_LEN,
                                ) {
                                    Ok(h) => h,
                                    Err(e) => {
                                        tracing::error!(
                                            "Contact BG: encrypt_message_header failed: {e}"
                                        );
                                        break 'msg_proc;
                                    }
                                };
                            // padded_msg_len budget when wrapped in PHConfirmation +
                            // AgentConfirmation envelope inside 15904B NaCl plaintext:
                            //   15904 (padded plaintext)
                            //   - 2  (Word16 length prefix)
                            //   - 46 (PHConfirmation: 'K' + 1B + 44B SPKI)
                            //   - 4  (AgentConfirmation envelope: 2B ver + 'C' + '0')
                            //   - 2  (EncRatchetMessage header length prefix)
                            //   - 2346 (enc_message_header)
                            //   - 16 (body AEAD tag)
                            //   = 13488 max
                            let enc_ratchet_msg = match crate::crypto::bob_ratchet::encrypt_and_assemble_ratchet_message(
                                &conn_info_plain,
                                &enc_message_header,
                                &mk,
                                &body_iv,
                                &ratchet.assoc_data,
                                13488,
                            ) {
                                Ok(m) => m,
                                Err(e) => {
                                    tracing::error!(
                                        "Contact BG: assemble ratchet message failed: {e}"
                                    );
                                    break 'msg_proc;
                                }
                            };
                            tracing::info!(
                                "Contact BG: EncRatchetMessage assembled {} bytes",
                                enc_ratchet_msg.len()
                            );

                            // 16.4: Wrap in AgentConfirmation envelope (tag 'C', v=7).
                            let agent_conf_envelope =
                                crate::e2e_crypto::build_agent_confirmation_envelope(
                                    7,
                                    &enc_ratchet_msg,
                                );

                            // 16.5: Build PHConfirmation 'K' priv_header with senderAuthKey.
                            let priv_header =
                                crate::e2e_crypto::build_phconfirmation_header(
                                    &sender_auth_spki,
                                );

                            // 16.6: Layer 2 NaCl encrypt with fresh X25519 keypair.
                            //
                            // Briefing 041b-crypto-fix CF3: PERSIST the
                            // private half. Peer stores the matching
                            // public half via its first-message PubHeader
                            // decode; all subsequent chat sends emit
                            // PubHeader `Nothing` and rely on the peer's
                            // stored key, so they must reuse THIS private
                            // to land on the same DH shared secret.
                            let (our_l2_priv, our_l2_pub) =
                                crate::crypto::keys::generate_x25519();
                            let our_l2_priv_bytes = *our_l2_priv.as_bytes();
                            if let Err(e) = store_bg.save_own_l2_ephemeral_private(
                                &contact_id_bg,
                                &our_l2_priv_bytes,
                            ) {
                                tracing::warn!(
                                    "Contact BG: save_own_l2_ephemeral_private failed: {e} (chat sends will fail until this is recovered)"
                                );
                            } else {
                                tracing::info!(
                                    "Contact BG: persisted own L2 ephemeral private for chat reuse"
                                );
                            }
                            let layer2_envelope = crate::e2e_crypto::e2e_encrypt_agent_msg(
                                &agent_conf_envelope,
                                &peer_queue_owned.sender_dh_public,
                                &our_l2_priv_bytes,
                                our_l2_pub.as_bytes(),
                                true, // first message to peer queue: inline DH pub
                                &priv_header,
                            );
                            tracing::info!(
                                "Contact BG: Layer 2 envelope {} bytes (PHConfirmation + inline DH pub)",
                                layer2_envelope.len()
                            );

                            // 16.7: SEND (unsigned) to peer reply queue.
                            let send_tx = crate::smp_commands::cmd_send_unsigned(
                                &smp,
                                &peer_queue_owned.queue_id,
                                &layer2_envelope,
                                b'C',
                                false,
                            );
                            if let Err(e) = smp.write_command_block(&send_tx).await {
                                tracing::error!(
                                    "Contact BG: AgentConfirmation SEND write failed: {e}"
                                );
                                break 'msg_proc;
                            }
                            match smp.read_responses().await {
                                Ok(r) => {
                                    let ok = r.iter().any(|x| {
                                        matches!(x, crate::smp_protocol::ServerResponse::Ok)
                                    });
                                    tracing::info!(
                                        "Contact BG: AgentConfirmation SEND response: {:?}",
                                        r.iter()
                                            .map(|x| format!("{x:?}")
                                                .chars()
                                                .take(80)
                                                .collect::<String>())
                                            .collect::<Vec<_>>()
                                    );
                                    if ok {
                                        // Commit ratchet state advance (no hash chain
                                        // for AgentConnInfo - it has no APrivHeader).
                                        ratchet.sending_chain_key = Some(new_ck);
                                        ratchet.ns += 1;
                                        tracing::info!(
                                            "*** INVITATION ACCEPTED *** Desktop should now show contact with display name '{profile_name_bg}'"
                                        );
                                        emit_progress(
                                            &update_tx_bg,
                                            "identity_established",
                                            "Contact established via end-to-end encrypted channel",
                                            "connected",
                                        );
                                    } else {
                                        emit_progress(
                                            &update_tx_bg,
                                            "send_failed",
                                            "Failed to send AgentConfirmation",
                                            "error",
                                        );
                                        tracing::error!(
                                            "Contact BG: AgentConfirmation SEND did not return Ok"
                                        );
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        "Contact BG: AgentConfirmation SEND response error: {e}"
                                    );
                                }
                            }
                            } else {
                                // ==== MSG #2+ flow: regular AgentMessage from peer ====
                                let ratchet = match ratchet_opt.as_mut() {
                                    Some(r) => r,
                                    None => {
                                        tracing::error!(
                                            "Contact BG: MSG #{} arrived but ratchet not initialized (MSG #1 never processed)",
                                            msg_counter
                                        );
                                        break 'msg_proc;
                                    }
                                };
                                if plaintext.is_empty() || plaintext[0] != b'_' {
                                    tracing::warn!(
                                        "Contact BG: MSG #{} - unexpected PrivHeader 0x{:02x} (expected '_' for regular AgentMessage)",
                                        msg_counter,
                                        plaintext.first().copied().unwrap_or(0)
                                    );
                                    break 'msg_proc;
                                }
                                let envelope_bytes = &plaintext[1..];
                                if envelope_bytes.len() < 3 || envelope_bytes[2] != b'M' {
                                    tracing::error!(
                                        "Contact BG: MSG #{} - AgentMsgEnvelope prefix invalid: {:02x?}",
                                        msg_counter,
                                        &envelope_bytes[..3.min(envelope_bytes.len())]
                                    );
                                    break 'msg_proc;
                                }
                                let peer_agent_version = u16::from_be_bytes([
                                    envelope_bytes[0],
                                    envelope_bytes[1],
                                ]);
                                let enc_agent_message = &envelope_bytes[3..];
                                tracing::info!(
                                    "Contact BG: MSG #{} - AgentMsgEnvelope v={}, encAgentMessage={}B",
                                    msg_counter,
                                    peer_agent_version,
                                    enc_agent_message.len()
                                );

                                let enc_msg = match crate::crypto::bob_ratchet::parse_enc_ratchet_message(
                                    enc_agent_message,
                                ) {
                                    Ok(m) => m,
                                    Err(e) => {
                                        tracing::error!(
                                            "Contact BG: MSG #{} - parse EncRatchetMessage FAILED: {}",
                                            msg_counter,
                                            e
                                        );
                                        break 'msg_proc;
                                    }
                                };
                                tracing::info!(
                                    "Contact BG: MSG #{} - EncRatchetMessage header={}B body={}B",
                                    msg_counter,
                                    enc_msg.enc_header.len(),
                                    enc_msg.body.len()
                                );

                                let (header, step) =
                                    match crate::crypto::bob_ratchet::decrypt_header_and_classify_step(
                                        enc_msg.enc_header,
                                        &ratchet,
                                    ) {
                                        Ok(x) => x,
                                        Err(e) => {
                                            tracing::error!(
                                                "Contact BG: MSG #{} - header decrypt FAILED: {}",
                                                msg_counter,
                                                e
                                            );
                                            break 'msg_proc;
                                        }
                                    };
                                tracing::info!(
                                    "Contact BG: MSG #{} - step={:?}, header PN={} Ns={}",
                                    msg_counter,
                                    step,
                                    header.pn,
                                    header.ns
                                );

                                let agent_msg_plaintext = match step {
                                    crate::crypto::bob_ratchet::RatchetStep::SameRatchet => {
                                        match crate::crypto::bob_ratchet::same_ratchet_decrypt_body(
                                            ratchet,
                                            &enc_msg,
                                        ) {
                                            Ok(p) => p,
                                            Err(e) => {
                                                tracing::error!(
                                                    "Contact BG: MSG #{} - SameRatchet body decrypt FAILED: {}",
                                                    msg_counter,
                                                    e
                                                );
                                                break 'msg_proc;
                                            }
                                        }
                                    }
                                    crate::crypto::bob_ratchet::RatchetStep::AdvanceRatchet => {
                                        match crate::crypto::bob_ratchet::dh_ratchet_and_decrypt_message(
                                            ratchet,
                                            &header,
                                            &enc_msg,
                                        ) {
                                            Ok(p) => p,
                                            Err(e) => {
                                                tracing::error!(
                                                    "Contact BG: MSG #{} - AdvanceRatchet body decrypt FAILED: {}",
                                                    msg_counter,
                                                    e
                                                );
                                                break 'msg_proc;
                                            }
                                        }
                                    }
                                };
                                tracing::info!(
                                    "Contact BG: MSG #{} - AgentMessage plaintext {}B",
                                    msg_counter,
                                    agent_msg_plaintext.len()
                                );
                                // Hex-dump of first 16 bytes for wire-format verification.
                                // Expected layout: 'M'(1) + sndMsgId(8 BE) + hashLen(1)
                                // + prevHash(hashLen) + inner tag ('H' / 'M' / ...)
                                let hex_preview_len = 16.min(agent_msg_plaintext.len());
                                tracing::info!(
                                    "Contact BG: MSG #{} - plaintext[..{}]={}",
                                    msg_counter,
                                    hex_preview_len,
                                    hex::encode(&agent_msg_plaintext[..hex_preview_len])
                                );

                                match crate::protocol::agent_msg::parse_agent_message_content(
                                    &agent_msg_plaintext,
                                ) {
                                    Ok(content) => match content {
                                        crate::protocol::agent_msg::AgentMessageContent::Hello {
                                            snd_msg_id,
                                            prev_msg_hash,
                                        } => {
                                            tracing::info!(
                                                "*** PEER HELLO *** sndMsgId={}, prev_hash={}B, prev_hash[..4]={}",
                                                snd_msg_id,
                                                prev_msg_hash.len(),
                                                hex::encode(
                                                    &prev_msg_hash[..4.min(prev_msg_hash.len())]
                                                )
                                            );

                                            // Briefing 041b-hello: send our
                                            // HELLO back to the peer on their
                                            // reply queue so they transition
                                            // the contact to CONNECTED state.
                                            // Without this, peer rejects all
                                            // subsequent chat messages with
                                            // `error chat contactNotReady`.
                                            //
                                            // Mirrors GoChat connection.ts:
                                            //   776-785 (auto-send on peer HELLO)
                                            //   1181-1211 (sendHello wire path)
                                            //
                                            // Idempotency: peer may retransmit
                                            // HELLO on reconnect; the
                                            // hello_sent flag guards against
                                            // double-send.
                                            let already_sent = store_bg
                                                .get_hello_sent(&contact_id_bg)
                                                .unwrap_or(false);
                                            if already_sent {
                                                tracing::info!(
                                                    "Contact BG: HELLO already sent to peer, skipping"
                                                );
                                            } else if let Some(peer_q) = peer_queue.as_ref() {
                                                match send_our_hello(
                                                    &mut smp,
                                                    ratchet,
                                                    &store_bg,
                                                    &contact_id_bg,
                                                    peer_q,
                                                )
                                                .await
                                                {
                                                    Ok(sent_msg_id) => {
                                                        tracing::info!(
                                                            "Contact BG: HELLO sent to peer reply queue sndMsgId={} (counters now at next_snd_msg_id={}, last_snd_hash[..4]={:02x?})",
                                                            sent_msg_id,
                                                            ratchet.next_snd_msg_id,
                                                            &ratchet
                                                                .last_snd_msg_hash
                                                                [..4.min(ratchet.last_snd_msg_hash.len())]
                                                        );
                                                        if let Err(e) = store_bg
                                                            .set_hello_sent(&contact_id_bg)
                                                        {
                                                            tracing::warn!(
                                                                "Contact BG: set_hello_sent persist failed: {e} (will retry on next peer HELLO)"
                                                            );
                                                        }
                                                    }
                                                    Err(e) => {
                                                        tracing::error!(
                                                            "Contact BG: HELLO send FAILED: {e} (peer will keep contact in pending state; send will retry on next peer HELLO)"
                                                        );
                                                    }
                                                }
                                            } else {
                                                tracing::warn!(
                                                    "Contact BG: cannot send HELLO, peer_queue not yet known"
                                                );
                                            }
                                        }
                                        crate::protocol::agent_msg::AgentMessageContent::Message {
                                            snd_msg_id,
                                            prev_msg_hash,
                                            body,
                                        } => {
                                            let body_text = String::from_utf8_lossy(&body);
                                            tracing::info!(
                                                "*** PEER MESSAGE *** sndMsgId={}, prev_hash[..4]={}, {}B body",
                                                snd_msg_id,
                                                hex::encode(&prev_msg_hash[..4.min(prev_msg_hash.len())]),
                                                body.len()
                                            );
                                            let body_preview_len = 48.min(body.len());
                                            tracing::info!(
                                                "*** BODY TEXT: {:?}",
                                                body_text
                                            );
                                            tracing::info!(
                                                "*** BODY HEX [..{}]: {}",
                                                body_preview_len,
                                                hex::encode(&body[..body_preview_len])
                                            );

                                            // Publish NewMessage event. We
                                            // forward the raw body as a UTF-8
                                            // string (the peer's x.msg.new
                                            // JSON envelope). Consumers are
                                            // expected to parse params.content
                                            // themselves; sending it as a
                                            // string keeps us agnostic of
                                            // SimpleX's chat schema evolution.
                                            let now = std::time::SystemTime::now()
                                                .duration_since(std::time::UNIX_EPOCH)
                                                .map(|d| d.as_secs() as i64)
                                                .unwrap_or(0);
                                            let update = SimplexUpdate {
                                                update: Some(
                                                    simplex_update::Update::NewMessage(
                                                        SimplexNewMessage {
                                                            contact_id: contact_id_bg.clone(),
                                                            msg_id: snd_msg_id as i64,
                                                            timestamp: now,
                                                            body: body_text.into_owned(),
                                                            is_own: false,
                                                        },
                                                    ),
                                                ),
                                            };
                                            let _ = update_tx_bg.send(update);
                                        }
                                        crate::protocol::agent_msg::AgentMessageContent::Receipt {
                                            snd_msg_id,
                                            raw,
                                            ..
                                        } => {
                                            tracing::info!(
                                                "*** PEER RECEIPT *** sndMsgId={}, {}B raw",
                                                snd_msg_id,
                                                raw.len()
                                            );
                                        }
                                        crate::protocol::agent_msg::AgentMessageContent::Other {
                                            snd_msg_id,
                                            tag,
                                            raw,
                                            ..
                                        } => {
                                            tracing::info!(
                                                "*** PEER OTHER *** sndMsgId={}, tag=0x{:02x} ({}), {}B raw",
                                                snd_msg_id,
                                                tag,
                                                tag as char,
                                                raw.len()
                                            );
                                        }
                                    },
                                    Err(e) => {
                                        tracing::error!(
                                            "Contact BG: MSG #{} - parse_agent_message_content FAILED: {}",
                                            msg_counter,
                                            e
                                        );
                                        tracing::debug!(
                                            "Contact BG: plaintext[..32]={}",
                                            hex::encode(
                                                &agent_msg_plaintext
                                                    [..32.min(agent_msg_plaintext.len())]
                                            )
                                        );
                                    }
                                }
                            }
                            } // 'msg_proc

                            if msg_counter >= 20 {
                                tracing::warn!(
                                    "BG loop: reached msg_counter={}, stopping for safety",
                                    msg_counter
                                );
                                break 'outer;
                            }
                        } else if let ServerResponse::End = resp {
                            tracing::warn!("Contact BG: END");
                            break 'outer;
                        } else if let ServerResponse::Pong = resp {
                            // Briefing 044 W2: silent keep-alive
                            // acknowledgement. `trace!` only so the 30 s
                            // cadence does not appear in default-level
                            // logs. Presence of PONG implicitly proves the
                            // full TLS + SMP stack is alive; no further
                            // action needed.
                            tracing::trace!("Contact BG: PONG");
                        }
                    }
                }
                Err(e) => {
                    // Briefing 044 W3: recoverable IO / TLS errors feed
                    // into the reconnect loop; anything else (protocol
                    // parse fail, crypto mismatch, wrong-version) exits
                    // the session because a reconnect cannot fix
                    // state-level disagreement.
                    let now_ms = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis() as i64)
                        .unwrap_or(0);
                    // Briefing 044a D2.6: full error surface BEFORE the
                    // is_recoverable verdict. We log Display, Debug, and
                    // (for SmpError::Io) the inner io::Error.kind() so the
                    // post-mortem tells us exactly what tokio_rustls
                    // surfaced when the server dropped us.
                    let io_kind_dbg: String = match &e {
                        crate::smp_protocol::SmpError::Io(io_err) => {
                            format!("Io(kind={:?}, msg={:?})", io_err.kind(), io_err.to_string())
                        }
                        other => format!("{other:?}"),
                    };
                    tracing::info!(
                        "READ: error from read_responses, error_display={e}, error_debug={io_kind_dbg}, checking if recoverable"
                    );
                    let recoverable_verdict = e.is_recoverable();
                    // Briefing 044a D2.7: log the verdict so the recoverable
                    // path's own logs (line below) can be cross-referenced.
                    tracing::info!(
                        "READ: is_recoverable={recoverable_verdict} for error above"
                    );
                    if !recoverable_verdict {
                        tracing::error!(
                            "Contact BG: unrecoverable read error: {e}, exiting"
                        );
                        let _ = state_tx.send(crate::contact_session::ConnState::Dead);
                        let _ = update_tx_bg.send(SimplexUpdate {
                            update: Some(simplex_update::Update::ContactDead(
                                SimplexContactDead {
                                    contact_id: contact_id_bg.clone(),
                                    timestamp: now_ms,
                                },
                            )),
                        });
                        break 'outer;
                    }
                    tracing::warn!(
                        "Contact BG: connection lost: {e}. Entering reconnect."
                    );
                    // Briefing 044 W5: publish Reconnecting BEFORE the
                    // long-running reconnect call so the gRPC handler's
                    // state check immediately sees the new state and
                    // fails fast (no mpsc pile-up behind the blocked
                    // select!).
                    let _ = state_tx.send(crate::contact_session::ConnState::Reconnecting);
                    // Briefing 044 W6: announce Disconnected so the UI
                    // dot flips to yellow before the first reconnect
                    // attempt fires its own ContactReconnecting event.
                    let _ = update_tx_bg.send(SimplexUpdate {
                        update: Some(simplex_update::Update::ContactDisconnected(
                            SimplexContactDisconnected {
                                contact_id: contact_id_bg.clone(),
                                timestamp: now_ms,
                            },
                        )),
                    });
                    match reconnect_with_backoff(
                        &mut smp,
                        &addr,
                        server_key_hash,
                        &rcv_auth,
                        &rcv_id,
                        &update_tx_bg,
                        &contact_id_bg,
                        // Briefing 044c: queue_auth_private lookup target.
                        // `store_bg` is the Arc<QueueStore> already captured
                        // by this BG-loop closure; deref to &QueueStore to
                        // match the helper's borrowed-reference signature.
                        &store_bg,
                    )
                    .await
                    {
                        Ok(()) => {
                            tracing::info!(
                                "Contact BG: reconnected (fresh session_id={}...), resuming receive loop",
                                hex::encode(&smp.session_id[..4])
                            );
                            let _ = state_tx.send(
                                crate::contact_session::ConnState::Connected,
                            );
                            let ok_ts = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .map(|d| d.as_millis() as i64)
                                .unwrap_or(0);
                            let _ = update_tx_bg.send(SimplexUpdate {
                                update: Some(simplex_update::Update::ContactReconnected(
                                    SimplexContactReconnected {
                                        contact_id: contact_id_bg.clone(),
                                        timestamp: ok_ts,
                                    },
                                )),
                            });
                        }
                        Err(e) => {
                            tracing::error!(
                                "Contact BG: reconnect gave up: {e}, exiting"
                            );
                            let _ = state_tx.send(
                                crate::contact_session::ConnState::Dead,
                            );
                            let dead_ts = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .map(|d| d.as_millis() as i64)
                                .unwrap_or(0);
                            let _ = update_tx_bg.send(SimplexUpdate {
                                update: Some(simplex_update::Update::ContactDead(
                                    SimplexContactDead {
                                        contact_id: contact_id_bg.clone(),
                                        timestamp: dead_ts,
                                    },
                                )),
                            });
                            break 'outer;
                        }
                    }
                }
            }
                } // close `read_result` arm body of tokio::select!
                cmd_opt = cmd_rx.recv() => {
                    match cmd_opt {
                        Some(cmd) => match cmd {
                            ContactCommand::SendText { body, reply } => {
                                tracing::info!(
                                    "Contact BG: SendText received contact={} body_len={}",
                                    contact_id_bg,
                                    body.len()
                                );

                                // Both ratchet and peer_queue are populated
                                // by MSG #1's Stage 16; before that, sending
                                // is not possible.
                                let ratchet = match ratchet_opt.as_mut() {
                                    Some(r) => r,
                                    None => {
                                        tracing::warn!(
                                            "Contact BG: SendText rejected, ratchet not initialised yet"
                                        );
                                        let _ = reply.send(Err(
                                            ContactSendError::RatchetStateMissing,
                                        ));
                                        continue;
                                    }
                                };
                                let peer_q = match peer_queue.as_ref() {
                                    Some(q) => q,
                                    None => {
                                        tracing::warn!(
                                            "Contact BG: SendText rejected, peer reply queue not yet known"
                                        );
                                        let _ = reply.send(Err(
                                            ContactSendError::RatchetStateMissing,
                                        ));
                                        continue;
                                    }
                                };

                                // Briefing 041b-fix: load the X25519
                                // sender_auth_private persisted during the
                                // handshake Stage 16.1. This key authenticates
                                // SENDs on the peer's reply queue (which the
                                // peer secured with our matching public half
                                // via SKEY). Without it the server replies
                                // ERR AUTH.
                                let sender_auth_private = match store_bg
                                    .load_sender_auth_private(&contact_id_bg)
                                {
                                    Ok(Some(k)) => k,
                                    Ok(None) => {
                                        tracing::error!(
                                            "Contact BG: SendText rejected, sender_auth_private not persisted for contact={} (handshake incomplete?)",
                                            contact_id_bg
                                        );
                                        let _ = reply.send(Err(
                                            ContactSendError::RatchetStateMissing,
                                        ));
                                        continue;
                                    }
                                    Err(e) => {
                                        tracing::error!(
                                            "Contact BG: SendText rejected, load_sender_auth_private failed: {e}"
                                        );
                                        let _ = reply.send(Err(
                                            ContactSendError::PersistenceFailed(format!(
                                                "load_sender_auth_private: {e}"
                                            )),
                                        ));
                                        continue;
                                    }
                                };

                                // Briefing 041b-crypto-fix CF3: load the
                                // X25519 L2 ephemeral private key that was
                                // generated + persisted during Stage 16's
                                // AgentConfirmation reply. Reusing this
                                // exact key is mandatory: peer stored the
                                // matching public half and derives the
                                // L2 shared secret against it. A fresh
                                // keypair would desync the L2 NaCl box.
                                let own_l2_ephemeral_private = match store_bg
                                    .load_own_l2_ephemeral_private(&contact_id_bg)
                                {
                                    Ok(Some(k)) => k,
                                    Ok(None) => {
                                        tracing::error!(
                                            "Contact BG: SendText rejected, own_l2_ephemeral_private not persisted for contact={} (handshake pre-dates 041b-crypto-fix or Stage 16 did not persist)",
                                            contact_id_bg
                                        );
                                        let _ = reply.send(Err(
                                            ContactSendError::RatchetStateMissing,
                                        ));
                                        continue;
                                    }
                                    Err(e) => {
                                        tracing::error!(
                                            "Contact BG: SendText rejected, load_own_l2_ephemeral_private failed: {e}"
                                        );
                                        let _ = reply.send(Err(
                                            ContactSendError::PersistenceFailed(format!(
                                                "load_own_l2_ephemeral_private: {e}"
                                            )),
                                        ));
                                        continue;
                                    }
                                };

                                // Snapshot the values used in the AgentMessage
                                // before encryption. send_agent_message_encrypted
                                // advances the ratchet ONLY on SMP Ok, so on a
                                // failed send the same snd_msg_id and
                                // last_snd_msg_hash remain valid for retry.
                                let send_msg_id = ratchet.next_snd_msg_id;
                                let prev_hash = ratchet.last_snd_msg_hash.clone();

                                // Briefing 041b-json: SimpleX Desktop
                                // silently drops A_MSG bodies that are not
                                // parseable as a ChatMessage JSON envelope.
                                // Wrap the raw user text in the canonical
                                // {"event":"x.msg.new","params":{"content":
                                // {"text":"...","type":"text"}}} shape.
                                // Reference: goChatX connection.ts:1218-1233.
                                let json_body =
                                    crate::protocol::agent_msg::build_chat_text_envelope(
                                        &body,
                                    );
                                tracing::info!(
                                    target: "sgx_simplex::send",
                                    "SendText wrapping body: raw_len={}, json_envelope_len={}",
                                    body.len(),
                                    json_body.len()
                                );
                                tracing::debug!(
                                    target: "sgx_simplex::send",
                                    "SendText json_body preview: {}",
                                    &json_body.chars().take(120).collect::<String>()
                                );

                                let agent_msg_plain =
                                    crate::protocol::agent_msg::encode_agent_message_text(
                                        send_msg_id,
                                        &prev_hash,
                                        json_body.as_bytes(),
                                    );

                                let send_ok = send_agent_message_encrypted(
                                    &mut smp,
                                    ratchet,
                                    &sender_auth_private,
                                    &own_l2_ephemeral_private,
                                    peer_q,
                                    &agent_msg_plain,
                                    // Briefing 041b-crypto-fix CF3: chat
                                    // sends are PubHeader Nothing, pad 15840.
                                    // Stage 16's first-message-to-reply-queue
                                    // path (inline Just + pad 15904) stays
                                    // separate and unchanged.
                                    false,
                                    b'M',
                                    "Contact BG SendText",
                                )
                                .await;

                                let now_ms = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .map(|d| d.as_millis() as i64)
                                    .unwrap_or(0);

                                if send_ok {
                                    tracing::info!(
                                        "Contact BG: SendText OK contact={} msg_id={} body_len={}",
                                        contact_id_bg,
                                        send_msg_id,
                                        body.len()
                                    );

                                    // Echo the own message into the same
                                    // SimplexUpdate stream the receive path
                                    // uses, so the frontend's existing
                                    // sx-new-message subscriber renders the
                                    // own bubble. Timestamp uses seconds to
                                    // match the receive-side encoding.
                                    let update = SimplexUpdate {
                                        update: Some(simplex_update::Update::NewMessage(
                                            SimplexNewMessage {
                                                contact_id: contact_id_bg.clone(),
                                                msg_id: send_msg_id as i64,
                                                timestamp: now_ms / 1000,
                                                body: body.clone(),
                                                is_own: true,
                                            },
                                        )),
                                    };
                                    let _ = update_tx_bg.send(update);

                                    let _ = reply.send(Ok(SendTextResult {
                                        msg_id: send_msg_id,
                                        timestamp_ms: now_ms,
                                    }));
                                } else {
                                    tracing::error!(
                                        "Contact BG: SendText FAILED contact={} msg_id={} (ratchet not advanced, retry allowed)",
                                        contact_id_bg,
                                        send_msg_id
                                    );
                                    let _ = reply.send(Err(ContactSendError::SmpSendFailed(
                                        "SMP server did not return Ok or transport error"
                                            .into(),
                                    )));
                                }
                            }
                            ContactCommand::KeepAlive => {
                                // Briefing 044 W2.3: forward the PING on
                                // the TLS stream. Failures are logged at
                                // warn and do NOT break the loop - the
                                // next `read_responses` error will feed
                                // into the reconnect path (W3) naturally.
                                // Writes from here are safe w.r.t. the
                                // receive arm because `tokio::select!`
                                // never polls both arms concurrently on
                                // the same poll (W2.3 / Risk 1).
                                //
                                // Briefing 044 W5 nuance: skip when the
                                // connection is mid-reconnect or dead.
                                // The reconnect path already proves
                                // liveness via its grace-window SUB, and
                                // writing into a stream about to be
                                // swapped out would at best be wasted,
                                // at worst corrupt the replacement. No
                                // warn-level log either way; a healthy
                                // session never takes this branch.
                                let state_now = *state_tx.borrow();
                                // Briefing 044a D2.3: per-receipt visibility
                                // so we can see whether KeepAlive commands
                                // actually arrive at the select! arm. If
                                // D2.2 fires but D2.3 does not, the mpsc is
                                // disconnected from this loop (H1).
                                tracing::info!(
                                    "PING: handling KeepAlive for contact={contact_id_bg} (conn_state={state_now:?})"
                                );
                                if state_now
                                    != crate::contact_session::ConnState::Connected
                                {
                                    tracing::trace!(
                                        "Contact BG: skipping keep-alive (state={state_now:?})"
                                    );
                                } else if let Err(e) = smp.send_keepalive().await {
                                    tracing::warn!(
                                        "Contact BG: keep-alive send failed: {e}"
                                    );
                                }
                            }
                        },
                        None => {
                            tracing::info!(
                                "Contact BG: command channel closed (handle dropped), shutting down session"
                            );
                            break 'outer;
                        }
                    }
                }
            } // close tokio::select!
        }
        // Briefing 044 W5: drain pending commands and fail them with
        // `Dead` so the gRPC handlers do not wait on dropped oneshots.
        // The `state_tx` publication + gRPC pre-check catches most
        // cases; this handles the race where a SendText was enqueued
        // between the state flip and the handler observing it.
        while let Ok(cmd) = cmd_rx.try_recv() {
            if let ContactCommand::SendText { reply, .. } = cmd {
                let _ = reply.send(Err(ContactSendError::Dead(
                    "contact session exited".into(),
                )));
            }
            // KeepAlive has no reply channel; dropping it is safe.
        }

        // Briefing 044 W2: abort the PING timer task so it does not linger
        // past the BG loop. Dropping `cmd_rx` at scope exit would make the
        // timer's next `send()` fail on its own (up to 30 s later), but
        // the explicit abort is deterministic and matches the lifecycle
        // semantics the briefing prescribes for contact deletion.
        // Briefing 044a D2.5: visibility on the abort path. If we see an
        // abort log without ever having seen D2.2 ticks, the timer task was
        // killed before its first 30 s window elapsed.
        tracing::info!(
            "PING: timer task aborted for contact={contact_id_bg}"
        );
        ping_handle.abort();

        // Remove our handle from the contact_sessions map so further
        // SendSimplexMessage RPCs against this contact_id surface as
        // ContactNotFound rather than disappearing into a closed channel.
        {
            let mut sessions = contact_sessions_for_cleanup.lock().await;
            sessions.remove(&contact_id_bg);
        }
        tracing::info!("BG loop exit, total messages: {}", msg_counter);
    });

    Ok(())
}

/// Encrypt an already-encoded AgentMessage plaintext with the Double Ratchet,
/// wrap it in AgentMsgEnvelope + Layer 2 NaCl, and SEND it to the peer's
/// reply queue. On success, stores the SHA-256 of the plaintext in the
/// ratchet's `last_snd_msg_hash` for the next message's APrivHeader.prevMsgHash.
///
/// Returns `true` if the SMP server responded with Ok.
///
/// `sender_auth_private` is the 32-byte X25519 private key whose public
/// half was registered on the peer's reply queue via the peer's SKEY
/// during handshake. The crypto_box MAC on the SEND transmission is
/// computed with this key (nacl.box of SHA-512(sessionId||trans_body)
/// against the server's session DH pub, nonce=corrId). Signing with the
/// connection's `queue_auth_private` produces ERR AUTH because that key
/// only authorises commands on OUR queue.
///
/// Reference: goChatX `smp-web/src/protocol.ts::encodeTransmission` v9
/// branch + `client.ts::sendMessageSigned`.
///
/// The `corr_id` parameter is retained for log correlation with the
/// outer helper; the actual wire corrId is re-generated inside
/// `build_signed_transmission_with_sender_auth` because the crypto_box
/// nonce must be the corrId bytes themselves.
#[allow(clippy::too_many_arguments)] // each parameter is a distinct piece of
// per-send state with no obvious natural grouping; bundling into a struct
// would be premature abstraction at one call site.
async fn send_agent_message_encrypted(
    smp: &mut crate::smp_protocol::SmpConnection,
    ratchet: &mut crate::crypto::bob_ratchet::BobRatchet,
    sender_auth_private: &[u8; 32],
    own_l2_ephemeral_private: &[u8; 32],
    peer_queue: &crate::protocol::smp_queue_info::SmpQueueInfo,
    agent_msg_plaintext: &[u8],
    is_first_to_peer_queue: bool,
    corr_id: u8,
    log_prefix: &str,
) -> bool {
    use sha2::{Digest, Sha256};

    // ---- Briefing 041b-crypto Phase C3: full ratchet state on entry ----
    // Everything the peer's agent layer would need to re-derive to verify
    // our emHeader AAD and body AEAD tag. Logged ONCE per send so we can
    // spot ratchet drift (Ns out of sync, root_key mismatch with what
    // the peer derived from the same X3DH, etc.).
    {
        let sending_ck_hex = ratchet
            .sending_chain_key
            .map(|c| format!("{:02x?}", &c[..4]))
            .unwrap_or_else(|| "None".into());
        let receiving_ck_hex = ratchet
            .receiving_chain_key
            .map(|c| format!("{:02x?}", &c[..4]))
            .unwrap_or_else(|| "None".into());
        let last_hash_hex: String = ratchet
            .last_snd_msg_hash
            .iter()
            .take(8)
            .map(|b| format!("{:02x}", b))
            .collect();
        let our_new_pub_hex = ratchet
            .our_new_ratchet_pub
            .map(|p| format!("{:02x?}", &p[..4]))
            .unwrap_or_else(|| "None".into());
        tracing::error!(
            target: "sgx_simplex::ratchet_debug",
            "C3 outbound ratchet state (pre-encrypt): ns={}, nr={}, pn={}, next_snd_msg_id={}, root_key[..4]={:02x?}, sending_ck[..4]={}, receiving_ck[..4]={}, sending_header_key[..4]={:02x?}, next_sending_header_key[..4]={:02x?}, next_header_key_receive[..4]={:02x?}, our_new_ratchet_pub[..4]={}, last_snd_msg_hash[..8]={}, assoc_data_len={}",
            ratchet.ns,
            ratchet.nr,
            ratchet.pn,
            ratchet.next_snd_msg_id,
            &ratchet.root_key[..4],
            sending_ck_hex,
            receiving_ck_hex,
            &ratchet.sending_header_key[..4],
            &ratchet.next_sending_header_key[..4],
            &ratchet.next_header_key_receive[..4],
            our_new_pub_hex,
            last_hash_hex,
            ratchet.assoc_data.len()
        );
    }

    // ---- Briefing 041b-crypto Phase C3: full AgentMessage plaintext hex ----
    {
        let full_len = agent_msg_plaintext.len().min(256);
        let full_hex: String = agent_msg_plaintext[..full_len]
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect();
        tracing::error!(
            target: "sgx_simplex::ratchet_debug",
            "C3 full AgentMessage plaintext (first {} B of {}): {}",
            full_len,
            agent_msg_plaintext.len(),
            full_hex
        );
    }

    // ---- Briefing 041b-debug Phase D4: AgentMessage plaintext first bytes ----
    // Expected for first-ever send with empty prev_hash:
    //   4d 00 00 00 00 00 00 00 01 00 4d <body...>
    // where 4d = 'M' outer tag, 8-byte BE sndMsgId=1, 0 = hashLen,
    // 4d = 'M' inner A_MSG tag, followed by raw UTF-8 body.
    {
        let preview_len = agent_msg_plaintext.len().min(48);
        let preview_hex: String = agent_msg_plaintext[..preview_len]
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect();
        let decoded_snd_msg_id = if agent_msg_plaintext.len() >= 9 {
            u64::from_be_bytes(agent_msg_plaintext[1..9].try_into().unwrap_or([0u8; 8]))
        } else {
            0
        };
        let decoded_hash_len = agent_msg_plaintext.get(9).copied().unwrap_or(0);
        tracing::error!(
            target: "sgx_simplex::send_debug",
            "D4 AgentMessage plaintext: total_len={}, first {} B = {}, decoded_snd_msg_id={}, decoded_hash_len={}",
            agent_msg_plaintext.len(),
            preview_len,
            preview_hex,
            decoded_snd_msg_id,
            decoded_hash_len
        );
    }

    let our_new_pub_raw = match ratchet.our_new_ratchet_pub {
        Some(p) => p,
        None => {
            tracing::error!("{log_prefix}: send_agent_message: our_new_ratchet_pub missing");
            return false;
        }
    };
    let sending_ck = match ratchet.sending_chain_key {
        Some(c) => c,
        None => {
            tracing::error!("{log_prefix}: send_agent_message: sending_chain_key missing");
            return false;
        }
    };

    let (new_ck, message_key, body_iv, _header_iv_unused) =
        match crate::crypto::bob_ratchet::chain_kdf(&sending_ck) {
            Ok(x) => x,
            Err(e) => {
                tracing::error!("{log_prefix}: chainKdf failed: {e}");
                return false;
            }
        };

    let mut our_new_pub_spki = [0u8; 68];
    our_new_pub_spki[..12].copy_from_slice(&crate::crypto::x3dh::X448_SPKI_HEADER);
    our_new_pub_spki[12..].copy_from_slice(&our_new_pub_raw);

    let msg_header_plain = crate::crypto::bob_ratchet::encode_msg_header(
        3, // maxVersion
        &our_new_pub_spki,
        ratchet.pn,
        ratchet.ns,
    );

    // ---- Briefing 041b-crypto Phase C3: header-encryption inputs ----
    // Logs the plain MsgHeader bytes and the keys/AAD that feed the
    // AES-256-GCM-16 header encryption. Post 041b-crypto-fix CF1 the
    // chat/HELLO path pads to PADDED_HEADER_LEN_V3_CHAT = 88 to match
    // GoChat HEADER_PAD_V3. The log line reads the actual constant
    // passed into encrypt_message_header (see below) so it cannot drift
    // out of sync with runtime behaviour.
    let chat_padded_header_len = crate::crypto::bob_ratchet::PADDED_HEADER_LEN_V3_CHAT;
    {
        let msg_header_plain_hex: String = msg_header_plain
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect();
        tracing::error!(
            target: "sgx_simplex::ratchet_debug",
            "C3 header encryption inputs: msg_header_plain_len={} (GoChat expects 80 content), padded_header_len={} (GoChat HEADER_PAD_V3=88 for chat/HELLO), header_key[..4]={:02x?} (sending_header_key / hks analog), aad_len={} (X3DH assoc_data), eh_version=3",
            msg_header_plain.len(),
            chat_padded_header_len,
            &ratchet.sending_header_key[..4],
            ratchet.assoc_data.len()
        );
        tracing::error!(
            target: "sgx_simplex::ratchet_debug",
            "C3 msg_header_plain bytes ({} B): {}",
            msg_header_plain.len(),
            msg_header_plain_hex
        );
    }

    let enc_message_header = match crate::crypto::bob_ratchet::encrypt_message_header(
        &msg_header_plain,
        // 041b-ratchet-fix RF4: use the CURRENT send-side header key
        // (hks analog). For HELLO send (fires after MSG #2 peer HELLO
        // receive = second AdvanceRatchet on our side), this is the
        // nhks derived from MSG #1's second rootKdf, promoted via
        // `dh_ratchet_and_decrypt_message`. For subsequent chat sends
        // after further AdvanceRatchets, it's the next hops in the
        // chain. Previously this read `next_header_key_send` which
        // never rotated past the X3DH init value, causing A_CRYPTO on
        // peer's side after MSG #2.
        &ratchet.sending_header_key,
        &ratchet.assoc_data,
        3,
        // Briefing 041b-crypto-fix CF1: chat sends pad the plain MsgHeader
        // to 88 B (GoChat HEADER_PAD_V3) to produce a 124-B wire
        // emHeader that the peer's v3 no-PQ ratchet accepts. The old
        // 2310-B value (simplexmq PQSupportOn) produced `A_CRYPTO`.
        crate::crypto::bob_ratchet::PADDED_HEADER_LEN_V3_CHAT,
    ) {
        Ok(h) => h,
        Err(e) => {
            tracing::error!("{log_prefix}: encrypt_message_header failed: {e}");
            return false;
        }
    };

    // Expose IV (bytes 2..18 of emHeader) so it can be cross-checked
    // against the peer log if Desktop dumps its received IV.
    {
        let iv_hex: String = enc_message_header
            .get(2..18)
            .unwrap_or(&[])
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect();
        let tag_hex: String = enc_message_header
            .get(18..34)
            .unwrap_or(&[])
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect();
        let body_len_field = if enc_message_header.len() >= 36 {
            u16::from_be_bytes([enc_message_header[34], enc_message_header[35]])
        } else {
            0
        };
        tracing::error!(
            target: "sgx_simplex::ratchet_debug",
            "C3 enc_message_header: total_len={} (GoChat expects 124 = 2+16+16+2+88), iv(16B)={}, auth_tag(16B)={}, ehBody_len_field={} (GoChat expects 88)",
            enc_message_header.len(),
            iv_hex,
            tag_hex,
            body_len_field
        );
    }

    // Briefing 041b-crypto-fix CF2: chat-send body pad target matches
    // GoChat `connection.ts:1002-1005` derivation for isFirstMessage=false:
    //   bodyPadSize = naclPadTarget(15840) - 2 - smpConfOH(1) - envOH(3) - encRatchetOH(142)
    //               = 15692
    // The old 13500 assumed our 2346-B emHeader; after CF1 reduces
    // emHeader to 124 B the budget opens up and the correct pad is 15692.
    let padded_msg_len: usize = 15692;

    // ---- Briefing 041b-crypto Phase C3: body-pad target vs GoChat ----
    // Briefing 041b-hello-fix HF3: log the ACTUAL NaCl pad target that
    // e2e_encrypt_agent_msg will pick. The function branches on the
    // is_first_message flag (= our `is_first_to_peer_queue` param). Chat
    // sends and HELLO pass `false` -> 15840; only the Stage 16 reply
    // path passes `true` -> 15904 but that path does not reach this
    // helper.
    let expected_nacl_pad = if is_first_to_peer_queue {
        crate::e2e_crypto::E2E_PADDED_LENGTH
    } else {
        crate::e2e_crypto::E2E_PADDED_LENGTH_SUBSEQUENT
    };
    tracing::error!(
        target: "sgx_simplex::ratchet_debug",
        "C3 body pad: our padded_msg_len={} (GoChat subsequent-chat=15692), NaCl pad target={} (is_first_to_peer_queue={}; GoChat first=15904, subsequent=15840)",
        padded_msg_len,
        expected_nacl_pad,
        is_first_to_peer_queue
    );
    let enc_ratchet_msg = match crate::crypto::bob_ratchet::encrypt_and_assemble_ratchet_message(
        agent_msg_plaintext,
        &enc_message_header,
        &message_key,
        &body_iv,
        &ratchet.assoc_data,
        padded_msg_len,
    ) {
        Ok(m) => m,
        Err(e) => {
            tracing::error!("{log_prefix}: assemble ratchet message failed: {e}");
            return false;
        }
    };

    let agent_msg_envelope = crate::e2e_crypto::build_agent_msg_envelope(&enc_ratchet_msg);

    // ---- Briefing 041b-debug Phase D3: outbound AgentMsgEnvelope header ----
    // build_agent_msg_envelope hardcodes agentVersion=1 and tag 'M' (0x4D).
    // The first 3 bytes of agent_msg_envelope should be: 00 01 4d
    // (Word16 BE agentVersion=1, then 'M'). Any other prefix means a wrong
    // envelope path was taken (e.g. agentVersion=7 from AgentConfirmation).
    {
        let hdr_len = agent_msg_envelope.len().min(4);
        let hdr_hex: String = agent_msg_envelope[..hdr_len]
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect();
        let decoded_agent_version = if agent_msg_envelope.len() >= 2 {
            u16::from_be_bytes([agent_msg_envelope[0], agent_msg_envelope[1]])
        } else {
            0
        };
        let decoded_tag = agent_msg_envelope.get(2).copied().unwrap_or(0);
        tracing::error!(
            target: "sgx_simplex::send_debug",
            "D3 outbound AgentMsgEnvelope: first {} B = {}, agent_version={}, tag=0x{:02x} ('{}')",
            hdr_len,
            hdr_hex,
            decoded_agent_version,
            decoded_tag,
            if decoded_tag.is_ascii_graphic() { decoded_tag as char } else { '.' }
        );
    }

    // Briefing 041b-crypto-fix CF3: REUSE the X25519 L2 ephemeral we
    // persisted during Stage 16, don't generate a fresh one. Peer has
    // our Stage-16 public stored and will DH against it when decoding
    // our PubHeader Nothing variant. A fresh keypair per send would
    // produce a different shared secret and fail peer decrypt.
    let own_l2_priv = x25519_dalek::StaticSecret::from(*own_l2_ephemeral_private);
    let own_l2_pub = x25519_dalek::PublicKey::from(&own_l2_priv);
    let own_l2_pub_bytes = *own_l2_pub.as_bytes();

    // Briefing 041b-crypto-fix CF3: the caller hands through
    // `is_first_to_peer_queue`. Chat sends pass `false` so this helper
    // emits PubHeader Nothing ('0'); peer uses its stored key derived
    // from our Stage-16 AgentConfirmation reply. Also drops the L2
    // NaCl pad from 15904 down to 15840 per GoChat reference (the
    // branching is done inside `e2e_encrypt_agent_msg`).
    let layer2_envelope = crate::e2e_crypto::e2e_encrypt_agent_msg(
        &agent_msg_envelope,
        &peer_queue.sender_dh_public,
        own_l2_ephemeral_private,
        &own_l2_pub_bytes,
        is_first_to_peer_queue,
        b"_",
    );

    // Briefing 041b-fix: sign SEND with the per-contact X25519
    // sender_auth_private (crypto_box MAC), NOT with the session-bound
    // queue_auth_private. The peer's reply queue was secured via SKEY
    // with our sender_auth_pub during handshake; the server accepts
    // SENDs authenticated against that specific key only. Signing with
    // queue_auth_private produced ERR AUTH (confirmed in the 041b-debug
    // diagnostic run). corr_id is retained for log correlation; the
    // new helper regenerates the 24-byte corrId internally.
    let _ = corr_id; // retained for log correlation only; unused on wire
    let send_tx = crate::smp_commands::cmd_send_with_sender_auth(
        smp,
        sender_auth_private,
        &peer_queue.queue_id,
        &layer2_envelope,
        false,
    );

    // ---- Briefing 041b-debug Phase D2: byte length summary per layer ----
    // GoChat reference numbers are 15692 for Layer 2 plaintext after PADDED
    // NaCl framing; our padded_msg_len=13500 plus overhead should land near
    // the same zone. A 4-byte mismatch vs. peer's expected size silently
    // desyncs decoding (GoChat PR #82).
    tracing::error!(
        target: "sgx_simplex::send_debug",
        "D2 outbound layers: agent_msg_plaintext_len={}, enc_message_header_len={}, padded_msg_len={}, enc_ratchet_msg_len={}, agent_msg_envelope_len={}, layer2_envelope_len={}, send_tx_len={}",
        agent_msg_plaintext.len(),
        enc_message_header.len(),
        padded_msg_len,
        enc_ratchet_msg.len(),
        agent_msg_envelope.len(),
        layer2_envelope.len(),
        send_tx.len()
    );

    if let Err(e) = smp.write_command_block(&send_tx).await {
        tracing::error!("{log_prefix}: SEND write failed: {e}");
        return false;
    }

    let got_ok = match smp.read_responses().await {
        Ok(responses) => {
            let ok = responses
                .iter()
                .any(|r| matches!(r, crate::smp_protocol::ServerResponse::Ok));
            // ---- Briefing 041b-debug Phase D1: full parsed SMP responses ----
            // Print each ServerResponse variant in full (no 80-char truncation)
            // so ERR kinds (AUTH / CMD / BLOCK / QUOTA) and their payloads are
            // fully visible. Luna needs the actual error variant to diagnose.
            for (i, r) in responses.iter().enumerate() {
                tracing::error!(
                    target: "sgx_simplex::send_debug",
                    "D1 SMP response[{}] parsed: {:?}",
                    i,
                    r
                );
            }
            tracing::info!(
                "{log_prefix}: SEND response: {:?}",
                responses
                    .iter()
                    .map(|x| format!("{x:?}").chars().take(80).collect::<String>())
                    .collect::<Vec<_>>()
            );
            ok
        }
        Err(e) => {
            tracing::error!(
                target: "sgx_simplex::send_debug",
                "D1 SMP read_responses transport error: {e}"
            );
            tracing::warn!("{log_prefix}: SEND response error: {e}");
            false
        }
    };

    if got_ok {
        // Commit ratchet state advance: chainKdf rotation, Ns++,
        // sndMsgId++, hash chain step (SHA-256 of the encoded AgentMessage
        // plaintext per Agent.hs:2013-2019 internalHash).
        ratchet.sending_chain_key = Some(new_ck);
        ratchet.ns += 1;
        ratchet.next_snd_msg_id += 1;
        let hash = Sha256::digest(agent_msg_plaintext);
        ratchet.last_snd_msg_hash = hash.to_vec();
        tracing::info!(
            "{log_prefix}: ratchet advanced (ns={}, next_snd_msg_id={}, last_snd_hash[..4]={})",
            ratchet.ns,
            ratchet.next_snd_msg_id,
            hex::encode(&ratchet.last_snd_msg_hash[..4])
        );
    }

    got_ok
}

/// Send our post-handshake HELLO to the peer's reply queue.
///
/// Briefing 041b-hello Phase HE4/HE5. Peer transitions the contact to
/// `CONNECTED` state on receipt; without this, peer rejects our chat
/// messages with `error chat contactNotReady`.
///
/// Wire path mirrors [`send_agent_message_encrypted`] (ratchet encrypt +
/// L2 NaCl + SEND). The key semantic difference from a chat send comes
/// from GoChat's `connection.ts::sendHello` behaviour (verified via grep
/// of the single `sndMsgId++` at `connection.ts:1073` which lives only
/// in `sendChatMessage`):
///
/// - **HELLO DOES advance the Double-Ratchet chain** (`ns++`,
///   `sending_chain_key` rotated via chainKdf) because it shares the
///   same `rcEncrypt` path.
/// - **HELLO does NOT advance the agent counters** (`next_snd_msg_id`,
///   `last_snd_msg_hash`). Both HELLO and the first chat that follows
///   read `sndMsgId = 1, prevMsgHash = []` on GoChat's wire.
///
/// We implement the counter invariance via snapshot-and-restore around
/// the send helper call. On success the restore only touches
/// `next_snd_msg_id` and `last_snd_msg_hash`, leaving the chain advance
/// in place.
///
/// Returns the `sndMsgId` that went on the wire (always 1 today, since
/// HELLO fires exactly once per contact direction) or a descriptive
/// error string.
async fn send_our_hello(
    smp: &mut crate::smp_protocol::SmpConnection,
    ratchet: &mut crate::crypto::bob_ratchet::BobRatchet,
    store: &std::sync::Arc<crate::queue_store::QueueStore>,
    contact_id: &str,
    peer_queue: &crate::protocol::smp_queue_info::SmpQueueInfo,
) -> Result<u64, String> {
    // Same keypairs chat sends use, loaded fresh to keep the helper
    // self-contained (one SQLite round-trip is negligible vs. the
    // SMP+TLS send cost).
    let sender_auth_private = store
        .load_sender_auth_private(contact_id)
        .map_err(|e| format!("load_sender_auth_private: {e}"))?
        .ok_or_else(|| {
            "sender_auth_private not persisted (handshake incomplete?)".to_string()
        })?;
    let own_l2_ephemeral_private = store
        .load_own_l2_ephemeral_private(contact_id)
        .map_err(|e| format!("load_own_l2_ephemeral_private: {e}"))?
        .ok_or_else(|| {
            "own_l2_ephemeral_private not persisted (pre-041b-crypto-fix contact?)"
                .to_string()
        })?;

    // Briefing 041b-duplicate-fix: read the current agent counters to
    // build the HELLO APrivHeader, but do NOT restore them after the
    // send. GoChat's sendHello-followed-by-sendChatMessage flow visibly
    // produces A_DUPLICATE on SimpleX Desktop when both messages carry
    // sndMsgId=1, so whatever internal mechanism GoChat uses to keep its
    // counters from colliding, the observable peer contract is: HELLO
    // and the following first chat message need DIFFERENT sndMsgIds.
    //
    // By letting `send_agent_message_encrypted`'s natural advance stand
    // (`next_snd_msg_id += 1`, `last_snd_msg_hash = SHA256(plain)`), the
    // sequence becomes HELLO=sndMsgId=1, first chat=sndMsgId=2, second
    // chat=sndMsgId=3, ... which is what Desktop's agent layer accepts.
    let hello_snd_msg_id = ratchet.next_snd_msg_id;
    let hello_prev_hash = ratchet.last_snd_msg_hash.clone();

    let hello_plain = crate::protocol::agent_msg::encode_agent_message_hello(
        hello_snd_msg_id,
        &hello_prev_hash,
    );

    let ok = send_agent_message_encrypted(
        smp,
        ratchet,
        &sender_auth_private,
        &own_l2_ephemeral_private,
        peer_queue,
        &hello_plain,
        // PubHeader Nothing + pad 15840: same as chat sends. Peer
        // decoded our Stage-16 ephemeral pub and stored it.
        false,
        b'H',
        "Contact BG HELLO",
    )
    .await;

    if !ok {
        return Err("send_agent_message_encrypted returned false".to_string());
    }

    // Briefing 041b-duplicate-fix: NO restore of next_snd_msg_id /
    // last_snd_msg_hash. send_agent_message_encrypted already advanced
    // them on SMP Ok, and we intentionally keep that advance so the
    // next chat send uses sndMsgId = hello_snd_msg_id + 1 and peer
    // accepts it as a distinct message.

    Ok(hello_snd_msg_id)
}

/// Generate a simple UUID v4 string.
fn uuid_v4() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let r: u64 = (t & 0xFFFFFFFFFFFFFFFF) as u64;
    format!("{:016x}-{:04x}", r, (r >> 48) & 0xFFFF)
}

/// Rebuild the TCP + TLS + SMP-session stack for a contact whose background
/// loop has observed a recoverable IO or TLS error (Briefing 044 W3).
///
/// Parameters:
/// - `smp`:             connection to swap in-place on success
/// - `addr`:            server host/port/fingerprint (cloned per attempt
///                      because `SmpClient::new` takes by value)
/// - `server_key_hash`: 32-byte SHA-256 of the server CA cert (passed by
///                      value; the type is Copy)
/// - `rcv_auth`:        Ed25519 recipient signing key (used to resubscribe)
/// - `rcv_id`:          24-byte recipient queue id on the server
///
/// Backoff matches GoChat reference exactly:
///
///   base_delay = 500ms * 2^(attempt - 1), capped at 30s, 50% jitter.
///   max_attempts = 12.
///
/// Total worst-case wall-clock before giving up: ~500 ms + 1 s + 2 s + 4 s
/// + 8 s + 16 s + 30 s * 6  = ~3.5 minutes plus jitter and per-attempt
/// connect/handshake time.
///
/// Each attempt goes through handshake AND a grace-window SUB to prove the
/// connection is not just TCP-alive but protocol-alive (Briefing 044 W3.3
/// nuance). If the server hangs up or refuses the SUB inside
/// `MIN_UPTIME_SECS`, we retry - a GoChat-style handshake-rejection guard.
///
/// On success, `*smp` is replaced with the fresh connection, which carries
/// a fresh `session_id` derived from the new ServerHello. Session-id
/// re-derivation is implicit: the struct swap brings it along.
///
/// On exhaustion, returns a synthetic `SmpError::Io(NotConnected, ...)`
/// so the caller can distinguish give-up from a transient read error.
async fn reconnect_with_backoff(
    smp: &mut crate::smp_protocol::SmpConnection,
    addr: &crate::smp_client::SmpServerAddr,
    server_key_hash: [u8; 32],
    rcv_auth: &ed25519_dalek::SigningKey,
    rcv_id: &[u8; 24],
    // Briefing 044 W6: lifecycle events for the frontend.
    update_tx: &tokio::sync::broadcast::Sender<SimplexUpdate>,
    contact_id: &str,
    // Briefing 044c: persistent queue_auth_private lookup. The fresh
    // SmpConnection from smp_handshake carries a random queue_auth pair
    // that does NOT match the rcvAuthKey registered on the server during
    // the original NEW. Load the persisted key from the queue store and
    // patch it into the fresh connection before the re-SUB signs.
    store: &crate::queue_store::QueueStore,
) -> Result<(), crate::smp_protocol::SmpError> {
    use crate::smp_client::SmpClient;
    use crate::smp_commands::cmd_sub;
    use crate::smp_protocol::{ServerResponse, SmpConnection, SmpError};

    // Briefing 044a D2.8: confirms that control actually entered the
    // reconnect helper. If the read-arm logged is_recoverable=true but this
    // line does not appear, the call was skipped or panicked at the boundary.
    tracing::info!(
        "RECONNECT: entering reconnect_with_backoff for contact={contact_id}"
    );

    const MAX_ATTEMPTS: u32 = 12;
    const BASE_DELAY_MS: u64 = 500;
    const MAX_DELAY_MS: u64 = 30_000;
    const MIN_UPTIME_SECS: u64 = 5;

    for attempt in 1..=MAX_ATTEMPTS {
        // Exponential backoff with 50% jitter. `saturating_*` guards
        // against the tiny possibility of overflow on 2^N for large N
        // (N goes up to 11 here, so 2^11 * 500 = 1 048 576 which is well
        // under u64::MAX, but keeping the saturating arithmetic costs
        // nothing and documents intent).
        let base = BASE_DELAY_MS.saturating_mul(2u64.saturating_pow(attempt - 1));
        let capped = base.min(MAX_DELAY_MS);
        let jitter_range = (capped / 2).max(1);
        let jitter = rand::random::<u64>() % jitter_range;
        let delay_ms = capped
            .saturating_sub(jitter_range / 2)
            .saturating_add(jitter);

        tracing::info!(
            "reconnect attempt {}/{} in {}ms",
            attempt,
            MAX_ATTEMPTS,
            delay_ms
        );
        tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;

        // Briefing 044 W6: publish per-attempt progress so the frontend
        // can show "attempt N/max" on the contact's reconnecting badge.
        let _ = update_tx.send(SimplexUpdate {
            update: Some(simplex_update::Update::ContactReconnecting(
                SimplexContactReconnecting {
                    contact_id: contact_id.to_string(),
                    attempt,
                    max_attempts: MAX_ATTEMPTS,
                    timestamp: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis() as i64)
                        .unwrap_or(0),
                },
            )),
        });

        // --- Fresh TLS handshake (SO_KEEPALIVE re-applied by SmpClient::connect) ---
        let client = SmpClient::new(addr.clone(), None);
        let tls_stream = match client.connect().await {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("reconnect attempt {} TLS connect failed: {e}", attempt);
                continue;
            }
        };

        // --- Fresh SMP handshake - this is where the new session_id is born ---
        let mut new_conn = match SmpConnection::smp_handshake(tls_stream, server_key_hash).await {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("reconnect attempt {} SMP handshake failed: {e}", attempt);
                continue;
            }
        };

        // Briefing 044c: overwrite the freshly-generated queue_auth pair
        // with the one originally registered on the server via NEW. The
        // public half is derived from the private by X25519 basepoint
        // mult (single source of truth) to guarantee consistency. If the
        // persisted key is missing we cannot sign a valid recipient
        // command for this queue on a fresh session, so the attempt is
        // aborted and the backoff retries - but the root cause is
        // permanent, so every subsequent attempt will fail for the same
        // reason until the user re-establishes the contact.
        let persisted_priv = match store.load_queue_auth_private(contact_id) {
            Ok(Some(k)) => k,
            Ok(None) => {
                tracing::error!(
                    "reconnect attempt {}: no persisted queue_auth_private for contact={}, re-SUB impossible",
                    attempt,
                    contact_id
                );
                continue;
            }
            Err(e) => {
                tracing::warn!(
                    "reconnect attempt {}: load_queue_auth_private failed: {e}",
                    attempt
                );
                continue;
            }
        };
        {
            use x25519_dalek::{PublicKey as X25519Public, StaticSecret};
            let priv_key = StaticSecret::from(persisted_priv);
            let pub_key = X25519Public::from(&priv_key);
            new_conn.queue_auth_private = persisted_priv;
            new_conn.queue_auth_public = *pub_key.as_bytes();
            tracing::info!(
                "reconnect attempt {}: restored queue_auth pair (pub[..4]={}) onto fresh SmpConnection",
                attempt,
                hex::encode(&new_conn.queue_auth_public[..4])
            );
        }

        // --- Grace-window re-SUB (protocol-liveness proof) ---
        // Briefing 044 W3.3 nuance: instead of a passive `sleep(5s) +
        // is_alive()` check, actively re-subscribe to our recipient queue
        // and watch for OK. This tests that the server is not just
        // accepting the TCP connection but willing to honour our SMP
        // session and signing key. A handshake-rejection pattern - where
        // the server drops us within MIN_UPTIME_SECS - is handled as a
        // retry, not a success.
        let sub_tx = cmd_sub(&new_conn, rcv_auth, rcv_id);
        if let Err(e) = new_conn.write_command_block(&sub_tx).await {
            tracing::warn!(
                "reconnect attempt {} SUB write failed (connection died during grace): {e}",
                attempt
            );
            continue;
        }

        let grace = tokio::time::timeout(
            std::time::Duration::from_secs(MIN_UPTIME_SECS),
            new_conn.read_responses(),
        )
        .await;
        match grace {
            Ok(Ok(responses)) => {
                let got_ok = responses.iter().any(|r| matches!(r, ServerResponse::Ok));
                if got_ok {
                    tracing::info!(
                        "reconnect attempt {} succeeded (fresh session_id={}..., SUB OK)",
                        attempt,
                        hex::encode(&new_conn.session_id[..4])
                    );
                    // Swap in the fresh connection atomically. All future
                    // calls through `smp` (signing, reads, writes) will
                    // use the new session_id, the new session_shared_secret,
                    // and the fresh underlying TLS stream.
                    *smp = new_conn;
                    return Ok(());
                }
                tracing::warn!(
                    "reconnect attempt {} got grace-window response but no OK: {:?}",
                    attempt,
                    responses
                        .iter()
                        .map(|r| format!("{r:?}").chars().take(80).collect::<String>())
                        .collect::<Vec<_>>()
                );
                continue;
            }
            Ok(Err(e)) => {
                tracing::warn!(
                    "reconnect attempt {} died within {}s grace: {e}",
                    attempt,
                    MIN_UPTIME_SECS
                );
                continue;
            }
            Err(_timeout) => {
                tracing::warn!(
                    "reconnect attempt {} got no SUB response within {}s grace",
                    attempt,
                    MIN_UPTIME_SECS
                );
                continue;
            }
        }
    }

    Err(SmpError::Io(std::io::Error::new(
        std::io::ErrorKind::NotConnected,
        format!("reconnect exhausted {MAX_ATTEMPTS} attempts"),
    )))
}
