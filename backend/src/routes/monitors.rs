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

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct ConversationMonitor {
    pub id: i64,
    pub account_id: i64,
    pub user_id: Option<i64>,
    pub name: String,
    pub condition: String,
    pub icon: Option<String>,
    pub icon_color: Option<String>,
    pub model: String,
    pub threshold: f64,
    pub history_since: chrono::NaiveDateTime,
    pub paused_at: Option<chrono::NaiveDateTime>,
    pub resumed_at: Option<chrono::NaiveDateTime>,
    pub deleted_at: Option<chrono::NaiveDateTime>,
    pub data_revision: i64,
    pub collection_version: i64,
    pub recheck_requested_at: Option<chrono::NaiveDateTime>,
    pub created_at: chrono::NaiveDateTime,
    pub updated_at: chrono::NaiveDateTime,
}

#[derive(Debug, Deserialize)]
pub struct ListMonitorsQuery {
    pub page: Option<i64>,
    pub active: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateMonitorPayload {
    pub name: Option<String>,
    pub condition: Option<String>,
    pub icon: Option<String>,
    pub icon_color: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateMonitorPayload {
    pub name: Option<String>,
    pub condition: Option<String>,
    pub icon: Option<String>,
    pub icon_color: Option<String>,
    pub paused: Option<bool>,
    pub collection_version: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct PreviewPayload {
    pub condition: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TimeseriesQuery {
    pub since: Option<i64>,
    pub until: Option<i64>,
    pub interval: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ConversationsQuery {
    pub page: Option<i64>,
    pub bucket_start: Option<i64>,
    pub data_revision: Option<i64>,
}

fn get_usage_snapshot() -> serde_json::Value {
    let now = chrono::Utc::now();
    let first_day = now.format("%Y-%m-01").to_string();
    let resets_at = (now + chrono::Duration::days(30)).timestamp();
    json!({
        "limit": 10000,
        "used": 0,
        "remaining": 10000,
        "limit_reached": false,
        "limit_reached_at": serde_json::Value::Null,
        "resets_at": resets_at,
        "period_start": first_day,
        "timezone": "UTC",
        "daily": []
    })
}

fn monitor_to_json(m: &ConversationMonitor, recent_count: i64) -> serde_json::Value {
    let state_str = if m.paused_at.is_some() {
        "paused"
    } else if m.recheck_requested_at.is_some() {
        "rechecking"
    } else {
        "live"
    };

    json!({
        "id": m.id,
        "name": m.name,
        "condition": m.condition,
        "icon": m.icon.clone().unwrap_or_else(|| "chat-3-line".to_string()),
        "icon_color": m.icon_color.clone().unwrap_or_else(|| "#3B82F6".to_string()),
        "created_at": m.created_at.and_utc().timestamp(),
        "history_since": m.history_since.and_utc().timestamp(),
        "data_revision": m.data_revision,
        "collection_version": m.collection_version,
        "paused_at": m.paused_at.map(|t| t.and_utc().timestamp()),
        "recent_count": recent_count,
        "processing": {
            "state": state_str,
            "errors": 0,
            "error_codes": []
        }
    })
}

/// GET /api/v1/accounts/:account_id/monitors
pub async fn list_monitors(
    Path(account_id): Path<i64>,
    Query(query): Query<ListMonitorsQuery>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id as i32, AccountRole::Agent).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let page = query.page.unwrap_or(1).max(1);
    let offset = (page - 1) * 20;
    let only_active = query.active.as_deref() == Some("true");

    let monitors_result = if only_active {
        sqlx::query_as::<_, ConversationMonitor>(
            r#"
            SELECT id, account_id, user_id, name, condition, icon, icon_color,
                   model, threshold, history_since, paused_at, resumed_at, deleted_at,
                   data_revision, collection_version, recheck_requested_at, created_at, updated_at
            FROM conversation_monitors
            WHERE account_id = $1 AND deleted_at IS NULL AND paused_at IS NULL
            ORDER BY created_at DESC, id DESC
            LIMIT 20 OFFSET $2
            "#,
        )
        .bind(account_id)
        .bind(offset)
        .fetch_all(&state.db)
        .await
    } else {
        sqlx::query_as::<_, ConversationMonitor>(
            r#"
            SELECT id, account_id, user_id, name, condition, icon, icon_color,
                   model, threshold, history_since, paused_at, resumed_at, deleted_at,
                   data_revision, collection_version, recheck_requested_at, created_at, updated_at
            FROM conversation_monitors
            WHERE account_id = $1 AND deleted_at IS NULL
            ORDER BY created_at DESC, id DESC
            LIMIT 20 OFFSET $2
            "#,
        )
        .bind(account_id)
        .bind(offset)
        .fetch_all(&state.db)
        .await
    };

    let monitors = match monitors_result {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Error fetching monitors: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "database_error" })),
            );
        }
    };

    let total_count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM conversation_monitors
        WHERE account_id = $1 AND deleted_at IS NULL
        "#,
    )
    .bind(account_id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    let payload: Vec<serde_json::Value> = monitors
        .iter()
        .map(|m| monitor_to_json(m, 0))
        .collect();

    (
        StatusCode::OK,
        Json(json!({
            "payload": payload,
            "meta": {
                "total_count": total_count,
                "page": page,
                "configured": true,
                "usage": get_usage_snapshot()
            }
        })),
    )
}

