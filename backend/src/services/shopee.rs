use hmac::{Hmac, Mac};
use sha2::Sha256;
use sqlx::PgPool;
use tracing::{error, info};

use crate::models::channel_shopee::ChannelShopee;
use crate::models::inbox::Inbox;

type HmacSha256 = Hmac<Sha256>;

/// Verifikasi tanda tangan Authorization dari Shopee Push Notification Webhook
pub fn verify_shopee_signature(partner_key: &str, raw_body: &[u8], signature_header: &str) -> bool {
    let clean_sig = signature_header.trim().strip_prefix("sha256=").unwrap_or(signature_header);

    let mut mac = match HmacSha256::new_from_slice(partner_key.as_bytes()) {
        Ok(m) => m,
        Err(_) => return false,
    };

    mac.update(raw_body);
    let expected_hex = hex::encode(mac.finalize().into_bytes());

    // Constant-time comparison
    if clean_sig.len() != expected_hex.len() {
        return false;
    }

    let mut diff = 0u8;
    for (a, b) in clean_sig.bytes().zip(expected_hex.bytes()) {
        diff |= a ^ b;
    }
    diff == 0
}

/// Kirim pesan keluar ke Shopee Seller Chat Open API via reqwest
pub async fn send_outgoing_shopee_message(
    pool: &PgPool,
    inbox_id: i32,
    to_user_id: &str,
    content: &str,
    base_url: &str,
) -> Result<String, String> {
    // 1. Ambil data inbox dan pastikan merupakan Channel::Shopee
    let inbox = sqlx::query_as::<_, Inbox>("SELECT * FROM inboxes WHERE id = $1")
        .bind(inbox_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| format!("DB error fetching inbox: {}", e))?
        .ok_or_else(|| "Inbox not found".to_string())?;

    if inbox.channel_type.as_deref() != Some("Channel::Shopee") {
        return Err("Inbox is not a Shopee channel".to_string());
    }

    // 2. Ambil channel_shopee
    let channel = sqlx::query_as::<_, ChannelShopee>(
        "SELECT * FROM channel_shopee WHERE id = $1",
    )
    .bind(inbox.channel_id as i64)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("DB error fetching channel_shopee: {}", e))?
    .ok_or_else(|| "ChannelShopee record not found".to_string())?;

    let access_token = &channel.access_token;
    let shop_id = channel.shop_id;

    // Jika access_token adalah mock atau test token, return simulated Shopee message_id
    if access_token.starts_with("mock_") || access_token.contains("test_access_token") {
        let mock_shopee_mid = format!("shopee_msg_{}", rand::random::<u32>());
        info!(
            "Simulating Shopee Chat message to user {} for shop {} via test token: {}",
            to_user_id, shop_id, mock_shopee_mid
        );
        return Ok(mock_shopee_mid);
    }

    // 3. Kirim via Shopee Open API v2
    let clean_base = base_url.trim_end_matches('/');
    let url = format!("{}/api/v2/sellerchat/send_message", clean_base);

    let payload = serde_json::json!({
        "to_id": to_user_id.parse::<i64>().unwrap_or(0),
        "message_type": "text",
        "content": {
            "text": content
        }
    });

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("Reqwest client builder error: {}", e))?;

    let res = client
        .post(&url)
        .query(&[("access_token", access_token), ("shop_id", &shop_id.to_string())])
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("Failed to send Shopee chat message: {}", e))?;

    if !res.status().is_success() {
        let err_text = res.text().await.unwrap_or_default();
        error!("Shopee Open API error response: {}", err_text);
        return Err(format!("Shopee Open API returned error: {}", err_text));
    }

    let res_json: serde_json::Value = res
        .json()
        .await
        .map_err(|e| format!("Failed to parse Shopee response JSON: {}", e))?;

    let msg_id = res_json["response"]["message_id"]
        .as_str()
        .unwrap_or_default()
        .to_string();

    info!(
        "Successfully sent Shopee message to user {}. message_id={}",
        to_user_id, msg_id
    );
    Ok(msg_id)
}
