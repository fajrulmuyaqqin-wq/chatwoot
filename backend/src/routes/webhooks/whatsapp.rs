use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use tracing::{info, warn};

use crate::models::channel_whatsapp::ChannelWhatsapp;
use crate::models::contact::{Contact, ContactResponse};
use crate::models::conversation::Conversation;
use crate::models::inbox::Inbox;
use crate::models::message::{Message, MessageSender};
use crate::services::event_dispatcher;
use crate::services::meta_whatsapp::verify_meta_signature;
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

/// GET /webhooks/whatsapp/:phone_number
/// Verifikasi handshake webhook dari Meta Developer Dashboard
pub async fn verify_webhook(
    Path(phone_number): Path<String>,
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

    // Cari channel di database berdasarkan nomor telepon (atau nomor dengan prefix +)
    let phone_with_plus = if phone_number.starts_with('+') {
        phone_number.clone()
    } else {
        format!("+{}", phone_number)
    };

    let channel = sqlx::query_as::<_, ChannelWhatsapp>(
        "SELECT * FROM channel_whatsapp WHERE phone_number = $1 OR phone_number = $2",
    )
    .bind(&phone_number)
    .bind(&phone_with_plus)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();

    let channel_token = channel
        .as_ref()
        .and_then(|c| c.webhook_verify_token())
        .map(|s| s.to_string());

    let global_token = state.config.whatsapp_verify_token.clone();

    let is_valid = match (channel_token, global_token) {
        (Some(c_tok), _) if c_tok == verify_token => true,
        (_, Some(g_tok)) if g_tok == verify_token => true,
        _ => false,
    };

    if is_valid {
        info!("Meta WhatsApp webhook handshake verified for {}", phone_number);
        (
            StatusCode::OK,
            query.challenge.unwrap_or_else(|| "".to_string()),
        )
    } else {
        warn!(
            "Meta WhatsApp webhook verification failed for {}: token mismatch",
            phone_number
        );
        (
            StatusCode::UNAUTHORIZED,
            "Error; wrong verify token".to_string(),
        )
    }
}

