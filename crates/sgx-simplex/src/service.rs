//! MessengerService gRPC implementation for SimpleX.
//!
//! Phase 1: Auth flow (profile setup + invitation parsing), ListChats, stubs for rest.
//! Phase 2 will add actual SMP message exchange.

use crate::invitation;
use crate::queue_store::QueueStore;
use sgx_proto::messenger::v1::*;
use sgx_proto::messenger::v1::messenger_service_server::MessengerService;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_stream::Stream;
use tonic::{Request, Response, Status};
use tracing::info;

/// gRPC service for SimpleX protocol.
pub struct SimplexService {
    store: Arc<QueueStore>,
    display_name: Mutex<Option<String>>,
}

impl SimplexService {
    pub fn new(store: QueueStore) -> Self {
        // Load persisted display name
        let name = store.get_profile_name().ok().flatten();
        if let Some(ref n) = name {
            info!("SimpleX: loaded profile '{n}'");
        }
        Self {
            store: Arc::new(store),
            display_name: Mutex::new(name),
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

        // Resolve short links (https://<server>/a#<key>) to full invitation links
        let link = if code.contains("/a#") {
            info!("SimpleX: resolving short link...");
            invitation::resolve_short_link(&code)
                .await
                .map_err(|e| Status::internal(format!("Short link resolution failed: {e}")))?
        } else {
            code
        };

        info!("SimpleX: parsing invitation link: {link}");
        let parsed = invitation::parse_invitation_link(&link)
            .map_err(|e| Status::invalid_argument(format!("Invalid invitation link: {e}")))?;

        let contact_id = uuid_v4();
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
            .map_err(|e| Status::internal(format!("Store error: {e}")))?;

        info!(
            "SimpleX: contact saved (id={contact_id}, server={}:{})",
            parsed.server_host, parsed.server_port
        );

        // Start handshake in background (don't block the gRPC response)
        let store = self.store.clone();
        let profile_name = self.display_name.lock().await.clone().unwrap_or_default();
        tokio::spawn(async move {
            info!(
                "SimpleX: handshake task started for contact={contact_id} server={}:{}",
                parsed.server_host, parsed.server_port
            );
            match execute_handshake(&parsed, &profile_name, &store, &contact_id).await {
                Ok(_) => info!("SimpleX: handshake COMPLETED for {contact_id}"),
                Err(e) => tracing::error!("SimpleX: handshake FAILED for {contact_id}: {e:?}"),
            }
        });

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
        Ok(Response::new(GetMessagesResponse {
            messages: vec![],
        }))
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
}

/// Execute the SimpleX connection handshake in the background.
///
/// 7 steps: TLS+SMP -> NEW -> X3DH -> SKEY -> CONF -> KEY -> HELLO
async fn execute_handshake(
    invitation: &invitation::ParsedInvitation,
    profile_name: &str,
    _store: &QueueStore,
    contact_id: &str,
) -> Result<(), anyhow::Error> {
    use base64::Engine;
    use crate::crypto::keys::*;
    use crate::e2e_crypto::*;
    use crate::protocol::agent_msg::*;
    use crate::smp_client::{SmpClient, SmpServerAddr};
    use crate::smp_commands::*;
    use crate::smp_protocol::*;

    // ---- Step 1: TLS + SMP handshake ----
    tracing::info!("Step 1: TLS + SMP handshake to {}:{}", invitation.server_host, invitation.server_port);

    let addr = SmpServerAddr {
        host: invitation.server_host.clone(),
        port: invitation.server_port,
        fingerprint: invitation.server_fingerprint.clone(),
    };
    let client = SmpClient::new(addr, None);
    let tls_stream = client.connect().await
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

    let mut smp = SmpConnection::smp_handshake(tls_stream, server_key_hash).await
        .map_err(|e| anyhow::anyhow!("SMP handshake: {e}"))?;

    tracing::info!("Step 1: SMP handshake OK, session_id={}...", hex::encode(&smp.session_id[..4]));

    // ---- Step 2: Create receive queue ----
    tracing::info!("Step 2: Creating receive queue");

    let rcv_auth = generate_ed25519();
    let (rcv_dh_priv, rcv_dh_pub) = generate_x25519();

    let new_tx = cmd_new(&smp, &rcv_auth, rcv_dh_pub.as_bytes());

    tracing::debug!("NEW transmission: {} bytes", new_tx.len());
    if new_tx.len() > 100 {
        tracing::debug!("  [0] sig_len: {}", new_tx[0]);
        tracing::debug!("  [1..5] sig start: {:02x}{:02x}{:02x}{:02x}",
            new_tx[1], new_tx[2], new_tx[3], new_tx[4]);
        tracing::debug!("  [65] sess_len: {}", new_tx[65]);
        tracing::debug!("  [66..70] sess start: {:02x}{:02x}{:02x}{:02x}",
            new_tx[66], new_tx[67], new_tx[68], new_tx[69]);
        tracing::debug!("  [98] corr_id_len: {}", new_tx[98]);
        if new_tx[98] > 0 {
            tracing::debug!("  [99] corr_id: 0x{:02x} '{}'", new_tx[99], new_tx[99] as char);
        }
        let eid_pos = 99 + new_tx[98] as usize;
        if eid_pos < new_tx.len() {
            tracing::debug!("  [{}] entity_id_len: {}", eid_pos, new_tx[eid_pos]);
        }
        let cmd_start = eid_pos + 1 + new_tx[eid_pos] as usize;
        if cmd_start + 4 < new_tx.len() {
            tracing::debug!("  [{}..] cmd start: {:02x}{:02x}{:02x}{:02x} '{}'",
                cmd_start,
                new_tx[cmd_start], new_tx[cmd_start+1], new_tx[cmd_start+2], new_tx[cmd_start+3],
                String::from_utf8_lossy(&new_tx[cmd_start..cmd_start+4]));
        }
    }
    tracing::debug!("NEW full hex (first 120): {}", hex::encode(&new_tx[..new_tx.len().min(120)]));

    smp.write_command_block(&new_tx).await
        .map_err(|e| anyhow::anyhow!("NEW send: {e}"))?;

    let responses = smp.read_responses().await
        .map_err(|e| anyhow::anyhow!("NEW response: {e}"))?;

    tracing::info!("Step 2: NEW response count={}", responses.len());
    for (i, r) in responses.iter().enumerate() {
        tracing::info!("Step 2: response[{i}]: {:?}", format!("{r:?}").chars().take(100).collect::<String>());
    }

    // Extract IDS
    let mut rcv_id = [0u8; 24];
    let mut snd_id = [0u8; 24];
    let mut _srv_dh = [0u8; 32];
    let mut got_ids = false;

    for resp in &responses {
        if let ServerResponse::Ids { rcv_id: r, snd_id: s, srv_dh_public: d } = resp {
            rcv_id = *r;
            snd_id = *s;
            _srv_dh = *d;
            got_ids = true;
            break;
        }
    }

    if !got_ids {
        return Err(anyhow::anyhow!("No IDS response from server"));
    }

    tracing::info!("Step 2: Queue created rcv_id={}... snd_id={}...",
        hex::encode(&rcv_id[..4]), hex::encode(&snd_id[..4]));

    // ---- Step 3: Generate sender auth key ----
    tracing::info!("Step 3: Generating sender auth key");

    let snd_auth = generate_ed25519();
    tracing::info!("Step 3: Sender auth key ready");

    // ---- Step 4: SKEY to peer's queue ----
    tracing::info!("Step 4: Sending SKEY to peer queue");

    let peer_snd_id = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(&invitation.queue_id)
        .unwrap_or_default();

    let skey_tx = cmd_skey(&smp, &snd_auth, &peer_snd_id);
    tracing::debug!("SKEY transmission: {} bytes", skey_tx.len());
    tracing::debug!("SKEY hex (first 120): {}", hex::encode(&skey_tx[..skey_tx.len().min(120)]));
    smp.write_command_block(&skey_tx).await
        .map_err(|e| anyhow::anyhow!("SKEY send: {e}"))?;

    let skey_resp = smp.read_responses().await
        .map_err(|e| anyhow::anyhow!("SKEY response: {e}"))?;

    for (i, r) in skey_resp.iter().enumerate() {
        tracing::info!("Step 4: SKEY response[{i}]: {:?}", format!("{r:?}").chars().take(100).collect::<String>());
    }

    // ---- Step 5: Send AgentConfirmation ----
    tracing::info!("Step 5: Sending AgentConfirmation with profile '{profile_name}'");

    // E2E ratchet keys - X448 (68-byte SPKI with OID 2b656f)
    let our_key1 = crate::crypto::keys::X448Keypair::generate();
    let our_key2 = crate::crypto::keys::X448Keypair::generate();

    // Build full AgentInvitation message ('_' + agentVer + 'I' + connReq + ConnInfo)
    let conf_body = encode_agent_invitation(
        &invitation.server_host,
        invitation.server_port,
        &server_key_hash,
        &snd_id,
        rcv_dh_pub.as_bytes(),
        &our_key1.encode_spki(),
        &our_key2.encode_spki(),
        profile_name,
    );

    // Decode peer's DH public key from invitation sender_key
    let peer_dh_bytes = base64::engine::general_purpose::URL_SAFE
        .decode(&invitation.sender_key)
        .map_err(|e| anyhow::anyhow!("peer DH key decode: {e} (raw: '{}')", invitation.sender_key))?;
    tracing::debug!("peer DH key decoded: {} bytes = {}", peer_dh_bytes.len(), hex::encode(&peer_dh_bytes));
    let mut peer_dh_pub = [0u8; 32];
    if peer_dh_bytes.len() == 44 {
        peer_dh_pub.copy_from_slice(&peer_dh_bytes[12..44]);
    } else if peer_dh_bytes.len() == 32 {
        peer_dh_pub.copy_from_slice(&peer_dh_bytes);
    } else {
        return Err(anyhow::anyhow!("Unexpected peer DH key length: {}", peer_dh_bytes.len()));
    }

    tracing::debug!("CONF body {} bytes (first 80): {}",
        conf_body.len(), hex::encode(&conf_body[..80.min(conf_body.len())]));
    tracing::debug!("CONF peer_dh_pub: {}", hex::encode(&peer_dh_pub));
    tracing::debug!("CONF our_dh_pub: {}", hex::encode(rcv_dh_pub.as_bytes()));

    // PrivHeader is empty - '_' is already inside conf_body
    let conf_client_msg = e2e_encrypt_agent_msg(
        &conf_body,
        &peer_dh_pub,
        rcv_dh_priv.as_bytes(),
        rcv_dh_pub.as_bytes(),
        true,  // is_first_message - inline our DH key
        &[],   // no separate PrivHeader - '_' is in conf_body
    );

    let conf_tx = cmd_send(&smp, &snd_auth, &peer_snd_id, &conf_client_msg, b'D', true);
    smp.write_command_block(&conf_tx).await
        .map_err(|e| anyhow::anyhow!("CONF send: {e}"))?;

    let conf_resp = smp.read_responses().await
        .map_err(|e| anyhow::anyhow!("CONF response: {e}"))?;

    for (i, r) in conf_resp.iter().enumerate() {
        tracing::info!("Step 5: CONF response[{i}]: {:?}", format!("{r:?}").chars().take(100).collect::<String>());
    }

    tracing::info!("Step 5: AgentConfirmation sent");

    // ---- Step 6: Wait for peer response ----
    // No explicit SUB needed - NEW with subMode 'S' already subscribed.
    tracing::info!("Step 6: Waiting for peer response (up to 60s)...");

    // Wait for any message from peer (KEY, HELLO, etc.)
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(60);
    loop {
        if tokio::time::Instant::now() > deadline {
            tracing::warn!("Step 6: Timeout waiting for peer response");
            break;
        }

        match tokio::time::timeout(
            tokio::time::Duration::from_secs(10),
            smp.read_responses(),
        )
        .await
        {
            Ok(Ok(responses)) => {
                for (i, r) in responses.iter().enumerate() {
                    tracing::info!("Step 6: response[{i}]: {:?}", format!("{r:?}").chars().take(200).collect::<String>());
                }
                // If we got a MSG, the peer responded
                if responses.iter().any(|r| matches!(r, ServerResponse::Msg { .. })) {
                    tracing::info!("Step 6: Received MSG from peer");
                    break;
                }
            }
            Ok(Err(e)) => {
                tracing::warn!("Step 6: Read error: {e}, retrying...");
            }
            Err(_) => {
                tracing::debug!("Step 6: Read timeout, retrying...");
            }
        }
    }

    tracing::info!("*** Handshake flow complete for contact {contact_id} ***");
    tracing::info!("Note: Full KEY/HELLO exchange requires Phase 4 parsing");

    Ok(())
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
