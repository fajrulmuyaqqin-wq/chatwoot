use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::middleware::auth::CurrentUser;
use crate::middleware::rbac::{verify_account_access, AccountRole};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct FilterQuery {
    pub filter_type: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct CustomFilter {
    pub id: i64,
    pub name: String,
    pub filter_type: i32,
    pub query: serde_json::Value,
    pub account_id: i64,
    pub user_id: i64,
    pub created_at: chrono::NaiveDateTime,
    pub updated_at: chrono::NaiveDateTime,
}

#[derive(Debug, Deserialize)]
pub struct CreateFilterPayload {
    pub name: Option<String>,
    pub filter_type: Option<serde_json::Value>,
    pub query: Option<serde_json::Value>,
}

fn parse_filter_type(val: Option<&str>) -> i32 {
    match val {
        Some("contact") | Some("1") => 1,
        _ => 0, // default conversation
    }
}

pub async fn list_custom_filters(
    Path(account_id): Path<i32>,
    Query(query): Query<FilterQuery>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Agent).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let f_type = parse_filter_type(query.filter_type.as_deref());

    let filters = sqlx::query_as::<_, CustomFilter>(
        r#"
        SELECT id, name, filter_type, query, account_id, user_id, created_at, updated_at
        FROM custom_filters
        WHERE account_id = $1 AND user_id = $2 AND filter_type = $3
        ORDER BY created_at DESC
        "#,
    )
    .bind(account_id as i64)
    .bind(current_user.id as i64)
    .bind(f_type)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let res: Vec<serde_json::Value> = filters
        .into_iter()
        .map(|f| {
            let type_str = if f.filter_type == 1 {
                "contact"
            } else {
                "conversation"
            };
            json!({
                "id": f.id,
                "name": f.name,
                "filter_type": type_str,
                "query": f.query,
                "created_at": f.created_at,
                "updated_at": f.updated_at
            })
        })
        .collect();

    (StatusCode::OK, Json(json!(res)))
}

pub async fn create_custom_filter(
    Path(account_id): Path<i32>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    Json(body): Json<CreateFilterPayload>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Agent).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let filter_type_num = match body.filter_type {
        Some(serde_json::Value::String(ref s)) => parse_filter_type(Some(s)),
        Some(serde_json::Value::Number(ref n)) => n.as_i64().unwrap_or(0) as i32,
        _ => 0,
    };

    let name = body.name.unwrap_or_else(|| "My Filter".to_string());
    let query_val = body.query.unwrap_or(json!({}));

    let filter = sqlx::query_as::<_, CustomFilter>(
        r#"
        INSERT INTO custom_filters (name, filter_type, query, account_id, user_id, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, NOW(), NOW())
        RETURNING *
        "#,
    )
    .bind(name)
    .bind(filter_type_num)
    .bind(query_val)
    .bind(account_id as i64)
    .bind(current_user.id as i64)
    .fetch_one(&state.db)
    .await;

    match filter {
        Ok(f) => {
            let type_str = if f.filter_type == 1 {
                "contact"
            } else {
                "conversation"
            };
            (
                StatusCode::OK,
                Json(json!({
                    "id": f.id,
                    "name": f.name,
                    "filter_type": type_str,
                    "query": f.query,
                    "created_at": f.created_at,
                    "updated_at": f.updated_at
                })),
            )
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}

pub async fn delete_custom_filter(
    Path((account_id, id)): Path<(i32, i64)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Agent).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let _ = sqlx::query("DELETE FROM custom_filters WHERE id = $1 AND account_id = $2 AND user_id = $3")
        .bind(id)
        .bind(account_id as i64)
        .bind(current_user.id as i64)
        .execute(&state.db)
        .await;

    (StatusCode::NO_CONTENT, Json(json!({})))
}
