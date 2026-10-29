use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use tracing::warn;

use crate::middleware::auth::CurrentUser;
use crate::models::attachment::{Attachment, AttachmentResponse};
use crate::models::contact::{Contact, ContactResponse};
use crate::models::conversation::Conversation;
use crate::models::message::{Message, MessageSender};
use crate::services::event_dispatcher;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct CreateMessagePayload {
    pub content: Option<String>,
    pub private: Option<bool>,
    pub message_type: Option<serde_json::Value>,
    pub content_attributes: Option<serde_json::Value>,
}

pub async fn list_messages(
    Path((account_id, display_id)): Path<(i32, i32)>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let conv = match sqlx::query_as::<_, Conversation>(
        "SELECT * FROM conversations WHERE account_id = $1 AND display_id = $2",
    )
    .bind(account_id)
    .bind(display_id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(c)) => c,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Conversation not found" })),
            );
        }
    };

    let messages = sqlx::query_as::<_, Message>(
        "SELECT * FROM messages WHERE conversation_id = $1 ORDER BY created_at ASC",
    )
    .bind(conv.id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let mut payload = Vec::new();
    for msg in messages {
        let attachments = sqlx::query_as::<_, Attachment>(
            "SELECT * FROM attachments WHERE message_id = $1",
        )
        .bind(msg.id)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(AttachmentResponse::from)
        .collect();

        let sender = match (msg.sender_type.as_deref(), msg.sender_id) {
            (Some("User"), Some(user_id)) => {
                #[derive(sqlx::FromRow)]
                struct UserName {
                    name: String,
                }
                let u = sqlx::query_as::<_, UserName>("SELECT name FROM users WHERE id = $1")
                    .bind(user_id as i32)
                    .fetch_optional(&state.db)
                    .await
                    .ok()
                    .flatten();

                Some(MessageSender {
                    id: user_id,
                    name: u.map(|n| n.name).unwrap_or_else(|| "Agent".to_string()),
                    avatar_url: "".to_string(),
                    r#type: "user".to_string(),
                })
            }
            (Some("Contact"), Some(contact_id)) => {
                let c = sqlx::query_as::<_, Contact>("SELECT * FROM contacts WHERE id = $1")
                    .bind(contact_id as i32)
                    .fetch_optional(&state.db)
                    .await
                    .ok()
                    .flatten();

                Some(MessageSender {
                    id: contact_id,
                    name: c.and_then(|ct| ct.name).unwrap_or_else(|| "Customer".to_string()),
                    avatar_url: "".to_string(),
                    r#type: "contact".to_string(),
                })
            }
            _ => None,
        };

        payload.push(msg.to_response(conv.display_id, sender, attachments));
    }

    let contact_meta = if let Some(contact_id) = conv.contact_id {
        sqlx::query_as::<_, Contact>("SELECT * FROM contacts WHERE id = $1")
            .bind(contact_id)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten()
            .map(ContactResponse::from)
    } else {
        None
    };

    (
        StatusCode::OK,
        Json(json!({
            "meta": {
                "contact": contact_meta,
                "assignee": null,
                "labels": []
            },
            "payload": payload
        })),
    )
}

