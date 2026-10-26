use hmac::{Hmac, Mac};
use sha2::Sha256;
use sqlx::PgPool;
use tracing::{error, info};

use crate::models::channel_whatsapp::ChannelWhatsapp;
use crate::models::inbox::Inbox;

type HmacSha256 = Hmac<Sha256>;

/// Verifikasi tanda tangan X-Hub-Signature-256 dari Meta webhook
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

/// Kirim pesan keluar ke Meta WhatsApp Cloud API via reqwest
pub async fn send_outgoing_message(
    pool: &PgPool,
    inbox_id: i32,
    to_phone: &str,
    content: &str,
    base_url: &str,
) -> Result<String, String> {
    // 1. Ambil data inbox dan pastikan merupakan Channel::Whatsapp
    let inbox = sqlx::query_as::<_, Inbox>("SELECT * FROM inboxes WHERE id = $1")
        .bind(inbox_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| format!("DB error fetching inbox: {}", e))?
        .ok_or_else(|| "Inbox not found".to_string())?;

    if inbox.channel_type.as_deref() != Some("Channel::Whatsapp") {
        return Err("Inbox is not a WhatsApp channel".to_string());
    }

    // 2. Ambil channel_whatsapp
    let channel = sqlx::query_as::<_, ChannelWhatsapp>("SELECT * FROM channel_whatsapp WHERE id = $1")
        .bind(inbox.channel_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| format!("DB error fetching channel_whatsapp: {}", e))?
        .ok_or_else(|| "ChannelWhatsapp record not found".to_string())?;

    let phone_number_id = channel
        .phone_number_id()
        .ok_or_else(|| "phone_number_id not found in provider_config".to_string())?;
    let api_key = channel
        .api_key()
        .ok_or_else(|| "api_key not found in provider_config".to_string())?;

    // Jika api_key adalah mock (misal di testing), return simulated wamid
    if api_key.starts_with("mock_") {
        let mock_wamid = format!("wamid.MOCK_{}", uuid::Uuid::new_v4());
        info!(
            "Simulating WhatsApp message to {} via mock api_key: {}",
            to_phone, mock_wamid
        );
        return Ok(mock_wamid);
    }

    // 3. Kirim via HTTP Client reqwest ke Graph API
    let url = format!("{}/v19.0/{}/messages", base_url, phone_number_id);
    let payload = serde_json::json!({
        "messaging_product": "whatsapp",
        "recipient_type": "individual",
        "to": to_phone,
        "type": "text",
        "text": {
            "body": content
        }
    });

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("Reqwest client builder error: {}", e))?;

    let res = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("Failed to send WhatsApp message via Meta API: {}", e))?;

    if !res.status().is_success() {
        let err_text = res.text().await.unwrap_or_default();
        error!("Meta Graph API error response: {}", err_text);
        return Err(format!("Meta Graph API returned error: {}", err_text));
    }

    let res_json: serde_json::Value = res
        .json()
        .await
        .map_err(|e| format!("Failed to parse Meta response JSON: {}", e))?;

    let wamid = res_json["messages"][0]["id"]
        .as_str()
        .unwrap_or_default()
        .to_string();

    info!("Successfully sent WhatsApp message to {}. wamid={}", to_phone, wamid);
    Ok(wamid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verify_meta_signature_valid() {
        let secret = "test_app_secret_123";
        let body = b"{\"hello\":\"world\"}";

        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        let valid_sig = format!("sha256={}", hex::encode(mac.finalize().into_bytes()));

        assert!(verify_meta_signature(secret, body, &valid_sig));
    }

    #[test]
    fn test_verify_meta_signature_invalid() {
        let secret = "test_app_secret_123";
        let body = b"{\"hello\":\"world\"}";
        let invalid_sig = "sha256=0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

        assert!(!verify_meta_signature(secret, body, invalid_sig));
        assert!(!verify_meta_signature(secret, body, "invalid_prefix"));
    }
}
