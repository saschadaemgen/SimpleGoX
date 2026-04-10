//! Convert TDLib types to sgx-proto gRPC types.

use prost_types::Timestamp;
use sgx_proto::messenger::v1::*;
use tdlib_rs::types;

/// Convert a TDLib chat to a proto Chat.
pub fn tdlib_chat_to_proto(chat: &types::Chat) -> Chat {
    let chat_type = match &chat.r#type {
        tdlib_rs::enums::ChatType::Private(_) => ChatType::Private,
        tdlib_rs::enums::ChatType::BasicGroup(_) => ChatType::Group,
        tdlib_rs::enums::ChatType::Supergroup(sg) if sg.is_channel => ChatType::Channel,
        tdlib_rs::enums::ChatType::Supergroup(_) => ChatType::Group,
        tdlib_rs::enums::ChatType::Secret(_) => ChatType::Private,
    };

    let last_message = chat
        .last_message
        .as_ref()
        .map(|m| tdlib_message_to_proto(m));

    let photo_url = chat
        .photo
        .as_ref()
        .map(|p| format!("tg-file:{}", p.small.id))
        .unwrap_or_default();

    Chat {
        chat_id: Some(ChatId {
            backend: "telegram".into(),
            id: chat.id.to_string(),
        }),
        title: chat.title.clone(),
        chat_type: chat_type as i32,
        avatar_url: photo_url,
        last_message,
        unread_count: chat.unread_count,
        is_encrypted: matches!(chat.r#type, tdlib_rs::enums::ChatType::Secret(_)),
        is_muted: chat.default_disable_notification,
        is_pinned: false,
        last_activity: chat.last_message.as_ref().map(|m| Timestamp {
            seconds: m.date as i64,
            nanos: 0,
        }),
    }
}

/// Convert a TDLib message to a proto UnifiedMessage.
pub fn tdlib_message_to_proto(msg: &types::Message) -> UnifiedMessage {
    let content = match &msg.content {
        tdlib_rs::enums::MessageContent::MessageText(text) => {
            Some(unified_message::Content::Text(TextContent {
                body: text.text.text.clone(),
            }))
        }
        tdlib_rs::enums::MessageContent::MessagePhoto(photo) => {
            let url = photo
                .photo
                .sizes
                .last()
                .map(|s| format!("tg-file:{}", s.photo.id))
                .unwrap_or_default();
            let thumb = photo
                .photo
                .sizes
                .first()
                .map(|s| format!("tg-file:{}", s.photo.id))
                .unwrap_or_default();
            Some(unified_message::Content::Media(MediaContent {
                media_type: MediaType::Photo as i32,
                url,
                thumbnail_url: thumb,
                caption: photo.caption.text.clone(),
                file_name: String::new(),
                file_size: 0,
            }))
        }
        tdlib_rs::enums::MessageContent::MessageVideo(video) => {
            Some(unified_message::Content::Media(MediaContent {
                media_type: MediaType::Video as i32,
                url: format!("tg-file:{}", video.video.video.id),
                thumbnail_url: video
                    .video
                    .thumbnail
                    .as_ref()
                    .map(|t| format!("tg-file:{}", t.file.id))
                    .unwrap_or_default(),
                caption: video.caption.text.clone(),
                file_name: video.video.file_name.clone(),
                file_size: video.video.video.expected_size as i64,
            }))
        }
        tdlib_rs::enums::MessageContent::MessageDocument(doc) => {
            Some(unified_message::Content::Media(MediaContent {
                media_type: MediaType::Document as i32,
                url: format!("tg-file:{}", doc.document.document.id),
                thumbnail_url: doc
                    .document
                    .thumbnail
                    .as_ref()
                    .map(|t| format!("tg-file:{}", t.file.id))
                    .unwrap_or_default(),
                caption: doc.caption.text.clone(),
                file_name: doc.document.file_name.clone(),
                file_size: doc.document.document.expected_size as i64,
            }))
        }
        tdlib_rs::enums::MessageContent::MessageVoiceNote(voice) => {
            Some(unified_message::Content::Media(MediaContent {
                media_type: MediaType::Voice as i32,
                url: format!("tg-file:{}", voice.voice_note.voice.id),
                thumbnail_url: String::new(),
                caption: voice.caption.text.clone(),
                file_name: String::new(),
                file_size: voice.voice_note.voice.expected_size as i64,
            }))
        }
        tdlib_rs::enums::MessageContent::MessageSticker(sticker) => {
            // Show sticker as its emoji character
            Some(unified_message::Content::Text(TextContent {
                body: sticker.sticker.emoji.clone(),
            }))
        }
        tdlib_rs::enums::MessageContent::MessageAnimatedEmoji(animated) => {
            Some(unified_message::Content::Text(TextContent {
                body: animated.emoji.clone(),
            }))
        }
        other => {
            tracing::info!(
                "=== UNMATCHED CONTENT TYPE: {:?}",
                std::mem::discriminant(other)
            );
            Some(unified_message::Content::Text(TextContent {
                body: format!("[Unsupported: {:?}]", std::mem::discriminant(other)),
            }))
        }
    };

    let sender_id = match &msg.sender_id {
        tdlib_rs::enums::MessageSender::User(u) => u.user_id.to_string(),
        tdlib_rs::enums::MessageSender::Chat(c) => c.chat_id.to_string(),
    };

    let reply_to_id = match &msg.reply_to {
        Some(tdlib_rs::enums::MessageReplyTo::Message(r)) => r.message_id.to_string(),
        _ => String::new(),
    };

    UnifiedMessage {
        message_id: Some(MessageId {
            backend: "telegram".into(),
            chat_id: msg.chat_id.to_string(),
            id: msg.id.to_string(),
        }),
        sender_id,
        sender_name: String::new(),
        sender_avatar_url: String::new(),
        timestamp: Some(Timestamp {
            seconds: msg.date as i64,
            nanos: 0,
        }),
        is_outgoing: msg.is_outgoing,
        content,
        reply_to_message_id: reply_to_id,
        is_edited: msg.edit_date > 0,
    }
}

