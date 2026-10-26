use hmac::{Hmac, Mac};
use sha2::Sha256;
use sqlx::PgPool;
use tracing::{error, info};

use crate::models::channel_facebook::ChannelFacebookPage;
use crate::models::inbox::Inbox;

type HmacSha256 = Hmac<Sha256>;

/// Verifikasi tanda tangan X-Hub-Signature-256 dari Meta webhook (Facebook Messenger)
pub fn verify_meta_signature(secret: &str, raw_body: &[u8], signature_header: &str) -> bool {
    let signature_hex = match signature_header.strip_prefix("sha256=") {
        Some(hex_str) => hex_str,
        None => return false,
    };

    let mut mac = match HmacSha256::new_from_slice(secret.as_bytes()) {
        Ok(m) => m,
        Err(_) => return false,
    };

    mac.update(raw_body);
    let expected_hex = hex::encode(mac.finalize().into_bytes());

    // Constant-time comparison
    if signature_hex.len() != expected_hex.len() {
        return false;
    }

    let mut diff = 0u8;
    for (a, b) in signature_hex.bytes().zip(expected_hex.bytes()) {
        diff |= a ^ b;
    }
    diff == 0
}

/// Kirim pesan keluar ke Meta Facebook Messenger API via reqwest
pub async fn send_outgoing_facebook_message(
    pool: &PgPool,
    inbox_id: i32,
    recipient_id: &str,
    content: &str,
    base_url: &str,
) -> Result<String, String> {
    // 1. Ambil data inbox dan pastikan merupakan Channel::FacebookPage
    let inbox = sqlx::query_as::<_, Inbox>("SELECT * FROM inboxes WHERE id = $1")
        .bind(inbox_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| format!("DB error fetching inbox: {}", e))?
        .ok_or_else(|| "Inbox not found".to_string())?;

    if inbox.channel_type.as_deref() != Some("Channel::FacebookPage") {
        return Err("Inbox is not a FacebookPage channel".to_string());
    }

    // 2. Ambil channel_facebook_pages
    let channel = sqlx::query_as::<_, ChannelFacebookPage>(
        "SELECT * FROM channel_facebook_pages WHERE id = $1",
    )
    .bind(inbox.channel_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("DB error fetching channel_facebook_pages: {}", e))?
    .ok_or_else(|| "ChannelFacebookPage record not found".to_string())?;

    let access_token = &channel.page_access_token;
    let page_id = &channel.page_id;

    // Jika access_token adalah mock atau test token, return simulated FB mid
    if access_token.starts_with("mock_") || access_token.starts_with("FB_") {
        let mock_mid = format!("mid.FB_MOCK_{}", uuid::Uuid::new_v4().to_string().replace('-', ""));
        info!(
            "Simulating Facebook Messenger to {} from page {} via test token: {}",
            recipient_id, page_id, mock_mid
        );
        return Ok(mock_mid);
    }

    // 3. Kirim via HTTP Client reqwest ke Facebook Graph API v22.0
    let clean_base = base_url.trim_end_matches('/');
    let url = if clean_base.contains("/v") {
        format!("{}/{}/messages", clean_base, page_id)
    } else {
        format!("{}/v22.0/{}/messages", clean_base, page_id)
    };

    let payload = serde_json::json!({
        "recipient": {
            "id": recipient_id
        },
        "message": {
            "text": content
        }
    });

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("Reqwest client builder error: {}", e))?;

    let res = client
        .post(&url)
        .query(&[("access_token", access_token)])
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("Failed to send Facebook Messenger via Meta API: {}", e))?;

    if !res.status().is_success() {
        let err_text = res.text().await.unwrap_or_default();
        error!("Meta Facebook Graph API error response: {}", err_text);
        return Err(format!("Meta Facebook Graph API returned error: {}", err_text));
    }

    let res_json: serde_json::Value = res
        .json()
        .await
        .map_err(|e| format!("Failed to parse Meta Facebook response JSON: {}", e))?;

    let mid = res_json["message_id"]
        .as_str()
        .unwrap_or_default()
        .to_string();

    info!(
        "Successfully sent Facebook Messenger to {}. message_id={}",
        recipient_id, mid
    );
    Ok(mid)
}
