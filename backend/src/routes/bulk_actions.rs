use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use tracing::{error, info};

use crate::state::{AppState, CableBroadcastMsg};

#[derive(Debug, Deserialize)]
pub struct BulkFields {
    pub status: Option<String>,
    pub assignee_id: Option<Option<i32>>,
    pub team_id: Option<Option<i32>>,
    pub labels: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct BulkActionsPayload {
    pub r#type: String, // "Conversation" atau "Contact"
    pub ids: Vec<i64>,
    pub fields: Option<BulkFields>,
    pub action_name: Option<String>,
}

pub async fn perform_bulk_action(
    Path(account_id): Path<i32>,
    State(state): State<AppState>,
    Json(payload): Json<BulkActionsPayload>,
) -> impl IntoResponse {
    let normalized_type = payload.r#type.to_lowercase();
    let ids: Vec<i32> = payload.ids.into_iter().map(|id| id as i32).collect();

    if ids.is_empty() {
        return (
            StatusCode::OK,
            Json(json!({ "success": true, "affected_count": 0 })),
        );
    }

    match normalized_type.as_str() {
        "conversation" => {
            let mut updated_count: u64 = 0;

            if let Some(fields) = payload.fields {
                // 1. Update status
                if let Some(status_str) = fields.status {
                    let status_code: i32 = match status_str.to_lowercase().as_str() {
                        "open" => 0,
                        "resolved" => 1,
                        "pending" => 2,
                        "snoozed" => 3,
                        _ => 0,
                    };

                    match sqlx::query(
                        r#"
                        UPDATE conversations
                        SET status = $1, updated_at = NOW()
                        WHERE account_id = $2 AND id = ANY($3)
                        "#
                    )
                    .bind(status_code)
                    .bind(account_id)
                    .bind(&ids)
                    .execute(&state.db)
                    .await
                    {
                        Ok(res) => updated_count = res.rows_affected(),
                        Err(e) => error!("Bulk update status error: {:?}", e),
                    }
                }

                // 2. Update assignee
                if let Some(assignee_opt) = fields.assignee_id {
                    let _ = sqlx::query(
                        r#"
                        UPDATE conversations
                        SET assignee_id = $1, updated_at = NOW()
                        WHERE account_id = $2 AND id = ANY($3)
                        "#
                    )
                    .bind(assignee_opt)
                    .bind(account_id)
                    .bind(&ids)
                    .execute(&state.db)
                    .await;
                }

                // 3. Update team
                if let Some(team_opt) = fields.team_id {
                    let team_id = team_opt.filter(|&id| id > 0);
                    let _ = sqlx::query(
                        r#"
                        UPDATE conversations
                        SET team_id = $1, updated_at = NOW()
                        WHERE account_id = $2 AND id = ANY($3)
                        "#
                    )
                    .bind(team_id)
                    .bind(account_id)
                    .bind(&ids)
                    .execute(&state.db)
                    .await;
                }

                // 4. Update labels jika disediakan
                if let Some(labels) = fields.labels {
                    for label in &labels {
                        // Pastikan label ada di tabel labels
                        let label_row = sqlx::query_scalar::<_, i32>(
                            "SELECT id FROM labels WHERE account_id = $1 AND title = $2"
                        )
                        .bind(account_id)
                        .bind(label)
                        .fetch_optional(&state.db)
                        .await
                        .ok()
                        .flatten();

                        let tag_id = match label_row {
                            Some(lid) => lid,
                            None => {
                                sqlx::query_scalar::<_, i32>(
                                    "INSERT INTO labels (account_id, title, color, created_at, updated_at) VALUES ($1, $2, '#1f93ff', NOW(), NOW()) RETURNING id"
                                )
                                .bind(account_id)
                                .bind(label)
                                .fetch_one(&state.db)
                                .await
                                .unwrap_or(1)
                            }
                        };

                        for &cid in &ids {
                            let _ = sqlx::query(
                                r#"
                                INSERT INTO taggings (tag_id, taggable_type, taggable_id, tagger_type, tagger_id, context, created_at)
                                VALUES ($1, 'Conversation', $2, 'User', 1, 'labels', NOW())
                                ON CONFLICT DO NOTHING
                                "#
                            )
                            .bind(tag_id)
                            .bind(cid)
                            .execute(&state.db)
                            .await;
                        }
                    }
                }
            }

            // Broadcast websocket event
            for &cid in &ids {
                let _ = state.broadcast_tx.send(CableBroadcastMsg {
                    account_id,
                    event: "conversation.updated".to_string(),
                    data: json!({ "id": cid }),
                });
            }

            info!(
                account_id = account_id,
                ids_count = ids.len(),
                "Berhasil mengeksekusi bulk action conversation"
            );

            (
                StatusCode::OK,
                Json(json!({
                    "success": true,
                    "type": "Conversation",
                    "affected_count": updated_count
                })),
            )
        }
        "contact" => {
            if payload.action_name.as_deref() == Some("delete") {
                let rows = match sqlx::query(
                    "DELETE FROM contacts WHERE account_id = $1 AND id = ANY($2)"
                )
                .bind(account_id)
                .bind(&ids)
                .execute(&state.db)
                .await
                {
                    Ok(res) => res.rows_affected(),
                    Err(e) => {
                        error!("Bulk delete contacts error: {:?}", e);
                        0
                    }
                };

                for &cid in &ids {
                    let _ = state.broadcast_tx.send(CableBroadcastMsg {
                        account_id,
                        event: "contact.deleted".to_string(),
                        data: json!({ "id": cid }),
                    });
                }

                (
                    StatusCode::OK,
                    Json(json!({
                        "success": true,
                        "type": "Contact",
                        "deleted_count": rows
                    })),
                )
            } else {
                (
                    StatusCode::OK,
                    Json(json!({ "success": true, "type": "Contact" })),
                )
            }
        }
        _ => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": "Invalid bulk action type. Must be 'Conversation' or 'Contact'" })),
        ),
    }
}
