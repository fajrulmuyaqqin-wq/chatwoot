use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::Utc;
use serde::Deserialize;
use serde_json::json;

use crate::middleware::auth::CurrentUser;
use crate::middleware::rbac::{verify_account_access, AccountRole};
use crate::models::inbox::{Inbox, InboxResponse};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct CreateInboxPayload {
    pub name: Option<String>,
    pub channel: Option<serde_json::Value>,
    pub greeting_enabled: Option<bool>,
    pub greeting_message: Option<String>,
    pub enable_auto_assignment: Option<bool>,
    pub csat_survey_enabled: Option<bool>,
    pub lock_to_single_conversation: Option<bool>,
    pub timezone: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateInboxPayload {
    pub name: Option<String>,
    pub greeting_enabled: Option<bool>,
    pub greeting_message: Option<String>,
    pub enable_auto_assignment: Option<bool>,
    pub working_hours_enabled: Option<bool>,
    pub out_of_office_message: Option<String>,
    pub timezone: Option<String>,
    pub enable_email_collect: Option<bool>,
    pub csat_survey_enabled: Option<bool>,
    pub allow_messages_after_resolved: Option<bool>,
    pub lock_to_single_conversation: Option<bool>,
    pub business_name: Option<String>,
    pub csat_config: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct MessageTemplatesQuery {
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct InboxMembersPayload {
    pub inbox_id: Option<i32>,
    pub user_ids: Option<Vec<i32>>,
}

/// GET /api/v1/accounts/:account_id/inboxes
pub async fn list_inboxes(
    Path(account_id): Path<i32>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let inboxes = match sqlx::query_as::<_, Inbox>(
        r#"
        SELECT id, channel_id, account_id, name, channel_type, enable_auto_assignment,
               greeting_enabled, greeting_message, email_address, working_hours_enabled,
               out_of_office_message, timezone, enable_email_collect, csat_survey_enabled,
               allow_messages_after_resolved, auto_assignment_config, lock_to_single_conversation,
               portal_id, sender_name_type, business_name, csat_config, created_at, updated_at
        FROM inboxes
        WHERE account_id = $1
        ORDER BY id ASC
        "#,
    )
    .bind(account_id)
    .fetch_all(&state.db)
    .await
    {
        Ok(list) => list,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Failed to fetch inboxes" })),
            );
        }
    };

    let payload: Vec<InboxResponse> = inboxes.into_iter().map(InboxResponse::from).collect();

    (StatusCode::OK, Json(json!({ "payload": payload })))
}

/// GET /api/v1/accounts/:account_id/inboxes/:id
pub async fn get_inbox(
    Path((account_id, inbox_id)): Path<(i32, i32)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Agent).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let inbox = sqlx::query_as::<_, Inbox>(
        r#"
        SELECT id, channel_id, account_id, name, channel_type, enable_auto_assignment,
               greeting_enabled, greeting_message, email_address, working_hours_enabled,
               out_of_office_message, timezone, enable_email_collect, csat_survey_enabled,
               allow_messages_after_resolved, auto_assignment_config, lock_to_single_conversation,
               portal_id, sender_name_type, business_name, csat_config, created_at, updated_at
        FROM inboxes
        WHERE account_id = $1 AND id = $2
        "#,
    )
    .bind(account_id)
    .bind(inbox_id)
    .fetch_optional(&state.db)
    .await;

    match inbox {
        Ok(Some(i)) => (StatusCode::OK, Json(json!(InboxResponse::from(i)))),
        Ok(None) => (StatusCode::NOT_FOUND, Json(json!({ "error": "Inbox not found" }))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}

/// POST /api/v1/accounts/:account_id/inboxes
pub async fn create_inbox(
    Path(account_id): Path<i32>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    Json(body): Json<CreateInboxPayload>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let name = match body.name {
        Some(n) if !n.trim().is_empty() => n.trim().to_string(),
        _ => {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error": "Inbox name is required" })),
            )
        }
    };

    let channel_type = body
        .channel
        .as_ref()
        .and_then(|c| c.get("type"))
        .and_then(|t| t.as_str())
        .unwrap_or("Channel::Api")
        .to_string();

    let channel_type_db = match channel_type.as_str() {
        "web_widget" => "Channel::WebWidget",
        "api" => "Channel::Api",
        "email" => "Channel::Email",
        "whatsapp" => "Channel::Whatsapp",
        "telegram" => "Channel::Telegram",
        "line" => "Channel::Line",
        "sms" => "Channel::Sms",
        other => other,
    };

    let inbox = sqlx::query_as::<_, Inbox>(
        r#"
        INSERT INTO inboxes (
            name, channel_type, channel_id, account_id, enable_auto_assignment,
            greeting_enabled, greeting_message, csat_survey_enabled,
            lock_to_single_conversation, timezone, created_at, updated_at
        )
        VALUES (
            $1, $2, 0, $3, $4,
            $5, $6, $7,
            $8, $9, NOW(), NOW()
        )
        RETURNING id, channel_id, account_id, name, channel_type, enable_auto_assignment,
                  greeting_enabled, greeting_message, email_address, working_hours_enabled,
                  out_of_office_message, timezone, enable_email_collect, csat_survey_enabled,
                  allow_messages_after_resolved, auto_assignment_config, lock_to_single_conversation,
                  portal_id, sender_name_type, business_name, csat_config, created_at, updated_at
        "#,
    )
    .bind(name)
    .bind(channel_type_db)
    .bind(account_id)
    .bind(body.enable_auto_assignment.unwrap_or(true))
    .bind(body.greeting_enabled.unwrap_or(false))
    .bind(body.greeting_message)
    .bind(body.csat_survey_enabled.unwrap_or(false))
    .bind(body.lock_to_single_conversation.unwrap_or(false))
    .bind(body.timezone.unwrap_or_else(|| "UTC".to_string()))
    .fetch_one(&state.db)
    .await;

    match inbox {
        Ok(i) => (StatusCode::OK, Json(json!(InboxResponse::from(i)))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}

/// PATCH/PUT /api/v1/accounts/:account_id/inboxes/:id
pub async fn update_inbox(
    Path((account_id, inbox_id)): Path<(i32, i32)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    Json(body): Json<UpdateInboxPayload>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let inbox = sqlx::query_as::<_, Inbox>(
        r#"
        UPDATE inboxes
        SET name = COALESCE($1, name),
            greeting_enabled = COALESCE($2, greeting_enabled),
            greeting_message = COALESCE($3, greeting_message),
            enable_auto_assignment = COALESCE($4, enable_auto_assignment),
            working_hours_enabled = COALESCE($5, working_hours_enabled),
            out_of_office_message = COALESCE($6, out_of_office_message),
            timezone = COALESCE($7, timezone),
            enable_email_collect = COALESCE($8, enable_email_collect),
            csat_survey_enabled = COALESCE($9, csat_survey_enabled),
            allow_messages_after_resolved = COALESCE($10, allow_messages_after_resolved),
            lock_to_single_conversation = COALESCE($11, lock_to_single_conversation),
            business_name = COALESCE($12, business_name),
            csat_config = COALESCE($13, csat_config),
            updated_at = NOW()
        WHERE account_id = $14 AND id = $15
        RETURNING id, channel_id, account_id, name, channel_type, enable_auto_assignment,
                  greeting_enabled, greeting_message, email_address, working_hours_enabled,
                  out_of_office_message, timezone, enable_email_collect, csat_survey_enabled,
                  allow_messages_after_resolved, auto_assignment_config, lock_to_single_conversation,
                  portal_id, sender_name_type, business_name, csat_config, created_at, updated_at
        "#,
    )
    .bind(body.name)
    .bind(body.greeting_enabled)
    .bind(body.greeting_message)
    .bind(body.enable_auto_assignment)
    .bind(body.working_hours_enabled)
    .bind(body.out_of_office_message)
    .bind(body.timezone)
    .bind(body.enable_email_collect)
    .bind(body.csat_survey_enabled)
    .bind(body.allow_messages_after_resolved)
    .bind(body.lock_to_single_conversation)
    .bind(body.business_name)
    .bind(body.csat_config)
    .bind(account_id)
    .bind(inbox_id)
    .fetch_optional(&state.db)
    .await;

    match inbox {
        Ok(Some(i)) => (StatusCode::OK, Json(json!(InboxResponse::from(i)))),
        Ok(None) => (StatusCode::NOT_FOUND, Json(json!({ "error": "Inbox not found" }))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}

/// DELETE /api/v1/accounts/:account_id/inboxes/:id
pub async fn delete_inbox(
    Path((account_id, inbox_id)): Path<(i32, i32)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let res = sqlx::query("DELETE FROM inboxes WHERE account_id = $1 AND id = $2")
        .bind(account_id)
        .bind(inbox_id)
        .execute(&state.db)
        .await;

    match res {
        Ok(r) if r.rows_affected() > 0 => (
            StatusCode::OK,
            Json(json!({ "message": "Inbox deletion initiated successfully" })),
        ),
        Ok(_) => (StatusCode::NOT_FOUND, Json(json!({ "error": "Inbox not found" }))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}

/// GET /api/v1/accounts/:account_id/inboxes/:id/message_templates
pub async fn get_message_templates(
    Path((account_id, inbox_id)): Path<(i32, i32)>,
    Query(query): Query<MessageTemplatesQuery>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Agent).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    // Ambil info inbox
    let inbox = match sqlx::query_as::<_, Inbox>(
        "SELECT * FROM inboxes WHERE account_id = $1 AND id = $2",
    )
    .bind(account_id)
    .bind(inbox_id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(i)) => i,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Inbox not found" })),
            )
        }
    };

    let channel_type = inbox.channel_type.unwrap_or_default();
    if channel_type != "Channel::Whatsapp" && !channel_type.to_lowercase().contains("whatsapp") {
        return (
            StatusCode::OK,
            Json(json!({
                "payload": [],
                "meta": { "last_sync_attempt_at": Utc::now().to_rfc3339() }
            })),
        );
    }

    let channel_record = sqlx::query!(
        r#"
        SELECT message_templates, message_templates_last_updated
        FROM channel_whatsapp
        WHERE id = $1
        "#,
        inbox.channel_id
    )
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();

    let (templates_val, last_updated) = match channel_record {
        Some(r) => {
            let val = r.message_templates.unwrap_or_else(|| json!([]));
            let lu = r
                .message_templates_last_updated
                .map(|t| t.and_utc().to_rfc3339())
                .unwrap_or_else(|| Utc::now().to_rfc3339());
            (val, lu)
        }
        None => (json!([]), Utc::now().to_rfc3339()),
    };

    let mut templates: Vec<serde_json::Value> = match templates_val {
        serde_json::Value::Array(arr) => arr,
        _ => Vec::new(),
    };

    if let Some(target_name) = query.name {
        if !target_name.trim().is_empty() {
            templates.retain(|t| {
                t.get("name")
                    .and_then(|n| n.as_str())
                    .map(|n| n == target_name.trim())
                    .unwrap_or(false)
            });
        }
    }

    (
        StatusCode::OK,
        Json(json!({
            "payload": templates,
            "meta": {
                "last_sync_attempt_at": last_updated
            }
        })),
    )
}

/// POST /api/v1/accounts/:account_id/inboxes/:id/sync_templates
pub async fn sync_templates(
    Path((account_id, inbox_id)): Path<(i32, i32)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let inbox = match sqlx::query_as::<_, Inbox>(
        "SELECT * FROM inboxes WHERE account_id = $1 AND id = $2",
    )
    .bind(account_id)
    .bind(inbox_id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(i)) => i,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Inbox not found" })),
            )
        }
    };

    let _ = sqlx::query(
        "UPDATE channel_whatsapp SET message_templates_last_updated = NOW() WHERE id = $1",
    )
    .bind(inbox.channel_id)
    .execute(&state.db)
    .await;

    (
        StatusCode::OK,
        Json(json!({
            "message": "Template sync initiated successfully"
        })),
    )
}

