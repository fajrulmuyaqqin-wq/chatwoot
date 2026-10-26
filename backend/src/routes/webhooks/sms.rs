use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Form, Json,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use tracing::{info, warn};

use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct TwilioSmsPayload {
    #[serde(rename = "From")]
    pub from: Option<String>,
    #[serde(rename = "To")]
    pub to: Option<String>,
    #[serde(rename = "Body")]
    pub body: Option<String>,
    #[serde(rename = "MessageSid")]
    pub message_sid: Option<String>,
    #[serde(rename = "AccountSid")]
    pub account_sid: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BandwidthSmsPayload {
    pub from: Option<String>,
    pub to: Option<String>,
    pub text: Option<String>,
    #[serde(rename = "messageId")]
    pub message_id: Option<String>,
}

/// Inbound Twilio SMS Webhook: POST /webhooks/sms/twilio
pub async fn twilio_sms_webhook(
    State(state): State<AppState>,
    Form(payload): Form<TwilioSmsPayload>,
) -> impl IntoResponse {
    let from_number = match payload.from {
        Some(f) if !f.is_empty() => f,
        _ => return (StatusCode::BAD_REQUEST, "Missing From number").into_response(),
    };
    let to_number = payload.to.unwrap_or_default();
    let message_body = payload.body.unwrap_or_default();
    let external_id = payload.message_sid.unwrap_or_else(|| format!("tw-{}", Utc::now().timestamp()));

    info!("Received Twilio SMS from {} to {}: {}", from_number, to_number, message_body);

    // 1. Temukan channel_twilio_sms berdasarkan nomor tujuan / account_sid
    let channel = sqlx::query!(
        "SELECT id, account_id FROM channel_twilio_sms WHERE phone_number = $1 OR account_sid = $2 LIMIT 1",
        to_number,
        payload.account_sid.unwrap_or_default()
    )
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None);

    let (channel_id, account_id) = match channel {
        Some(c) => (c.id, c.account_id),
        None => {
            // Fallback ke default account 1 jika belum dipetakan
            (1i64, 1i32)
        }
    };

    // 2. Temukan inbox terkait
    let inbox = sqlx::query!(
        "SELECT id FROM inboxes WHERE account_id = $1 AND channel_type = 'Channel::TwilioSms' LIMIT 1",
        account_id
    )
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None);

    let inbox_id = match inbox {
        Some(i) => i.id,
        None => 1,
    };

    // 3. Cari atau buat kontak berdasarkan nomor telepon
    let contact_id = match sqlx::query!(
        "SELECT id FROM contacts WHERE account_id = $1 AND phone_number = $2 LIMIT 1",
        account_id,
        from_number
    )
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None)
    {
        Some(c) => c.id,
        None => {
            let new_c = sqlx::query!(
                "INSERT INTO contacts (account_id, name, phone_number, created_at, updated_at) VALUES ($1, $2, $3, NOW(), NOW()) RETURNING id",
                account_id,
                from_number,
                from_number
            )
            .fetch_one(&state.db)
            .await;
            new_c.map(|c| c.id).unwrap_or(1)
        }
    };

    // 4. Cari atau buat percakapan aktif
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

    // 5. Simpan pesan masuk (message_type = 0 untuk incoming)
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
        message_body,
        external_id,
        contact_id as i64
    )
    .execute(&state.db)
    .await;

    // Response TwiML kosong menandakan sukses diterima
    (
        StatusCode::OK,
        [("content-type", "application/xml")],
        "<Response></Response>",
    )
        .into_response()
}

/// Inbound Bandwidth SMS Webhook: POST /webhooks/sms/bandwidth
pub async fn bandwidth_sms_webhook(
    State(state): State<AppState>,
    Json(payload): Json<Value>,
) -> impl IntoResponse {
    info!("Received Bandwidth SMS payload: {:?}", payload);
    (StatusCode::OK, Json(json!({ "status": "received" }))).into_response()
}
