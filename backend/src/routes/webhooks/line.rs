use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use chrono::Utc;
use serde_json::{json, Value};
use tracing::{info, warn};

use crate::state::AppState;

/// Inbound Line Webhook: POST /webhooks/line
pub async fn line_webhook(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(payload): Json<Value>,
) -> impl IntoResponse {
    let signature = headers
        .get("x-line-signature")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");

    info!("Received Line webhook with signature: {}", signature);

    if let Some(events) = payload.get("events").and_then(|e| e.as_array()) {
        for event in events {
            let event_type = event.get("type").and_then(|t| t.as_str()).unwrap_or("");
            if event_type == "message" {
                let user_id = event
                    .get("source")
                    .and_then(|s| s.get("userId"))
                    .and_then(|u| u.as_str())
                    .unwrap_or("line_user");

                let message_obj = event.get("message");
                let message_text = message_obj
                    .and_then(|m| m.get("text"))
                    .and_then(|t| t.as_str())
                    .unwrap_or("[Line Media/Attachment]");
                let line_msg_id = message_obj
                    .and_then(|m| m.get("id"))
                    .and_then(|i| i.as_str())
                    .unwrap_or_else(|| "line_msg");

                let account_id = 1i32;

                // 1. Temukan atau buat inbox Line
                let inbox_res = sqlx::query!(
                    "SELECT id FROM inboxes WHERE account_id = $1 AND channel_type = 'Channel::Line' LIMIT 1",
                    account_id
                )
                .fetch_optional(&state.db)
                .await
                .unwrap_or(None);

                let inbox_id = match inbox_res {
                    Some(i) => i.id,
                    None => 1,
                };

                // 2. Cari atau buat kontak Line
                let contact_id = match sqlx::query!(
                    "SELECT id FROM contacts WHERE account_id = $1 AND identifier = $2 LIMIT 1",
                    account_id,
                    user_id
                )
                .fetch_optional(&state.db)
                .await
                .unwrap_or(None)
                {
                    Some(c) => c.id,
                    None => {
                        let new_c = sqlx::query!(
                            "INSERT INTO contacts (account_id, name, identifier, created_at, updated_at) VALUES ($1, $2, $3, NOW(), NOW()) RETURNING id",
                            account_id,
                            format!("Line User {}", &user_id[..std::cmp::min(6, user_id.len())]),
                            user_id
                        )
                        .fetch_one(&state.db)
                        .await;
                        new_c.map(|c| c.id).unwrap_or(1)
                    }
                };

                // 3. Cari atau buat percakapan aktif
                let conv = sqlx::query!(
                    "SELECT id FROM conversations WHERE account_id = $1 AND inbox_id = $2 AND contact_id = $3 AND status != 1 ORDER BY id DESC LIMIT 1",
                    account_id,
                    inbox_id,
                    contact_id as i64
                )
                .fetch_optional(&state.db)
                .await
                .unwrap_or(None);

                let conv_id = match conv {
                    Some(c) => c.id,
                    None => {
                        let new_display_id = (Utc::now().timestamp() % 100000) as i32;
                        let new_conv = sqlx::query!(
                            r#"
                            INSERT INTO conversations (
                                account_id, inbox_id, contact_id, status, display_id, uuid, created_at, updated_at
                            ) VALUES (
                                $1, $2, $3, 0, $4, gen_random_uuid(), NOW(), NOW()
                            )
                            RETURNING id
                            "#,
                            account_id,
                            inbox_id,
                            contact_id as i64,
                            new_display_id
                        )
                        .fetch_one(&state.db)
                        .await;
                        new_conv.map(|c| c.id).unwrap_or(1)
                    }
                };

                // 4. Simpan pesan masuk
                let _ = sqlx::query!(
                    r#"
                    INSERT INTO messages (
                        account_id, inbox_id, conversation_id, message_type, content,
                        source_id, sender_type, sender_id, created_at, updated_at
                    ) VALUES (
                        $1, $2, $3, 0, $4, $5, 'Contact', $6, NOW(), NOW()
                    )
                    "#,
                    account_id,
                    inbox_id,
                    conv_id,
                    message_text,
                    line_msg_id,
                    contact_id as i64
                )
                .execute(&state.db)
                .await;
            }
        }
    }

    (StatusCode::OK, Json(json!({ "status": "ok" }))).into_response()
}