async fn fetch_members_list(db: &sqlx::PgPool, account_id: i32, inbox_id: i32) -> Vec<serde_json::Value> {
    let members = sqlx::query!(
        r#"
        SELECT u.id, u.name, u.email, au.role
        FROM users u
        JOIN inbox_members im ON im.user_id = u.id
        JOIN account_users au ON au.user_id = u.id AND au.account_id = $1
        WHERE im.inbox_id = $2
        ORDER BY u.name ASC
        "#,
        account_id as i64,
        inbox_id
    )
    .fetch_all(db)
    .await
    .unwrap_or_default();

    members
        .into_iter()
        .map(|m| {
            let role_str = match m.role {
                Some(1) => "administrator",
                _ => "agent",
            };
            json!({
                "id": m.id,
                "name": m.name,
                "email": m.email,
                "role": role_str,
                "availability_status": "online"
            })
        })
        .collect()
}

/// GET /api/v1/accounts/:account_id/inbox_members/:inbox_id
pub async fn get_inbox_members(
    Path((account_id, inbox_id)): Path<(i32, i32)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Agent).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let payload = fetch_members_list(&state.db, account_id, inbox_id).await;
    (StatusCode::OK, Json(json!({ "payload": payload })))
}

/// POST /api/v1/accounts/:account_id/inbox_members
/// PATCH /api/v1/accounts/:account_id/inbox_members
pub async fn update_inbox_members(
    Path(account_id): Path<i32>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    Json(body): Json<InboxMembersPayload>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let inbox_id = match body.inbox_id {
        Some(id) => id,
        None => {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error": "inbox_id is required" })),
            )
        }
    };

    let user_ids = body.user_ids.unwrap_or_default();

    // Hapus anggota yang tidak ada dalam daftar
    let _ = sqlx::query(
        "DELETE FROM inbox_members WHERE inbox_id = $1 AND NOT (user_id = ANY($2))",
    )
    .bind(inbox_id)
    .bind(&user_ids)
    .execute(&state.db)
    .await;

    // Masukkan anggota baru
    for uid in &user_ids {
        let _ = sqlx::query(
            r#"
            INSERT INTO inbox_members (inbox_id, user_id, created_at, updated_at)
            VALUES ($1, $2, NOW(), NOW())
            ON CONFLICT (inbox_id, user_id) DO NOTHING
            "#,
        )
        .bind(inbox_id)
        .bind(uid)
        .execute(&state.db)
        .await;
    }

    let payload = fetch_members_list(&state.db, account_id, inbox_id).await;
    (StatusCode::OK, Json(json!({ "payload": payload })))
}

