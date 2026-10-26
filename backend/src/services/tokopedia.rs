use hmac::{Hmac, Mac};
use sha2::Sha256;
use sqlx::PgPool;
use tracing::info;

type HmacSha256 = Hmac<Sha256>;

pub fn verify_tokopedia_signature(secret: &str, raw_body: &[u8], signature_header: &str) -> bool {
    let mut mac = match HmacSha256::new_from_slice(secret.as_bytes()) {
        Ok(m) => m,
        Err(_) => return false,
    };
    mac.update(raw_body);
    let expected_hex = hex::encode(mac.finalize().into_bytes());

    if signature_header.len() != expected_hex.len() {
        return false;
    }

    let mut diff = 0u8;
    for (a, b) in signature_header.bytes().zip(expected_hex.bytes()) {
        diff |= a ^ b;
    }
    diff == 0
}

pub async fn send_outgoing_message(
    _pool: &PgPool,
    shop_id: i64,
    to_user_id: i64,
    content: &str,
    api_url: &str,
) -> Result<String, String> {
    info!(
        shop_id = shop_id,
        to_user_id = to_user_id,
        content_len = content.len(),
        api_url = api_url,
        "Mengirim balasan pesan ke Tokopedia Open API"
    );

    // Kirim via HTTP client reqwest jika api_url bukan dummy
    if api_url.starts_with("http") && !api_url.contains("example.com") {
        let client = reqwest::Client::new();
        let payload = serde_json::json!({
            "shop_id": shop_id,
            "to_user_id": to_user_id,
            "message": content
        });

        match client.post(api_url).json(&payload).send().await {
            Ok(resp) => {
                if resp.status().is_success() {
                    Ok("tkpd_msg_sent".to_string())
                } else {
                    Err(format!("Tokopedia API returned status: {}", resp.status()))
                }
            }
            Err(e) => Err(format!("Tokopedia API connection error: {:?}", e)),
        }
    } else {
        Ok("tkpd_msg_simulated".to_string())
    }
}