/// POST /webhooks/whatsapp/:phone_number
/// Menerima pesan inbound & status update dari Meta Cloud API
pub async fn process_webhook(
    Path(phone_number): Path<String>,
    headers: HeaderMap,
    State(state): State<AppState>,
    body_bytes: Bytes,
) -> impl IntoResponse {
    // 1. Ambil channel WhatsApp
    let phone_with_plus = if phone_number.starts_with('+') {
        phone_number.clone()
    } else {
        format!("+{}", phone_number)
    };

    let channel = match sqlx::query_as::<_, ChannelWhatsapp>(
        "SELECT * FROM channel_whatsapp WHERE phone_number = $1 OR phone_number = $2",
    )
    .bind(&phone_number)
    .bind(&phone_with_plus)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(c)) => c,
        _ => {
            warn!("Received WhatsApp webhook for unknown channel: {}", phone_number);
            return (StatusCode::OK, Json(json!({ "status": "unknown_channel" })));
        }
    };

    // 2. Verifikasi HMAC-SHA256 signature jika app_secret terdefinisi
    let secret = channel
        .app_secret()
        .map(|s| s.to_string())
        .or_else(|| state.config.whatsapp_app_secret.clone());

    if let Some(ref sec) = secret {
        let sig_header = headers
            .get("X-Hub-Signature-256")
            .and_then(|h| h.to_str().ok())
            .unwrap_or_default();

        if sig_header.is_empty() || !verify_meta_signature(sec, &body_bytes, sig_header) {
            warn!("WhatsApp webhook HMAC signature mismatch or missing for {}", phone_number);
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "Invalid or missing HMAC signature" })),
            );
        }
    }

    // 3. Parse JSON payload dari Meta
    let payload: serde_json::Value = match serde_json::from_slice(&body_bytes) {
        Ok(v) => v,
        Err(e) => {
            warn!("Failed to parse WhatsApp webhook JSON payload: {}", e);
            return (StatusCode::BAD_REQUEST, Json(json!({ "error": "Invalid JSON" })));
        }
    };

    // 4. Cari inbox terkait dengan channel_whatsapp ini
    let inbox = match sqlx::query_as::<_, Inbox>(
        "SELECT * FROM inboxes WHERE channel_type = 'Channel::Whatsapp' AND channel_id = $1",
    )
    .bind(channel.id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(ib)) => ib,
        _ => {
            warn!("Inbox not found for channel_whatsapp id {}", channel.id);
            return (StatusCode::OK, Json(json!({ "status": "no_inbox" })));
        }
    };

    let account_id = inbox.account_id;

    // Ambil entry -> changes -> value
    let changes_value = match payload.get("entry")
        .and_then(|e| e.as_array())
        .and_then(|arr| arr.first())
        .and_then(|entry| entry.get("changes"))
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .and_then(|change| change.get("value"))
    {
        Some(v) => v,
        None => {
            // Payload tidak berisi changes yang valid
            return (StatusCode::OK, Json(json!({ "status": "ignored" })));
        }
    };

    // 5. Tangani status delivery updates (sent, delivered, read, failed)
    if let Some(statuses) = changes_value.get("statuses").and_then(|s| s.as_array()) {
        for st in statuses {
            let wamid = match st.get("id").and_then(|id| id.as_str()) {
                Some(id) => id,
                None => continue,
            };
            let status_str = st.get("status").and_then(|s| s.as_str()).unwrap_or("sent");
            let status_int = match status_str {
                "delivered" => 1,
                "read" => 2,
                "failed" => 3,
                _ => 0, // sent
            };

            // Update status pesan di database
            if let Ok(Some(msg)) = sqlx::query_as::<_, Message>(
                "UPDATE messages SET status = $1, updated_at = NOW() WHERE source_id = $2 RETURNING *",
            )
            .bind(status_int)
            .bind(wamid)
            .fetch_optional(&state.db)
            .await
            {
                // Ambil display_id percakapan untuk ActionCable
                #[derive(sqlx::FromRow)]
                struct ConvDisplay {
                    display_id: i32,
                }
                if let Ok(Some(conv)) = sqlx::query_as::<_, ConvDisplay>(
                    "SELECT display_id FROM conversations WHERE id = $1",
                )
                .bind(msg.conversation_id)
                .fetch_optional(&state.db)
                .await
                {
                    let resp = msg.to_response(conv.display_id, None, vec![]);
                    let resp_val = serde_json::to_value(&resp).unwrap_or_default();
                    event_dispatcher::dispatch_event(&state, account_id, "message.updated", resp_val);
                    info!("Updated message {} status to {}", wamid, status_str);
                }
            }
        }
        return (StatusCode::OK, Json(json!({ "status": "statuses_processed" })));
    }

    // 6. Tangani pesan masuk (inbound messages)
    if let Some(messages) = changes_value.get("messages").and_then(|m| m.as_array()) {
        for msg in messages {
            let wamid = match msg.get("id").and_then(|id| id.as_str()) {
                Some(id) => id,
                None => continue,
            };

            // Deduplikasi pesan berdasarkan source_id (wamid)
            let existing = sqlx::query_scalar::<_, i32>(
                "SELECT id FROM messages WHERE source_id = $1",
            )
            .bind(wamid)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten();

            if existing.is_some() {
                info!("WhatsApp message {} already processed, skipping duplicate", wamid);
                continue;
            }

            // Ambil nomor pengirim
            let from_phone = match msg.get("from").and_then(|f| f.as_str()) {
                Some(f) => f,
                None => continue,
            };

            // Format nomor dengan + jika belum ada
            let sender_phone_formatted = if from_phone.starts_with('+') {
                from_phone.to_string()
            } else {
                format!("+{}", from_phone)
            };

            // Ambil nama profil kontak jika ada
            let profile_name = changes_value
                .get("contacts")
                .and_then(|c| c.as_array())
                .and_then(|arr| arr.first())
                .and_then(|contact| contact.get("profile"))
                .and_then(|p| p.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or(&sender_phone_formatted);

            // 7. Cari atau buat Contact
            let contact = match sqlx::query_as::<_, Contact>(
                "SELECT * FROM contacts WHERE account_id = $1 AND (phone_number = $2 OR phone_number = $3)",
            )
            .bind(account_id)
            .bind(from_phone)
            .bind(&sender_phone_formatted)
            .fetch_optional(&state.db)
            .await
            {
                Ok(Some(c)) => c,
                _ => {
                    match sqlx::query_as::<_, Contact>(
                        r#"
                        INSERT INTO contacts (account_id, name, phone_number, created_at, updated_at)
                        VALUES ($1, $2, $3, NOW(), NOW())
                        RETURNING *
                        "#,
                    )
                    .bind(account_id)
                    .bind(profile_name)
                    .bind(&sender_phone_formatted)
                    .fetch_one(&state.db)
                    .await
                    {
                        Ok(c) => c,
                        Err(e) => {
                            warn!("Failed to create contact for WhatsApp: {}", e);
                            continue;
                        }
                    }
                }
            };

            // 8. Cari atau buat ContactInbox
            #[derive(sqlx::FromRow)]
            struct ContactInboxRow {
                id: i32,
            }
            let contact_inbox_id = match sqlx::query_as::<_, ContactInboxRow>(
                "SELECT id FROM contact_inboxes WHERE inbox_id = $1 AND contact_id = $2",
            )
            .bind(inbox.id)
            .bind(contact.id)
            .fetch_optional(&state.db)
            .await
            {
                Ok(Some(ci)) => ci.id,
                _ => {
                    match sqlx::query_as::<_, ContactInboxRow>(
                        r#"
                        INSERT INTO contact_inboxes (inbox_id, contact_id, source_id, created_at, updated_at)
                        VALUES ($1, $2, $3, NOW(), NOW())
                        RETURNING id
                        "#,
                    )
                    .bind(inbox.id)
                    .bind(contact.id)
                    .bind(from_phone)
                    .fetch_one(&state.db)
                    .await
                    {
                        Ok(ci) => ci.id,
                        Err(e) => {
                            warn!("Failed to create contact_inbox for WhatsApp: {}", e);
                            continue;
                        }
                    }
                }
            };

            // 9. Cari percakapan aktif atau buat percakapan baru
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
            .bind(contact.id as i64)
            .fetch_optional(&state.db)
            .await
            {
                Ok(Some(c)) => c,
                _ => {
                    // Buat percakapan baru
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
                    .bind(contact.id as i64)
                    .bind(contact_inbox_id as i64)
                    .bind(next_display_id)
                    .bind(new_uuid)
                    .fetch_one(&state.db)
                    .await
                    {
                        Ok(conv) => conv,
                        Err(e) => {
                            warn!("Failed to create conversation for WhatsApp: {}", e);
                            continue;
                        }
                    }
                }
            };

            // 10. Ekstrak konten pesan
            let msg_type = msg.get("type").and_then(|t| t.as_str()).unwrap_or("text");
            let content_str = match msg_type {
                "text" => msg
                    .get("text")
                    .and_then(|t| t.get("body"))
                    .and_then(|b| b.as_str())
                    .unwrap_or("")
                    .to_string(),
                "image" => msg
                    .get("image")
                    .and_then(|i| i.get("caption"))
                    .and_then(|c| c.as_str())
                    .unwrap_or("[Gambar]")
                    .to_string(),
                "audio" => "[Audio]".to_string(),
                "video" => msg
                    .get("video")
                    .and_then(|v| v.get("caption"))
                    .and_then(|c| c.as_str())
                    .unwrap_or("[Video]")
                    .to_string(),
                "document" => msg
                    .get("document")
                    .and_then(|d| d.get("caption"))
                    .and_then(|c| c.as_str())
                    .unwrap_or("[Dokumen]")
                    .to_string(),
                "location" => "[Lokasi]".to_string(),
                _ => "[Pesan WhatsApp]".to_string(),
            };

            // 11. Simpan pesan ke database
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
            .bind(wamid)
            .bind(contact.id as i64)
            .fetch_one(&state.db)
            .await
            {
                Ok(m) => m,
                Err(e) => {
                    warn!("Failed to insert inbound WhatsApp message: {}", e);
                    continue;
                }
            };

            // Perbarui waktu aktivitas percakapan
            let _ = sqlx::query(
                "UPDATE conversations SET last_activity_at = NOW(), updated_at = NOW() WHERE id = $1",
            )
            .bind(conversation.id)
            .execute(&state.db)
            .await;

            // 12. Broadcast via ActionCable
            let contact_resp = ContactResponse::from(contact);
            let sender = Some(MessageSender {
                id: contact_resp.id as i64,
                name: contact_resp.name.clone(),
                avatar_url: "".to_string(),
                r#type: "contact".to_string(),
            });

            let msg_resp = new_msg.to_response(conversation.display_id, sender, vec![]);
            let msg_resp_val = serde_json::to_value(&msg_resp).unwrap_or_default();

            if is_new_conversation {
                let conv_resp = conversation.to_response(
                    Some(contact_resp),
                    "Channel::Whatsapp".to_string(),
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

            // Emit message.created
            event_dispatcher::dispatch_event(
                &state,
                account_id,
                "message.created",
                msg_resp_val,
            );

            info!(
                "Successfully processed inbound WhatsApp message {} from {}",
                wamid, sender_phone_formatted
            );
        }
    }

    (StatusCode::OK, Json(json!({ "status": "success" })))
}
