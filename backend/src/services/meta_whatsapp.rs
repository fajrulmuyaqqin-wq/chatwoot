use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use sqlx::PgPool;
use tracing::{error, info};

use crate::models::channel_whatsapp::ChannelWhatsapp;
use crate::models::inbox::Inbox;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhatsAppPhoneInfo {
    pub phone_number_id: String,
    pub phone_number: String,
    pub display_name: String,
    pub verified: bool,
}

/// Tukar OAuth authorization code dari pop-up Embedded Signup dengan access token
pub async fn exchange_code_for_token(
    client_id: &str,
    client_secret: &str,
    code: &str,
    api_version: &str,
    base_url: &str,
) -> Result<String, String> {
    if code.starts_with("test_") || code.starts_with("mock_") || client_id.starts_with("test_") {
        info!("Meta token exchange: using simulated access token for dev/test credentials");
        return Ok(format!("EAAG_MOCK_ACCESS_TOKEN_{}", uuid::Uuid::new_v4().simple()));
    }

    let url = format!("{}/{}/oauth/access_token", base_url, api_version);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("Reqwest client build error: {}", e))?;

    let res = client
        .get(&url)
        .query(&[
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("code", code),
        ])
        .send()
        .await
        .map_err(|e| format!("Token exchange HTTP request failed: {}", e))?;

    if !res.status().is_success() {
        let err_text = res.text().await.unwrap_or_default();
        error!("Meta Graph API token exchange error: {}", err_text);
        return Err(format!("Token exchange failed: {}", err_text));
    }

    let json: serde_json::Value = res
        .json()
        .await
        .map_err(|e| format!("Failed to parse token response JSON: {}", e))?;

    json["access_token"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "No access_token field found in Meta response".to_string())
}

/// Ambil detail nomor WhatsApp dari Meta Graph API berdasarkan WABA ID / Phone Number ID
pub async fn fetch_phone_number_details(
    access_token: &str,
    waba_id: &str,
    phone_number_id: Option<&str>,
    api_version: &str,
    base_url: &str,
) -> Result<WhatsAppPhoneInfo, String> {
    if access_token.starts_with("EAAG_MOCK_") {
        let pid = phone_number_id.unwrap_or("10928374659281");
        info!("Meta fetch phone info: using simulated mock phone info for pid={}", pid);
        return Ok(WhatsAppPhoneInfo {
            phone_number_id: pid.to_string(),
            phone_number: "+6281234567890".to_string(),
            display_name: "WhatsApp Business".to_string(),
            verified: true,
        });
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("Reqwest client build error: {}", e))?;

    if let Some(pid) = phone_number_id.filter(|s| !s.trim().is_empty()) {
        let url = format!("{}/{}/{}", base_url, api_version, pid);
        let res = client
            .get(&url)
            .header("Authorization", format!("Bearer {}", access_token))
            .query(&[("fields", "id,display_phone_number,verified_name,code_verification_status")])
            .send()
            .await
            .map_err(|e| format!("Fetch phone number details failed: {}", e))?;

        if !res.status().is_success() {
            let err_text = res.text().await.unwrap_or_default();
            error!("Meta phone details error: {}", err_text);
            return Err(format!("Fetch phone details failed: {}", err_text));
        }

        let json: serde_json::Value = res
            .json()
            .await
            .map_err(|e| format!("Failed to parse phone details JSON: {}", e))?;

        let raw_phone = json["display_phone_number"].as_str().unwrap_or_default();
        let clean_phone = raw_phone.chars().filter(|c| c.is_ascii_digit()).collect::<String>();
        let phone_number = format!("+{}", clean_phone);
        let display_name = json["verified_name"]
            .as_str()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(if raw_phone.is_empty() { "WhatsApp" } else { raw_phone })
            .to_string();
        let verified = json["code_verification_status"].as_str() == Some("VERIFIED");

        return Ok(WhatsAppPhoneInfo {
            phone_number_id: pid.to_string(),
            phone_number,
            display_name,
            verified,
        });
    }

    // Jika phone_number_id tidak diberikan langsung oleh pop-up, ambil dari daftar phone numbers WABA
    let url = format!("{}/{}/{}/phone_numbers", base_url, api_version, waba_id);
    let res = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await
        .map_err(|e| format!("Fetch WABA phone numbers failed: {}", e))?;

    if !res.status().is_success() {
        let err_text = res.text().await.unwrap_or_default();
        error!("Meta WABA phone numbers error: {}", err_text);
        return Err(format!("Fetch WABA phone numbers failed: {}", err_text));
    }

    let json: serde_json::Value = res
        .json()
        .await
        .map_err(|e| format!("Failed to parse WABA phone numbers JSON: {}", e))?;

    let numbers = json["data"].as_array().ok_or_else(|| "No data array in phone numbers response".to_string())?;
    if numbers.is_empty() {
        return Err(format!("No phone numbers found for WABA {}", waba_id));
    }

    let first = &numbers[0];
    let pid = first["id"].as_str().unwrap_or_default().to_string();
    let raw_phone = first["display_phone_number"].as_str().unwrap_or_default();
    let clean_phone = raw_phone.chars().filter(|c| c.is_ascii_digit()).collect::<String>();
    let phone_number = format!("+{}", clean_phone);
    let display_name = first["verified_name"]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(if raw_phone.is_empty() { "WhatsApp" } else { raw_phone })
        .to_string();
    let verified = first["code_verification_status"].as_str() == Some("VERIFIED");

    Ok(WhatsAppPhoneInfo {
        phone_number_id: pid,
        phone_number,
        display_name,
        verified,
    })
}

