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
pub struct AccountSamlSettings {
    pub id: i64,
    pub account_id: i64,
    pub sso_url: Option<String>,
    pub certificate: Option<String>,
    pub sp_entity_id: Option<String>,
    pub idp_entity_id: Option<String>,
    pub role_mappings: Option<serde_json::Value>,
    pub metadata_url: Option<String>,
    pub metadata_refreshed_at: Option<chrono::NaiveDateTime>,
    pub created_at: chrono::NaiveDateTime,
    pub updated_at: chrono::NaiveDateTime,
}

#[derive(Debug, Deserialize)]
pub struct SamlSettingsPayload {
    pub sso_url: Option<String>,
    pub certificate: Option<String>,
    pub idp_entity_id: Option<String>,
    pub sp_entity_id: Option<String>,
    pub metadata_url: Option<String>,
    pub role_mappings: Option<serde_json::Value>,
}

fn saml_to_json(s: AccountSamlSettings) -> serde_json::Value {
    json!({
        "id": s.id,
        "account_id": s.account_id,
        "sso_url": s.sso_url,
        "certificate": s.certificate,
        "idp_entity_id": s.idp_entity_id,
        "sp_entity_id": s.sp_entity_id,
        "metadata_url": s.metadata_url,
        "role_mappings": s.role_mappings.unwrap_or_else(|| json!({})),
        "created_at": s.created_at.and_utc().timestamp(),
        "updated_at": s.updated_at.and_utc().timestamp()
    })
}

/// GET /api/v1/accounts/:account_id/saml_settings
pub async fn get_saml_settings(
    Path(account_id): Path<i64>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id as i32, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let settings = sqlx::query_as::<_, AccountSamlSettings>(
        r#"
        SELECT id, account_id, sso_url, certificate, sp_entity_id, idp_entity_id,
               role_mappings, metadata_url, metadata_refreshed_at, created_at, updated_at
        FROM account_saml_settings
        WHERE account_id = $1
        LIMIT 1
        "#,
    )
    .bind(account_id)
    .fetch_optional(&state.db)
    .await;

    match settings {
        Ok(Some(s)) => (StatusCode::OK, Json(saml_to_json(s))),
        _ => (StatusCode::NOT_FOUND, Json(json!({ "error": "SAML settings not found" }))),
    }
}

/// POST /api/v1/accounts/:account_id/saml_settings
pub async fn create_saml_settings(
    Path(account_id): Path<i64>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    Json(body): Json<SamlSettingsPayload>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id as i32, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let role_mappings = body.role_mappings.unwrap_or_else(|| json!({}));

    let settings = sqlx::query_as::<_, AccountSamlSettings>(
        r#"
        INSERT INTO account_saml_settings (account_id, sso_url, certificate, sp_entity_id, idp_entity_id, metadata_url, role_mappings, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, NOW(), NOW())
        RETURNING id, account_id, sso_url, certificate, sp_entity_id, idp_entity_id, role_mappings, metadata_url, metadata_refreshed_at, created_at, updated_at
        "#,
    )
    .bind(account_id)
    .bind(body.sso_url)
    .bind(body.certificate)
    .bind(body.sp_entity_id)
    .bind(body.idp_entity_id)
    .bind(body.metadata_url)
    .bind(role_mappings)
    .fetch_one(&state.db)
    .await;

    match settings {
        Ok(s) => (StatusCode::OK, Json(saml_to_json(s))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}

/// PATCH /api/v1/accounts/:account_id/saml_settings
pub async fn update_saml_settings(
    Path(account_id): Path<i64>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    Json(body): Json<SamlSettingsPayload>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id as i32, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let settings = sqlx::query_as::<_, AccountSamlSettings>(
        r#"
        UPDATE account_saml_settings
        SET sso_url = COALESCE($1, sso_url),
            certificate = COALESCE($2, certificate),
            sp_entity_id = COALESCE($3, sp_entity_id),
            idp_entity_id = COALESCE($4, idp_entity_id),
            metadata_url = COALESCE($5, metadata_url),
            role_mappings = COALESCE($6, role_mappings),
            updated_at = NOW()
        WHERE account_id = $7
        RETURNING id, account_id, sso_url, certificate, sp_entity_id, idp_entity_id, role_mappings, metadata_url, metadata_refreshed_at, created_at, updated_at
        "#,
    )
    .bind(body.sso_url)
    .bind(body.certificate)
    .bind(body.sp_entity_id)
    .bind(body.idp_entity_id)
    .bind(body.metadata_url)
    .bind(body.role_mappings)
    .bind(account_id)
    .fetch_optional(&state.db)
    .await;

    match settings {
        Ok(Some(s)) => (StatusCode::OK, Json(saml_to_json(s))),
        _ => (StatusCode::NOT_FOUND, Json(json!({ "error": "SAML settings not found" }))),
    }
}

/// DELETE /api/v1/accounts/:account_id/saml_settings
pub async fn delete_saml_settings(
    Path(account_id): Path<i64>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id as i32, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let _ = sqlx::query("DELETE FROM account_saml_settings WHERE account_id = $1")
        .bind(account_id)
        .execute(&state.db)
        .await;

    (StatusCode::NO_CONTENT, Json(json!({})))
}

/// POST /api/v1/accounts/:account_id/saml_settings/refresh
pub async fn refresh_saml_metadata(
    Path(account_id): Path<i64>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id as i32, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let _ = sqlx::query("UPDATE account_saml_settings SET metadata_refreshed_at = NOW(), updated_at = NOW() WHERE account_id = $1")
        .bind(account_id)
        .execute(&state.db)
        .await;

    let settings = sqlx::query_as::<_, AccountSamlSettings>(
        r#"
        SELECT id, account_id, sso_url, certificate, sp_entity_id, idp_entity_id, role_mappings, metadata_url, metadata_refreshed_at, created_at, updated_at
        FROM account_saml_settings
        WHERE account_id = $1
        "#,
    )
    .bind(account_id)
    .fetch_optional(&state.db)
    .await;

    match settings {
        Ok(Some(s)) => (StatusCode::OK, Json(saml_to_json(s))),
        _ => (StatusCode::NOT_FOUND, Json(json!({ "error": "SAML settings not found" }))),
    }
}
