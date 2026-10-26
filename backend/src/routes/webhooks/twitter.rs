use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use chrono::Utc;
use hmac::{Hmac, Mac};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::Sha256;
use tracing::info;

use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct TwitterCrcQuery {
    pub crc_token: Option<String>,
}

/// Twitter CRC Challenge Verification: GET /webhooks/twitter
pub async fn twitter_crc(
    Query(query): Query<TwitterCrcQuery>,
) -> impl IntoResponse {
    let crc_token = query.crc_token.unwrap_or_default();
    let consumer_secret = std::env::var("TWITTER_CONSUMER_SECRET")
        .unwrap_or_else(|_| "chatwoot_twitter_default_secret_2026".to_string());

    type HmacSha256 = Hmac<Sha256>;
    let mut mac = HmacSha256::new_from_slice(consumer_secret.as_bytes())
        .expect("HMAC can take key of any size");
    mac.update(crc_token.as_bytes());
    let response_token = format!("sha256={}", BASE64.encode(mac.finalize().into_bytes()));

    (StatusCode::OK, Json(json!({ "response_token": response_token }))).into_response()
}

/// Twitter Direct Message Events Webhook: POST /webhooks/twitter
pub async fn twitter_webhook(
    State(state): State<AppState>,
    Json(payload): Json<Value>,
) -> impl IntoResponse {
    info!("Received Twitter/X webhook event: {:?}", payload);

    if let Some(dm_events) = payload.get("direct_message_events").and_then(|d| d.as_array()) {
        for dm in dm_events {
            let message_create = dm.get("message_create");
            let text = message_create
                .and_then(|m| m.get("message_data"))
                .and_then(|d| d.get("text"))
                .and_then(|t| t.as_str())
                .unwrap_or("");
            let sender_id = message_create
                .and_then(|m| m.get("sender_id"))
                .and_then(|s| s.as_str())
                .unwrap_or("twitter_sender");

            let account_id = 1i32;

            // 1. Temukan inbox Twitter
            let inbox_res = sqlx::query!(
                "SELECT id FROM inboxes WHERE account_id = $1 AND channel_type = 'Channel::TwitterProfile' LIMIT 1",
                account_id
            )
            .fetch_optional(&state.db)
            .await
            .unwrap_or(None);

            let inbox_id = match inbox_res {
                Some(i) => i.id,
                None => 1,
            };

            // 2. Cari atau buat kontak Twitter
            let contact_id = match sqlx::query!(
                "SELECT id FROM contacts WHERE account_id = $1 AND identifier = $2 LIMIT 1",
                account_id,
                sender_id
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
                        format!("Twitter @{}", &sender_id[..std::cmp::min(8, sender_id.len())]),
                        sender_id
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
                    sender_type, sender_id, created_at, updated_at
                ) VALUES (
                    $1, $2, $3, 0, $4, 'Contact', $5, NOW(), NOW()
                )
                "#,
                account_id,
                inbox_id,
                conv_id,
                text,
                contact_id as i64
            )
            .execute(&state.db)
            .await;
        }
    }

    (StatusCode::OK, Json(json!({ "status": "received" }))).into_response()
}