/// GET /api/v1/accounts/:account_id/monitors/:id
pub async fn get_monitor(
    Path((account_id, id)): Path<(i64, i64)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id as i32, AccountRole::Agent).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let monitor = sqlx::query_as::<_, ConversationMonitor>(
        r#"
        SELECT id, account_id, user_id, name, condition, icon, icon_color,
               model, threshold, history_since, paused_at, resumed_at, deleted_at,
               data_revision, collection_version, recheck_requested_at, created_at, updated_at
        FROM conversation_monitors
        WHERE account_id = $1 AND id = $2 AND deleted_at IS NULL
        "#,
    )
    .bind(account_id)
    .bind(id)
    .fetch_optional(&state.db)
    .await;

    match monitor {
        Ok(Some(m)) => {
            let mut obj = monitor_to_json(&m, 0);
            if let Some(map) = obj.as_object_mut() {
                map.insert("usage".to_string(), get_usage_snapshot());
            }
            (StatusCode::OK, Json(obj))
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "monitor_not_found" })),
        ),
        Err(e) => {
            eprintln!("Error fetching monitor: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "database_error" })),
            )
        }
    }
}

/// POST /api/v1/accounts/:account_id/monitors
pub async fn create_monitor(
    Path(account_id): Path<i64>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    Json(payload): Json<CreateMonitorPayload>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id as i32, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let name = match payload.name {
        Some(ref n) if !n.trim().is_empty() => n.trim().to_string(),
        _ => {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error": "invalid_parameters" })),
            )
        }
    };

    let condition = match payload.condition {
        Some(ref c) if !c.trim().is_empty() => c.trim().to_string(),
        _ => {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error": "invalid_parameters" })),
            )
        }
    };

    let icon = payload.icon.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).unwrap_or_else(|| "chat-3-line".to_string());
    let icon_color = payload.icon_color.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).unwrap_or_else(|| "#3B82F6".to_string());

    let monitor = sqlx::query_as::<_, ConversationMonitor>(
        r#"
        INSERT INTO conversation_monitors (
            account_id, user_id, name, condition, icon, icon_color,
            model, threshold, history_since, data_revision, collection_version,
            created_at, updated_at
        )
        VALUES (
            $1, $2, $3, $4, $5, $6,
            'gemini-flash', 0.7, NOW() - INTERVAL '7 days', 0, 0,
            NOW(), NOW()
        )
        RETURNING id, account_id, user_id, name, condition, icon, icon_color,
                  model, threshold, history_since, paused_at, resumed_at, deleted_at,
                  data_revision, collection_version, recheck_requested_at, created_at, updated_at
        "#,
    )
    .bind(account_id)
    .bind(current_user.id as i64)
    .bind(&name)
    .bind(&condition)
    .bind(&icon)
    .bind(&icon_color)
    .fetch_one(&state.db)
    .await;

    match monitor {
        Ok(m) => (StatusCode::CREATED, Json(monitor_to_json(&m, 0))),
        Err(e) => {
            eprintln!("Error creating monitor: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "database_error" })),
            )
        }
    }
}

