use hmac::{Hmac, Mac};
use sha2::Sha256;
use sqlx::PgPool;
use tracing::{error, info};

use crate::models::channel_instagram::ChannelInstagram;
use crate::models::inbox::Inbox;

type HmacSha256 = Hmac<Sha256>;

/// Verifikasi tanda tangan X-Hub-Signature-256 dari Meta webhook (Instagram)
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

/// Kirim pesan keluar ke Meta Instagram Graph API via reqwest
pub async fn send_outgoing_instagram_message(
    pool: &PgPool,
    inbox_id: i32,
    recipient_id: &str,
    content: &str,
    base_url: &str,
) -> Result<String, String> {
    // 1. Ambil data inbox dan pastikan merupakan Channel::Instagram
    let inbox = sqlx::query_as::<_, Inbox>("SELECT * FROM inboxes WHERE id = $1")
        .bind(inbox_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| format!("DB error fetching inbox: {}", e))?
        .ok_or_else(|| "Inbox not found".to_string())?;

    if inbox.channel_type.as_deref() != Some("Channel::Instagram") {
        return Err("Inbox is not an Instagram channel".to_string());
    }

    // 2. Ambil channel_instagram
    let channel = sqlx::query_as::<_, ChannelInstagram>(
        "SELECT * FROM channel_instagram WHERE id = $1",
    )
    .bind(inbox.channel_id as i64)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("DB error fetching channel_instagram: {}", e))?
    .ok_or_else(|| "ChannelInstagram record not found".to_string())?;

    let access_token = &channel.access_token;
    let instagram_id = &channel.instagram_id;

    // Jika access_token adalah mock atau test token, return simulated IG mid
    if access_token.starts_with("mock_") || access_token.starts_with("IG_TEST_") {
        let mock_mid = format!("mid.IG_MOCK_{}", uuid::Uuid::new_v4().to_string().replace('-', ""));
        info!(
            "Simulating Instagram DM to {} from {} via test token: {}",
            recipient_id, instagram_id, mock_mid
        );
        return Ok(mock_mid);
    }

    // 3. Kirim via HTTP Client reqwest ke Instagram Graph API v22.0
    // URL: https://graph.instagram.com/v22.0/{instagram_id}/messages?access_token={token}
    let clean_base = base_url.trim_end_matches('/');
    let url = if clean_base.contains("/v") {
        format!("{}/{}/messages", clean_base, instagram_id)
    } else {
        format!("{}/v22.0/{}/messages", clean_base, instagram_id)
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
        .map_err(|e| format!("Failed to send Instagram DM via Meta API: {}", e))?;

    if !res.status().is_success() {
        let err_text = res.text().await.unwrap_or_default();
        error!("Meta Instagram Graph API error response: {}", err_text);
        return Err(format!("Meta Instagram Graph API returned error: {}", err_text));
    }

    let res_json: serde_json::Value = res
        .json()
        .await
        .map_err(|e| format!("Failed to parse Meta Instagram response JSON: {}", e))?;

    let mid = res_json["message_id"]
        .as_str()
        .unwrap_or_default()
        .to_string();

    info!(
        "Successfully sent Instagram message to {}. message_id={}",
        recipient_id, mid
    );
    Ok(mid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verify_instagram_meta_signature() {
        let secret = "ig_secret_indonesia_2026";
        let body = b"{\"object\":\"instagram\",\"entry\":[]}";

        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        let valid_sig = format!("sha256={}", hex::encode(mac.finalize().into_bytes()));

        assert!(verify_meta_signature(secret, body, &valid_sig));
        assert!(!verify_meta_signature(secret, body, "sha256=invalid"));
    }
}
