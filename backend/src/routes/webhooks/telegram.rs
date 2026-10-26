use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde_json::json;
use tracing::{info, warn};

use crate::models::channel_telegram::ChannelTelegram;
use crate::models::contact::{Contact, ContactResponse};
use crate::models::conversation::Conversation;
use crate::models::inbox::Inbox;
use crate::models::message::{Message, MessageSender};
use crate::services::event_dispatcher;
use crate::state::AppState;

/// POST /webhooks/telegram/:bot_token
/// Menerima webhook event dari Telegram Bot API
pub async fn process_webhook(
    Path(bot_token): Path<String>,
    State(state): State<AppState>,
    Json(payload): Json<serde_json::Value>,
) -> impl IntoResponse {
    // 1. Cari channel_telegram berdasarkan bot_token
    let channel = match sqlx::query_as::<_, ChannelTelegram>(
        "SELECT * FROM channel_telegram WHERE bot_token = $1",
    )
    .bind(&bot_token)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(c)) => c,
        _ => {
            warn!("Telegram webhook received for unknown bot_token: {}", bot_token);
            return (StatusCode::NOT_FOUND, Json(json!({ "error": "Bot token not found" })));
        }
    };

    // 2. Cari Inbox terkait channel_telegram ini
    let inbox = match sqlx::query_as::<_, Inbox>(
        "SELECT * FROM inboxes WHERE channel_type = 'Channel::Telegram' AND channel_id = $1",
    )
    .bind(channel.id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(ib)) => ib,
        _ => {
            warn!("Inbox not found for channel_telegram id {}", channel.id);
            return (StatusCode::OK, Json(json!({ "status": "no_inbox" })));
        }
    };

    let account_id = inbox.account_id;

    // 3. Ekstrak pesan dari payload Telegram (message atau callback_query)
    let message_obj = match payload.get("message") {
        Some(m) => m,
        None => {
            // Bisa berupa update lain seperti callback_query atau my_chat_member
            return (StatusCode::OK, Json(json!({ "ok": true })));
        }
    };

    let message_id_val = match message_obj.get("message_id") {
        Some(id) => id.to_string(),
        None => return (StatusCode::OK, Json(json!({ "ok": true }))),
    };

    // Deduplikasi pesan
    let existing = sqlx::query_scalar::<_, i32>(
        "SELECT id FROM messages WHERE source_id = $1",
    )
    .bind(&message_id_val)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();

    if existing.is_some() {
        info!("Telegram message {} already processed, skipping duplicate", message_id_val);
        return (StatusCode::OK, Json(json!({ "ok": true })));
    }

    // Ambil data pengirim (from)
    let from_obj = match message_obj.get("from") {
        Some(f) => f,
        None => return (StatusCode::OK, Json(json!({ "ok": true }))),
    };

    let telegram_user_id = from_obj
        .get("id")
        .map(|id| id.to_string())
        .unwrap_or_default();

    let first_name = from_obj.get("first_name").and_then(|f| f.as_str()).unwrap_or("");
    let last_name = from_obj.get("last_name").and_then(|l| l.as_str()).unwrap_or("");
    let full_name = format!("{} {}", first_name, last_name).trim().to_string();
    let contact_name = if full_name.is_empty() {
        from_obj
            .get("username")
            .and_then(|u| u.as_str())
            .unwrap_or("Telegram User")
            .to_string()
    } else {
        full_name
    };

    // 4. Cari atau buat Contact dan ContactInbox
    #[derive(sqlx::FromRow)]
    struct ContactInboxRow {
        id: i32,
        contact_id: i32,
    }

    let (contact_id, contact_inbox_id) = match sqlx::query_as::<_, ContactInboxRow>(
        "SELECT id, contact_id FROM contact_inboxes WHERE inbox_id = $1 AND source_id = $2",
    )
    .bind(inbox.id)
    .bind(&telegram_user_id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(ci)) => (ci.contact_id, ci.id),
        _ => {
            let new_contact = match sqlx::query_as::<_, Contact>(
                r#"
                INSERT INTO contacts (account_id, name, created_at, updated_at)
                VALUES ($1, $2, NOW(), NOW())
                RETURNING *
                "#,
            )
            .bind(account_id)
            .bind(&contact_name)
            .fetch_one(&state.db)
            .await
            {
                Ok(c) => c,
                Err(e) => {
                    warn!("Failed to create contact for Telegram: {}", e);
                    return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Contact creation failed" })));
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
            .bind(&telegram_user_id)
            .fetch_one(&state.db)
            .await
            {
                Ok(ci) => ci,
                Err(e) => {
                    warn!("Failed to create contact_inbox for Telegram: {}", e);
                    return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "ContactInbox creation failed" })));
                }
            };

            (new_contact.id, new_ci.id)
        }
    };

    // 5. Cari percakapan aktif atau buat percakapan baru
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
                    warn!("Failed to create conversation for Telegram: {}", e);
                    return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Conversation creation failed" })));
                }
            }
        }
    };

    let content_str = message_obj
        .get("text")
        .and_then(|t| t.as_str())
        .unwrap_or("[Pesan Telegram]")
        .to_string();

    // 6. Simpan pesan
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
    .bind(&message_id_val)
    .bind(contact_id as i64)
    .fetch_one(&state.db)
    .await
    {
        Ok(m) => m,
        Err(e) => {
            warn!("Failed to insert inbound Telegram message: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Message insert failed" })));
        }
    };

    let _ = sqlx::query(
        "UPDATE conversations SET last_activity_at = NOW(), updated_at = NOW() WHERE id = $1",
    )
    .bind(conversation.id)
    .execute(&state.db)
    .await;

    // 7. Siarkan via ActionCable
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
            "Channel::Telegram".to_string(),
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

    info!("Successfully processed inbound Telegram message {} from {}", message_id_val, telegram_user_id);
    (StatusCode::OK, Json(json!({ "ok": true })))
}
