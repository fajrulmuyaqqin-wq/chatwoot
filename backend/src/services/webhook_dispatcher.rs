use hmac::{Hmac, Mac};
use serde_json::Value;
use sha2::Sha256;
use sqlx::PgPool;
use tracing::{error, info};

type HmacSha256 = Hmac<Sha256>;

pub async fn dispatch_webhook_event(
    db: &PgPool,
    account_id: i64,
    event_name: &str,
    payload: Value,
) {
    let rows = sqlx::query!(
        r#"
        SELECT url, secret, subscriptions
        FROM webhooks
        WHERE account_id = $1
        "#,
        account_id as i32
    )
    .fetch_all(db)
    .await;

    let webhooks = match rows {
        Ok(w) => w,
        Err(e) => {
            error!("Failed to fetch webhooks for account {}: {}", account_id, e);
            return;
        }
    };

    if webhooks.is_empty() {
        return;
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap_or_default();

    for wh in webhooks {
        let url = match wh.url {
            Some(u) if !u.is_empty() => u,
            _ => continue,
        };

        // Check if webhook subscribes to this event
        if let Some(subs) = wh.subscriptions {
            if let Some(arr) = subs.as_array() {
                let subscribed = arr.iter().any(|v| v.as_str() == Some(event_name));
                if !subscribed {
                    continue;
                }
            }
        }

        let secret = wh.secret.clone();
        let payload_clone = payload.clone();
        let client_clone = client.clone();
        let event = event_name.to_string();

        tokio::spawn(async move {
            let body_str = serde_json::to_string(&payload_clone).unwrap_or_default();
            let mut req = client_clone
                .post(&url)
                .header("Content-Type", "application/json")
                .header("X-Chatwoot-Event", &event);

            if let Some(sec) = secret {
                if !sec.is_empty() {
                    if let Ok(mut mac) = HmacSha256::new_from_slice(sec.as_bytes()) {
                        mac.update(body_str.as_bytes());
                        let signature = hex::encode(mac.finalize().into_bytes());
                        req = req.header("X-Chatwoot-Signature", signature);
                    }
                }
            }

            match req.body(body_str).send().await {
                Ok(resp) => {
                    info!(
                        "Webhook dispatched for event {} to {}, status: {}",
                        event,
                        url,
                        resp.status()
                    );
                }
                Err(e) => {
                    error!("Failed to deliver webhook to {}: {}", url, e);
                }
            }
        });
    }
}