/// PATCH/PUT /api/v1/accounts/:account_id/monitors/:id
pub async fn update_monitor(
    Path((account_id, id)): Path<(i64, i64)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    Json(payload): Json<UpdateMonitorPayload>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id as i32, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let existing = sqlx::query_as::<_, ConversationMonitor>(
        r#"
        SELECT id, account_id, user_id, name, condition, icon, icon_color,
               model, threshold, history_since, paused_at, resumed_at, deleted_at,
               data_revision, collection_version, recheck_requested_at, created_at, updated_at
        FROM conversation_monitors
        WHERE account_id = $1 AND id = $2 AND deleted_at IS NULL
        "#,
    )
    .bind(account_id)
    .bind(id)
    .fetch_optional(&state.db)
    .await;

    let current = match existing {
        Ok(Some(m)) => m,
        Ok(None) => return (StatusCode::NOT_FOUND, Json(json!({ "error": "monitor_not_found" }))),
        Err(e) => {
            eprintln!("Error querying monitor: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "database_error" })));
        }
    };

    let name = payload.name.map(|s| s.trim().to_string()).unwrap_or(current.name);
    let condition = payload.condition.map(|s| s.trim().to_string()).unwrap_or(current.condition);
    let icon = payload.icon.map(|s| s.trim().to_string()).or(current.icon);
    let icon_color = payload.icon_color.map(|s| s.trim().to_string()).or(current.icon_color);

    let (paused_at, data_revision) = match payload.paused {
        Some(true) => (Some(chrono::Utc::now().naive_utc()), current.data_revision + 1),
        Some(false) => (None, current.data_revision + 1),
        None => (current.paused_at, current.data_revision),
    };

    let updated = sqlx::query_as::<_, ConversationMonitor>(
        r#"
        UPDATE conversation_monitors
        SET name = $1, condition = $2, icon = $3, icon_color = $4,
            paused_at = $5, data_revision = $6, updated_at = NOW()
        WHERE id = $7 AND account_id = $8
        RETURNING id, account_id, user_id, name, condition, icon, icon_color,
                  model, threshold, history_since, paused_at, resumed_at, deleted_at,
                  data_revision, collection_version, recheck_requested_at, created_at, updated_at
        "#,
    )
    .bind(&name)
    .bind(&condition)
    .bind(&icon)
    .bind(&icon_color)
    .bind(paused_at)
    .bind(data_revision)
    .bind(id)
    .bind(account_id)
    .fetch_one(&state.db)
    .await;

    match updated {
        Ok(m) => (StatusCode::OK, Json(monitor_to_json(&m, 0))),
        Err(e) => {
            eprintln!("Error updating monitor: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "database_error" })))
        }
    }
}

/// DELETE /api/v1/accounts/:account_id/monitors/:id
pub async fn delete_monitor(
    Path((account_id, id)): Path<(i64, i64)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id as i32, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let res = sqlx::query(
        r#"
        UPDATE conversation_monitors
        SET deleted_at = NOW(), data_revision = data_revision + 1
        WHERE id = $1 AND account_id = $2 AND deleted_at IS NULL
        "#,
    )
    .bind(id)
    .bind(account_id)
    .execute(&state.db)
    .await;

    match res {
        Ok(r) if r.rows_affected() > 0 => (StatusCode::NO_CONTENT, Json(json!({}))),
        Ok(_) => (StatusCode::NOT_FOUND, Json(json!({ "error": "monitor_not_found" }))),
        Err(e) => {
            eprintln!("Error deleting monitor: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "database_error" })))
        }
    }
}

/// POST /api/v1/accounts/:account_id/monitors/preview
pub async fn preview_monitor(
    Path(account_id): Path<i64>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    Json(payload): Json<PreviewPayload>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id as i32, AccountRole::Agent).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    if payload.condition.is_none() || payload.condition.as_deref().unwrap_or("").trim().is_empty() {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": "invalid_parameters" })),
        );
    }

    let token = uuid::Uuid::new_v4().simple().to_string();
    (
        StatusCode::ACCEPTED,
        Json(json!({
            "token": token,
            "status": "complete",
            "retry_after": 30,
            "payload": []
        })),
    )
}

/// GET /api/v1/accounts/:account_id/monitors/preview/:token
pub async fn preview_status(
    Path((account_id, _token)): Path<(i64, String)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id as i32, AccountRole::Agent).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    (
        StatusCode::OK,
        Json(json!({
            "status": "complete",
            "payload": []
        })),
    )
}

