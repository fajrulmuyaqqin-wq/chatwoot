use axum::{
    body::Bytes,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use tracing::{error, info, warn};

use crate::models::conversation::Conversation;
use crate::models::inbox::Inbox;
use crate::models::message::Message;
use crate::services::tokopedia::verify_tokopedia_signature;
use crate::state::{AppState, CableBroadcastMsg};

#[derive(Debug, Deserialize)]
pub struct TokopediaWebhookQuery {
    pub challenge: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TokoSender {
    pub user_id: i64,
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TokoChatData {
    pub msg_id: String,
    pub sender: TokoSender,
    pub message: String,
    pub timestamp: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct TokopediaWebhookPayload {
    pub event: String,
    pub shop_id: i64,
    pub data: TokoChatData,
}

pub async fn handle_tokopedia_handshake(
    Query(query): Query<TokopediaWebhookQuery>,
) -> impl IntoResponse {
    let challenge = query.challenge.unwrap_or_else(|| "tokopedia_webhook_active".to_string());
    (StatusCode::OK, challenge)
}

pub async fn handle_tokopedia_webhook(
    headers: HeaderMap,
    State(state): State<AppState>,
    body_bytes: Bytes,
) -> impl IntoResponse {
    // 1. Verifikasi tanda tangan HMAC jika TOKOPEDIA_APP_SECRET dikonfigurasi
    let secret = std::env::var("TOKOPEDIA_APP_SECRET").ok();
    if let Some(sec) = secret {
        let sig_opt = headers.get("X-Tkpd-Signature").and_then(|h| h.to_str().ok());
        let is_valid = sig_opt.map(|sig| verify_tokopedia_signature(&sec, &body_bytes, sig)).unwrap_or(false);
        if !is_valid {
            warn!("Tanda tangan webhook Tokopedia tidak valid atau tidak disediakan!");
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "Invalid or missing Tokopedia signature" })),
            );
        }
    }

    let payload: TokopediaWebhookPayload = match serde_json::from_slice(&body_bytes) {
        Ok(p) => p,
        Err(e) => {
            error!("Gagal parse JSON payload Tokopedia: {:?}", e);
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "Invalid JSON payload" })),
            );
        }
    };

    if payload.event != "chat.receive" {
        return (
            StatusCode::OK,
            Json(json!({ "message": "Ignored non-chat event" })),
        );
    }

    // 2. Temukan inbox Tokopedia
    let inbox_name = format!("Tokopedia Shop {}", payload.shop_id);
    let inbox = match sqlx::query_as::<_, Inbox>(
        "SELECT * FROM inboxes WHERE name = $1 LIMIT 1"
    )
    .bind(&inbox_name)
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None)
    {
        Some(i) => i,
        None => {
            // Auto create inbox jika belum ada
            let account_id = 1;
            match sqlx::query_as::<_, Inbox>(
                r#"
                INSERT INTO inboxes (name, account_id, channel_type, channel_id, created_at, updated_at)
                VALUES ($1, $2, 'Channel::Api', 0, NOW(), NOW())
                RETURNING *
                "#
            )
            .bind(&inbox_name)
            .bind(account_id)
            .fetch_one(&state.db)
            .await
            {
                Ok(i) => i,
                Err(e) => {
                    error!("Gagal membuat inbox Tokopedia: {:?}", e);
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({ "error": "Failed to create Tokopedia inbox" })),
                    );
                }
            }
        }
    };

    let account_id = inbox.account_id;
    let buyer_id = payload.data.sender.user_id;
    let buyer_identifier = format!("tokopedia:{}", buyer_id);
    let buyer_name = payload.data.sender.name.unwrap_or_else(|| format!("Tokopedia Buyer {}", buyer_id));

    // 3. Auto create/find Contact
    let contact_id = match sqlx::query_scalar::<_, i32>(
        "SELECT id FROM contacts WHERE account_id = $1 AND identifier = $2"
    )
    .bind(account_id)
    .bind(&buyer_identifier)
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None)
    {
        Some(cid) => cid,
        None => {
            sqlx::query_scalar::<_, i32>(
                r#"
                INSERT INTO contacts (account_id, name, identifier, created_at, updated_at)
                VALUES ($1, $2, $3, NOW(), NOW())
                RETURNING id
                "#
            )
            .bind(account_id)
            .bind(&buyer_name)
            .bind(&buyer_identifier)
            .fetch_one(&state.db)
            .await
            .unwrap_or(1)
        }
    };

    // 4. Pastikan contact_inbox ada
    let _ = sqlx::query(
        r#"
        INSERT INTO contact_inboxes (contact_id, inbox_id, source_id, created_at, updated_at)
        VALUES ($1, $2, $3, NOW(), NOW())
        ON CONFLICT (inbox_id, source_id) DO NOTHING
        "#
    )
    .bind(contact_id)
    .bind(inbox.id)
    .bind(&buyer_identifier)
    .execute(&state.db)
    .await;

    // 5. Cari atau buat conversation
    let conv_opt = sqlx::query_as::<_, Conversation>(
        r#"
        SELECT * FROM conversations
        WHERE account_id = $1 AND inbox_id = $2 AND contact_id = $3
        ORDER BY created_at DESC
        LIMIT 1
        "#
    )
    .bind(account_id)
    .bind(inbox.id)
    .bind(contact_id)
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None);

    let conversation_id = match conv_opt {
        Some(c) => {
            if c.status == 1 {
                let _ = sqlx::query("UPDATE conversations SET status = 0, updated_at = NOW() WHERE id = $1")
                    .bind(c.id)
                    .execute(&state.db)
                    .await;
            }
            c.id
        }
        None => {
            let max_display_id: i32 = sqlx::query_scalar(
                "SELECT COALESCE(MAX(display_id), 0) FROM conversations WHERE account_id = $1"
            )
            .bind(account_id)
            .fetch_one(&state.db)
            .await
            .unwrap_or(0);

            sqlx::query_scalar::<_, i32>(
                r#"
                INSERT INTO conversations (account_id, inbox_id, status, contact_id, display_id, created_at, updated_at)
                VALUES ($1, $2, 0, $3, $4, NOW(), NOW())
                RETURNING id
                "#
            )
            .bind(account_id)
            .bind(inbox.id)
            .bind(contact_id)
            .bind(max_display_id + 1)
            .fetch_one(&state.db)
            .await
            .unwrap_or(1)
        }
    };

    // 6. Deduplikasi pesan berdasarkan msg_id
    let source_id = payload.data.msg_id;
    let existing_msg = sqlx::query_scalar::<_, i32>(
        "SELECT id FROM messages WHERE account_id = $1 AND source_id = $2 LIMIT 1"
    )
    .bind(account_id)
    .bind(&source_id)
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None);

    if let Some(existing_id) = existing_msg {
        return (
            StatusCode::OK,
            Json(json!({ "status": "duplicate", "message_id": existing_id })),
        );
    }

    // 7. Simpan message ke database
    let message = match sqlx::query_as::<_, Message>(
        r#"
        INSERT INTO messages (
            content, account_id, inbox_id, conversation_id, message_type,
            status, source_id, sender_type, sender_id, created_at, updated_at
        )
        VALUES ($1, $2, $3, $4, 0, 0, $5, 'Contact', $6, NOW(), NOW())
        RETURNING *
        "#
    )
    .bind(&payload.data.message)
    .bind(account_id)
    .bind(inbox.id)
    .bind(conversation_id)
    .bind(&source_id)
    .bind(contact_id)
    .fetch_one(&state.db)
    .await
    {
        Ok(m) => m,
        Err(e) => {
            error!("Gagal menyimpan pesan Tokopedia: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Failed to store message" })),
            );
        }
    };

    info!(
        account_id = account_id,
        inbox_id = inbox.id,
        conversation_id = conversation_id,
        message_id = message.id,
        shop_id = payload.shop_id,
        "Berhasil memproses pesan masuk Tokopedia"
    );

    // 8. Broadcast ActionCable
    let _ = state.broadcast_tx.send(CableBroadcastMsg {
        account_id,
        event: "message.created".to_string(),
        data: json!({
            "id": message.id,
            "content": message.content,
            "message_type": message.message_type,
            "conversation_id": conversation_id,
            "inbox_id": inbox.id,
            "sender": {
                "id": contact_id,
                "name": buyer_name,
                "type": "contact"
            }
        }),
    });

    (
        StatusCode::OK,
        Json(json!({
            "success": true,
            "conversation_id": conversation_id,
            "message_id": message.id
        })),
    )
}
