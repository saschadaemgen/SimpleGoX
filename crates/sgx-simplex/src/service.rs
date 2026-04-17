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
        let profile_name = self.display_name.lock().await.clone().unwrap_or_default();
        let contact_id = uuid_v4();

        if invitation::is_contact_address(&link) {
            // Contact Address -> AgentInvitation ('I'), no Double Ratchet
            info!("SimpleX: detected CONTACT ADDRESS");

            let contact = invitation::parse_contact_address(&link)
                .map_err(|e| Status::invalid_argument(format!("Contact parse: {e}")))?;

            self.store.save_contact(
                &contact_id, None,
                &contact.server_host, contact.server_port,
                &contact.server_fingerprint, &contact.queue_id, &contact.sender_key,
            ).map_err(|e| Status::internal(format!("Store: {e}")))?;

            info!("SimpleX: contact saved (id={contact_id}, server={}:{})",
                contact.server_host, contact.server_port);

            tokio::spawn(async move {
                info!("SimpleX: contact handshake task started for {contact_id}");
                match execute_contact_handshake(&contact, &profile_name, store, &contact_id).await {
                    Ok(_) => info!("SimpleX: contact handshake COMPLETED for {contact_id}"),
                    Err(e) => tracing::error!("SimpleX: contact handshake FAILED: {e:?}"),
                }
            });
        } else {
            // One-Time Invitation -> AgentConfirmation ('C')
            info!("SimpleX: detected ONE-TIME INVITATION");

            let parsed = invitation::parse_invitation_link(&link)
                .map_err(|e| Status::invalid_argument(format!("Invitation parse: {e}")))?;

            self.store.save_contact(
                &contact_id, None,
                &parsed.server_host, parsed.server_port,
                &parsed.server_fingerprint, &parsed.queue_id, &parsed.sender_key,
            ).map_err(|e| Status::internal(format!("Store: {e}")))?;

            info!("SimpleX: contact saved (id={contact_id}, server={}:{})",
                parsed.server_host, parsed.server_port);

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
/// 7 steps: TLS+SMP -> NEW -> KEY -> SUB -> CONF -> wait -> HELLO
async fn execute_handshake(
    invitation: &invitation::ParsedInvitation,
    profile_name: &str,
    store: Arc<QueueStore>,
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
    // v9: separate X25519 auth key for NEW
    let (_rcv_auth_x25519_priv, rcv_auth_x25519_pub) = generate_x25519();

    let new_tx = cmd_new(&smp, &rcv_auth, rcv_auth_x25519_pub.as_bytes(), rcv_dh_pub.as_bytes());

    tracing::debug!("NEW tx {} bytes, sig_len={}", new_tx.len(), new_tx[0]);
    // After sig (65 bytes): corrIdLen, corrId, entityIdLen, entityId, cmd
    let after_sig = &new_tx[65..];
    tracing::debug!("NEW after sig ({} bytes): {}", after_sig.len(), hex::encode(&after_sig[..after_sig.len().min(60)]));
    // Find cmd start: skip corrIdLen+corrId+entityIdLen+entityId
    if after_sig.len() > 2 {
        let cid_len = after_sig[0] as usize;
        let eid_offset = 1 + cid_len;
        if eid_offset < after_sig.len() {
            let eid_len = after_sig[eid_offset] as usize;
            let cmd_offset = eid_offset + 1 + eid_len;
            if cmd_offset + 4 < after_sig.len() {
                tracing::debug!("NEW cmd at +{}: '{}'",
                    cmd_offset,
                    String::from_utf8_lossy(&after_sig[cmd_offset..after_sig.len().min(cmd_offset+20)]));
            }
        }
    }
    tracing::debug!("NEW full hex (first 140): {}", hex::encode(&new_tx[..new_tx.len().min(140)]));

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
    let mut srv_dh = [0u8; 32];
    let mut got_ids = false;

    for resp in &responses {
        if let ServerResponse::Ids { rcv_id: r, snd_id: s, srv_dh_public: d } = resp {
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

    tracing::info!("Step 2: Queue created rcv_id={}... snd_id={}...",
        hex::encode(&rcv_id[..4]), hex::encode(&snd_id[..4]));

    // ---- Step 3: Generate sender auth key ----
    tracing::info!("Step 3: Generating sender auth key");

    let snd_auth = generate_ed25519();
    tracing::info!("Step 3: Sender auth key ready");

    // ---- Step 4: KEY on OUR queue (register sender auth key, v6) ----
    tracing::info!("Step 4: Sending KEY to OUR queue (rcv_id={}...)", hex::encode(&rcv_id[..4]));

    let key_tx = cmd_key(&smp, &rcv_auth, &rcv_id, snd_auth.verifying_key().as_bytes());
    smp.write_command_block(&key_tx).await
        .map_err(|e| anyhow::anyhow!("KEY send: {e}"))?;
    let key_resp = smp.read_responses().await
        .map_err(|e| anyhow::anyhow!("KEY response: {e}"))?;
    tracing::info!("Step 4: KEY response: {:?}",
        key_resp.iter().map(|x| format!("{x:?}").chars().take(60).collect::<String>()).collect::<Vec<_>>());

    // Step 4b: SUB to our queue (subscribe for incoming messages)
    tracing::info!("Step 4b: SUB to our queue (rcv_id={}...)", hex::encode(&rcv_id[..4]));
    let sub_tx = cmd_sub(&smp, &rcv_auth, &rcv_id);
    smp.write_command_block(&sub_tx).await
        .map_err(|e| anyhow::anyhow!("SUB send: {e}"))?;
    let sub_resp = smp.read_responses().await
        .map_err(|e| anyhow::anyhow!("SUB response: {e}"))?;
    tracing::info!("Step 4b: SUB response: {:?}",
        sub_resp.iter().map(|x| format!("{x:?}").chars().take(60).collect::<String>()).collect::<Vec<_>>());

    let peer_snd_id = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(&invitation.queue_id)
        .unwrap_or_default();

    // ---- Step 5: Send AgentConfirmation ----
    tracing::info!("Step 5: Sending AgentConfirmation with profile '{profile_name}'");

    // E2E ratchet keys - X448 (68-byte SPKI with OID 2b656f)
    let our_key1 = crate::crypto::keys::X448Keypair::generate();
    let our_key2 = crate::crypto::keys::X448Keypair::generate();

    // PERSIST E2E keypairs BEFORE sending (needed for X3DH when peer responds)
    store.save_e2e_keypairs(
        contact_id,
        &our_key1.private, &our_key1.public,
        &our_key2.private, &our_key2.public,
    ).ok();

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
        return Err(anyhow::anyhow!("Unexpected peer DH key length: {}", peer_dh_bytes.len()));
    }

    tracing::debug!("CONF priv_header: {} bytes", priv_header.len());
    tracing::debug!("CONF body: {} bytes, first 20: {}", conf_body.len(),
        hex::encode(&conf_body[..20.min(conf_body.len())]));
    tracing::debug!("CONF peer_dh_pub: {}", hex::encode(&peer_dh_pub));

    let conf_client_msg = e2e_encrypt_agent_msg(
        &conf_body,
        &peer_dh_pub,
        rcv_dh_priv.as_bytes(),
        rcv_dh_pub.as_bytes(),
        true,           // is_first_message - inline our DH key
        &priv_header,   // 'K' + snd_auth SPKI
    );

    let conf_tx = cmd_send_unsigned(&smp, &peer_snd_id, &conf_client_msg, b'D', true);
    smp.write_command_block(&conf_tx).await
        .map_err(|e| anyhow::anyhow!("CONF send: {e}"))?;

    let conf_resp = smp.read_responses().await
        .map_err(|e| anyhow::anyhow!("CONF response: {e}"))?;

    for (i, r) in conf_resp.iter().enumerate() {
        tracing::info!("Step 5: CONF response[{i}]: {:?}", format!("{r:?}").chars().take(100).collect::<String>());
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
                            tracing::info!("BG: MSG received, msg_id={}, body={} bytes",
                                hex::encode(&msg_id[..4]), body.len());

                            // ACK immediately
                            let ack = cmd_ack(&smp, &rcv_auth, &rcv_id, msg_id);
                            if let Err(e) = smp.write_command_block(&ack).await {
                                tracing::error!("BG: ACK failed: {e}");
                                return;
                            }
                            let _ = smp.read_responses().await;

                            // Decrypt peer's confirmation (Layer 3 server + Layer 2 E2E)
                            let conf_plaintext = match e2e_decrypt_incoming(msg_id, body, &srv_dh_bytes, &rcv_dh_priv_bytes) {
                                Ok(p) => {
                                    tracing::info!("BG: Layer 3+2 decrypted OK, {} bytes", p.len());
                                    p
                                }
                                Err(e) => {
                                    tracing::error!("BG: E2E decrypt failed: {e}");
                                    tracing::debug!("BG: srv_dh={}, rcv_dh_priv={}...",
                                        hex::encode(&srv_dh_bytes[..4]),
                                        hex::encode(&rcv_dh_priv_bytes[..4]));
                                    tracing::debug!("BG: body first 20: {}",
                                        hex::encode(&body[..20.min(body.len())]));
                                    return;
                                }
                            };

                            tracing::info!("BG: PrivHeader = 0x{:02x} '{}'",
                                conf_plaintext[0], conf_plaintext[0] as char);
                            tracing::debug!("BG: Plaintext first 40: {}",
                                hex::encode(&conf_plaintext[..40.min(conf_plaintext.len())]));

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
                            tracing::info!("BG: Peer e2e_key1={} bytes, e2e_key2={} bytes",
                                peer_conf.e2e_key1.len(), peer_conf.e2e_key2.len());

                            // KEY command
                            tracing::info!("BG: Sending KEY (snd_auth={}...)", hex::encode(&peer_conf.snd_auth_public[..4]));
                            let key_tx = cmd_key(&smp, &rcv_auth, &rcv_id, &peer_conf.snd_auth_public);
                            if let Err(e) = smp.write_command_block(&key_tx).await {
                                tracing::error!("BG: KEY send failed: {e}");
                                return;
                            }
                            match smp.read_responses().await {
                                Ok(r) => tracing::info!("BG: KEY response: {:?}",
                                    r.iter().map(|x| format!("{x:?}").chars().take(60).collect::<String>()).collect::<Vec<_>>()),
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

                            let hello_tx = cmd_send(&smp, &snd_auth, &peer_snd_id, &hello_client_msg, b'H', false);
                            if let Err(e) = smp.write_command_block(&hello_tx).await {
                                tracing::error!("HELLO send failed: {e}");
                                return;
                            }
                            match smp.read_responses().await {
                                Ok(r) => tracing::info!("BG: HELLO response: {:?}",
                                    r.iter().map(|x| format!("{x:?}").chars().take(60).collect::<String>()).collect::<Vec<_>>()),
                                Err(e) => tracing::warn!("BG: HELLO response error: {e}"),
                            }
                            tracing::info!("BG: HELLO sent, waiting for peer HELLO...");
                            // The next MSG on our queue should be peer's HELLO
                            // It will arrive in the next loop iteration or we handle it here
                            // For now, mark as connected after sending our HELLO
                            store_bg.set_contact_status(&contact_id_bg, "connected").ok();
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

/// Execute contact address handshake (AgentInvitation 'I', no Double Ratchet).
async fn execute_contact_handshake(
    contact: &invitation::ParsedContactAddress,
    profile_name: &str,
    store: Arc<QueueStore>,
    contact_id: &str,
) -> Result<(), anyhow::Error> {
    use base64::Engine;
    use crate::crypto::keys::*;
    use crate::e2e_crypto::*;
    use crate::protocol::agent_msg::*;
    use crate::smp_client::{SmpClient, SmpServerAddr};
    use crate::smp_commands::*;
    use crate::smp_protocol::*;

    // Step 1: TLS + SMP handshake
    tracing::info!("Contact Step 1: Connecting to {}:{}", contact.server_host, contact.server_port);
    let addr = SmpServerAddr {
        host: contact.server_host.clone(),
        port: contact.server_port,
        fingerprint: contact.server_fingerprint.clone(),
    };
    let client = SmpClient::new(addr, None);
    let tls_stream = client.connect().await
        .map_err(|e| anyhow::anyhow!("TLS: {e}"))?;

    let fp_clean = contact.server_fingerprint.replace("%3D", "=");
    let fp_bytes = base64::engine::general_purpose::URL_SAFE
        .decode(&fp_clean)
        .map_err(|e| anyhow::anyhow!("fingerprint decode: {e} (raw: '{}')", contact.server_fingerprint))?;
    let mut server_key_hash = [0u8; 32];
    if fp_bytes.len() == 32 {
        server_key_hash.copy_from_slice(&fp_bytes);
    } else {
        return Err(anyhow::anyhow!("bad fingerprint len: {} (expected 32)", fp_bytes.len()));
    }

    let mut smp = SmpConnection::smp_handshake(tls_stream, server_key_hash).await
        .map_err(|e| anyhow::anyhow!("SMP: {e}"))?;
    tracing::info!("Contact Step 1: SMP OK, session_id={}...", hex::encode(&smp.session_id[..4]));

    // Step 2: Create reply queue
    tracing::info!("Contact Step 2: Creating reply queue");
    let rcv_auth = generate_ed25519();
    let (rcv_dh_priv, rcv_dh_pub) = generate_x25519();
    // v9: separate X25519 auth key for NEW
    let (_rcv_auth_x25519_priv, rcv_auth_x25519_pub) = generate_x25519();

    let new_tx = cmd_new(&smp, &rcv_auth, rcv_auth_x25519_pub.as_bytes(), rcv_dh_pub.as_bytes());
    smp.write_command_block(&new_tx).await.map_err(|e| anyhow::anyhow!("NEW: {e}"))?;
    let responses = smp.read_responses().await.map_err(|e| anyhow::anyhow!("NEW resp: {e}"))?;

    let mut rcv_id = [0u8; 24];
    let mut snd_id = [0u8; 24];
    let mut srv_dh = [0u8; 32];
    for resp in &responses {
        if let ServerResponse::Ids { rcv_id: r, snd_id: s, srv_dh_public: d } = resp {
            rcv_id = *r; snd_id = *s; srv_dh = *d; break;
        }
    }
    tracing::info!("Contact Step 2: rcv_id={}... snd_id={}...",
        hex::encode(&rcv_id[..4]), hex::encode(&snd_id[..4]));

    // Generate E2E ratchet keypairs for the reply queue.
    // sndSecure=True in NEW means the queue is already secured; no separate
    // KEY/SUB calls are required in SMP v9 (subscription is active via the
    // '0ST' flag tuple in NEW command options).
    // snd_auth (for sending back to peer) will be generated on demand in
    // Briefing 036 when the HELLO response is built.
    let our_key1 = X448Keypair::generate();
    let our_key2 = X448Keypair::generate();
    store.save_e2e_keypairs(contact_id,
        &our_key1.private, &our_key1.public,
        &our_key2.private, &our_key2.public).ok();

    // Step 3: Build and send AgentInvitation
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
        &contact.server_host, contact.server_port, &server_key_hash,
        &snd_id, rcv_dh_pub.as_bytes(),
        &our_key1.encode_spki(), &our_key2.encode_spki(), profile_name,
    );

    // encode_agent_invitation already includes '_' at the start
    let inv_client_msg = e2e_encrypt_agent_msg(
        &inv_body, &peer_dh_pub, rcv_dh_priv.as_bytes(), rcv_dh_pub.as_bytes(),
        true, &[], // empty - '_' is already in inv_body
    );

    let peer_snd_id = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(&contact.queue_id).unwrap_or_default();

    let send_tx = cmd_send_unsigned(&smp, &peer_snd_id, &inv_client_msg, b'S', false);
    smp.write_command_block(&send_tx).await.map_err(|e| anyhow::anyhow!("SEND: {e}"))?;
    let send_resp = smp.read_responses().await.map_err(|e| anyhow::anyhow!("SEND resp: {e}"))?;
    tracing::info!("Contact Step 3: SEND: {:?}",
        send_resp.iter().map(|x| format!("{x:?}").chars().take(60).collect::<String>()).collect::<Vec<_>>());

    tracing::info!("Contact: AgentInvitation sent! Background loop starting...");

    // Step 6: Background - wait for contact's AgentConfirmation
    let contact_id_bg = contact_id.to_string();
    let rcv_dh_priv_bytes = *rcv_dh_priv.as_bytes();
    let srv_dh_bytes = srv_dh;
    let store_bg = store.clone();

    tokio::spawn(async move {
        tracing::info!("Contact BG: Waiting for AgentConfirmation...");
        loop {
            match smp.read_responses().await {
                Ok(responses) => {
                    for resp in &responses {
                        if let ServerResponse::Msg { msg_id, body } = resp {
                            tracing::info!("Contact BG: MSG received, {} bytes, msg_id={}...",
                                body.len(), hex::encode(&msg_id[..4]));

                            let ack = cmd_ack(&smp, &rcv_auth, &rcv_id, msg_id);
                            if smp.write_command_block(&ack).await.is_err() { return; }
                            let _ = smp.read_responses().await;

                            match e2e_decrypt_incoming(msg_id, body, &srv_dh_bytes, &rcv_dh_priv_bytes) {
                                Ok(plaintext) => {
                                    tracing::info!("Contact BG: Decrypted {} bytes, priv_header=0x{:02x}",
                                        plaintext.len(), plaintext.first().unwrap_or(&0));
                                    tracing::debug!("Contact BG: first 40: {}",
                                        hex::encode(&plaintext[..40.min(plaintext.len())]));
                                    store_bg.set_contact_status(&contact_id_bg, "pending_hello").ok();
                                    tracing::info!("Contact BG: Received confirmation! Phase 6: HELLO exchange");
                                }
                                Err(e) => tracing::error!("Contact BG: Decrypt failed: {e}"),
                            }
                        } else if let ServerResponse::End = resp {
                            tracing::warn!("Contact BG: END");
                            return;
                        }
                    }
                }
                Err(e) => { tracing::error!("Contact BG: Error: {e}"); return; }
            }
        }
    });

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
