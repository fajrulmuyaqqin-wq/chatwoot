use crate::models::webhook::{CreateWebhookPayload, UpdateWebhookPayload, Webhook};
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde_json::json;
use tracing::warn;

/// GET /api/v1/accounts/:account_id/webhooks
pub async fn list_webhooks(
    Path(account_id): Path<i32>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let webhooks = sqlx::query_as::<_, Webhook>(
        "SELECT * FROM webhooks WHERE account_id = $1 ORDER BY id ASC",
    )
    .bind(account_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    (StatusCode::OK, Json(json!({ "payload": webhooks })))
}

/// POST /api/v1/accounts/:account_id/webhooks
pub async fn create_webhook(
    Path(account_id): Path<i32>,
    State(state): State<AppState>,
    Json(body): Json<CreateWebhookPayload>,
) -> impl IntoResponse {
    let default_subs = json!([
        "conversation_status_changed",
        "conversation_updated",
        "conversation_created",
        "contact_created",
        "contact_updated",
        "message_created",
        "message_updated",
        "webwidget_triggered"
    ]);

    let subs_json = body
        .subscriptions
        .map(|s| json!(s))
        .unwrap_or(default_subs);

    let result = sqlx::query_as::<_, Webhook>(
        r#"
        INSERT INTO webhooks (account_id, inbox_id, name, url, subscriptions, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, NOW(), NOW())
        RETURNING *
        "#,
    )
    .bind(account_id)
    .bind(body.inbox_id)
    .bind(body.name)
    .bind(&body.url)
    .bind(subs_json)
    .fetch_one(&state.db)
    .await;

    match result {
        Ok(wh) => (StatusCode::OK, Json(json!({ "payload": { "webhook": wh } }))),
        Err(e) => {
            warn!("Failed to create webhook: {}", e);
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error": format!("Could not create webhook: {}", e) })),
            )
        }
    }
}

/// PATCH /api/v1/accounts/:account_id/webhooks/:id
pub async fn update_webhook(
    Path((account_id, id)): Path<(i32, i64)>,
    State(state): State<AppState>,
    Json(body): Json<UpdateWebhookPayload>,
) -> impl IntoResponse {
    let subs_json = body.subscriptions.map(|s| json!(s));

    let result = sqlx::query_as::<_, Webhook>(
        r#"
        UPDATE webhooks
        SET inbox_id = COALESCE($1, inbox_id),
            name = COALESCE($2, name),
            url = COALESCE($3, url),
            subscriptions = COALESCE($4, subscriptions),
            updated_at = NOW()
        WHERE id = $5 AND account_id = $6
        RETURNING *
        "#,
    )
    .bind(body.inbox_id)
    .bind(body.name)
    .bind(body.url)
    .bind(subs_json)
    .bind(id)
    .bind(account_id)
    .fetch_optional(&state.db)
    .await;

    match result {
        Ok(Some(wh)) => (StatusCode::OK, Json(json!({ "payload": { "webhook": wh } }))),
        Ok(None) => (StatusCode::NOT_FOUND, Json(json!({ "error": "Webhook not found" }))),
        Err(e) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}

/// DELETE /api/v1/accounts/:account_id/webhooks/:id
pub async fn delete_webhook(
    Path((account_id, id)): Path<(i32, i64)>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let _ = sqlx::query("DELETE FROM webhooks WHERE id = $1 AND account_id = $2")
        .bind(id)
        .bind(account_id)
        .execute(&state.db)
        .await;

    (StatusCode::OK, Json(json!({ "status": "ok" })))
}
