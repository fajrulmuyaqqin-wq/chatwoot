use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::middleware::auth::CurrentUser;
use crate::middleware::rbac::{verify_account_access, AccountRole};
use crate::state::AppState;

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct CustomRole {
    pub id: i64,
    pub account_id: i64,
    pub name: String,
    pub description: Option<String>,
    pub permissions: Vec<String>,
    pub created_at: chrono::NaiveDateTime,
    pub updated_at: chrono::NaiveDateTime,
}

#[derive(Debug, Deserialize)]
pub struct CreateCustomRolePayload {
    pub name: Option<String>,
    pub description: Option<String>,
    pub permissions: Option<Vec<String>>,
}

fn role_to_json(r: CustomRole) -> serde_json::Value {
    json!({
        "id": r.id,
        "account_id": r.account_id,
        "name": r.name,
        "description": r.description,
        "permissions": r.permissions,
        "created_at": r.created_at.and_utc().timestamp(),
        "updated_at": r.updated_at.and_utc().timestamp()
    })
}

/// GET /api/v1/accounts/:account_id/custom_roles
pub async fn list_custom_roles(
    Path(account_id): Path<i64>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id as i32, AccountRole::Agent).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let roles = sqlx::query_as::<_, CustomRole>(
        r#"
        SELECT id, account_id, name, description, permissions, created_at, updated_at
        FROM custom_roles
        WHERE account_id = $1
        ORDER BY id ASC
        "#,
    )
    .bind(account_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let list: Vec<serde_json::Value> = roles.into_iter().map(role_to_json).collect();
    (StatusCode::OK, Json(serde_json::Value::Array(list)))
}

/// POST /api/v1/accounts/:account_id/custom_roles
pub async fn create_custom_role(
    Path(account_id): Path<i64>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    Json(body): Json<CreateCustomRolePayload>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id as i32, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let name = match body.name {
        Some(n) if !n.trim().is_empty() => n,
        _ => return (StatusCode::UNPROCESSABLE_ENTITY, Json(json!({ "error": "Name is required" }))),
    };

    let permissions = body.permissions.unwrap_or_default();

    let role = sqlx::query_as::<_, CustomRole>(
        r#"
        INSERT INTO custom_roles (account_id, name, description, permissions, created_at, updated_at)
        VALUES ($1, $2, $3, $4, NOW(), NOW())
        RETURNING id, account_id, name, description, permissions, created_at, updated_at
        "#,
    )
    .bind(account_id)
    .bind(name)
    .bind(body.description)
    .bind(&permissions)
    .fetch_one(&state.db)
    .await;

    match role {
        Ok(r) => (StatusCode::OK, Json(role_to_json(r))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}

/// GET /api/v1/accounts/:account_id/custom_roles/:id
pub async fn get_custom_role(
    Path((account_id, id)): Path<(i64, i64)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id as i32, AccountRole::Agent).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let role = sqlx::query_as::<_, CustomRole>(
        r#"
        SELECT id, account_id, name, description, permissions, created_at, updated_at
        FROM custom_roles
        WHERE account_id = $1 AND id = $2
        "#,
    )
    .bind(account_id)
    .bind(id)
    .fetch_optional(&state.db)
    .await;

    match role {
        Ok(Some(r)) => (StatusCode::OK, Json(role_to_json(r))),
        _ => (StatusCode::NOT_FOUND, Json(json!({ "error": "Custom role not found" }))),
    }
}

/// PATCH /api/v1/accounts/:account_id/custom_roles/:id
pub async fn update_custom_role(
    Path((account_id, id)): Path<(i64, i64)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    Json(body): Json<CreateCustomRolePayload>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id as i32, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let role = sqlx::query_as::<_, CustomRole>(
        r#"
        UPDATE custom_roles
        SET name = COALESCE($1, name),
            description = COALESCE($2, description),
            permissions = COALESCE($3, permissions),
            updated_at = NOW()
        WHERE account_id = $4 AND id = $5
        RETURNING id, account_id, name, description, permissions, created_at, updated_at
        "#,
    )
    .bind(body.name)
    .bind(body.description)
    .bind(body.permissions)
    .bind(account_id)
    .bind(id)
    .fetch_optional(&state.db)
    .await;

    match role {
        Ok(Some(r)) => (StatusCode::OK, Json(role_to_json(r))),
        _ => (StatusCode::NOT_FOUND, Json(json!({ "error": "Custom role not found" }))),
    }
}

/// DELETE /api/v1/accounts/:account_id/custom_roles/:id
pub async fn delete_custom_role(
    Path((account_id, id)): Path<(i64, i64)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id as i32, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let _ = sqlx::query("DELETE FROM custom_roles WHERE account_id = $1 AND id = $2")
        .bind(account_id)
        .bind(id)
        .execute(&state.db)
        .await;

    (StatusCode::OK, Json(json!({ "success": true })))
}