pub async fn create_message(
    Path((account_id, display_id)): Path<(i32, i32)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    Json(body): Json<CreateMessagePayload>,
) -> impl IntoResponse {
    let conv = match sqlx::query_as::<_, Conversation>(
        "SELECT * FROM conversations WHERE account_id = $1 AND display_id = $2",
    )
    .bind(account_id)
    .bind(display_id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(c)) => c,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Conversation not found" })),
            );
        }
    };

    let is_private = body.private.unwrap_or(false);
    let message_type_int: i32 = match body.message_type {
        Some(serde_json::Value::Number(n)) => n.as_i64().unwrap_or(1) as i32,
        Some(serde_json::Value::String(ref s)) if s == "incoming" => 0,
        _ => 1, // default outgoing
    };

    let new_message = match sqlx::query_as::<_, Message>(
        r#"
        INSERT INTO messages (
            account_id, inbox_id, conversation_id, content, message_type,
            private, status, content_type, content_attributes, sender_type,
            sender_id, created_at, updated_at
        )
        VALUES (
            $1, $2, $3, $4, $5,
            $6, 0, 0, $7, 'User',
            $8, NOW(), NOW()
        )
        RETURNING *
        "#,
    )
    .bind(account_id)
    .bind(conv.inbox_id)
    .bind(conv.id)
    .bind(&body.content)
    .bind(message_type_int)
    .bind(is_private)
    .bind(body.content_attributes.unwrap_or_else(|| json!({})))
    .bind(current_user.id as i64)
    .fetch_one(&state.db)
    .await
    {
        Ok(m) => m,
        Err(e) => {
            warn!("Error creating message in DB: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Failed to create message: {}", e) })),
            );
        }
    };

    // Update last_activity_at pada conversation
    let _ = sqlx::query("UPDATE conversations SET last_activity_at = NOW(), updated_at = NOW() WHERE id = $1")
        .bind(conv.id)
        .execute(&state.db)
        .await;

    // Ambil nama user untuk sender
    #[derive(sqlx::FromRow)]
    struct UserName {
        name: String,
    }
    let u = sqlx::query_as::<_, UserName>("SELECT name FROM users WHERE id = $1")
        .bind(current_user.id)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten();

    let sender = Some(MessageSender {
        id: current_user.id as i64,
        name: u.map(|n| n.name).unwrap_or_else(|| "Agent".to_string()),
        avatar_url: "".to_string(),
        r#type: "user".to_string(),
    });

    let resp = new_message.to_response(conv.display_id, sender.clone(), vec![]);
    let resp_json = serde_json::to_value(&resp).unwrap_or_default();

    // Broadcast ke ActionCable agar semua agen menerima pesan baru secara instan!
    event_dispatcher::dispatch_event(&state, account_id, "message.created", resp_json.clone());

    // Dispatch outbound webhook ke sistem eksternal
    let db_wh = state.db.clone();
    let wh_msg = resp_json.clone();
    tokio::spawn(async move {
        crate::services::webhook_dispatcher::dispatch_webhook_event(
            &db_wh,
            account_id as i64,
            "message_created",
            wh_msg,
        )
        .await;
    });

    // Evaluasi Automation Rules secara asinkron
    let st_auto = state.clone();
    let conv_id = conv.id;
    let msg_id = new_message.id;
    tokio::spawn(async move {
        crate::services::automation_rules_matcher::evaluate_and_execute_rules(
            &st_auto,
            account_id as i64,
            "message_created",
            conv_id,
            Some(msg_id),
        )
        .await;
    });

    // In-App Notification untuk agent yang di-assign pada percakapan
    if let Some(assignee) = conv.assignee_id {
        if assignee != current_user.id {
            let st_notif = state.clone();
            tokio::spawn(async move {
                let _ = crate::services::notification_dispatcher::create_and_dispatch_notification(
                    &st_notif,
                    account_id as i64,
                    assignee as i64,
                    3, // assigned_conversation_new_message
                    conv_id as i64,
                    "Conversation",
                    Some(msg_id as i64),
                    Some("Message"),
                    None,
                )
                .await;
            });
        }
    }

    // Jika percakapan merupakan channel WhatsApp dan bukan pesan privat, kirim keluar via Meta API
    let inbox_channel_type = sqlx::query_scalar::<_, Option<String>>(
        "SELECT channel_type FROM inboxes WHERE id = $1",
    )
    .bind(conv.inbox_id)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten()
    .flatten();

    if !is_private && inbox_channel_type.as_deref() == Some("Channel::Whatsapp") {
        if let Some(contact_id) = conv.contact_id {
            let contact_phone = sqlx::query_scalar::<_, Option<String>>(
                "SELECT phone_number FROM contacts WHERE id = $1",
            )
            .bind(contact_id as i32)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten()
            .flatten();

            if let Some(phone) = contact_phone {
                let db_clone = state.db.clone();
                let base_url = state.config.whatsapp_cloud_base_url.clone();
                let secret_key_base = state.config.secret_key_base.clone();
                let content_str = body.content.clone().unwrap_or_default();
                let msg_id = new_message.id;
                let inbox_id = conv.inbox_id;
                let display_id = conv.display_id;
                let state_clone = state.clone();
                let sender_clone = sender.clone();

                tokio::spawn(async move {
                    match crate::services::meta_whatsapp::send_outgoing_message(
                        &db_clone,
                        inbox_id,
                        &phone,
                        &content_str,
                        &base_url,
                        &secret_key_base,
                    )
                    .await
                    {
                        Ok(wamid) => {
                            let _ = sqlx::query(
                                "UPDATE messages SET source_id = $1, status = 1, updated_at = NOW() WHERE id = $2",
                            )
                            .bind(&wamid)
                            .bind(msg_id)
                            .execute(&db_clone)
                            .await;

                            if let Ok(Some(updated_msg)) = sqlx::query_as::<_, Message>(
                                "SELECT * FROM messages WHERE id = $1",
                            )
                            .bind(msg_id)
                            .fetch_optional(&db_clone)
                            .await
                            {
                                let updated_resp = updated_msg.to_response(display_id, sender_clone, vec![]);
                                let updated_val = serde_json::to_value(&updated_resp).unwrap_or_default();
                                event_dispatcher::dispatch_event(
                                    &state_clone,
                                    account_id,
                                    "message.updated",
                                    updated_val,
                                );
                            }
                        }
                        Err(e) => {
                            warn!("Failed to dispatch outgoing WhatsApp message: {}", e);
                        }
                    }
                });
            }
        }
    }

    // Jika percakapan merupakan channel Instagram dan bukan pesan privat, kirim keluar via Meta Instagram API
    if !is_private && inbox_channel_type.as_deref() == Some("Channel::Instagram") {
        if let Some(contact_id) = conv.contact_id {
            let ig_source_id = sqlx::query_scalar::<_, Option<String>>(
                "SELECT source_id FROM contact_inboxes WHERE inbox_id = $1 AND contact_id = $2",
            )
            .bind(conv.inbox_id)
            .bind(contact_id as i32)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten()
            .flatten();

            if let Some(recipient_id) = ig_source_id {
                let db_clone = state.db.clone();
                let base_url = state.config.instagram_graph_base_url.clone();
                let content_str = body.content.clone().unwrap_or_default();
                let msg_id = new_message.id;
                let inbox_id = conv.inbox_id;
                let display_id = conv.display_id;
                let state_clone = state.clone();
                let sender_clone = sender.clone();

                tokio::spawn(async move {
                    match crate::services::meta_instagram::send_outgoing_instagram_message(
                        &db_clone,
                        inbox_id,
                        &recipient_id,
                        &content_str,
                        &base_url,
                    )
                    .await
                    {
                        Ok(mid) => {
                            let _ = sqlx::query(
                                "UPDATE messages SET source_id = $1, status = 1, updated_at = NOW() WHERE id = $2",
                            )
                            .bind(&mid)
                            .bind(msg_id)
                            .execute(&db_clone)
                            .await;

                            if let Ok(Some(updated_msg)) = sqlx::query_as::<_, Message>(
                                "SELECT * FROM messages WHERE id = $1",
                            )
                            .bind(msg_id)
                            .fetch_optional(&db_clone)
                            .await
                            {
                                let updated_resp = updated_msg.to_response(display_id, sender_clone, vec![]);
                                let updated_val = serde_json::to_value(&updated_resp).unwrap_or_default();
                                event_dispatcher::dispatch_event(
                                    &state_clone,
                                    account_id,
                                    "message.updated",
                                    updated_val,
                                );
                            }
                        }
                        Err(e) => {
                            warn!("Failed to dispatch outgoing Instagram DM: {}", e);
                        }
                    }
                });
            }
        }
    }

    // Jika percakapan merupakan channel Facebook Page dan bukan pesan privat, kirim keluar via Meta Facebook API
    if !is_private && inbox_channel_type.as_deref() == Some("Channel::FacebookPage") {
        if let Some(contact_id) = conv.contact_id {
            let fb_source_id = sqlx::query_scalar::<_, Option<String>>(
                "SELECT source_id FROM contact_inboxes WHERE inbox_id = $1 AND contact_id = $2",
            )
            .bind(conv.inbox_id)
            .bind(contact_id as i32)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten()
            .flatten();

            if let Some(recipient_id) = fb_source_id {
                let db_clone = state.db.clone();
                let base_url = state.config.fb_graph_base_url.clone();
                let content_str = body.content.clone().unwrap_or_default();
                let msg_id = new_message.id;
                let inbox_id = conv.inbox_id;
                let display_id = conv.display_id;
                let state_clone = state.clone();
                let sender_clone = sender.clone();

                tokio::spawn(async move {
                    match crate::services::meta_facebook::send_outgoing_facebook_message(
                        &db_clone,
                        inbox_id,
                        &recipient_id,
                        &content_str,
                        &base_url,
                    )
                    .await
                    {
                        Ok(mid) => {
                            let _ = sqlx::query(
                                "UPDATE messages SET source_id = $1, status = 1, updated_at = NOW() WHERE id = $2",
                            )
                            .bind(&mid)
                            .bind(msg_id)
                            .execute(&db_clone)
                            .await;

                            if let Ok(Some(updated_msg)) = sqlx::query_as::<_, Message>(
                                "SELECT * FROM messages WHERE id = $1",
                            )
                            .bind(msg_id)
                            .fetch_optional(&db_clone)
                            .await
                            {
                                let updated_resp = updated_msg.to_response(display_id, sender_clone, vec![]);
                                let updated_val = serde_json::to_value(&updated_resp).unwrap_or_default();
                                event_dispatcher::dispatch_event(
                                    &state_clone,
                                    account_id,
                                    "message.updated",
                                    updated_val,
                                );
                            }
                        }
                        Err(e) => {
                            warn!("Failed to dispatch outgoing Facebook Messenger: {}", e);
                        }
                    }
                });
            }
        }
    }

    // Jika percakapan merupakan channel Telegram dan bukan pesan privat, kirim keluar via Telegram Bot API
    if !is_private && inbox_channel_type.as_deref() == Some("Channel::Telegram") {
        if let Some(contact_id) = conv.contact_id {
            let tg_source_id = sqlx::query_scalar::<_, Option<String>>(
                "SELECT source_id FROM contact_inboxes WHERE inbox_id = $1 AND contact_id = $2",
            )
            .bind(conv.inbox_id)
            .bind(contact_id as i32)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten()
            .flatten();

            if let Some(chat_id) = tg_source_id {
                let db_clone = state.db.clone();
                let base_url = state.config.telegram_api_base_url.clone();
                let content_str = body.content.clone().unwrap_or_default();
                let msg_id = new_message.id;
                let inbox_id = conv.inbox_id;
                let display_id = conv.display_id;
                let state_clone = state.clone();
                let sender_clone = sender.clone();

                tokio::spawn(async move {
                    match crate::services::telegram::send_outgoing_telegram_message(
                        &db_clone,
                        inbox_id,
                        &chat_id,
                        &content_str,
                        &base_url,
                    )
                    .await
                    {
                        Ok(mid) => {
                            let _ = sqlx::query(
                                "UPDATE messages SET source_id = $1, status = 1, updated_at = NOW() WHERE id = $2",
                            )
                            .bind(&mid)
                            .bind(msg_id)
                            .execute(&db_clone)
                            .await;

                            if let Ok(Some(updated_msg)) = sqlx::query_as::<_, Message>(
                                "SELECT * FROM messages WHERE id = $1",
                            )
                            .bind(msg_id)
                            .fetch_optional(&db_clone)
                            .await
                            {
                                let updated_resp = updated_msg.to_response(display_id, sender_clone, vec![]);
                                let updated_val = serde_json::to_value(&updated_resp).unwrap_or_default();
                                event_dispatcher::dispatch_event(
                                    &state_clone,
                                    account_id,
                                    "message.updated",
                                    updated_val,
                                );
                            }
                        }
                        Err(e) => {
                            warn!("Failed to dispatch outgoing Telegram message: {}", e);
                        }
                    }
                });
            }
        }
    }

    // Jika percakapan merupakan channel Shopee dan bukan pesan privat, kirim keluar via Shopee Open API
    if !is_private && inbox_channel_type.as_deref() == Some("Channel::Shopee") {
        if let Some(contact_id) = conv.contact_id {
            let shopee_source_id = sqlx::query_scalar::<_, Option<String>>(
                "SELECT source_id FROM contact_inboxes WHERE inbox_id = $1 AND contact_id = $2",
            )
            .bind(conv.inbox_id)
            .bind(contact_id as i32)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten()
            .flatten();

            if let Some(to_user_id) = shopee_source_id {
                let db_clone = state.db.clone();
                let base_url = state.config.shopee_api_base_url.clone();
                let content_str = body.content.clone().unwrap_or_default();
                let msg_id = new_message.id;
                let inbox_id = conv.inbox_id;
                let display_id = conv.display_id;
                let state_clone = state.clone();
                let sender_clone = sender.clone();

                tokio::spawn(async move {
                    match crate::services::shopee::send_outgoing_shopee_message(
                        &db_clone,
                        inbox_id,
                        &to_user_id,
                        &content_str,
                        &base_url,
                    )
                    .await
                    {
                        Ok(mid) => {
                            let _ = sqlx::query(
                                "UPDATE messages SET source_id = $1, status = 1, updated_at = NOW() WHERE id = $2",
                            )
                            .bind(&mid)
                            .bind(msg_id)
                            .execute(&db_clone)
                            .await;

                            if let Ok(Some(updated_msg)) = sqlx::query_as::<_, Message>(
                                "SELECT * FROM messages WHERE id = $1",
                            )
                            .bind(msg_id)
                            .fetch_optional(&db_clone)
                            .await
                            {
                                let updated_resp = updated_msg.to_response(display_id, sender_clone, vec![]);
                                let updated_val = serde_json::to_value(&updated_resp).unwrap_or_default();
                                event_dispatcher::dispatch_event(
                                    &state_clone,
                                    account_id,
                                    "message.updated",
                                    updated_val,
                                );
                            }
                        }
                        Err(e) => {
                            warn!("Failed to dispatch outgoing Shopee message: {}", e);
                        }
                    }
                });
            }
        }
    }

    // Jika percakapan merupakan channel Email dan bukan pesan privat, kirim keluar via Email Mailer
    if !is_private && inbox_channel_type.as_deref() == Some("Channel::Email") {
        if let Some(contact_id) = conv.contact_id {
            let contact_email = sqlx::query_scalar::<_, Option<String>>(
                "SELECT email FROM contacts WHERE id = $1",
            )
            .bind(contact_id as i32)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten()
            .flatten();

            if let Some(to_email) = contact_email {
                let db_clone = state.db.clone();
                let content_str = body.content.clone().unwrap_or_default();
                let msg_id = new_message.id;
                let inbox_id = conv.inbox_id;
                let conv_id = conv.id;

                tokio::spawn(async move {
                    let _ = crate::services::email_mailer::send_outgoing_email(
                        &db_clone,
                        account_id,
                        inbox_id,
                        conv_id,
                        msg_id,
                        &to_email,
                        "Support Ticket Update",
                        &content_str,
                    )
                    .await;
                });
            }
        }
    }

    // Jika percakapan merupakan channel Tokopedia (Channel::Api dengan identifier tokopedia)
    if !is_private && inbox_channel_type.as_deref() == Some("Channel::Api") {
        if let Some(contact_id) = conv.contact_id {
            let contact_identifier = sqlx::query_scalar::<_, Option<String>>(
                "SELECT identifier FROM contacts WHERE id = $1",
            )
            .bind(contact_id as i32)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten()
            .flatten();

            if let Some(id_str) = contact_identifier {
                if let Some(buyer_id_str) = id_str.strip_prefix("tokopedia:") {
                    if let Ok(to_user_id) = buyer_id_str.parse::<i64>() {
                        let db_clone = state.db.clone();
                        let content_str = body.content.clone().unwrap_or_default();
                        tokio::spawn(async move {
                            let _ = crate::services::tokopedia::send_outgoing_message(
                                &db_clone,
                                12345,
                                to_user_id,
                                &content_str,
                                "https://api.tokopedia.com/v1/chat/api/send",
                            )
                            .await;
                        });
                    }
                }
            }
        }
    }

    (StatusCode::OK, Json(resp_json))
}