/// GET /api/v1/accounts/:account_id/monitors/:id/timeseries
pub async fn get_monitor_timeseries(
    Path((account_id, id)): Path<(i64, i64)>,
    Query(query): Query<TimeseriesQuery>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id as i32, AccountRole::Agent).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let monitor = sqlx::query_as::<_, ConversationMonitor>(
        r#"
        SELECT id, account_id, user_id, name, condition, icon, icon_color,
               model, threshold, history_since, paused_at, resumed_at, deleted_at,
               data_revision, collection_version, recheck_requested_at, created_at, updated_at
        FROM conversation_monitors
        WHERE account_id = $1 AND id = $2 AND deleted_at IS NULL
        "#,
    )
    .bind(account_id)
    .bind(id)
    .fetch_optional(&state.db)
    .await;

    match monitor {
        Ok(Some(m)) => {
            let interval = query.interval.unwrap_or_else(|| "day".to_string());
            (
                StatusCode::OK,
                Json(json!({
                    "time_basis": "conversation_created_at",
                    "timezone": "UTC",
                    "interval": interval,
                    "data_revision": m.data_revision,
                    "total_count": 0,
                    "buckets": [],
                    "monitor": monitor_to_json(&m, 0),
                    "usage": get_usage_snapshot()
                })),
            )
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "monitor_not_found" })),
        ),
        Err(e) => {
            eprintln!("Error fetching monitor timeseries: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "database_error" })),
            )
        }
    }
}

/// GET /api/v1/accounts/:account_id/monitors/:id/conversations
pub async fn get_monitor_conversations(
    Path((account_id, id)): Path<(i64, i64)>,
    Query(query): Query<ConversationsQuery>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id as i32, AccountRole::Agent).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let monitor = sqlx::query_as::<_, ConversationMonitor>(
        r#"
        SELECT id, account_id, user_id, name, condition, icon, icon_color,
               model, threshold, history_since, paused_at, resumed_at, deleted_at,
               data_revision, collection_version, recheck_requested_at, created_at, updated_at
        FROM conversation_monitors
        WHERE account_id = $1 AND id = $2 AND deleted_at IS NULL
        "#,
    )
    .bind(account_id)
    .bind(id)
    .fetch_optional(&state.db)
    .await;

    match monitor {
        Ok(Some(m)) => {
            let page = query.page.unwrap_or(1).max(1);
            let b_start = query.bucket_start.unwrap_or(0);
            (
                StatusCode::OK,
                Json(json!({
                    "meta": {
                        "record_type": "conversation",
                        "total_count": 0,
                        "conversation_count": 0,
                        "current_page": page,
                        "per_page": 25,
                        "data_revision": m.data_revision,
                        "bucket": {
                            "since": b_start,
                            "until": b_start + 86400
                        }
                    },
                    "payload": []
                })),
            )
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "monitor_not_found" })),
        ),
        Err(e) => {
            eprintln!("Error fetching monitor conversations: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "database_error" })),
            )
        }
    }
}

/// POST /api/v1/accounts/:account_id/monitors/:id/retry_evaluations
pub async fn retry_evaluations(
    Path((account_id, id)): Path<(i64, i64)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id as i32, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let monitor = sqlx::query_as::<_, ConversationMonitor>(
        r#"
        SELECT id, account_id, user_id, name, condition, icon, icon_color,
               model, threshold, history_since, paused_at, resumed_at, deleted_at,
               data_revision, collection_version, recheck_requested_at, created_at, updated_at
        FROM conversation_monitors
        WHERE account_id = $1 AND id = $2 AND deleted_at IS NULL
        "#,
    )
    .bind(account_id)
    .bind(id)
    .fetch_optional(&state.db)
    .await;

    match monitor {
        Ok(Some(_)) => (StatusCode::ACCEPTED, Json(json!({ "status": "accepted" }))),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "monitor_not_found" })),
        ),
        Err(e) => {
            eprintln!("Error in retry_evaluations: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "database_error" })),
            )
        }
    }
}

/// POST /api/v1/accounts/:account_id/monitors/:id/resume
pub async fn resume_monitor(
    Path((account_id, id)): Path<(i64, i64)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id as i32, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let updated = sqlx::query_as::<_, ConversationMonitor>(
        r#"
        UPDATE conversation_monitors
        SET paused_at = NULL, resumed_at = NOW(), data_revision = data_revision + 1, updated_at = NOW()
        WHERE id = $1 AND account_id = $2 AND deleted_at IS NULL
        RETURNING id, account_id, user_id, name, condition, icon, icon_color,
                  model, threshold, history_since, paused_at, resumed_at, deleted_at,
                  data_revision, collection_version, recheck_requested_at, created_at, updated_at
        "#,
    )
    .bind(id)
    .bind(account_id)
    .fetch_optional(&state.db)
    .await;

    match updated {
        Ok(Some(m)) => (StatusCode::OK, Json(monitor_to_json(&m, 0))),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "monitor_not_found" })),
        ),
        Err(e) => {
            eprintln!("Error in resume_monitor: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "database_error" })),
            )
        }
    }
}
