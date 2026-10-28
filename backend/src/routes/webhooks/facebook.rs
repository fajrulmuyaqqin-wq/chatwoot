use axum::{
    body::Bytes,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use tracing::{info, warn};

use crate::models::channel_facebook::ChannelFacebookPage;
use crate::models::contact::{Contact, ContactResponse};
use crate::models::conversation::Conversation;
use crate::models::inbox::Inbox;
use crate::models::message::{Message, MessageSender};
use crate::services::event_dispatcher;
use crate::services::meta_facebook::verify_meta_signature;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct MetaVerifyQuery {
    #[serde(rename = "hub.mode")]
    pub mode: Option<String>,
    #[serde(rename = "hub.verify_token")]
    pub verify_token: Option<String>,
    #[serde(rename = "hub.challenge")]
    pub challenge: Option<String>,
}

/// GET /webhooks/facebook & GET /bot
/// Verifikasi handshake webhook dari Meta Developer Dashboard untuk Facebook Page Messenger
pub async fn verify_webhook(
    Query(query): Query<MetaVerifyQuery>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if query.mode.as_deref() != Some("subscribe") {
        return (StatusCode::BAD_REQUEST, "Invalid mode".to_string());
    }

    let verify_token = match query.verify_token {
        Some(token) => token,
        None => return (StatusCode::UNAUTHORIZED, "Missing verify token".to_string()),
    };

    let global_token = state.config.fb_verify_token.clone();

    let is_valid = match global_token {
        Some(ref g_tok) if g_tok == &verify_token => true,
        None if verify_token == "chatwoot_fb_token" || verify_token == "chatwoot_facebook_token" => true,
        _ => false,
    };

    if is_valid {
        info!("Meta Facebook Messenger webhook handshake successfully verified");
        (
            StatusCode::OK,
            query.challenge.unwrap_or_else(|| "".to_string()),
        )
    } else {
        warn!("Meta Facebook webhook verification failed: token mismatch");
        (
            StatusCode::UNAUTHORIZED,
            "Error; wrong verify token".to_string(),
        )
    }
}

/// POST /webhooks/facebook & POST /bot
/// Menerima pesan inbound dari Facebook Messenger
pub async fn process_webhook(
    headers: HeaderMap,
    State(state): State<AppState>,
    body_bytes: Bytes,
) -> impl IntoResponse {
    // 1. Verifikasi HMAC-SHA256 signature jika app_secret terdefinisi
    if let Some(ref sec) = state.config.fb_app_secret {
        let sig_header = headers
            .get("X-Hub-Signature-256")
            .and_then(|h| h.to_str().ok())
            .unwrap_or_default();

        if sig_header.is_empty() || !verify_meta_signature(sec, &body_bytes, sig_header) {
            warn!("Facebook webhook HMAC signature mismatch or missing");
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "Invalid or missing signature" })),
            );
        }
    }

    // 2. Parse JSON payload dari Meta
    let payload: serde_json::Value = match serde_json::from_slice(&body_bytes) {
        Ok(v) => v,
        Err(e) => {
            warn!("Failed to parse Facebook webhook JSON payload: {}", e);
            return (StatusCode::BAD_REQUEST, Json(json!({ "error": "Invalid JSON" })));
        }
    };

    let object = payload.get("object").and_then(|o| o.as_str()).unwrap_or("");
    if object != "page" {
        warn!("Facebook webhook object is not 'page': {}", object);
        return (StatusCode::UNPROCESSABLE_ENTITY, Json(json!({ "error": "Invalid object type" })));
    }

    let entries = match payload.get("entry").and_then(|e| e.as_array()) {
        Some(arr) => arr,
        None => return (StatusCode::OK, Json(json!({ "status": "no_entries" }))),
    };

    for entry in entries {
        let entry_page_id = entry.get("id").and_then(|id| id.as_str()).unwrap_or("");

        let messaging_events = entry
            .get("messaging")
            .or_else(|| entry.get("standby"))
            .and_then(|m| m.as_array());

        let messaging_list = match messaging_events {
            Some(list) => list,
            None => continue,
        };

        for messaging in messaging_list {
            let is_echo = messaging
                .get("message")
                .and_then(|m| m.get("is_echo"))
                .and_then(|e| e.as_bool())
                .unwrap_or(false);

            if is_echo {
                info!("Facebook webhook echo event detected, skipping");
                continue;
            }

            let sender_id = match messaging.get("sender").and_then(|s| s.get("id")).and_then(|id| id.as_str()) {
                Some(id) => id,
                None => continue,
            };

            let recipient_id = match messaging.get("recipient").and_then(|r| r.get("id")).and_then(|id| id.as_str()) {
                Some(id) => id,
                None => entry_page_id,
            };

            let message_obj = match messaging.get("message") {
                Some(m) => m,
                None => continue,
            };

            let mid = match message_obj.get("mid").and_then(|id| id.as_str()) {
                Some(id) => id,
                None => continue,
            };

            // Deduplikasi
            let existing = sqlx::query_scalar::<_, i32>(
                "SELECT id FROM messages WHERE source_id = $1",
            )
            .bind(mid)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten();

            if existing.is_some() {
                info!("Facebook message {} already processed, skipping duplicate", mid);
                continue;
            }

            // Cari channel_facebook_pages
            let channel = match sqlx::query_as::<_, ChannelFacebookPage>(
                "SELECT * FROM channel_facebook_pages WHERE page_id = $1 OR page_id = $2",
            )
            .bind(recipient_id)
            .bind(entry_page_id)
            .fetch_optional(&state.db)
            .await
            {
                Ok(Some(c)) => c,
                _ => {
                    warn!("Received FB Messenger for unknown page_id: {}", recipient_id);
                    continue;
                }
            };

            // Cari Inbox
            let inbox = match sqlx::query_as::<_, Inbox>(
                "SELECT * FROM inboxes WHERE channel_type = 'Channel::FacebookPage' AND channel_id = $1",
            )
            .bind(channel.id)
            .fetch_optional(&state.db)
            .await
            {
                Ok(Some(ib)) => ib,
                _ => {
                    warn!("Inbox not found for channel_facebook_pages id {}", channel.id);
                    continue;
                }
            };

            let account_id = inbox.account_id;

            // Cari atau buat Contact dan ContactInbox
            #[derive(sqlx::FromRow)]
            struct ContactInboxRow {
                id: i32,
                contact_id: i32,
            }

            let (contact_id, contact_inbox_id, contact_name) = match sqlx::query_as::<_, ContactInboxRow>(
                "SELECT id, contact_id FROM contact_inboxes WHERE inbox_id = $1 AND source_id = $2",
            )
            .bind(inbox.id)
            .bind(sender_id)
            .fetch_optional(&state.db)
            .await
            {
                Ok(Some(ci)) => {
                    let c_name = sqlx::query_scalar::<_, String>(
                        "SELECT name FROM contacts WHERE id = $1",
                    )
                    .bind(ci.contact_id)
                    .fetch_optional(&state.db)
                    .await
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| format!("Facebook User {}", sender_id));

                    (ci.contact_id, ci.id, c_name)
                }
                _ => {
                    let default_name = format!("Facebook User {}", &sender_id[..sender_id.len().min(8)]);
                    let new_contact = match sqlx::query_as::<_, Contact>(
                        r#"
                        INSERT INTO contacts (account_id, name, created_at, updated_at)
                        VALUES ($1, $2, NOW(), NOW())
                        RETURNING *
                        "#,
                    )
                    .bind(account_id)
                    .bind(&default_name)
                    .fetch_one(&state.db)
                    .await
                    {
                        Ok(c) => c,
                        Err(e) => {
                            warn!("Failed to create contact for Facebook: {}", e);
                            continue;
                        }
                    };

                    let new_ci = match sqlx::query_as::<_, ContactInboxRow>(
                        r#"
                        INSERT INTO contact_inboxes (inbox_id, contact_id, source_id, created_at, updated_at)
                        VALUES ($1, $2, $3, NOW(), NOW())
                        RETURNING id, contact_id
                        "#,
                    )
                    .bind(inbox.id)
                    .bind(new_contact.id)
                    .bind(sender_id)
                    .fetch_one(&state.db)
                    .await
                    {
                        Ok(ci) => ci,
                        Err(e) => {
                            warn!("Failed to create contact_inbox for Facebook: {}", e);
                            continue;
                        }
                    };

                    let contact_saved_name = new_contact.name.unwrap_or(default_name);
                    (new_contact.id, new_ci.id, contact_saved_name)
                }
            };

            // Cari atau buat percakapan
            let mut is_new_conversation = false;
            let conversation = match sqlx::query_as::<_, Conversation>(
                r#"
                SELECT * FROM conversations
                WHERE account_id = $1 AND inbox_id = $2 AND contact_id = $3 AND status != 1
                ORDER BY id DESC LIMIT 1
                "#,
            )
            .bind(account_id)
            .bind(inbox.id)
            .bind(contact_id as i64)
            .fetch_optional(&state.db)
            .await
            {
                Ok(Some(c)) => c,
                _ => {
                    is_new_conversation = true;
                    let next_display_id = sqlx::query_scalar::<_, i32>(
                        "SELECT COALESCE(MAX(display_id), 0) + 1 FROM conversations WHERE account_id = $1",
                    )
                    .bind(account_id)
                    .fetch_one(&state.db)
                    .await
                    .unwrap_or(1);

                    let new_uuid = uuid::Uuid::new_v4();

                    match sqlx::query_as::<_, Conversation>(
                        r#"
                        INSERT INTO conversations (
                            account_id, inbox_id, status, assignee_id, contact_id,
                            contact_inbox_id, display_id, uuid, last_activity_at,
                            created_at, updated_at
                        )
                        VALUES ($1, $2, 0, NULL, $3, $4, $5, $6, NOW(), NOW(), NOW())
                        RETURNING *
                        "#,
                    )
                    .bind(account_id)
                    .bind(inbox.id)
                    .bind(contact_id as i64)
                    .bind(contact_inbox_id as i64)
                    .bind(next_display_id)
                    .bind(new_uuid)
                    .fetch_one(&state.db)
                    .await
                    {
                        Ok(conv) => conv,
                        Err(e) => {
                            warn!("Failed to create conversation for Facebook: {}", e);
                            continue;
                        }
                    }
                }
            };

            let content_str = message_obj
                .get("text")
                .and_then(|t| t.as_str())
                .unwrap_or("[Pesan Facebook]")
                .to_string();

            // Simpan pesan
            let new_msg = match sqlx::query_as::<_, Message>(
                r#"
                INSERT INTO messages (
                    account_id, inbox_id, conversation_id, content, message_type,
                    private, status, content_type, source_id, sender_type,
                    sender_id, created_at, updated_at
                )
                VALUES (
                    $1, $2, $3, $4, 0,
                    false, 1, 0, $5, 'Contact',
                    $6, NOW(), NOW()
                )
                RETURNING *
                "#,
            )
            .bind(account_id)
            .bind(inbox.id)
            .bind(conversation.id)
            .bind(&content_str)
            .bind(mid)
            .bind(contact_id as i64)
            .fetch_one(&state.db)
            .await
            {
                Ok(m) => m,
                Err(e) => {
                    warn!("Failed to insert inbound Facebook message: {}", e);
                    continue;
                }
            };

            let _ = sqlx::query(
                "UPDATE conversations SET last_activity_at = NOW(), updated_at = NOW() WHERE id = $1",
            )
            .bind(conversation.id)
            .execute(&state.db)
            .await;

            // Broadcast via ActionCable
            let contact = sqlx::query_as::<_, Contact>("SELECT * FROM contacts WHERE id = $1")
                .bind(contact_id)
                .fetch_optional(&state.db)
                .await
                .ok()
                .flatten();

            let contact_resp = contact.map(ContactResponse::from).unwrap_or_else(|| ContactResponse {
                id: contact_id,
                name: contact_name.clone(),
                email: None,
                phone_number: None,
                identifier: None,
                thumbnail: "".to_string(),
                additional_attributes: serde_json::json!({}),
                custom_attributes: serde_json::json!({}),
                created_at: chrono::Utc::now().timestamp(),
                company_id: None,
            });

            let sender = Some(MessageSender {
                id: contact_id as i64,
                name: contact_name,
                avatar_url: "".to_string(),
                r#type: "contact".to_string(),
            });

            let msg_resp = new_msg.to_response(conversation.display_id, sender, vec![]);
            let msg_resp_val = serde_json::to_value(&msg_resp).unwrap_or_default();

            if is_new_conversation {
                let conv_resp = conversation.to_response(
                    Some(contact_resp),
                    "Channel::FacebookPage".to_string(),
                    None,
                    Some(msg_resp.clone()),
                    1,
                );
                let conv_resp_val = serde_json::to_value(&conv_resp).unwrap_or_default();
                event_dispatcher::dispatch_event(
                    &state,
                    account_id,
                    "conversation.created",
                    conv_resp_val,
                );
            }

            event_dispatcher::dispatch_event(
                &state,
                account_id,
                "message.created",
                msg_resp_val,
            );

            info!("Successfully processed inbound Facebook Messenger {} from {}", mid, sender_id);
        }
    }

    (StatusCode::OK, Json(json!({ "status": "success" })))
}
