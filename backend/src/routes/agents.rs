use crate::middleware::auth::CurrentUser;
use crate::middleware::rbac::{verify_account_access, AccountRole};
use crate::models::agent::{AgentResponse, CreateAgentPayload, UpdateAgentPayload};
use crate::state::AppState;
use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde_json::json;
use tracing::warn;

/// GET /api/v1/accounts/:account_id/agents
pub async fn list_agents(
    Path(account_id): Path<i32>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) = verify_account_access(&state.db, &current_user, account_id, AccountRole::Agent).await {
        return (code, Json(json!({ "error": msg })));
    }

    let rows = sqlx::query!(
        r#"
        SELECT u.id, u.name, u.email,
               CASE au.role WHEN 1 THEN 'administrator' ELSE 'agent' END as role,
               CASE au.availability 
                   WHEN 1 THEN 'offline' 
                   WHEN 2 THEN 'busy' 
                   ELSE 'online' 
               END as availability_status,
               au.auto_offline,
               (u.confirmed_at IS NOT NULL) as confirmed,
               au.custom_role_id,
               cr.name as "custom_role_name?",
               cr.description as "custom_role_description?",
               cr.permissions as "custom_role_permissions?"
        FROM users u
        INNER JOIN account_users au ON au.user_id = u.id
        LEFT JOIN custom_roles cr ON cr.id = au.custom_role_id
        WHERE au.account_id = $1
        ORDER BY u.id ASC
        "#,
        account_id as i64
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let agents: Vec<AgentResponse> = rows
        .into_iter()
        .map(|r| {
            let custom_role = if let (Some(c_id), Some(c_name)) = (r.custom_role_id, r.custom_role_name) {
                Some(crate::models::agent::CustomRoleBrief {
                    id: c_id,
                    name: c_name,
                    description: r.custom_role_description,
                    permissions: r.custom_role_permissions.unwrap_or_default(),
                })
            } else {
                None
            };

            AgentResponse {
                id: r.id,
                name: r.name,
                email: r.email,
                role: r.role.unwrap_or_else(|| "agent".to_string()),
                availability_status: r.availability_status.unwrap_or_else(|| "online".to_string()),
                auto_offline: r.auto_offline,
                confirmed: r.confirmed.unwrap_or(true),
                custom_role_id: r.custom_role_id,
                custom_role,
            }
        })
        .collect();

    (StatusCode::OK, Json(json!(agents)))
}

/// GET /api/v1/accounts/:account_id/assignable_agents
pub async fn list_assignable_agents(
    Path(account_id): Path<i32>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    list_agents(Path(account_id), Extension(current_user), State(state)).await
}

/// POST /api/v1/accounts/:account_id/agents
pub async fn create_agent(
    Path(account_id): Path<i32>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    Json(body): Json<CreateAgentPayload>,
) -> impl IntoResponse {
    if let Err((code, msg)) = verify_account_access(&state.db, &current_user, account_id, AccountRole::Administrator).await {
        return (code, Json(json!({ "error": msg })));
    }

    let clean_email = body.email.trim().to_lowercase();
    let role_int = match body.role.as_deref() {
        Some("administrator") => 1,
        _ => 0, // agent
    };

    // 1. Cari atau buat user di tabel `users`
    let existing_user = sqlx::query!(
        "SELECT id, name, email FROM users WHERE LOWER(email) = $1",
        clean_email
    )
    .fetch_optional(&state.db)
    .await;

    let user_id = match existing_user {
        Ok(Some(u)) => u.id,
        _ => {
            // Buat user baru dengan default password hash
            let salt = bcrypt::DEFAULT_COST;
            let hashed_pw = bcrypt::hash("password123", salt).unwrap_or_default();
            let new_user = sqlx::query!(
                r#"
                INSERT INTO users (
                    name, email, uid, provider, encrypted_password,
                    created_at, updated_at, confirmed_at, sign_in_count
                )
                VALUES ($1, $2, $2, 'email', $3, NOW(), NOW(), NOW(), 0)
                RETURNING id
                "#,
                body.name,
                clean_email,
                hashed_pw
            )
            .fetch_one(&state.db)
            .await;

            match new_user {
                Ok(u) => u.id,
                Err(e) => {
                    warn!("Failed to create user record for agent: {}", e);
                    return (
                        StatusCode::UNPROCESSABLE_ENTITY,
                        Json(json!({ "error": format!("Could not create agent user: {}", e) })),
                    );
                }
            }
        }
    };

    // 2. Hubungkan ke account_users
    let au_res = sqlx::query!(
        r#"
        INSERT INTO account_users (
            account_id, user_id, role, inviter_id, auto_offline, availability, custom_role_id, created_at, updated_at
        )
        VALUES ($1, $2, $3, $4, $5, 0, $6, NOW(), NOW())
        ON CONFLICT (account_id, user_id) 
        DO UPDATE SET role = EXCLUDED.role, custom_role_id = EXCLUDED.custom_role_id, updated_at = NOW()
        RETURNING id
        "#,
        account_id as i64,
        user_id as i64,
        role_int,
        current_user.id as i64,
        body.auto_offline.unwrap_or(true),
        body.custom_role_id
    )
    .fetch_one(&state.db)
    .await;

    match au_res {
        Ok(_) => {
            let resp = AgentResponse {
                id: user_id,
                name: body.name,
                email: Some(clean_email),
                role: if role_int == 1 { "administrator".to_string() } else { "agent".to_string() },
                availability_status: "online".to_string(),
                auto_offline: body.auto_offline.unwrap_or(true),
                confirmed: true,
                custom_role_id: body.custom_role_id,
                custom_role: None,
            };
            (StatusCode::OK, Json(json!(resp)))
        }
        Err(e) => {
            warn!("Failed to link user to account: {}", e);
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error": format!("Could not assign agent to account: {}", e) })),
            )
        }
    }
}

