use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use tracing::warn;

use crate::models::productivity::CannedResponse;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub search: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CannedResponsePayload {
    pub canned_response: Option<CannedResponseFields>,
    pub short_code: Option<String>,
    pub content: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CannedResponseFields {
    pub short_code: Option<String>,
    pub content: Option<String>,
}

/// GET /api/v1/accounts/:account_id/canned_responses
pub async fn list_canned_responses(
    Path(account_id): Path<i32>,
    Query(query): Query<SearchQuery>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let records = if let Some(ref search) = query.search {
        let pattern = format!("%{}%", search.trim());
        sqlx::query_as::<_, CannedResponse>(
            r#"
            SELECT * FROM canned_responses
            WHERE account_id = $1 AND (short_code ILIKE $2 OR content ILIKE $2)
            ORDER BY
                CASE
                    WHEN short_code ILIKE $3 THEN 1
                    WHEN short_code ILIKE $2 THEN 2
                    ELSE 3
                END,
                short_code ASC
            "#,
        )
        .bind(account_id)
        .bind(&pattern)
        .bind(format!("{}%", search.trim()))
        .fetch_all(&state.db)
        .await
    } else {
        sqlx::query_as::<_, CannedResponse>(
            "SELECT * FROM canned_responses WHERE account_id = $1 ORDER BY short_code ASC",
        )
        .bind(account_id)
        .fetch_all(&state.db)
        .await
    };

    match records {
        Ok(list) => (StatusCode::OK, Json(json!(list))),
        Err(e) => {
            warn!("Failed to fetch canned responses: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Database error" })),
            )
        }
    }
}

/// POST /api/v1/accounts/:account_id/canned_responses
pub async fn create_canned_response(
    Path(account_id): Path<i32>,
    State(state): State<AppState>,
    Json(body): Json<CannedResponsePayload>,
) -> impl IntoResponse {
    let short_code = body
        .canned_response
        .as_ref()
        .and_then(|c| c.short_code.clone())
        .or(body.short_code)
        .unwrap_or_default();

    let content = body
        .canned_response
        .as_ref()
        .and_then(|c| c.content.clone())
        .or(body.content)
        .unwrap_or_default();

    if short_code.trim().is_empty() || content.trim().is_empty() {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": "short_code and content are required" })),
        );
    }

    let result = sqlx::query_as::<_, CannedResponse>(
        r#"
        INSERT INTO canned_responses (account_id, short_code, content, created_at, updated_at)
        VALUES ($1, $2, $3, NOW(), NOW())
        RETURNING *
        "#,
    )
    .bind(account_id)
    .bind(&short_code)
    .bind(&content)
    .fetch_one(&state.db)
    .await;

    match result {
        Ok(created) => (StatusCode::OK, Json(json!(created))),
        Err(e) => {
            warn!("Failed to create canned response: {}", e);
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error": format!("Could not create canned response: {}", e) })),
            )
        }
    }
}

/// PUT/PATCH /api/v1/accounts/:account_id/canned_responses/:id
pub async fn update_canned_response(
    Path((account_id, id)): Path<(i32, i32)>,
    State(state): State<AppState>,
    Json(body): Json<CannedResponsePayload>,
) -> impl IntoResponse {
    let short_code = body
        .canned_response
        .as_ref()
        .and_then(|c| c.short_code.clone())
        .or(body.short_code);

    let content = body
        .canned_response
        .as_ref()
        .and_then(|c| c.content.clone())
        .or(body.content);

    let result = sqlx::query_as::<_, CannedResponse>(
        r#"
        UPDATE canned_responses
        SET short_code = COALESCE($1, short_code),
            content = COALESCE($2, content),
            updated_at = NOW()
        WHERE id = $3 AND account_id = $4
        RETURNING *
        "#,
    )
    .bind(short_code)
    .bind(content)
    .bind(id)
    .bind(account_id)
    .fetch_optional(&state.db)
    .await;

    match result {
        Ok(Some(updated)) => (StatusCode::OK, Json(json!(updated))),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Canned response not found" })),
        ),
        Err(e) => {
            warn!("Failed to update canned response: {}", e);
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error": format!("Could not update: {}", e) })),
            )
        }
    }
}

/// DELETE /api/v1/accounts/:account_id/canned_responses/:id
pub async fn delete_canned_response(
    Path((account_id, id)): Path<(i32, i32)>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let res = sqlx::query("DELETE FROM canned_responses WHERE id = $1 AND account_id = $2")
        .bind(id)
        .bind(account_id)
        .execute(&state.db)
        .await;

    match res {
        Ok(_) => (StatusCode::OK, Json(json!({ "status": "ok" }))),
        Err(e) => {
            warn!("Failed to delete canned response: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Failed to delete" })),
            )
        }
    }
}
