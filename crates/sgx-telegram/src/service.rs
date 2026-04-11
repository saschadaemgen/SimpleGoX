//! gRPC MessengerService implementation backed by TDLib.
//!
//! A background receive loop (td::start_pump) runs on a dedicated OS thread
//! to drain TDLib's event queue. Internally, tdlib_rs::receive() routes
//! @extra-tagged responses to their function callers via an Observer pattern.
//! This means functions::* work normally as long as the pump is running.

use crate::auth::{AuthManager, AuthStatus};
use crate::convert;
use crate::td;
use base64::Engine;
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
    /// Own display name (cached from get_me at startup)
    own_display_name: tokio::sync::Mutex<String>,
    update_tx: broadcast::Sender<enums::Update>,
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

        // Cache own display name for sender resolution on outgoing messages
        let own_name = match tdlib_rs::functions::get_me(client_id).await {
            Ok(enums::User::User(u)) => {
                let name = format!("{} {}", u.first_name, u.last_name)
                    .trim()
                    .to_string();
                info!("Own user: {} (id={})", name, u.id);
                name
            }
            Err(_) => "You".into(),
        };

        Ok(Self {
            client_id,
            auth,
            own_display_name: tokio::sync::Mutex::new(own_name),
            update_tx,
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
            AuthStatus::WaitCode {
                phone_hint: _,
                code_type,
            } => {
                info!("=== gRPC get_auth_state: WaitCode code_type={code_type}");
                Some(auth_state::State::WaitCode(WaitCode {
                    phone_number_hint: code_type,
                }))
            }
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

        info!("list_chats: loading {limit} chats into TDLib memory...");
        // loadChats MUST be called first - it loads chats from DB into memory.
        // getChats only returns chats already in memory.
        if let Err(e) =
            tdlib_rs::functions::load_chats(Some(enums::ChatList::Main), limit, self.client_id)
                .await
        {
            // Error 404 = "chat list has already been loaded" - safe to ignore
            info!("load_chats: {e:?} (may be already loaded)");
        }

        info!("list_chats: fetching loaded chats...");
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

        info!(
            "=== GET_MESSAGES START: chat_id={}, limit={}, from_msg_id={}",
            tg_chat_id, limit, from_msg_id
        );

        // Open chat so TDLib starts loading history
        match tdlib_rs::functions::open_chat(tg_chat_id, self.client_id).await {
            Ok(_) => info!("=== OPEN_CHAT: OK"),
            Err(e) => info!("=== OPEN_CHAT: FAILED {e:?}"),
        }

        // TDLib's get_chat_history returns only a few cached messages per call
        // (known behavior, TDLib issues #317, #702, #168, #740).
        // Paginate: each call uses the oldest message ID as from_message_id
        // for the next, until we have enough or no more come back.
        let mut all_tdlib_msgs: Vec<tdlib_rs::types::Message> = Vec::new();
        let mut cursor: i64 = from_msg_id;
        let max_rounds = 10; // safety limit

        for round in 0..max_rounds {
            let result = tdlib_rs::functions::get_chat_history(
                tg_chat_id,
                cursor,
                0,
                limit,
                false,
                self.client_id,
            )
            .await
            .map_err(|e| Status::internal(format!("get_chat_history: {e:?}")))?;

            let enums::Messages::Messages(batch) = result;
            let batch_msgs: Vec<_> = batch.messages.into_iter().flatten().collect();
            info!(
                "=== HISTORY round {}: cursor={}, returned={}, total_count={}",
                round,
                cursor,
                batch_msgs.len(),
                batch.total_count
            );

            if batch_msgs.is_empty() {
                break;
            }

            // Move cursor to the oldest message in this batch
            cursor = batch_msgs.last().unwrap().id;
            all_tdlib_msgs.extend(batch_msgs);

            if all_tdlib_msgs.len() >= limit as usize {
                all_tdlib_msgs.truncate(limit as usize);
                break;
            }
        }

        info!(
            "=== HISTORY DONE: collected {} messages",
            all_tdlib_msgs.len()
        );

        // Fetch chat info
        let (chat_title, chat_type_dbg) =
            match tdlib_rs::functions::get_chat(tg_chat_id, self.client_id).await {
                Ok(enums::Chat::Chat(c)) => {
                    let type_str = match &c.r#type {
                        tdlib_rs::enums::ChatType::Private(_) => "Private",
                        tdlib_rs::enums::ChatType::BasicGroup(_) => "BasicGroup",
                        tdlib_rs::enums::ChatType::Supergroup(sg) => {
                            if sg.is_channel {
                                "Channel"
                            } else {
                                "Supergroup"
                            }
                        }
                        tdlib_rs::enums::ChatType::Secret(_) => "Secret",
                    };
                    info!("=== CHAT INFO: title='{}', type={}", c.title, type_str);
                    (c.title, type_str.to_string())
                }
                Err(e) => {
                    info!("=== CHAT INFO: get_chat FAILED {e:?}");
                    (String::new(), "unknown".to_string())
                }
            };

        let is_private = chat_type_dbg == "Private" || chat_type_dbg == "Secret";
        let own_name = self.own_display_name.lock().await.clone();
        info!("=== OWN NAME: '{}'", own_name);

        let mut messages = Vec::new();
        for msg in all_tdlib_msgs {
            let sender_id_str = match &msg.sender_id {
                tdlib_rs::enums::MessageSender::User(u) => format!("User({})", u.user_id),
                tdlib_rs::enums::MessageSender::Chat(c) => format!("Chat({})", c.chat_id),
            };
            let content_type = match &msg.content {
                tdlib_rs::enums::MessageContent::MessageText(_) => "text",
                tdlib_rs::enums::MessageContent::MessagePhoto(_) => "photo",
                tdlib_rs::enums::MessageContent::MessageVideo(_) => "video",
                tdlib_rs::enums::MessageContent::MessageDocument(_) => "document",
                tdlib_rs::enums::MessageContent::MessageSticker(_) => "sticker",
                tdlib_rs::enums::MessageContent::MessageAnimatedEmoji(e) => {
                    info!("    animated_emoji: '{}'", e.emoji);
                    "animated_emoji"
                }
                tdlib_rs::enums::MessageContent::MessageVoiceNote(_) => "voice",
                other => {
                    info!("    unknown content: {:?}", std::mem::discriminant(other));
                    "other"
                }
            };
            info!(
                "=== MSG: id={}, sender={}, is_outgoing={}, content={}",
                msg.id, sender_id_str, msg.is_outgoing, content_type
            );

            let mut proto = convert::tdlib_message_to_proto(&msg);

            // Sender name resolution
            if proto.sender_name.is_empty() {
                proto.sender_name = if msg.is_outgoing {
                    own_name.clone()
                } else if is_private {
                    chat_title.clone()
                } else {
                    let resolved =
                        convert::resolve_sender_name(&proto.sender_id, self.client_id).await;
                    resolved
                };
            }

            info!(
                "=== NAME: chose '{}' for msg {} (outgoing={}, chat_type={})",
                proto.sender_name, msg.id, msg.is_outgoing, chat_type_dbg
            );
            messages.push(proto);
        }

        info!(
            "=== GET_MESSAGES DONE: returning {} messages",
            messages.len()
        );
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
        info!("=== StreamUpdates: new subscriber connected");

        let mut rx = self.update_tx.subscribe();
        let client_id = self.client_id;
        let own_name = self.own_display_name.lock().await.clone();

        let stream = async_stream::stream! {
            loop {
                match rx.recv().await {
                    Ok(tdlib_update) => {
                        if let Some(mut proto_update) = convert::tdlib_update_to_proto(&tdlib_update) {
                            // Resolve sender name for new messages
                            if let Some(update::Update::NewMessage(ref mut nm)) = proto_update.update {
                                if let Some(ref mut msg) = nm.message {
                                    if msg.sender_name.is_empty() {
                                        if msg.is_outgoing {
                                            msg.sender_name = own_name.clone();
                                        } else {
                                            msg.sender_name = convert::resolve_sender_name(
                                                &msg.sender_id, client_id,
                                            ).await;
                                        }
                                    }
                                }
                            }
                            yield Ok(proto_update);
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!("StreamUpdates subscriber lagged by {n} messages");
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        info!("StreamUpdates: broadcast channel closed");
                        break;
                    }
                }
            }
        };

        Ok(Response::new(Box::pin(stream)))
    }

    async fn logout(
        &self,
        _request: Request<LogoutRequest>,
    ) -> Result<Response<LogoutResponse>, Status> {
        info!("=== LOGOUT: logging out of TDLib");
        tdlib_rs::functions::log_out(self.client_id)
            .await
            .map_err(|e| Status::internal(format!("log_out: {e:?}")))?;
        info!("=== LOGOUT: TDLib logged out, shutting down sidecar");

        // After logout the client_id is invalid.
        // Exit the process - the frontend will restart it on next "Add Account".
        tokio::spawn(async {
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            std::process::exit(0);
        });

        Ok(Response::new(LogoutResponse { success: true }))
    }

    async fn download_avatar(
        &self,
        request: Request<DownloadAvatarRequest>,
    ) -> Result<Response<DownloadAvatarResponse>, Status> {
        let file_id = request.into_inner().file_id;
        info!("download_avatar: file_id={file_id}");

        let result = tdlib_rs::functions::download_file(
            file_id,
            32,   // priority (highest)
            0,    // offset
            0,    // limit (0 = whole file)
            true, // synchronous
            self.client_id,
        )
        .await
        .map_err(|e| Status::internal(format!("download_file: {e:?}")))?;

        let tdlib_rs::enums::File::File(f) = result;
        if f.local.is_downloading_completed {
            let bytes = std::fs::read(&f.local.path)
                .map_err(|e| Status::internal(format!("read file: {e}")))?;
            let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
            let data_url = format!("data:image/jpeg;base64,{b64}");
            info!("download_avatar: OK, {} bytes", bytes.len());
            Ok(Response::new(DownloadAvatarResponse { data_url }))
        } else {
            Err(Status::not_found("File not downloaded"))
        }
    }
}