#[derive(Debug, Deserialize)]
pub struct AutoTranslatePayload {
    pub message_ids: Vec<i64>,
}

#[derive(Debug, Deserialize)]
pub struct TranslateMessagePayload {
    pub target_language: Option<String>,
}

/// POST /api/v1/accounts/:account_id/conversations/:display_id/messages/auto_translate
pub async fn auto_translate_messages(
    Path((account_id, display_id)): Path<(i32, i32)>,
    State(state): State<AppState>,
    Json(payload): Json<AutoTranslatePayload>,
) -> impl IntoResponse {
    let conv = match sqlx::query_as::<_, Conversation>(
        "SELECT * FROM conversations WHERE account_id = $1 AND display_id = $2",
    )
    .bind(account_id)
    .bind(display_id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(c)) => c,
        _ => return StatusCode::NOT_FOUND.into_response(),
    };

    let db_clone = state.db.clone();
    let state_clone = state.clone();
    let message_ids = payload.message_ids;
    let conv_id = conv.id;

    tokio::spawn(async move {
        let llm_provider = crate::services::llm::get_llm_provider(None, None, None, &state_clone.config);
        for mid in message_ids {
            let msg = match sqlx::query_as::<_, Message>(
                "SELECT * FROM messages WHERE id = $1 AND conversation_id = $2",
            )
            .bind(mid)
            .bind(conv_id)
            .fetch_optional(&db_clone)
            .await
            {
                Ok(Some(m)) => m,
                _ => continue,
            };

            let content = match msg.content {
                Some(ref c) if !c.trim().is_empty() => c.clone(),
                _ => continue,
            };

            let prompt = format!(
                "Terjemahkan pesan dukungan pelanggan berikut ke dalam Bahasa Indonesia yang alami, akurat, dan sopan. Hanya berikan hasil terjemahan:\n\n{}",
                content
            );
            let chat_msgs = vec![
                crate::services::llm::ChatMessage::system("Anda adalah penerjemah pesan customer support yang handal.".to_string()),
                crate::services::llm::ChatMessage::user(prompt),
            ];
            let opts = crate::services::llm::LlmOptions {
                model: Some("gpt-4o-mini".to_string()),
                temperature: Some(0.2),
                max_tokens: Some(512),
            };

            if let Ok(translated) = llm_provider.chat_completion(&chat_msgs, &opts).await {
                let mut attrs = msg.content_attributes.unwrap_or_else(|| json!({}));
                if !attrs.is_object() {
                    attrs = json!({});
                }
                if let Some(obj) = attrs.as_object_mut() {
                    let mut translations = obj.get("translations").cloned().unwrap_or_else(|| json!({}));
                    if let Some(t_obj) = translations.as_object_mut() {
                        t_obj.insert("id".to_string(), json!(translated.trim()));
                    } else {
                        translations = json!({ "id": translated.trim() });
                    }
                    obj.insert("translations".to_string(), translations);
                }

                let _ = sqlx::query(
                    "UPDATE messages SET content_attributes = $1, updated_at = NOW() WHERE id = $2",
                )
                .bind(&attrs)
                .bind(mid)
                .execute(&db_clone)
                .await;

                let updated_msg = json!({
                    "id": mid,
                    "content": msg.content,
                    "content_attributes": attrs,
                    "conversation_id": conv_id
                });
                event_dispatcher::dispatch_event(&state_clone, account_id as crate::state::AccountId, "message.updated", updated_msg);
            }
        }
    });

    StatusCode::ACCEPTED.into_response()
}

