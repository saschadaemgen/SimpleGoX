//! gRPC MessengerService implementation backed by TDLib.
//!
//! A background receive loop (td::start_pump) runs on a dedicated OS thread
//! to drain TDLib's event queue. Internally, tdlib_rs::receive() routes
//! @extra-tagged responses to their function callers via an Observer pattern.
//! This means functions::* work normally as long as the pump is running.

use crate::auth::{AuthManager, AuthStatus};
use crate::convert;
use crate::td;
use sgx_proto::messenger::v1::messenger_service_server::MessengerService;
use sgx_proto::messenger::v1::*;
use std::pin::Pin;
use std::sync::Arc;
use tdlib_rs::enums;
use tokio::sync::broadcast;
use tokio_stream::Stream;
use tonic::{Request, Response, Status};
use tracing::{info, warn};

pub struct TelegramService {
    client_id: i32,
    auth: Arc<AuthManager>,
    /// Own user ID for identifying outgoing messages
    own_user_id: tokio::sync::Mutex<Option<i64>>,
    /// Own display name (cached from get_me)
    own_display_name: tokio::sync::Mutex<String>,
    _update_tx: broadcast::Sender<enums::Update>,
}

impl TelegramService {
    pub async fn new(
        api_id: i32,
        api_hash: &str,
        data_dir: &str,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let client_id = tdlib_rs::create_client();
        let auth = Arc::new(AuthManager::new(client_id));

        // Start the receive pump on a dedicated OS thread.
        // This is REQUIRED - without it, function responses never arrive
        // because nothing calls td_receive to pull them off the wire.
        // The pump calls tdlib_rs::receive() which internally routes
        // @extra responses to function callers and returns only updates.
        let update_tx = td::start_pump();

        // Initialize TDLib (set params, detect existing session)
        auth.initialize(api_id, api_hash, data_dir).await?;

        // Cache own user info for sender name resolution
        let (own_id, own_name) = match tdlib_rs::functions::get_me(client_id).await {
            Ok(enums::User::User(u)) => {
                let name = format!("{} {}", u.first_name, u.last_name)
                    .trim()
                    .to_string();
                info!("Own user: {} ({})", name, u.id);
                (Some(u.id), name)
            }
            Err(_) => (None, "You".into()),
        };

        Ok(Self {
            client_id,
            auth,
            own_user_id: tokio::sync::Mutex::new(own_id),
            own_display_name: tokio::sync::Mutex::new(own_name),
            _update_tx: update_tx,
        })
    }
}

#[tonic::async_trait]
impl MessengerService for TelegramService {
    async fn get_backend_info(
        &self,
        _request: Request<GetBackendInfoRequest>,
    ) -> Result<Response<BackendInfo>, Status> {
        let is_authenticated = matches!(self.auth.status().await, AuthStatus::Ready { .. });

        Ok(Response::new(BackendInfo {
            backend_id: "telegram".into(),
            display_name: "Telegram".into(),
            version: "0.1.0".into(),
            is_authenticated,
            badge_label: "TG".into(),
            badge_color: "#61afef".into(),
        }))
    }

    async fn get_auth_state(
        &self,
        _request: Request<GetAuthStateRequest>,
    ) -> Result<Response<AuthState>, Status> {
        let status = self.auth.status().await;
        let state = match status {
            AuthStatus::WaitPhone => Some(auth_state::State::WaitPhone(WaitPhone {})),
            AuthStatus::WaitCode { phone_hint } => Some(auth_state::State::WaitCode(WaitCode {
                phone_number_hint: phone_hint,
            })),
            AuthStatus::WaitPassword { hint } => {
                Some(auth_state::State::WaitPassword(WaitPassword {
                    password_hint: hint,
                }))
            }
            AuthStatus::Ready {
                user_id,
                display_name,
            } => Some(auth_state::State::Ready(Ready {
                user_id,
                display_name,
            })),
            AuthStatus::LoggedOut => Some(auth_state::State::LoggedOut(LoggedOut {})),
            AuthStatus::Error(msg) => Some(auth_state::State::Error(AuthError { message: msg })),
        };

        Ok(Response::new(AuthState { state }))
    }

