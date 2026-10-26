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
pub struct AgentCapacityPolicy {
    pub id: i64,
    pub account_id: i64,
    pub name: String,
    pub description: Option<String>,
    pub exclusion_rules: serde_json::Value,
    pub created_at: chrono::NaiveDateTime,
    pub updated_at: chrono::NaiveDateTime,
}

#[derive(Debug, Deserialize)]
pub struct CreatePolicyPayload {
    pub name: Option<String>,
    pub description: Option<String>,
    pub exclusion_rules: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct InboxLimitPayload {
    pub inbox_id: Option<i64>,
    pub conversation_limit: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct UserAssignmentPayload {
    pub user_id: Option<i64>,
}

async fn policy_to_json(p: AgentCapacityPolicy, db: &sqlx::PgPool) -> serde_json::Value {
    let assigned_agent_count: i64 = sqlx::query_scalar!(
        "SELECT COUNT(*)::int8 FROM account_users WHERE agent_capacity_policy_id = $1",
        p.id
    )
    .fetch_one(db)
    .await
    .unwrap_or(Some(0))
    .unwrap_or(0);

    let limits = sqlx::query!(
        "SELECT id, inbox_id, conversation_limit FROM inbox_capacity_limits WHERE agent_capacity_policy_id = $1",
        p.id
    )
    .fetch_all(db)
    .await
    .unwrap_or_default();

    let limits_json: Vec<_> = limits
        .into_iter()
        .map(|l| {
            json!({
                "id": l.id,
                "inbox_id": l.inbox_id,
                "conversation_limit": l.conversation_limit
            })
        })
        .collect();

    json!({
        "id": p.id,
        "name": p.name,
        "description": p.description,
        "exclusion_rules": p.exclusion_rules,
        "account_id": p.account_id,
        "assigned_agent_count": assigned_agent_count,
        "inbox_capacity_limits": limits_json,
        "created_at": p.created_at.and_utc().timestamp(),
        "updated_at": p.updated_at.and_utc().timestamp()
    })
}

/// GET /api/v1/accounts/:account_id/agent_capacity_policies
pub async fn list_policies(
    Path(account_id): Path<i64>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id as i32, AccountRole::Agent).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let policies = sqlx::query_as::<_, AgentCapacityPolicy>(
        r#"
        SELECT id, account_id, name, description, exclusion_rules, created_at, updated_at
        FROM agent_capacity_policies
        WHERE account_id = $1
        ORDER BY id ASC
        "#,
    )
    .bind(account_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let mut list = Vec::new();
    for p in policies {
        list.push(policy_to_json(p, &state.db).await);
    }

    (StatusCode::OK, Json(serde_json::Value::Array(list)))
}

/// POST /api/v1/accounts/:account_id/agent_capacity_policies
pub async fn create_policy(
    Path(account_id): Path<i64>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    Json(body): Json<CreatePolicyPayload>,
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

    let exclusion_rules = body.exclusion_rules.unwrap_or_else(|| json!({}));

    let policy = sqlx::query_as::<_, AgentCapacityPolicy>(
        r#"
        INSERT INTO agent_capacity_policies (account_id, name, description, exclusion_rules, created_at, updated_at)
        VALUES ($1, $2, $3, $4, NOW(), NOW())
        RETURNING id, account_id, name, description, exclusion_rules, created_at, updated_at
        "#,
    )
    .bind(account_id)
    .bind(name)
    .bind(body.description)
    .bind(exclusion_rules)
    .fetch_one(&state.db)
    .await;

    match policy {
        Ok(p) => (StatusCode::OK, Json(policy_to_json(p, &state.db).await)),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}

/// GET /api/v1/accounts/:account_id/agent_capacity_policies/:id
pub async fn get_policy(
    Path((account_id, id)): Path<(i64, i64)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id as i32, AccountRole::Agent).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let policy = sqlx::query_as::<_, AgentCapacityPolicy>(
        r#"
        SELECT id, account_id, name, description, exclusion_rules, created_at, updated_at
        FROM agent_capacity_policies
        WHERE account_id = $1 AND id = $2
        "#,
    )
    .bind(account_id)
    .bind(id)
    .fetch_optional(&state.db)
    .await;

    match policy {
        Ok(Some(p)) => (StatusCode::OK, Json(policy_to_json(p, &state.db).await)),
        _ => (StatusCode::NOT_FOUND, Json(json!({ "error": "Policy not found" }))),
    }
}

/// PATCH /api/v1/accounts/:account_id/agent_capacity_policies/:id
pub async fn update_policy(
    Path((account_id, id)): Path<(i64, i64)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    Json(body): Json<CreatePolicyPayload>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id as i32, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let policy = sqlx::query_as::<_, AgentCapacityPolicy>(
        r#"
        UPDATE agent_capacity_policies
        SET name = COALESCE($1, name),
            description = COALESCE($2, description),
            exclusion_rules = COALESCE($3, exclusion_rules),
            updated_at = NOW()
        WHERE account_id = $4 AND id = $5
        RETURNING id, account_id, name, description, exclusion_rules, created_at, updated_at
        "#,
    )
    .bind(body.name)
    .bind(body.description)
    .bind(body.exclusion_rules)
    .bind(account_id)
    .bind(id)
    .fetch_optional(&state.db)
    .await;

    match policy {
        Ok(Some(p)) => (StatusCode::OK, Json(policy_to_json(p, &state.db).await)),
        _ => (StatusCode::NOT_FOUND, Json(json!({ "error": "Policy not found" }))),
    }
}

/// DELETE /api/v1/accounts/:account_id/agent_capacity_policies/:id
pub async fn delete_policy(
    Path((account_id, id)): Path<(i64, i64)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id as i32, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let _ = sqlx::query("UPDATE account_users SET agent_capacity_policy_id = NULL WHERE agent_capacity_policy_id = $1")
        .bind(id)
        .execute(&state.db)
        .await;

    let _ = sqlx::query("DELETE FROM agent_capacity_policies WHERE account_id = $1 AND id = $2")
        .bind(account_id)
        .bind(id)
        .execute(&state.db)
        .await;

    (StatusCode::OK, Json(json!({ "success": true })))
}

// ======================== INBOX CAPACITY LIMITS ========================

/// POST /api/v1/accounts/:account_id/agent_capacity_policies/:id/inbox_limits
pub async fn create_inbox_limit(
    Path((_account_id, policy_id)): Path<(i64, i64)>,
    State(state): State<AppState>,
    Json(body): Json<InboxLimitPayload>,
) -> impl IntoResponse {
    let inbox_id = match body.inbox_id {
        Some(id) => id,
        None => return (StatusCode::UNPROCESSABLE_ENTITY, Json(json!({ "error": "inbox_id is required" }))),
    };
    let limit = body.conversation_limit.unwrap_or(5);

    let res = sqlx::query!(
        r#"
        INSERT INTO inbox_capacity_limits (agent_capacity_policy_id, inbox_id, conversation_limit, created_at, updated_at)
        VALUES ($1, $2, $3, NOW(), NOW())
        RETURNING id, inbox_id, conversation_limit
        "#,
        policy_id,
        inbox_id,
        limit
    )
    .fetch_one(&state.db)
    .await;

    match res {
        Ok(r) => (
            StatusCode::OK,
            Json(json!({
                "id": r.id,
                "inbox_id": r.inbox_id,
                "conversation_limit": r.conversation_limit
            })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}

/// PATCH /api/v1/accounts/:account_id/agent_capacity_policies/:id/inbox_limits/:limit_id
pub async fn update_inbox_limit(
    Path((_account_id, policy_id, limit_id)): Path<(i64, i64, i64)>,
    State(state): State<AppState>,
    Json(body): Json<InboxLimitPayload>,
) -> impl IntoResponse {
    let limit = body.conversation_limit.unwrap_or(5);

    let res = sqlx::query!(
        r#"
        UPDATE inbox_capacity_limits
        SET conversation_limit = $1, updated_at = NOW()
        WHERE agent_capacity_policy_id = $2 AND id = $3
        RETURNING id, inbox_id, conversation_limit
        "#,
        limit,
        policy_id,
        limit_id
    )
    .fetch_optional(&state.db)
    .await;

    match res {
        Ok(Some(r)) => (
            StatusCode::OK,
            Json(json!({
                "id": r.id,
                "inbox_id": r.inbox_id,
                "conversation_limit": r.conversation_limit
            })),
        ),
        _ => (StatusCode::NOT_FOUND, Json(json!({ "error": "Inbox limit not found" }))),
    }
}

/// DELETE /api/v1/accounts/:account_id/agent_capacity_policies/:id/inbox_limits/:limit_id
pub async fn delete_inbox_limit(
    Path((_account_id, policy_id, limit_id)): Path<(i64, i64, i64)>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let _ = sqlx::query("DELETE FROM inbox_capacity_limits WHERE agent_capacity_policy_id = $1 AND id = $2")
        .bind(policy_id)
        .bind(limit_id)
        .execute(&state.db)
        .await;

    (StatusCode::NO_CONTENT, Json(json!({})))
}

// ======================== POLICY USERS ========================

/// GET /api/v1/accounts/:account_id/agent_capacity_policies/:id/users
pub async fn list_policy_users(
    Path((account_id, policy_id)): Path<(i64, i64)>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let users = sqlx::query!(
        r#"
        SELECT u.id, u.name, u.email, au.role
        FROM users u
        JOIN account_users au ON au.user_id = u.id
        WHERE au.account_id = $1 AND au.agent_capacity_policy_id = $2
        "#,
        account_id,
        policy_id
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let payload: Vec<_> = users
        .into_iter()
        .map(|u| {
            json!({
                "id": u.id,
                "name": u.name,
                "email": u.email,
                "role": u.role
            })
        })
        .collect();

    (StatusCode::OK, Json(serde_json::Value::Array(payload)))
}

/// POST /api/v1/accounts/:account_id/agent_capacity_policies/:id/users
pub async fn add_policy_user(
    Path((account_id, policy_id)): Path<(i64, i64)>,
    State(state): State<AppState>,
    Json(body): Json<UserAssignmentPayload>,
) -> impl IntoResponse {
    let user_id = match body.user_id {
        Some(uid) => uid,
        None => return (StatusCode::UNPROCESSABLE_ENTITY, Json(json!({ "error": "user_id is required" }))),
    };

    let res = sqlx::query(
        "UPDATE account_users SET agent_capacity_policy_id = $1 WHERE account_id = $2 AND user_id = $3",
    )
    .bind(policy_id)
    .bind(account_id)
    .bind(user_id)
    .execute(&state.db)
    .await;

    match res {
        Ok(_) => (StatusCode::OK, Json(json!({ "success": true }))),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))),
    }
}

/// DELETE /api/v1/accounts/:account_id/agent_capacity_policies/:id/users/:user_id
pub async fn remove_policy_user(
    Path((account_id, policy_id, user_id)): Path<(i64, i64, i64)>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let _ = sqlx::query(
        "UPDATE account_users SET agent_capacity_policy_id = NULL WHERE account_id = $1 AND agent_capacity_policy_id = $2 AND user_id = $3",
    )
    .bind(account_id)
    .bind(policy_id)
    .bind(user_id)
    .execute(&state.db)
    .await;

    (StatusCode::OK, Json(json!({ "success": true })))
}