/// POST /api/v1/accounts/:account_id/conversations/:display_id/messages/:id/translate
pub async fn translate_single_message(
    Path((account_id, display_id, message_id)): Path<(i32, i32, i64)>,
    State(state): State<AppState>,
    Json(payload): Json<TranslateMessagePayload>,
) -> impl IntoResponse {
    let conv = match sqlx::query_as::<_, Conversation>(
        "SELECT * FROM conversations WHERE account_id = $1 AND display_id = $2",
    )
    .bind(account_id)
    .bind(display_id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(c)) => c,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Conversation not found" })),
            )
                .into_response();
        }
    };

    let msg = match sqlx::query_as::<_, Message>(
        "SELECT * FROM messages WHERE id = $1 AND conversation_id = $2",
    )
    .bind(message_id)
    .bind(conv.id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(m)) => m,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Message not found" })),
            )
                .into_response();
        }
    };

    let content = msg.content.unwrap_or_default();
    let target_lang = payload.target_language.unwrap_or_else(|| "id".to_string());

    let prompt = format!(
        "Terjemahkan pesan berikut ke dalam kode bahasa '{}'. Hanya berikan teks hasil terjemahan:\n\n{}",
        target_lang, content
    );
    let chat_msgs = vec![
        crate::services::llm::ChatMessage::system("Anda adalah penerjemah pesan customer support yang handal.".to_string()),
        crate::services::llm::ChatMessage::user(prompt),
    ];
    let opts = crate::services::llm::LlmOptions {
        model: Some("gpt-4o-mini".to_string()),
        temperature: Some(0.2),
        max_tokens: Some(512),
    };

    let translated = match crate::services::llm::get_llm_provider(None, None, None, &state.config).chat_completion(&chat_msgs, &opts).await {
        Ok(t) => t.trim().to_string(),
        Err(e) => {
            warn!("Translation failed: {}", e);
            content
        }
    };

    (StatusCode::OK, Json(json!({ "content": translated }))).into_response()
}