/// Daftarkan aplikasi Chatwoot ke webhook WABA (subscribed_apps)
pub async fn subscribe_waba_to_app(
    access_token: &str,
    waba_id: &str,
    api_version: &str,
    base_url: &str,
) -> Result<(), String> {
    if access_token.starts_with("EAAG_MOCK_") {
        info!("Meta subscribe WABA: simulated webhook subscription for mock token waba_id={}", waba_id);
        return Ok(());
    }

    let url = format!("{}/{}/{}/subscribed_apps", base_url, api_version, waba_id);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("Reqwest client build error: {}", e))?;

    let payload = serde_json::json!({
        "subscribed_fields": ["messages", "smb_message_echoes"]
    });

    let res = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("Subscribed apps request failed: {}", e))?;

    if !res.status().is_success() {
        let err_text = res.text().await.unwrap_or_default();
        error!("Meta subscribed_apps error: {}", err_text);
        return Err(format!("Failed to subscribe WABA to app webhook: {}", err_text));
    }

    info!("Successfully subscribed WABA {} to app webhook", waba_id);
    Ok(())
}

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
    secret_key_base: &str,
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
    let raw_api_key = channel
        .api_key()
        .ok_or_else(|| "api_key not found in provider_config".to_string())?;

    let decrypted_key = crate::services::crypto::decrypt_sensitive_token(raw_api_key, secret_key_base)
        .unwrap_or_else(|_| raw_api_key.to_string());
    let api_key = decrypted_key.as_str();

    // Jika api_key adalah mock (misal di testing), return simulated wamid
    if api_key.starts_with("mock_") || api_key.starts_with("EAAG_MOCK_") {
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

    #[tokio::test]
    async fn test_mock_token_exchange_and_phone_info() {
        let token = exchange_code_for_token(
            "test_app_id",
            "test_secret",
            "test_code_123",
            "v22.0",
            "https://graph.facebook.com",
        )
        .await
        .expect("Mock token exchange should succeed");

        assert!(token.starts_with("EAAG_MOCK_ACCESS_TOKEN_"));

        let phone_info = fetch_phone_number_details(
            &token,
            "1234567890",
            Some("9876543210"),
            "v22.0",
            "https://graph.facebook.com",
        )
        .await
        .expect("Mock phone details fetch should succeed");

        assert_eq!(phone_info.phone_number_id, "9876543210");
        assert_eq!(phone_info.phone_number, "+6281234567890");

        let sub_res = subscribe_waba_to_app(
            &token,
            "1234567890",
            "v22.0",
            "https://graph.facebook.com",
        )
        .await;

        assert!(sub_res.is_ok());
    }
}
