use crate::models::team::{
    CreateTeamPayload, Team, TeamMemberUser, TeamMembersPayload, UpdateTeamPayload,
};
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde_json::json;
use tracing::warn;

/// GET /api/v1/accounts/:account_id/teams
pub async fn list_teams(
    Path(account_id): Path<i32>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let teams = sqlx::query_as::<_, Team>(
        "SELECT * FROM teams WHERE account_id = $1 ORDER BY id ASC",
    )
    .bind(account_id as i64)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    (StatusCode::OK, Json(json!(teams)))
}

/// POST /api/v1/accounts/:account_id/teams
pub async fn create_team(
    Path(account_id): Path<i32>,
    State(state): State<AppState>,
    Json(body): Json<CreateTeamPayload>,
) -> impl IntoResponse {
    let result = sqlx::query_as::<_, Team>(
        r#"
        INSERT INTO teams (name, description, allow_auto_assign, account_id, icon, icon_color, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, NOW(), NOW())
        RETURNING *
        "#,
    )
    .bind(&body.name)
    .bind(body.description)
    .bind(body.allow_auto_assign.unwrap_or(true))
    .bind(account_id as i64)
    .bind(body.icon.unwrap_or_default())
    .bind(body.icon_color.unwrap_or_default())
    .fetch_one(&state.db)
    .await;

    match result {
        Ok(t) => (StatusCode::OK, Json(json!(t))),
        Err(e) => {
            warn!("Failed to create team: {}", e);
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error": format!("Could not create team: {}", e) })),
            )
        }
    }
}

/// GET /api/v1/accounts/:account_id/teams/:id
pub async fn get_team(
    Path((account_id, id)): Path<(i32, i64)>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let team = sqlx::query_as::<_, Team>(
        "SELECT * FROM teams WHERE id = $1 AND account_id = $2",
    )
    .bind(id)
    .bind(account_id as i64)
    .fetch_optional(&state.db)
    .await;

    match team {
        Ok(Some(t)) => (StatusCode::OK, Json(json!(t))),
        _ => (StatusCode::NOT_FOUND, Json(json!({ "error": "Team not found" }))),
    }
}

/// PATCH /api/v1/accounts/:account_id/teams/:id
pub async fn update_team(
    Path((account_id, id)): Path<(i32, i64)>,
    State(state): State<AppState>,
    Json(body): Json<UpdateTeamPayload>,
) -> impl IntoResponse {
    let result = sqlx::query_as::<_, Team>(
        r#"
        UPDATE teams
        SET name = COALESCE($1, name),
            description = COALESCE($2, description),
            allow_auto_assign = COALESCE($3, allow_auto_assign),
            icon = COALESCE($4, icon),
            icon_color = COALESCE($5, icon_color),
            updated_at = NOW()
        WHERE id = $6 AND account_id = $7
        RETURNING *
        "#,
    )
    .bind(body.name)
    .bind(body.description)
    .bind(body.allow_auto_assign)
    .bind(body.icon)
    .bind(body.icon_color)
    .bind(id)
    .bind(account_id as i64)
    .fetch_optional(&state.db)
    .await;

    match result {
        Ok(Some(t)) => (StatusCode::OK, Json(json!(t))),
        Ok(None) => (StatusCode::NOT_FOUND, Json(json!({ "error": "Team not found" }))),
        Err(e) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}

/// DELETE /api/v1/accounts/:account_id/teams/:id
pub async fn delete_team(
    Path((account_id, id)): Path<(i32, i64)>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let _ = sqlx::query("DELETE FROM teams WHERE id = $1 AND account_id = $2")
        .bind(id)
        .bind(account_id as i64)
        .execute(&state.db)
        .await;

    (StatusCode::OK, Json(json!({ "status": "ok" })))
}

/// GET /api/v1/accounts/:account_id/teams/:id/team_members
pub async fn list_team_members(
    Path((account_id, team_id)): Path<(i32, i64)>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let members = sqlx::query_as::<_, TeamMemberUser>(
        r#"
        SELECT u.id, u.name, u.email,
               CASE au.role WHEN 1 THEN 'administrator' ELSE 'agent' END as role
        FROM users u
        INNER JOIN team_members tm ON tm.user_id = u.id
        INNER JOIN teams t ON t.id = tm.team_id
        LEFT JOIN account_users au ON au.user_id = u.id AND au.account_id = t.account_id
        WHERE tm.team_id = $1 AND t.account_id = $2
        ORDER BY u.id ASC
        "#,
    )
    .bind(team_id)
    .bind(account_id as i64)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    (StatusCode::OK, Json(json!(members)))
}

/// POST /api/v1/accounts/:account_id/teams/:id/team_members
pub async fn add_team_members(
    Path((account_id, team_id)): Path<(i32, i64)>,
    State(state): State<AppState>,
    Json(body): Json<TeamMembersPayload>,
) -> impl IntoResponse {
    // Verifikasi bahwa team milik account_id
    let team_exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM teams WHERE id = $1 AND account_id = $2)",
    )
    .bind(team_id)
    .bind(account_id as i64)
    .fetch_one(&state.db)
    .await
    .unwrap_or(false);

    if !team_exists {
        return (StatusCode::NOT_FOUND, Json(json!({ "error": "Team not found" })));
    }

    for user_id in body.user_ids {
        let _ = sqlx::query(
            r#"
            INSERT INTO team_members (team_id, user_id, created_at, updated_at)
            VALUES ($1, $2, NOW(), NOW())
            ON CONFLICT (team_id, user_id) DO NOTHING
            "#,
        )
        .bind(team_id)
        .bind(user_id)
        .execute(&state.db)
        .await;
    }

    let members = sqlx::query_as::<_, TeamMemberUser>(
        r#"
        SELECT u.id, u.name, u.email,
               CASE au.role WHEN 1 THEN 'administrator' ELSE 'agent' END as role
        FROM users u
        INNER JOIN team_members tm ON tm.user_id = u.id
        LEFT JOIN account_users au ON au.user_id = u.id AND au.account_id = $2
        WHERE tm.team_id = $1
        ORDER BY u.id ASC
        "#,
    )
    .bind(team_id)
    .bind(account_id as i64)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    (StatusCode::OK, Json(json!(members)))
}

/// DELETE /api/v1/accounts/:account_id/teams/:id/team_members
pub async fn remove_team_members(
    Path((account_id, team_id)): Path<(i32, i64)>,
    State(state): State<AppState>,
    Json(body): Json<TeamMembersPayload>,
) -> impl IntoResponse {
    let team_exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM teams WHERE id = $1 AND account_id = $2)",
    )
    .bind(team_id)
    .bind(account_id as i64)
    .fetch_one(&state.db)
    .await
    .unwrap_or(false);

    if !team_exists {
        return (StatusCode::NOT_FOUND, Json(json!({ "error": "Team not found" })));
    }

    for user_id in body.user_ids {
        let _ = sqlx::query(
            "DELETE FROM team_members WHERE team_id = $1 AND user_id = $2",
        )
        .bind(team_id)
        .bind(user_id)
        .execute(&state.db)
        .await;
    }

    (StatusCode::OK, Json(json!({ "status": "ok" })))
}
