use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use tracing::{error, info};

use crate::models::inbox::Inbox;
use crate::models::message::Message;
use crate::services::automation_rules_matcher::evaluate_and_execute_rules;
use crate::state::{AppState, CableBroadcastMsg};

#[derive(Debug, Deserialize)]
pub struct EmailWebhookQuery {
    pub inbox_id: Option<i32>,
    pub account_id: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct InboundEmailPayload {
    pub from: String,
    pub from_name: Option<String>,
    pub to: String,
    pub subject: Option<String>,
    pub text: Option<String>,
    pub html: Option<String>,
    pub message_id: Option<String>,
    pub in_reply_to: Option<String>,
    pub references: Option<String>,
}

pub async fn handle_inbound_email(
    Query(query): Query<EmailWebhookQuery>,
    State(state): State<AppState>,
    Json(payload): Json<InboundEmailPayload>,
) -> impl IntoResponse {
    let clean_to = payload.to.trim().to_lowercase();
    let clean_from = payload.from.trim().to_lowercase();

    if clean_from.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "'from' email address is required" })),
        );
    }

    // 1. Temukan inbox email berdasarkan email penerima (to) atau parameter query
    let inbox_opt = if let Some(inbox_id) = query.inbox_id {
        sqlx::query_as::<_, Inbox>("SELECT * FROM inboxes WHERE id = $1")
            .bind(inbox_id)
            .fetch_optional(&state.db)
            .await
            .unwrap_or(None)
    } else {
        let by_email = sqlx::query_as::<_, Inbox>(
            r#"
            SELECT i.* FROM inboxes i
            JOIN channel_email ce ON ce.id = i.channel_id AND i.channel_type = 'Channel::Email'
            WHERE LOWER(ce.email) = $1 OR LOWER(ce.forward_to_email) = $1
            LIMIT 1
            "#
        )
        .bind(&clean_to)
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None);

        if by_email.is_some() {
            by_email
        } else {
            // Fallback: Inbox email pertama jika ada account_id
            if let Some(acc_id) = query.account_id {
                sqlx::query_as::<_, Inbox>(
                    "SELECT * FROM inboxes WHERE account_id = $1 AND channel_type = 'Channel::Email' LIMIT 1"
                )
                .bind(acc_id)
                .fetch_optional(&state.db)
                .await
                .unwrap_or(None)
            } else {
                sqlx::query_as::<_, Inbox>(
                    "SELECT * FROM inboxes WHERE channel_type = 'Channel::Email' LIMIT 1"
                )
                .fetch_optional(&state.db)
                .await
                .unwrap_or(None)
            }
        }
    };

    let inbox = match inbox_opt {
        Some(i) => i,
        None => {
            let acc_id = query.account_id.unwrap_or(1);
            let channel_id = sqlx::query_scalar::<_, i32>(
                r#"
                INSERT INTO channel_email (account_id, email, forward_to_email, created_at, updated_at)
                VALUES ($1, $2, $2, NOW(), NOW())
                ON CONFLICT (email) DO UPDATE SET updated_at = NOW()
                RETURNING id
                "#
            )
            .bind(acc_id)
            .bind(&clean_to)
            .fetch_one(&state.db)
            .await
            .unwrap_or(1);

            let new_inbox = sqlx::query_as::<_, Inbox>(
                r#"
                INSERT INTO inboxes (name, account_id, channel_type, channel_id, created_at, updated_at)
                VALUES ($1, $2, 'Channel::Email', $3, NOW(), NOW())
                RETURNING *
                "#
            )
            .bind(format!("Email Support ({})", clean_to))
            .bind(acc_id)
            .bind(channel_id)
            .fetch_one(&state.db)
            .await;

            match new_inbox {
                Ok(i) => i,
                Err(e) => {
                    error!("Gagal membuat email inbox otomatis: {:?}", e);
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({ "error": "Failed to create email inbox" })),
                    );
                }
            }
        }
    };

    let account_id = inbox.account_id;
    let customer_name = payload.from_name.clone().unwrap_or_else(|| {
        clean_from
            .split('@')
            .next()
            .unwrap_or("Customer")
            .to_string()
    });

    // 2. Auto-create atau temukan Contact berdasarkan email
    let contact_id = match sqlx::query_scalar::<_, i32>(
        "SELECT id FROM contacts WHERE account_id = $1 AND LOWER(email) = $2"
    )
    .bind(account_id)
    .bind(&clean_from)
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None)
    {
        Some(cid) => cid,
        None => {
            match sqlx::query_scalar::<_, i32>(
                r#"
                INSERT INTO contacts (account_id, name, email, created_at, updated_at)
                VALUES ($1, $2, $3, NOW(), NOW())
                RETURNING id
                "#
            )
            .bind(account_id)
            .bind(&customer_name)
            .bind(&clean_from)
            .fetch_one(&state.db)
            .await
            {
                Ok(new_id) => new_id,
                Err(e) => {
                    error!("Gagal membuat contact untuk email inbound: {:?}", e);
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({ "error": "Failed to create contact" })),
                    );
                }
            }
        }
    };

    // 3. Pastikan contact_inbox ada
    let _ = sqlx::query(
        r#"
        INSERT INTO contact_inboxes (contact_id, inbox_id, source_id, created_at, updated_at)
        VALUES ($1, $2, $3, NOW(), NOW())
        ON CONFLICT (inbox_id, source_id) DO NOTHING
        "#
    )
    .bind(contact_id)
    .bind(inbox.id)
    .bind(&clean_from)
    .execute(&state.db)
    .await;

    // 4. Threading email: periksa In-Reply-To atau References
    let mut target_conv_id: Option<i32> = None;

    if let Some(ref in_reply) = payload.in_reply_to {
        let trimmed = in_reply.trim();
        if !trimmed.is_empty() {
            target_conv_id = sqlx::query_scalar::<_, i32>(
                "SELECT conversation_id FROM messages WHERE account_id = $1 AND source_id = $2 LIMIT 1"
            )
            .bind(account_id)
            .bind(trimmed)
            .fetch_optional(&state.db)
            .await
            .unwrap_or(None);
        }
    }

    if target_conv_id.is_none() {
        if let Some(ref refs) = payload.references {
            for ref_id in refs.split_whitespace() {
                let trimmed = ref_id.trim();
                if !trimmed.is_empty() {
                    if let Ok(Some(cid)) = sqlx::query_scalar::<_, i32>(
                        "SELECT conversation_id FROM messages WHERE account_id = $1 AND source_id = $2 LIMIT 1"
                    )
                    .bind(account_id)
                    .bind(trimmed)
                    .fetch_optional(&state.db)
                    .await
                    {
                        target_conv_id = Some(cid);
                        break;
                    }
                }
            }
        }
    }

    let conversation_id = match target_conv_id {
        Some(cid) => {
            // Reopen tiket jika sebelumnya resolved (1)
            let _ = sqlx::query(
                "UPDATE conversations SET status = 0, updated_at = NOW() WHERE id = $1 AND status = 1"
            )
            .bind(cid)
            .execute(&state.db)
            .await;
            cid
        }
        None => {
            // Buat conversation baru untuk email
            let max_display_id: i32 = sqlx::query_scalar(
                "SELECT COALESCE(MAX(display_id), 0) FROM conversations WHERE account_id = $1"
            )
            .bind(account_id)
            .fetch_one(&state.db)
            .await
            .unwrap_or(0);

            let subject = payload.subject.clone().unwrap_or_else(|| "No Subject".to_string());
            let additional_attrs = json!({
                "mail_subject": subject,
                "inbound_email": clean_to
            });

            match sqlx::query_scalar::<_, i32>(
                r#"
                INSERT INTO conversations (account_id, inbox_id, status, contact_id, display_id, additional_attributes, created_at, updated_at)
                VALUES ($1, $2, 0, $3, $4, $5, NOW(), NOW())
                RETURNING id
                "#
            )
            .bind(account_id)
            .bind(inbox.id)
            .bind(contact_id)
            .bind(max_display_id + 1)
            .bind(&additional_attrs)
            .fetch_one(&state.db)
            .await
            {
                Ok(new_cid) => new_cid,
                Err(e) => {
                    error!("Gagal membuat conversation email: {:?}", e);
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({ "error": "Failed to create conversation" })),
                    );
                }
            }
        }
    };

    // 5. Simpan pesan masuk
    let content = payload.text
        .or(payload.html)
        .or(payload.subject)
        .unwrap_or_else(|| "[Empty Email]".to_string());

    let message_source_id = payload.message_id.unwrap_or_else(|| {
        format!("<inbound-{}@chatwoot.local>", uuid::Uuid::new_v4())
    });

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
    .bind(&content)
    .bind(account_id)
    .bind(inbox.id)
    .bind(conversation_id)
    .bind(&message_source_id)
    .bind(contact_id)
    .fetch_one(&state.db)
    .await
    {
        Ok(m) => m,
        Err(e) => {
            error!("Gagal menyimpan message email: {:?}", e);
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
        from = %clean_from,
        "Berhasil memproses inbound email"
    );

    // 6. Broadcast ActionCable WebSocket
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
                "name": customer_name,
                "type": "contact"
            }
        }),
    });

    let _ = state.broadcast_tx.send(CableBroadcastMsg {
        account_id,
        event: "conversation.updated".to_string(),
        data: json!({ "id": conversation_id }),
    });

    // 7. Evaluasi automation rules secara asinkron
    let state_clone = state.clone();
    let msg_id = message.id;
    tokio::spawn(async move {
        evaluate_and_execute_rules(&state_clone, account_id as i64, "message_created", conversation_id, Some(msg_id)).await;
    });

    (
        StatusCode::OK,
        Json(json!({
            "success": true,
            "conversation_id": conversation_id,
            "message_id": message.id,
            "source_id": message_source_id
        })),
    )
}
