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

pub const VALID_PERMISSIONS: &[&str] = &[
    "conversation_manage",
    "conversation_unassigned_manage",
    "conversation_participating_manage",
    "contact_manage",
    "report_manage",
    "knowledge_base_manage",
];

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

#[derive(Debug, Deserialize, Clone, Default)]
pub struct CustomRoleData {
    pub name: Option<String>,
    pub description: Option<String>,
    pub permissions: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct CustomRolePayload {
    pub custom_role: Option<CustomRoleData>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub permissions: Option<Vec<String>>,
}

impl CustomRolePayload {
    pub fn get_name(&self) -> Option<String> {
        self.custom_role
            .as_ref()
            .and_then(|c| c.name.clone())
            .or_else(|| self.name.clone())
    }

    pub fn get_description(&self) -> Option<String> {
        self.custom_role
            .as_ref()
            .and_then(|c| c.description.clone())
            .or_else(|| self.description.clone())
    }

    pub fn get_permissions(&self) -> Option<Vec<String>> {
        self.custom_role
            .as_ref()
            .and_then(|c| c.permissions.clone())
            .or_else(|| self.permissions.clone())
    }
}

fn role_to_json(r: CustomRole) -> serde_json::Value {
    json!({
        "id": r.id,
        "account_id": r.account_id,
        "name": r.name,
        "description": r.description,
        "permissions": r.permissions,
        "created_at": r.created_at.and_utc().to_rfc3339(),
        "updated_at": r.updated_at.and_utc().to_rfc3339()
    })
}

fn validate_permissions(perms: &[String]) -> Result<(), String> {
    for p in perms {
        if !VALID_PERMISSIONS.contains(&p.as_str()) {
            return Err(format!("Invalid permission: '{}'. Allowed permissions are: {:?}", p, VALID_PERMISSIONS));
        }
    }
    Ok(())
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
    Json(body): Json<CustomRolePayload>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id as i32, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let name = match body.get_name() {
        Some(n) if !n.trim().is_empty() => n.trim().to_string(),
        _ => return (StatusCode::UNPROCESSABLE_ENTITY, Json(json!({ "error": "Name is required" }))),
    };

    let permissions = body.get_permissions().unwrap_or_default();
    if let Err(err_msg) = validate_permissions(&permissions) {
        return (StatusCode::UNPROCESSABLE_ENTITY, Json(json!({ "error": err_msg })));
    }

    let desc = body.get_description();

    let role = sqlx::query_as::<_, CustomRole>(
        r#"
        INSERT INTO custom_roles (account_id, name, description, permissions, created_at, updated_at)
        VALUES ($1, $2, $3, $4, NOW(), NOW())
        RETURNING id, account_id, name, description, permissions, created_at, updated_at
        "#,
    )
    .bind(account_id)
    .bind(name)
    .bind(desc)
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
    Json(body): Json<CustomRolePayload>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id as i32, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let permissions = body.get_permissions();
    if let Some(ref perms) = permissions {
        if let Err(err_msg) = validate_permissions(perms) {
            return (StatusCode::UNPROCESSABLE_ENTITY, Json(json!({ "error": err_msg })));
        }
    }

    let name = body.get_name();
    let desc = body.get_description();

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
    .bind(name)
    .bind(desc)
    .bind(permissions)
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

    // 1. Nullify on account_users so users don't hold dangling role references
    let _ = sqlx::query("UPDATE account_users SET custom_role_id = NULL WHERE account_id = $1 AND custom_role_id = $2")
        .bind(account_id)
        .bind(id)
        .execute(&state.db)
        .await;

    // 2. Delete the custom role
    let res = sqlx::query("DELETE FROM custom_roles WHERE account_id = $1 AND id = $2")
        .bind(account_id)
        .bind(id)
        .execute(&state.db)
        .await;

    match res {
        Ok(_) => (StatusCode::OK, Json(json!({ "success": true }))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}
