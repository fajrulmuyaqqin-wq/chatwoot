use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;

use crate::models::conversation::Conversation;
use crate::models::sla::{AppliedSla, SlaPolicy};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct CreateSlaPayload {
    pub name: String,
    pub description: Option<String>,
    pub first_response_time_threshold: Option<f64>,
    pub resolution_time_threshold: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct ApplySlaPayload {
    pub sla_policy_id: i64,
}

/// GET /api/v1/accounts/:account_id/sla_policies
pub async fn list_sla_policies(
    Path(account_id): Path<i32>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let policies = sqlx::query_as::<_, SlaPolicy>(
        "SELECT * FROM sla_policies WHERE account_id = $1 ORDER BY id DESC",
    )
    .bind(account_id as i64)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    (StatusCode::OK, Json(json!({ "payload": policies })))
}

/// POST /api/v1/accounts/:account_id/sla_policies
pub async fn create_sla_policy(
    Path(account_id): Path<i32>,
    State(state): State<AppState>,
    Json(payload): Json<CreateSlaPayload>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        crate::services::tier::verify_feature_access(&state.db, account_id as i64, "sla").await
    {
        return (
            code,
            Json(json!({ "error": msg, "error_code": "TIER_FEATURE_RESTRICTED" })),
        );
    }

    let policy = match sqlx::query_as::<_, SlaPolicy>(
        r#"
        INSERT INTO sla_policies (
            name, description, first_response_time_threshold,
            resolution_time_threshold, account_id, created_at, updated_at
        )
        VALUES ($1, $2, $3, $4, $5, NOW(), NOW())
        RETURNING *
        "#,
    )
    .bind(&payload.name)
    .bind(&payload.description)
    .bind(payload.first_response_time_threshold)
    .bind(payload.resolution_time_threshold)
    .bind(account_id as i64)
    .fetch_one(&state.db)
    .await
    {
        Ok(p) => p,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Failed to create SLA policy: {}", e) })),
            );
        }
    };

    (StatusCode::CREATED, Json(json!(policy)))
}

/// POST /api/v1/accounts/:account_id/conversations/:display_id/apply_sla
pub async fn apply_sla_to_conversation(
    Path((account_id, display_id)): Path<(i32, i32)>,
    State(state): State<AppState>,
    Json(payload): Json<ApplySlaPayload>,
) -> impl IntoResponse {
    let conv = match sqlx::query_as::<_, Conversation>(
        "SELECT * FROM conversations WHERE account_id = $1 AND display_id = $2",
    )
    .bind(account_id)
    .bind(display_id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(c)) => c,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Conversation not found" })),
            );
        }
    };

    let applied = match sqlx::query_as::<_, AppliedSla>(
        r#"
        INSERT INTO applied_slas (
            account_id, sla_policy_id, conversation_id, sla_status, created_at, updated_at
        )
        VALUES ($1, $2, $3, 0, NOW(), NOW())
        ON CONFLICT ON CONSTRAINT unique_account_sla_conversation DO UPDATE
        SET sla_status = 0, updated_at = NOW()
        RETURNING *
        "#,
    )
    .bind(account_id as i64)
    .bind(payload.sla_policy_id)
    .bind(conv.id as i64)
    .fetch_one(&state.db)
    .await
    {
        Ok(a) => a,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Failed to apply SLA: {}", e) })),
            );
        }
    };

    (StatusCode::OK, Json(json!(applied)))
}