/// DELETE /api/v1/accounts/:account_id/inbox_members
pub async fn delete_inbox_members(
    Path(account_id): Path<i32>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    Json(body): Json<InboxMembersPayload>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let inbox_id = match body.inbox_id {
        Some(id) => id,
        None => {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error": "inbox_id is required" })),
            )
        }
    };

    let user_ids = body.user_ids.unwrap_or_default();

    let _ = sqlx::query(
        "DELETE FROM inbox_members WHERE inbox_id = $1 AND user_id = ANY($2)",
    )
    .bind(inbox_id)
    .bind(&user_ids)
    .execute(&state.db)
    .await;

    (StatusCode::OK, Json(json!({ "success": true })))
}

/// GET /api/v1/accounts/:account_id/inboxes/:id/agent_bot
pub async fn get_agent_bot() -> impl IntoResponse {
    (StatusCode::OK, Json(json!({ "agent_bot": null })))
}

/// POST /api/v1/accounts/:account_id/inboxes/:id/set_agent_bot
pub async fn set_agent_bot() -> impl IntoResponse {
    (StatusCode::OK, Json(json!({ "success": true })))
}

/// DELETE /api/v1/accounts/:account_id/inboxes/:id/avatar
pub async fn delete_inbox_avatar() -> impl IntoResponse {
    (StatusCode::OK, Json(json!({ "success": true })))
}
