use crate::models::campaign::Campaign;
use crate::models::conversation::Conversation;
use crate::models::message::Message;
use crate::services::event_dispatcher;
use crate::state::AppState;
use serde_json::Value;
use tracing::{info, warn};

pub async fn trigger_one_off_campaign(
    state: &AppState,
    account_id: i64,
    campaign_id: i64,
) -> Result<usize, String> {
    let campaign = match sqlx::query_as::<_, Campaign>(
        "SELECT * FROM campaigns WHERE id = $1 AND account_id = $2",
    )
    .bind(campaign_id)
    .bind(account_id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(c)) => c,
        Ok(None) => return Err("Campaign not found".to_string()),
        Err(e) => return Err(format!("Database error: {}", e)),
    };

    if campaign.campaign_type != 1 {
        return Err("Only one_off campaigns can be triggered for blast".to_string());
    }

    // Update status to processing
    let _ = sqlx::query(
        "UPDATE campaigns SET campaign_status = 2, started_at = NOW(), updated_at = NOW() WHERE id = $1",
    )
    .bind(campaign.id)
    .execute(&state.db)
    .await;

    // Ambil daftar kontak sasaran (audience)
    // 1. Cek jika ada kontak spesifik di contact_inboxes untuk inbox_id ini
    let target_contact_ids: Vec<i64> = sqlx::query_scalar(
        "SELECT contact_id FROM contact_inboxes WHERE inbox_id = $1 LIMIT 500",
    )
    .bind(campaign.inbox_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    // Fallback jika contact_inboxes kosong, ambil kontak dari akun
    let contact_ids = if !target_contact_ids.is_empty() {
        target_contact_ids
    } else {
        sqlx::query_scalar(
            "SELECT id FROM contacts WHERE account_id = $1 LIMIT 50",
        )
        .bind(account_id)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default()
    };

    let total_recipients = contact_ids.len();
    info!(
        "Starting broadcast blast for campaign #{} '{}' to {} recipients",
        campaign.id, campaign.title, total_recipients
    );

    for contact_id in &contact_ids {
        // Cari atau buat conversation
        let conv = match sqlx::query_as::<_, Conversation>(
            r#"
            SELECT * FROM conversations
            WHERE account_id = $1 AND inbox_id = $2 AND contact_id = $3
            ORDER BY id DESC LIMIT 1
            "#,
        )
        .bind(account_id as i32)
        .bind(campaign.inbox_id as i32)
        .bind(*contact_id)
        .fetch_optional(&state.db)
        .await
        {
            Ok(Some(c)) => c,
            _ => {
                // Buat percakapan baru untuk blast ini
                let next_display_id: (i32,) = sqlx::query_as(
                    "SELECT COALESCE(MAX(display_id), 0) + 1 FROM conversations WHERE account_id = $1",
                )
                .bind(account_id as i32)
                .fetch_one(&state.db)
                .await
                .unwrap_or((1,));

                match sqlx::query_as::<_, Conversation>(
                    r#"
                    INSERT INTO conversations (
                        account_id, inbox_id, status, contact_id, display_id,
                        campaign_id, uuid, last_activity_at, created_at, updated_at
                    )
                    VALUES ($1, $2, 0, $3, $4, $5, gen_random_uuid(), NOW(), NOW(), NOW())
                    RETURNING *
                    "#,
                )
                .bind(account_id as i32)
                .bind(campaign.inbox_id as i32)
                .bind(*contact_id)
                .bind(next_display_id.0)
                .bind(campaign.id)
                .fetch_one(&state.db)
                .await
                {
                    Ok(nc) => nc,
                    Err(e) => {
                        warn!("Failed to create conversation for blast contact {}: {}", contact_id, e);
                        continue;
                    }
                }
            }
        };

        // Buat pesan blast
        let new_msg = sqlx::query_as::<_, Message>(
            r#"
            INSERT INTO messages (
                account_id, inbox_id, conversation_id, content, message_type,
                private, status, content_type, sender_type, sender_id, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, 1, FALSE, 0, 0, 'User', $5, NOW(), NOW())
            RETURNING *
            "#,
        )
        .bind(account_id as i32)
        .bind(campaign.inbox_id as i32)
        .bind(conv.id)
        .bind(&campaign.message)
        .bind(campaign.sender_id.map(|s| s as i64))
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None);

        if let Some(m) = new_msg {
            event_dispatcher::dispatch_event(
                state,
                account_id as i32,
                "message.created",
                serde_json::to_value(&m).unwrap_or(Value::Null),
            );
        }

        // Update conversation last activity
        let _ = sqlx::query("UPDATE conversations SET last_activity_at = NOW(), updated_at = NOW() WHERE id = $1")
            .bind(conv.id)
            .execute(&state.db)
            .await;
    }

    // Update status to completed
    let _ = sqlx::query(
        "UPDATE campaigns SET campaign_status = 1, completed_at = NOW(), updated_at = NOW() WHERE id = $1",
    )
    .bind(campaign.id)
    .execute(&state.db)
    .await;

    info!(
        "Completed broadcast blast for campaign #{} to {} recipients",
        campaign.id, total_recipients
    );

    Ok(total_recipients)
}

/// Helper untuk memeriksa dan mengeksekusi scheduled campaigns yang jatuh tempo
pub async fn check_and_execute_scheduled_campaigns(state: &AppState) {
    let campaigns: Vec<(i64, i64)> = match sqlx::query_as(
        r#"
        SELECT id, account_id FROM campaigns
        WHERE campaign_type = 1 AND campaign_status = 0 AND enabled = TRUE
          AND scheduled_at IS NOT NULL AND scheduled_at <= NOW()
        "#,
    )
    .fetch_all(&state.db)
    .await
    {
        Ok(c) => c,
        Err(_) => return,
    };

    for (cid, aid) in campaigns {
        info!("Executing scheduled campaign id={} for account_id={}", cid, aid);
        let _ = trigger_one_off_campaign(state, aid, cid).await;
    }
}