    async fn submit_phone_number(
        &self,
        request: Request<SubmitPhoneNumberRequest>,
    ) -> Result<Response<SubmitPhoneNumberResponse>, Status> {
        let phone = &request.into_inner().phone_number;
        self.auth
            .submit_phone(phone)
            .await
            .map_err(|e| Status::internal(e))?;
        Ok(Response::new(SubmitPhoneNumberResponse { success: true }))
    }

    async fn submit_auth_code(
        &self,
        request: Request<SubmitAuthCodeRequest>,
    ) -> Result<Response<SubmitAuthCodeResponse>, Status> {
        let code = &request.into_inner().code;
        self.auth
            .submit_code(code)
            .await
            .map_err(|e| Status::internal(e))?;
        Ok(Response::new(SubmitAuthCodeResponse { success: true }))
    }

    async fn submit_password(
        &self,
        request: Request<SubmitPasswordRequest>,
    ) -> Result<Response<SubmitPasswordResponse>, Status> {
        let password = &request.into_inner().password;
        self.auth
            .submit_password(password)
            .await
            .map_err(|e| Status::internal(e))?;
        Ok(Response::new(SubmitPasswordResponse { success: true }))
    }

    async fn list_chats(
        &self,
        request: Request<ListChatsRequest>,
    ) -> Result<Response<ListChatsResponse>, Status> {
        let req = request.into_inner();
        let limit = req.limit.max(1).min(200);

        info!("list_chats: requesting {limit} chats");
        let chat_list =
            tdlib_rs::functions::get_chats(Some(enums::ChatList::Main), limit, self.client_id)
                .await
                .map_err(|e| Status::internal(format!("get_chats: {e:?}")))?;

        let enums::Chats::Chats(chats_result) = chat_list;
        info!(
            "list_chats: got {} IDs: {:?}",
            chats_result.chat_ids.len(),
            chats_result.chat_ids
        );

        let mut chats = Vec::new();
        for chat_id in chats_result.chat_ids {
            info!("get_chat({chat_id})...");
            match tdlib_rs::functions::get_chat(chat_id, self.client_id).await {
                Ok(enums::Chat::Chat(chat)) => {
                    info!("get_chat({chat_id}) OK: {:?}", chat.title);
                    chats.push(convert::tdlib_chat_to_proto(&chat));
                }
                Err(e) => {
                    warn!("get_chat({chat_id}) error: {e:?}");
                }
            }
        }

        info!("list_chats returning {} chats", chats.len());
        Ok(Response::new(ListChatsResponse { chats }))
    }