/// PATCH /api/v1/accounts/:account_id/agents/:id
pub async fn update_agent(
    Path((account_id, agent_id)): Path<(i32, i32)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    Json(body): Json<UpdateAgentPayload>,
) -> impl IntoResponse {
    if let Err((code, msg)) = verify_account_access(&state.db, &current_user, account_id, AccountRole::Administrator).await {
        return (code, Json(json!({ "error": msg })));
    }

    if let Some(ref name) = body.name {
        let _ = sqlx::query("UPDATE users SET name = $1, updated_at = NOW() WHERE id = $2")
            .bind(name)
            .bind(agent_id)
            .execute(&state.db)
            .await;
    }

    let role_int = body.role.as_deref().map(|r| if r == "administrator" { 1 } else { 0 });
    let avail_int = body.availability.as_deref().map(|a| match a {
        "offline" => 1,
        "busy" => 2,
        _ => 0,
    });

    let has_custom_role_param = body.custom_role_id.is_some();
    let _ = sqlx::query!(
        r#"
        UPDATE account_users
        SET role = COALESCE($1, role),
            availability = COALESCE($2, availability),
            auto_offline = COALESCE($3, auto_offline),
            custom_role_id = CASE WHEN $4::bool THEN $5 ELSE custom_role_id END,
            updated_at = NOW()
        WHERE account_id = $6 AND user_id = $7
        "#,
        role_int,
        avail_int,
        body.auto_offline,
        has_custom_role_param,
        body.custom_role_id,
        account_id as i64,
        agent_id as i64
    )
    .execute(&state.db)
    .await;

    // Ambil data terbaru
    let updated = sqlx::query!(
        r#"
        SELECT u.id, u.name, u.email,
               CASE au.role WHEN 1 THEN 'administrator' ELSE 'agent' END as role,
               CASE au.availability 
                   WHEN 1 THEN 'offline' 
                   WHEN 2 THEN 'busy' 
                   ELSE 'online' 
               END as availability_status,
               au.auto_offline,
               (u.confirmed_at IS NOT NULL) as confirmed,
               au.custom_role_id,
               cr.name as "custom_role_name?",
               cr.description as "custom_role_description?",
               cr.permissions as "custom_role_permissions?"
        FROM users u
        INNER JOIN account_users au ON au.user_id = u.id
        LEFT JOIN custom_roles cr ON cr.id = au.custom_role_id
        WHERE au.account_id = $1 AND u.id = $2
        "#,
        account_id as i64,
        agent_id
    )
    .fetch_optional(&state.db)
    .await;

    match updated {
        Ok(Some(r)) => {
            let custom_role = if let (Some(c_id), Some(c_name)) = (r.custom_role_id, r.custom_role_name) {
                Some(crate::models::agent::CustomRoleBrief {
                    id: c_id,
                    name: c_name,
                    description: r.custom_role_description,
                    permissions: r.custom_role_permissions.unwrap_or_default(),
                })
            } else {
                None
            };

            let resp = AgentResponse {
                id: r.id,
                name: r.name,
                email: r.email,
                role: r.role.unwrap_or_else(|| "agent".to_string()),
                availability_status: r.availability_status.unwrap_or_else(|| "online".to_string()),
                auto_offline: r.auto_offline,
                confirmed: r.confirmed.unwrap_or(true),
                custom_role_id: r.custom_role_id,
                custom_role,
            };
            (StatusCode::OK, Json(json!(resp)))
        }
        _ => (StatusCode::NOT_FOUND, Json(json!({ "error": "Agent not found" }))),
    }
}

/// DELETE /api/v1/accounts/:account_id/agents/:id
pub async fn delete_agent(
    Path((account_id, agent_id)): Path<(i32, i32)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) = verify_account_access(&state.db, &current_user, account_id, AccountRole::Administrator).await {
        return (code, Json(json!({ "error": msg })));
    }

    let res = sqlx::query!(
        "DELETE FROM account_users WHERE account_id = $1 AND user_id = $2",
        account_id as i64,
        agent_id as i64
    )
    .execute(&state.db)
    .await;

    match res {
        Ok(_) => (StatusCode::OK, Json(json!({ "status": "ok" }))),
        Err(e) => {
            warn!("Failed to remove agent: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Failed to remove agent" })),
            )
        }
    }
}
