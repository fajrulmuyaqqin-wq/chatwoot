use chrono::NaiveDateTime;
use sqlx::FromRow;
use tracing::{info, warn};

use crate::services::event_dispatcher;
use crate::state::AppState;

#[derive(Debug, FromRow)]
struct ActiveSlaRow {
    applied_sla_id: i32,
    account_id: i64,
    conversation_id: i64,
    sla_policy_id: i64,
    display_id: i32,
    inbox_id: i32,
    conversation_created_at: NaiveDateTime,
    first_reply_created_at: Option<NaiveDateTime>,
    first_response_time_threshold: Option<f64>,
}

/// Menjalankan background loop Tokio untuk memonitor SLA tiket
pub fn start_sla_evaluator(state: AppState) {
    tokio::spawn(async move {
        info!("SLA Policy Evaluator background task running...");
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(3));
        loop {
            interval.tick().await;
            if let Err(e) = evaluate_active_slas(&state).await {
                warn!("Error evaluating SLAs: {}", e);
            }
        }
    });
}

async fn evaluate_active_slas(state: &AppState) -> Result<(), sqlx::Error> {
    let rows = sqlx::query_as::<_, ActiveSlaRow>(
        r#"
        SELECT 
            a.id as applied_sla_id,
            a.account_id,
            a.conversation_id,
            a.sla_policy_id,
            c.display_id,
            c.inbox_id,
            c.created_at as conversation_created_at,
            c.first_reply_created_at,
            p.first_response_time_threshold
        FROM applied_slas a
        JOIN conversations c ON c.id = a.conversation_id
        JOIN sla_policies p ON p.id = a.sla_policy_id
        WHERE a.sla_status = 0 AND c.status = 0
        "#,
    )
    .fetch_all(&state.db)
    .await?;

    let now = chrono::Utc::now().naive_utc();

    for row in rows {
        // Cek pelanggaran first response time
        if row.first_reply_created_at.is_none() {
            if let Some(threshold_secs) = row.first_response_time_threshold {
                let elapsed_secs = (now - row.conversation_created_at).num_seconds() as f64;
                if elapsed_secs >= threshold_secs {
                    // Update status SLA menjadi breached (1)
                    sqlx::query("UPDATE applied_slas SET sla_status = 1, updated_at = NOW() WHERE id = $1")
                        .bind(row.applied_sla_id)
                        .execute(&state.db)
                        .await?;

                    // Catat event pelanggaran SLA di sla_events
                    sqlx::query(
                        r#"
                        INSERT INTO sla_events (
                            applied_sla_id, conversation_id, account_id, sla_policy_id,
                            inbox_id, event_type, created_at, updated_at
                        )
                        VALUES ($1, $2, $3, $4, $5, 1, NOW(), NOW())
                        "#,
                    )
                    .bind(row.applied_sla_id as i64)
                    .bind(row.conversation_id)
                    .bind(row.account_id)
                    .bind(row.sla_policy_id)
                    .bind(row.inbox_id as i64)
                    .execute(&state.db)
                    .await?;

                    info!(
                        "SLA breached for conversation display_id {} (account {}). Elapsed: {:.1}s / Threshold: {:.1}s",
                        row.display_id, row.account_id, elapsed_secs, threshold_secs
                    );

                    // Broadcast alert pelanggaran SLA via ActionCable
                    let event_payload = serde_json::json!({
                        "applied_sla_id": row.applied_sla_id,
                        "conversation_id": row.display_id,
                        "sla_policy_id": row.sla_policy_id,
                        "event_type": "first_response_breached",
                        "elapsed_seconds": elapsed_secs,
                        "threshold_seconds": threshold_secs
                    });

                    event_dispatcher::dispatch_event(
                        state,
                        row.account_id as i32,
                        "sla.breached",
                        event_payload,
                    );
                }
            }
        }
    }

    Ok(())
}