    async fn get_messages(
        &self,
        request: Request<GetMessagesRequest>,
    ) -> Result<Response<GetMessagesResponse>, Status> {
        let req = request.into_inner();
        info!(
            "get_messages CALLED: chat_id={:?} limit={} from={}",
            req.chat_id.as_ref().map(|c| &c.id),
            req.limit,
            req.from_message_id
        );
        let chat_id = req
            .chat_id
            .as_ref()
            .ok_or_else(|| Status::invalid_argument("chat_id required"))?;

        let tg_chat_id: i64 = chat_id
            .id
            .parse()
            .map_err(|_| Status::invalid_argument("invalid chat_id"))?;

        let from_msg_id: i64 = if req.from_message_id.is_empty() {
            0
        } else {
            req.from_message_id
                .parse()
                .map_err(|_| Status::invalid_argument("invalid from_message_id"))?
        };

        let limit = req.limit.max(1).min(100);

        // TDLib requires open_chat before full history is available
        if let Err(e) = tdlib_rs::functions::open_chat(tg_chat_id, self.client_id).await {
            warn!("open_chat({tg_chat_id}) failed: {e:?}");
        }

        // First attempt - may return only cached messages
        let result = tdlib_rs::functions::get_chat_history(
            tg_chat_id,
            from_msg_id,
            0,
            limit,
            false,
            self.client_id,
        )
        .await
        .map_err(|e| Status::internal(format!("get_chat_history: {e:?}")))?;

        let enums::Messages::Messages(first_try) = result;
        info!(
            "get_messages: first try total_count={}, actual={}",
            first_try.total_count,
            first_try.messages.len()
        );

        // If we got fewer than requested and TDLib knows there are more,
        // wait for network fetch and retry
        let msgs = if (first_try.messages.len() as i32) < limit
            && first_try.total_count > first_try.messages.len() as i32
        {
            info!("get_messages: too few results, waiting 1s and retrying...");
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

            let retry = tdlib_rs::functions::get_chat_history(
                tg_chat_id,
                from_msg_id,
                0,
                limit,
                false,
                self.client_id,
            )
            .await
            .map_err(|e| Status::internal(format!("get_chat_history retry: {e:?}")))?;

            let enums::Messages::Messages(retry_msgs) = retry;
            info!(
                "get_messages: retry total_count={}, actual={}",
                retry_msgs.total_count,
                retry_msgs.messages.len()
            );
            retry_msgs
        } else {
            first_try
        };
        let mut messages = Vec::new();
        // Fetch chat title as fallback for sender name in DMs
        let chat_title = match tdlib_rs::functions::get_chat(tg_chat_id, self.client_id).await {
            Ok(enums::Chat::Chat(c)) => c.title,
            Err(_) => String::new(),
        };
        let own_name = self.own_display_name.lock().await.clone();
        let own_id = *self.own_user_id.lock().await;

        for msg in msgs.messages.into_iter().flatten() {
            info!(
                "  msg id={} is_outgoing={} date={}",
                msg.id, msg.is_outgoing, msg.date
            );
            let mut proto = convert::tdlib_message_to_proto(&msg);

            // Resolve sender display name
            if proto.sender_name.is_empty() {
                if msg.is_outgoing {
                    // Own message - use cached display name
                    proto.sender_name = own_name.clone();
                } else {
                    // Try get_user first
                    let resolved =
                        convert::resolve_sender_name(&proto.sender_id, self.client_id).await;
                    if resolved.starts_with("User ") && !chat_title.is_empty() {
                        // get_user failed - use chat title (works for DMs)
                        proto.sender_name = chat_title.clone();
                    } else {
                        proto.sender_name = resolved;
                    }
                }
                info!("  -> sender_name={:?}", proto.sender_name);
            }
            messages.push(proto);
        }

        Ok(Response::new(GetMessagesResponse { messages }))
    }

    async fn send_message(
        &self,
        request: Request<SendMessageRequest>,
    ) -> Result<Response<SendMessageResponse>, Status> {
        let req = request.into_inner();
        let chat_id = req
            .chat_id
            .as_ref()
            .ok_or_else(|| Status::invalid_argument("chat_id required"))?;

        let tg_chat_id: i64 = chat_id
            .id
            .parse()
            .map_err(|_| Status::invalid_argument("invalid chat_id"))?;

        let body = match req.content {
            Some(send_message_request::Content::Text(text)) => text.body,
            _ => return Err(Status::invalid_argument("text content required")),
        };

        let reply_to = if req.reply_to_message_id.is_empty() {
            None
        } else {
            let msg_id: i64 = req
                .reply_to_message_id
                .parse()
                .map_err(|_| Status::invalid_argument("invalid reply_to_message_id"))?;
            Some(enums::InputMessageReplyTo::Message(
                tdlib_rs::types::InputMessageReplyToMessage {
                    message_id: msg_id,
                    quote: None,
                    checklist_task_id: 0,
                },
            ))
        };

        let input_content =
            enums::InputMessageContent::InputMessageText(tdlib_rs::types::InputMessageText {
                text: tdlib_rs::types::FormattedText {
                    text: body,
                    entities: Vec::new(),
                },
                link_preview_options: None,
                clear_draft: true,
            });

        let result = tdlib_rs::functions::send_message(
            tg_chat_id,
            None,
            reply_to,
            None,
            input_content,
            self.client_id,
        )
        .await
        .map_err(|e| Status::internal(format!("send_message: {e:?}")))?;

        let enums::Message::Message(msg) = result;
        Ok(Response::new(SendMessageResponse {
            message_id: Some(MessageId {
                backend: "telegram".into(),
                chat_id: chat_id.id.clone(),
                id: msg.id.to_string(),
            }),
        }))
    }

    type StreamUpdatesStream =
        Pin<Box<dyn Stream<Item = Result<sgx_proto::messenger::v1::Update, Status>> + Send>>;

    async fn stream_updates(
        &self,
        _request: Request<StreamUpdatesRequest>,
    ) -> Result<Response<Self::StreamUpdatesStream>, Status> {
        Err(Status::unimplemented(
            "stream_updates not yet available for Telegram sidecar",
        ))
    }
}
