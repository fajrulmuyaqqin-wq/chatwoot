use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Extension, Json,
};
use serde::Deserialize;
use serde_json::json;
use tracing::warn;

use crate::middleware::auth::CurrentUser;
use crate::models::productivity::Macro;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct MacroPayload {
    pub name: Option<String>,
    pub visibility: Option<serde_json::Value>,
    pub actions: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct MacroExecutePayload {
    pub conversation_ids: Option<Vec<i32>>,
}

/// GET /api/v1/accounts/:account_id/macros
pub async fn list_macros(
    Path(account_id): Path<i32>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let result = sqlx::query_as::<_, Macro>(
        "SELECT * FROM macros WHERE account_id = $1 ORDER BY id ASC",
    )
    .bind(account_id as i64)
    .fetch_all(&state.db)
    .await;

    match result {
        Ok(list) => (StatusCode::OK, Json(json!({ "payload": list }))),
        Err(e) => {
            warn!("Failed to fetch macros: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Database error" })),
            )
        }
    }
}

/// POST /api/v1/accounts/:account_id/macros
pub async fn create_macro(
    Path(account_id): Path<i32>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    Json(body): Json<MacroPayload>,
) -> impl IntoResponse {
    let name = body.name.unwrap_or_default();
    if name.trim().is_empty() {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": "name is required" })),
        );
    }

    let visibility_int = match body.visibility {
        Some(serde_json::Value::Number(n)) => n.as_i64().unwrap_or(0) as i32,
        Some(serde_json::Value::String(ref s)) if s == "global" => 1,
        _ => 0,
    };

    let actions = body.actions.unwrap_or_else(|| json!([]));

    let result = sqlx::query_as::<_, Macro>(
        r#"
        INSERT INTO macros (
            account_id, name, visibility, created_by_id, updated_by_id,
            actions, created_at, updated_at
        )
        VALUES ($1, $2, $3, $4, $4, $5, NOW(), NOW())
        RETURNING *
        "#,
    )
    .bind(account_id as i64)
    .bind(&name)
    .bind(visibility_int)
    .bind(current_user.id as i64)
    .bind(actions)
    .fetch_one(&state.db)
    .await;

    match result {
        Ok(created) => (StatusCode::OK, Json(json!({ "payload": created }))),
        Err(e) => {
            warn!("Failed to create macro: {}", e);
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error": format!("Could not create macro: {}", e) })),
            )
        }
    }
}

/// PUT/PATCH /api/v1/accounts/:account_id/macros/:id
pub async fn update_macro(
    Path((account_id, id)): Path<(i32, i64)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    Json(body): Json<MacroPayload>,
) -> impl IntoResponse {
    let visibility_int = body.visibility.as_ref().map(|v| match v {
        serde_json::Value::Number(n) => n.as_i64().unwrap_or(0) as i32,
        serde_json::Value::String(s) if s == "global" => 1,
        _ => 0,
    });

    let result = sqlx::query_as::<_, Macro>(
        r#"
        UPDATE macros
        SET name = COALESCE($1, name),
            visibility = COALESCE($2, visibility),
            actions = COALESCE($3, actions),
            updated_by_id = $4,
            updated_at = NOW()
        WHERE id = $5 AND account_id = $6
        RETURNING *
        "#,
    )
    .bind(body.name)
    .bind(visibility_int)
    .bind(body.actions)
    .bind(current_user.id as i64)
    .bind(id)
    .bind(account_id as i64)
    .fetch_optional(&state.db)
    .await;

    match result {
        Ok(Some(updated)) => (StatusCode::OK, Json(json!({ "payload": updated }))),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Macro not found" })),
        ),
        Err(e) => {
            warn!("Failed to update macro: {}", e);
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error": format!("Could not update macro: {}", e) })),
            )
        }
    }
}

/// DELETE /api/v1/accounts/:account_id/macros/:id
pub async fn delete_macro(
    Path((account_id, id)): Path<(i32, i64)>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let res = sqlx::query("DELETE FROM macros WHERE id = $1 AND account_id = $2")
        .bind(id)
        .bind(account_id as i64)
        .execute(&state.db)
        .await;

    match res {
        Ok(_) => (StatusCode::OK, Json(json!({ "status": "ok" }))),
        Err(e) => {
            warn!("Failed to delete macro: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Failed to delete" })),
            )
        }
    }
}

/// POST /api/v1/accounts/:account_id/macros/:id/execute
/// Eksekusi otomatisasi macro terhadap daftar conversation_ids
pub async fn execute_macro(
    Path((account_id, id)): Path<(i32, i64)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    Json(body): Json<MacroExecutePayload>,
) -> impl IntoResponse {
    // 1. Ambil Macro
    let macro_item = match sqlx::query_as::<_, Macro>(
        "SELECT * FROM macros WHERE id = $1 AND account_id = $2",
    )
    .bind(id)
    .bind(account_id as i64)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(m)) => m,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Macro not found" })),
            );
        }
    };

    let conv_display_ids = body.conversation_ids.unwrap_or_default();
    if conv_display_ids.is_empty() {
        return (StatusCode::OK, Json(json!({ "status": "no_conversations" })));
    }

    let actions_list = match macro_item.actions.as_array() {
        Some(arr) => arr.clone(),
        None => return (StatusCode::OK, Json(json!({ "status": "no_actions" }))),
    };

    let user_id = current_user.id as i64;
    let _ = crate::services::macro_executor::execute_macro_actions(
        &state,
        account_id,
        user_id,
        &actions_list,
        &conv_display_ids,
    )
    .await;

    (StatusCode::OK, Json(json!({ "status": "executed" })))
}
