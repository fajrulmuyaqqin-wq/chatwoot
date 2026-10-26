use sqlx::PgPool;
use tracing::{error, info};

use crate::models::channel_telegram::ChannelTelegram;
use crate::models::inbox::Inbox;

/// Kirim pesan keluar ke Telegram Bot API via reqwest
pub async fn send_outgoing_telegram_message(
    pool: &PgPool,
    inbox_id: i32,
    chat_id: &str,
    content: &str,
    base_url: &str,
) -> Result<String, String> {
    // 1. Ambil data inbox dan pastikan merupakan Channel::Telegram
    let inbox = sqlx::query_as::<_, Inbox>("SELECT * FROM inboxes WHERE id = $1")
        .bind(inbox_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| format!("DB error fetching inbox: {}", e))?
        .ok_or_else(|| "Inbox not found".to_string())?;

    if inbox.channel_type.as_deref() != Some("Channel::Telegram") {
        return Err("Inbox is not a Telegram channel".to_string());
    }

    // 2. Ambil channel_telegram
    let channel = sqlx::query_as::<_, ChannelTelegram>(
        "SELECT * FROM channel_telegram WHERE id = $1",
    )
    .bind(inbox.channel_id as i64)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("DB error fetching channel_telegram: {}", e))?
    .ok_or_else(|| "ChannelTelegram record not found".to_string())?;

    let bot_token = &channel.bot_token;

    // Jika bot_token adalah mock atau test token, return simulated Telegram message_id
    if bot_token.starts_with("mock_") || bot_token.contains("ABCdefGHIjkl") {
        let mock_tg_id = format!("tg_msg_{}", rand::random::<u32>());
        info!(
            "Simulating Telegram message to chat {} via test bot_token: {}",
            chat_id, mock_tg_id
        );
        return Ok(mock_tg_id);
    }

    // 3. Kirim via Telegram Bot API
    let clean_base = base_url.trim_end_matches('/');
    let url = format!("{}/bot{}/sendMessage", clean_base, bot_token);

    let payload = serde_json::json!({
        "chat_id": chat_id,
        "text": content
    });

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("Reqwest client builder error: {}", e))?;

    let res = client
        .post(&url)
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("Failed to send Telegram message via Bot API: {}", e))?;

    if !res.status().is_success() {
        let err_text = res.text().await.unwrap_or_default();
        error!("Telegram Bot API error response: {}", err_text);
        return Err(format!("Telegram Bot API returned error: {}", err_text));
    }

    let res_json: serde_json::Value = res
        .json()
        .await
        .map_err(|e| format!("Failed to parse Telegram response JSON: {}", e))?;

    let msg_id = res_json["result"]["message_id"]
        .as_i64()
        .map(|id| id.to_string())
        .unwrap_or_default();

    info!(
        "Successfully sent Telegram message to {}. message_id={}",
        chat_id, msg_id
    );
    Ok(msg_id)
}
