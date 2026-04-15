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
/// Steps (from briefing 029):
/// 1. Parse invitation (done before this call)
/// 2. Create reply queue (NEW command)
/// 3. X3DH key agreement
/// 4. Send SKEY to secure peer queue
/// 5. Send CONF (AgentConfirmation) with profile
/// 6. Wait for KEY + SKEY from peer
/// 7. Send HELLO -> Receive HELLO -> CON
///
/// Phase 2: logs the steps, actual SMP commands come in Phase 3.
async fn execute_handshake(
    invitation: &invitation::ParsedInvitation,
    _profile_name: &str,
    _store: &QueueStore,
    contact_id: &str,
) -> Result<(), anyhow::Error> {
    use crate::smp_client::{SmpClient, SmpServerAddr};

    tracing::info!("Handshake Step 1: Invitation parsed for {}:{}", invitation.server_host, invitation.server_port);

    // Step 2: Connect to SMP server
    tracing::info!("Handshake Step 2: Connecting to SMP server...");
    let addr = SmpServerAddr {
        host: invitation.server_host.clone(),
        port: invitation.server_port,
        fingerprint: invitation.server_fingerprint.clone(),
    };
    let client = SmpClient::new(addr, None);
    let _tls_stream = client.connect().await
        .map_err(|e| anyhow::anyhow!("TLS connect failed: {e}"))?;
    tracing::info!("Handshake Step 2: TLS connected to {}:{}", invitation.server_host, invitation.server_port);

    // Step 3: X3DH key agreement
    tracing::info!("Handshake Step 3: X3DH key agreement (placeholder - needs peer E2E keys from invitation)");

    // Step 4-7: SMP commands (Phase 3 - requires 16KB framing)
    tracing::info!("Handshake Steps 4-7: SMP queue operations (Phase 3)");
    tracing::info!("Handshake: contact {} status -> connecting", contact_id);

    // TODO Phase 3: NEW, SKEY, CONF, SUB, HELLO exchange
    // For now, the TLS connection proves the transport layer works.

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