/// Resolve sender display name from TDLib user cache.
pub async fn resolve_sender_name(sender_id: &str, client_id: i32) -> String {
    use tracing::{info, warn};

    let user_id: i64 = match sender_id.parse() {
        Ok(id) => id,
        Err(_) => return sender_id.to_string(),
    };
    match tdlib_rs::functions::get_user(user_id, client_id).await {
        Ok(tdlib_rs::enums::User::User(user)) => {
            info!(
                "resolve_sender_name({user_id}): first={:?} last={:?}",
                user.first_name, user.last_name
            );
            let name = format!("{} {}", user.first_name, user.last_name);
            let name = name.trim();
            if name.is_empty() {
                format!("User {user_id}")
            } else {
                name.to_string()
            }
        }
        Err(e) => {
            warn!("resolve_sender_name({user_id}): get_user failed: {e:?}");
            format!("User {user_id}")
        }
    }
}

/// Convert a TDLib update to a proto Update (if relevant).
/// Synchronous - does not resolve sender names (caller must do that).
pub fn tdlib_update_to_proto(
    update: &tdlib_rs::enums::Update,
) -> Option<sgx_proto::messenger::v1::Update> {
    match update {
        tdlib_rs::enums::Update::NewMessage(new_msg) => Some(sgx_proto::messenger::v1::Update {
            update: Some(update::Update::NewMessage(NewMessage {
                message: Some(tdlib_message_to_proto(&new_msg.message)),
            })),
        }),
        tdlib_rs::enums::Update::MessageContent(edit) => {
            let new_body = match &edit.new_content {
                tdlib_rs::enums::MessageContent::MessageText(text) => text.text.text.clone(),
                _ => return None,
            };
            Some(sgx_proto::messenger::v1::Update {
                update: Some(update::Update::MessageEdited(MessageEdited {
                    message_id: Some(MessageId {
                        backend: "telegram".into(),
                        chat_id: edit.chat_id.to_string(),
                        id: edit.message_id.to_string(),
                    }),
                    new_body,
                })),
            })
        }
        tdlib_rs::enums::Update::DeleteMessages(del) if !del.from_cache => del
            .message_ids
            .first()
            .map(|id| sgx_proto::messenger::v1::Update {
                update: Some(update::Update::MessageDeleted(MessageDeleted {
                    message_id: Some(MessageId {
                        backend: "telegram".into(),
                        chat_id: del.chat_id.to_string(),
                        id: id.to_string(),
                    }),
                })),
            }),
        tdlib_rs::enums::Update::ChatLastMessage(clm) => {
            // Chat updated (new last message, changed sort order)
            Some(sgx_proto::messenger::v1::Update {
                update: Some(update::Update::ChatUpdated(ChatUpdated {
                    chat: Some(Chat {
                        chat_id: Some(ChatId {
                            backend: "telegram".into(),
                            id: clm.chat_id.to_string(),
                        }),
                        title: String::new(), // Filled by caller if needed
                        chat_type: 0,
                        avatar_url: String::new(),
                        last_message: clm.last_message.as_ref().map(|m| tdlib_message_to_proto(m)),
                        unread_count: 0, // Will be filled from ChatReadInbox
                        is_encrypted: false,
                        is_muted: false,
                        is_pinned: false,
                        last_activity: clm.last_message.as_ref().map(|m| Timestamp {
                            seconds: m.date as i64,
                            nanos: 0,
                        }),
                    }),
                })),
            })
        }
        tdlib_rs::enums::Update::ChatReadInbox(inbox) => {
            // Unread count changed
            Some(sgx_proto::messenger::v1::Update {
                update: Some(update::Update::ChatUpdated(ChatUpdated {
                    chat: Some(Chat {
                        chat_id: Some(ChatId {
                            backend: "telegram".into(),
                            id: inbox.chat_id.to_string(),
                        }),
                        unread_count: inbox.unread_count,
                        ..Default::default()
                    }),
                })),
            })
        }
        _ => None,
    }
}
