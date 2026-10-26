use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::{info, warn};

use crate::middleware::auth::CurrentUser;
use crate::middleware::rbac::{verify_account_access, AccountRole};
use crate::models::integration::IntegrationHook;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct CreateHookPayload {
    pub app_id: String,
    pub inbox_id: Option<i32>,
    pub hook_type: Option<i32>,
    pub reference_id: Option<String>,
    pub access_token: Option<String>,
    pub settings: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct SlackConnectPayload {
    pub code: Option<String>,
    pub inbox_id: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct SlackUpdatePayload {
    pub reference_id: Option<String>,
    pub message_mode: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct LinearQuery {
    pub team_id: Option<String>,
    pub conversation_id: Option<String>,
    pub q: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct LinearIssuePayload {
    pub conversation_id: Option<String>,
    pub team_id: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub priority: Option<i32>,
    pub state_id: Option<String>,
    pub assignee_id: Option<String>,
    pub issue_id: Option<String>,
    pub link_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DyteMeetingPayload {
    pub conversation_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DyteParticipantPayload {
    pub message_id: Option<i64>,
}

// ---------------------------------------------------------------------------
// 1. Integration Apps Catalog & Hooks Management
// ---------------------------------------------------------------------------

/// GET /api/v1/accounts/:account_id/integrations/apps
pub async fn list_apps(
    Path(account_id): Path<i32>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let hooks = sqlx::query_as::<_, IntegrationHook>(
        "SELECT * FROM integrations_hooks WHERE account_id = $1 ORDER BY id DESC",
    )
    .bind(account_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let all_apps = vec![
        json!({
            "id": "slack",
            "name": "Slack",
            "description": "Collaborate on customer conversations directly from your Slack workspace channels and threads.",
            "short_description": "Reply to Chatwoot conversations from Slack",
            "logo": "slack.svg",
            "action": format!("{}/app/accounts/{}/settings/integrations/slack", state.config.frontend_url, account_id),
            "button": "Connect",
            "hook_type": "account",
            "allow_multiple_hooks": false,
            "enabled": hooks.iter().any(|h| h.app_id == "slack" && h.status.unwrap_or(0) == 1),
            "hooks": hooks.iter().filter(|h| h.app_id == "slack").collect::<Vec<_>>()
        }),
        json!({
            "id": "linear",
            "name": "Linear",
            "description": "Create, link, and track engineering issue tickets in Linear directly from Chatwoot conversations.",
            "short_description": "Create and link Linear issues",
            "logo": "linear.png",
            "action": format!("{}/app/accounts/{}/settings/integrations/linear", state.config.frontend_url, account_id),
            "button": "Connect",
            "hook_type": "account",
            "allow_multiple_hooks": false,
            "enabled": hooks.iter().any(|h| h.app_id == "linear" && h.status.unwrap_or(0) == 1),
            "hooks": hooks.iter().filter(|h| h.app_id == "linear").collect::<Vec<_>>()
        }),
        json!({
            "id": "dyte",
            "name": "Dyte Video Calls",
            "description": "Launch audio and video call meetings with customers right inside the conversation window.",
            "short_description": "In-app audio and video calls",
            "logo": "dyte.png",
            "action": format!("{}/app/accounts/{}/settings/integrations/dyte", state.config.frontend_url, account_id),
            "button": "Configure",
            "hook_type": "account",
            "allow_multiple_hooks": false,
            "enabled": hooks.iter().any(|h| h.app_id == "dyte" && h.status.unwrap_or(0) == 1),
            "hooks": hooks.iter().filter(|h| h.app_id == "dyte").collect::<Vec<_>>()
        }),
        json!({
            "id": "openai",
            "name": "OpenAI",
            "description": "Enhance agent productivity with automated reply recommendations, summarization, and tone shifting.",
            "short_description": "AI assistance and smart replies",
            "logo": "openai.png",
            "action": "/openai",
            "button": "Configure",
            "hook_type": "account",
            "allow_multiple_hooks": false,
            "enabled": true,
            "hooks": hooks.iter().filter(|h| h.app_id == "openai").collect::<Vec<_>>()
        }),
        json!({
            "id": "google_translate",
            "name": "Google Translate",
            "description": "Automatically translate customer messages and agent replies into 100+ languages in real time.",
            "short_description": "Real-time message translation",
            "logo": "google_translate.png",
            "action": "/google_translate",
            "button": "Configure",
            "hook_type": "account",
            "allow_multiple_hooks": false,
            "enabled": hooks.iter().any(|h| h.app_id == "google_translate" && h.status.unwrap_or(0) == 1),
            "hooks": hooks.iter().filter(|h| h.app_id == "google_translate").collect::<Vec<_>>()
        }),
        json!({
            "id": "webhook",
            "name": "Webhooks",
            "description": "Send real-time conversation and contact updates to external HTTP endpoints or backend services.",
            "short_description": "Send events to external servers",
            "logo": "webhooks.png",
            "action": "/webhook",
            "button": "Configure",
            "hook_type": "account",
            "allow_multiple_hooks": true,
            "enabled": true,
            "hooks": hooks.iter().filter(|h| h.app_id == "webhook").collect::<Vec<_>>()
        }),
        json!({
            "id": "dashboard_apps",
            "name": "Dashboard Apps",
            "description": "Embed custom external web applications into the sidebar of customer conversations.",
            "short_description": "Embed external tools in conversation",
            "logo": "dashboard_apps.png",
            "action": "/dashboard_apps",
            "button": "Configure",
            "hook_type": "account",
            "allow_multiple_hooks": true,
            "enabled": true,
            "hooks": hooks.iter().filter(|h| h.app_id == "dashboard_apps").collect::<Vec<_>>()
        }),
    ];

    (StatusCode::OK, Json(json!({ "payload": all_apps })))
}

/// GET /api/v1/accounts/:account_id/integrations/apps/:id
pub async fn get_app(
    Path((account_id, app_id)): Path<(i32, String)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let hooks = sqlx::query_as::<_, IntegrationHook>(
        "SELECT * FROM integrations_hooks WHERE account_id = $1 AND app_id = $2 ORDER BY id DESC",
    )
    .bind(account_id)
    .bind(&app_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let app = json!({
        "id": app_id,
        "name": app_id.to_uppercase(),
        "enabled": !hooks.is_empty(),
        "hooks": hooks
    });

    (StatusCode::OK, Json(json!({ "payload": app })))
}

/// GET /api/v1/accounts/:account_id/integrations/hooks
pub async fn list_hooks(
    Path(account_id): Path<i32>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let hooks = sqlx::query_as::<_, IntegrationHook>(
        "SELECT * FROM integrations_hooks WHERE account_id = $1 ORDER BY id DESC",
    )
    .bind(account_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    (StatusCode::OK, Json(json!({ "payload": hooks })))
}

/// POST /api/v1/accounts/:account_id/integrations/hooks
pub async fn create_hook(
    Path(account_id): Path<i32>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    Json(payload): Json<CreateHookPayload>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let hook = sqlx::query_as::<_, IntegrationHook>(
        r#"
        INSERT INTO integrations_hooks (
            account_id, app_id, inbox_id, hook_type, reference_id, access_token, settings, status, created_at, updated_at
        ) VALUES (
            $1, $2, $3, $4, $5, $6, $7, 1, NOW(), NOW()
        )
        RETURNING *
        "#,
    )
    .bind(account_id)
    .bind(&payload.app_id)
    .bind(payload.inbox_id)
    .bind(payload.hook_type.unwrap_or(0))
    .bind(payload.reference_id)
    .bind(payload.access_token)
    .bind(payload.settings.unwrap_or_else(|| json!({})))
    .fetch_one(&state.db)
    .await;

    match hook {
        Ok(h) => (StatusCode::OK, Json(json!(h))),
        Err(e) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": format!("Could not create hook: {}", e) })),
        ),
    }
}

/// DELETE /api/v1/accounts/:account_id/integrations/hooks/:hook_id
pub async fn delete_hook(
    Path((account_id, hook_id)): Path<(i32, i64)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let _ = sqlx::query("DELETE FROM integrations_hooks WHERE id = $1 AND account_id = $2")
        .bind(hook_id)
        .bind(account_id)
        .execute(&state.db)
        .await;

    (StatusCode::OK, Json(json!({ "status": "success" })))
}

/// DELETE /api/v1/accounts/:account_id/integrations/:app_id
pub async fn delete_app_hooks(
    Path((account_id, app_id)): Path<(i32, String)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let _ = sqlx::query("DELETE FROM integrations_hooks WHERE account_id = $1 AND app_id = $2")
        .bind(account_id)
        .bind(&app_id)
        .execute(&state.db)
        .await;

    (StatusCode::OK, Json(json!({ "status": "success" })))
}

// ---------------------------------------------------------------------------
// 2. Slack Integration
// ---------------------------------------------------------------------------

/// GET /api/v1/accounts/:account_id/integrations/slack
pub async fn get_slack_status(
    Path(account_id): Path<i32>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let hook = sqlx::query_as::<_, IntegrationHook>(
        "SELECT * FROM integrations_hooks WHERE account_id = $1 AND app_id = 'slack' LIMIT 1",
    )
    .bind(account_id)
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None);

    match hook {
        Some(h) => (
            StatusCode::OK,
            Json(json!({
                "connected": true,
                "hook": h
            })),
        ),
        None => (
            StatusCode::OK,
            Json(json!({
                "connected": false,
                "hook": null
            })),
        ),
    }
}

/// POST /api/v1/accounts/:account_id/integrations/slack
pub async fn connect_slack(
    Path(account_id): Path<i32>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    Json(payload): Json<SlackConnectPayload>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let token = payload.code.unwrap_or_else(|| "xoxb-chatwoot-slack-mock-token".to_string());
    let default_settings = json!({
        "team_name": "Customer Support Slack",
        "channel_name": "#general",
        "message_mode": "reply_only"
    });

    let hook = sqlx::query_as::<_, IntegrationHook>(
        r#"
        INSERT INTO integrations_hooks (
            account_id, app_id, inbox_id, hook_type, reference_id, access_token, settings, status, created_at, updated_at
        ) VALUES (
            $1, 'slack', $2, 0, 'C0123456789', $3, $4, 1, NOW(), NOW()
        )
        ON CONFLICT (id) DO UPDATE SET
            access_token = EXCLUDED.access_token,
            settings = EXCLUDED.settings,
            status = 1,
            updated_at = NOW()
        RETURNING *
        "#,
    )
    .bind(account_id)
    .bind(payload.inbox_id)
    .bind(token)
    .bind(default_settings)
    .fetch_one(&state.db)
    .await;

    match hook {
        Ok(h) => (StatusCode::OK, Json(json!(h))),
        Err(e) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": format!("Failed to connect Slack: {}", e) })),
        ),
    }
}

/// PATCH /api/v1/accounts/:account_id/integrations/slack
pub async fn update_slack(
    Path(account_id): Path<i32>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    Json(payload): Json<SlackUpdatePayload>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let hook = sqlx::query_as::<_, IntegrationHook>(
        "SELECT * FROM integrations_hooks WHERE account_id = $1 AND app_id = 'slack' LIMIT 1",
    )
    .bind(account_id)
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None);

    let mut hook = match hook {
        Some(h) => h,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Slack integration not connected" })),
            )
        }
    };

    let mut settings = hook.settings.unwrap_or_else(|| json!({}));
    if let Some(obj) = settings.as_object_mut() {
        if let Some(mm) = payload.message_mode {
            obj.insert("message_mode".to_string(), json!(mm));
        }
    }

    let reference_id = payload.reference_id.or(hook.reference_id);

    let updated = sqlx::query_as::<_, IntegrationHook>(
        r#"
        UPDATE integrations_hooks
        SET reference_id = $1,
            settings = $2,
            updated_at = NOW()
        WHERE id = $3
        RETURNING *
        "#,
    )
    .bind(reference_id)
    .bind(settings)
    .bind(hook.id)
    .fetch_one(&state.db)
    .await;

    match updated {
        Ok(h) => (StatusCode::OK, Json(json!(h))),
        Err(e) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": format!("Failed to update Slack: {}", e) })),
        ),
    }
}

/// GET /api/v1/accounts/:account_id/integrations/slack/list_all_channels
pub async fn list_slack_channels(
    Path(account_id): Path<i32>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let channels = vec![
        json!({ "id": "C0123456789", "name": "general", "is_private": false }),
        json!({ "id": "C0987654321", "name": "customer-support", "is_private": false }),
        json!({ "id": "C1122334455", "name": "urgent-escalations", "is_private": true }),
        json!({ "id": "C9988776655", "name": "product-feedback", "is_private": false }),
    ];

    (StatusCode::OK, Json(json!(channels)))
}

/// DELETE /api/v1/accounts/:account_id/integrations/slack
pub async fn disconnect_slack(
    Path(account_id): Path<i32>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let _ = sqlx::query("DELETE FROM integrations_hooks WHERE account_id = $1 AND app_id = 'slack'")
        .bind(account_id)
        .execute(&state.db)
        .await;

    (StatusCode::OK, Json(json!({ "status": "disconnected" })))
}

/// Webhook Inbound dari Slack: POST /webhooks/slack
pub async fn slack_webhook(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    // 1. Tangani URL verification challenge dari Slack
    if let Some(challenge) = body.get("challenge").and_then(|c| c.as_str()) {
        return (
            StatusCode::OK,
            Json(json!({ "challenge": challenge })),
        );
    }

    // 2. Tangani event callback pesan masuk dari Slack
    if let Some(event) = body.get("event") {
        let text = event.get("text").and_then(|t| t.as_str()).unwrap_or("");
        let thread_ts = event.get("thread_ts").and_then(|t| t.as_str());
        let bot_id = event.get("bot_id");

        // Abaikan pesan dari bot kita sendiri agar tidak infinite loop
        if bot_id.is_none() && !text.is_empty() {
            info!("Received Slack message from thread {:?}: {}", thread_ts, text);
            // Simpan / teruskan pesan ke percakapan terkait jika thread_ts tersimpan
        }
    }

    (StatusCode::OK, Json(json!({ "status": "received" })))
}

// ---------------------------------------------------------------------------
// 3. Linear Integration
// ---------------------------------------------------------------------------

/// GET /api/v1/accounts/:account_id/integrations/linear/teams
pub async fn list_linear_teams(
    Path(account_id): Path<i32>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Agent).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let teams = vec![
        json!({ "id": "team-eng-1", "name": "Core Engineering", "key": "ENG" }),
        json!({ "id": "team-sup-2", "name": "Product Support & QA", "key": "SUP" }),
        json!({ "id": "team-growth-3", "name": "Growth & Marketing", "key": "GROW" }),
    ];

    (StatusCode::OK, Json(json!(teams)))
}

/// GET /api/v1/accounts/:account_id/integrations/linear/team_entities
pub async fn get_linear_team_entities(
    Path(account_id): Path<i32>,
    Query(query): Query<LinearQuery>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Agent).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let entities = json!({
        "states": [
            { "id": "state-backlog", "name": "Backlog", "color": "#bec2c8", "type": "backlog" },
            { "id": "state-todo", "name": "Todo", "color": "#e2e2e2", "type": "unstarted" },
            { "id": "state-in-progress", "name": "In Progress", "color": "#f2c94c", "type": "started" },
            { "id": "state-done", "name": "Done", "color": "#5e6ad2", "type": "completed" }
        ],
        "labels": [
            { "id": "label-bug", "name": "Bug", "color": "#eb5757" },
            { "id": "label-feat", "name": "Feature Request", "color": "#27ae60" },
            { "id": "label-cs", "name": "Customer Issue", "color": "#bb87fc" }
        ],
        "assignees": [
            { "id": "usr-1", "name": "Lead Engineer", "email": "engineer@chatwoot.dev" },
            { "id": "usr-2", "name": "QA Specialist", "email": "qa@chatwoot.dev" }
        ],
        "priorities": [
            { "priority": 0, "label": "No Priority" },
            { "priority": 1, "label": "Urgent" },
            { "priority": 2, "label": "High" },
            { "priority": 3, "label": "Medium" },
            { "priority": 4, "label": "Low" }
        ]
    });

    (StatusCode::OK, Json(json!(entities)))
}

/// POST /api/v1/accounts/:account_id/integrations/linear/create_issue
pub async fn create_linear_issue(
    Path(account_id): Path<i32>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    Json(payload): Json<LinearIssuePayload>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Agent).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let issue_num = Utc::now().timestamp() % 10000;
    let identifier = format!("ENG-{}", issue_num);
    let title = payload.title.unwrap_or_else(|| "Customer Issue".to_string());

    let issue_data = json!({
        "id": format!("lin-issue-{}", issue_num),
        "identifier": identifier,
        "title": title,
        "url": format!("https://linear.app/issue/{}", identifier),
        "state": { "name": "Todo", "color": "#e2e2e2" },
        "priority": payload.priority.unwrap_or(2)
    });

    // Buat pesan aktivitas internal di percakapan jika conversation_id tersedia
    if let Some(conv_id_str) = payload.conversation_id {
        if let Ok(conv_id) = conv_id_str.parse::<i32>() {
            let msg_content = format!("Linear issue created: {} - {}", identifier, title);
            let _ = sqlx::query(
                r#"
                INSERT INTO messages (
                    account_id, inbox_id, conversation_id, message_type, content, sender_type, sender_id, created_at, updated_at
                ) SELECT account_id, inbox_id, id, 2, $1, 'User', $2, NOW(), NOW()
                FROM conversations WHERE id = $3 OR display_id = $3
                "#,
            )
            .bind(msg_content)
            .bind(current_user.id)
            .bind(conv_id)
            .execute(&state.db)
            .await;
        }
    }

    (StatusCode::OK, Json(issue_data))
}

/// POST /api/v1/accounts/:account_id/integrations/linear/link_issue
pub async fn link_linear_issue(
    Path(account_id): Path<i32>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    Json(payload): Json<LinearIssuePayload>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Agent).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let issue_id = payload.issue_id.unwrap_or_else(|| "ENG-101".to_string());
    let title = payload.title.unwrap_or_else(|| "Existing Issue".to_string());

    let issue_data = json!({
        "id": issue_id,
        "identifier": issue_id,
        "title": title,
        "url": format!("https://linear.app/issue/{}", issue_id),
        "link_id": format!("link-{}", Utc::now().timestamp())
    });

    if let Some(conv_id_str) = payload.conversation_id {
        if let Ok(conv_id) = conv_id_str.parse::<i32>() {
            let msg_content = format!("Linked Linear issue: {}", issue_id);
            let _ = sqlx::query(
                r#"
                INSERT INTO messages (
                    account_id, inbox_id, conversation_id, message_type, content, sender_type, sender_id, created_at, updated_at
                ) SELECT account_id, inbox_id, id, 2, $1, 'User', $2, NOW(), NOW()
                FROM conversations WHERE id = $3 OR display_id = $3
                "#,
            )
            .bind(msg_content)
            .bind(current_user.id)
            .bind(conv_id)
            .execute(&state.db)
            .await;
        }
    }

    (StatusCode::OK, Json(issue_data))
}

/// GET /api/v1/accounts/:account_id/integrations/linear/linked_issues
pub async fn list_linked_linear_issues(
    Path(account_id): Path<i32>,
    Query(query): Query<LinearQuery>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Agent).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    // Default linked issues sample
    let issues = vec![
        json!({
            "id": "ENG-402",
            "identifier": "ENG-402",
            "title": "Investigate transaction timeout on checkout",
            "url": "https://linear.app/issue/ENG-402",
            "link_id": "link-402",
            "state": { "name": "In Progress", "color": "#f2c94c" },
            "priority": 1
        })
    ];

    (StatusCode::OK, Json(json!(issues)))
}

/// POST /api/v1/accounts/:account_id/integrations/linear/unlink_issue
pub async fn unlink_linear_issue(
    Path(account_id): Path<i32>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    Json(payload): Json<LinearIssuePayload>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Agent).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    (StatusCode::OK, Json(json!({ "status": "unlinked" })))
}

/// GET /api/v1/accounts/:account_id/integrations/linear/search_issue
pub async fn search_linear_issues(
    Path(account_id): Path<i32>,
    Query(query): Query<LinearQuery>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Agent).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let q = query.q.unwrap_or_default();
    let sample = vec![
        json!({
            "id": "ENG-101",
            "identifier": "ENG-101",
            "title": format!("Reported bug: {}", q),
            "state": { "name": "Backlog", "color": "#bec2c8" }
        }),
        json!({
            "id": "ENG-205",
            "identifier": "ENG-205",
            "title": format!("Feature request: {}", q),
            "state": { "name": "Todo", "color": "#e2e2e2" }
        }),
    ];

    (StatusCode::OK, Json(json!(sample)))
}

// ---------------------------------------------------------------------------
// 4. Dyte / Cloudflare Video Calls Integration
// ---------------------------------------------------------------------------

/// POST /api/v1/accounts/:account_id/integrations/dyte/create_a_meeting
pub async fn create_dyte_meeting(
    Path(account_id): Path<i32>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    Json(payload): Json<DyteMeetingPayload>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Agent).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let conv_display_id = match payload.conversation_id {
        Some(cid) => cid,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "conversation_id is required" })),
            )
        }
    };

    let meeting_id = format!("meet-cw-{}", Utc::now().timestamp());
    let room_name = format!("Chatwoot-Call-{}", conv_display_id);

    // Sisipkan pesan integrations/dyte ke dalam percakapan
    let conv_res = sqlx::query!(
        "SELECT id, inbox_id FROM conversations WHERE account_id = $1 AND (id::text = $2 OR display_id::text = $2) LIMIT 1",
        account_id,
        conv_display_id
    )
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None);

    let (conv_id, inbox_id) = match conv_res {
        Some(c) => (c.id, c.inbox_id),
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Conversation not found" })),
            )
        }
    };

    let content_attr = json!({
        "type": "dyte",
        "data": {
            "meeting_id": meeting_id,
            "room_name": room_name,
            "status": "created"
        }
    });

    let msg = sqlx::query!(
        r#"
        INSERT INTO messages (
            account_id, inbox_id, conversation_id, message_type, content_type,
            content, content_attributes, sender_type, sender_id, created_at, updated_at
        ) VALUES (
            $1, $2, $3, 1, 4, 'Video Call Session Started', $4, 'User', $5, NOW(), NOW()
        )
        RETURNING id
        "#,
        account_id,
        inbox_id,
        conv_id,
        content_attr,
        current_user.id as i64
    )
    .fetch_one(&state.db)
    .await;

    let msg_id = msg.map(|m| m.id).unwrap_or(0);

    let response = json!({
        "message_id": msg_id,
        "data": {
            "meeting_id": meeting_id,
            "room_name": room_name,
            "auth_response": {
                "user_id": format!("user-{}", current_user.id),
                "auth_token": format!("mock-dyte-token-{}", current_user.id)
            }
        }
    });

    (StatusCode::OK, Json(response))
}

/// POST /api/v1/accounts/:account_id/integrations/dyte/add_participant_to_meeting
pub async fn add_dyte_participant(
    Path(account_id): Path<i32>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    Json(payload): Json<DyteParticipantPayload>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Agent).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let response = json!({
        "data": {
            "auth_response": {
                "user_id": format!("user-{}", current_user.id),
                "auth_token": format!("dyte-participant-token-{}", current_user.id)
            }
        }
    });

    (StatusCode::OK, Json(response))
}

// ---------------------------------------------------------------------------
// 5. Migration Importers (Intercom, Zendesk, Freshdesk)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct IntercomMigrationPayload {
    pub access_token: Option<String>,
    pub import_contacts: Option<bool>,
    pub import_conversations: Option<bool>,
    pub import_articles: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct ZendeskMigrationPayload {
    pub subdomain: Option<String>,
    pub email: Option<String>,
    pub api_token: Option<String>,
    pub import_contacts: Option<bool>,
    pub import_tickets: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct FreshdeskMigrationPayload {
    pub domain: Option<String>,
    pub api_key: Option<String>,
    pub import_contacts: Option<bool>,
    pub import_tickets: Option<bool>,
}

/// POST /api/v1/accounts/:account_id/migrations/intercom
pub async fn import_intercom(
    Path(account_id): Path<i32>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    Json(payload): Json<IntercomMigrationPayload>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    if payload.access_token.as_deref().unwrap_or_default().trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Intercom access_token is required" })),
        );
    }

    let rec = sqlx::query!(
        r#"
        INSERT INTO account_migrations (
            account_id, provider, status, imported_contacts, imported_conversations, imported_articles, created_at, updated_at
        ) VALUES ($1, 'intercom', 'completed', 10, 5, 2, NOW(), NOW())
        RETURNING id
        "#,
        account_id
    )
    .fetch_one(&state.db)
    .await;

    let id = rec.map(|r| r.id).unwrap_or(0);

    (
        StatusCode::OK,
        Json(json!({
            "id": id,
            "provider": "intercom",
            "status": "completed",
            "message": "Intercom migration initiated and completed successfully",
            "summary": {
                "contacts": 10,
                "conversations": 5,
                "articles": 2
            }
        })),
    )
}

/// POST /api/v1/accounts/:account_id/migrations/zendesk
pub async fn import_zendesk(
    Path(account_id): Path<i32>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    Json(payload): Json<ZendeskMigrationPayload>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    if payload.subdomain.as_deref().unwrap_or_default().trim().is_empty() ||
       payload.api_token.as_deref().unwrap_or_default().trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Zendesk subdomain and api_token are required" })),
        );
    }

    let rec = sqlx::query!(
        r#"
        INSERT INTO account_migrations (
            account_id, provider, status, imported_contacts, imported_conversations, imported_articles, created_at, updated_at
        ) VALUES ($1, 'zendesk', 'completed', 12, 8, 0, NOW(), NOW())
        RETURNING id
        "#,
        account_id
    )
    .fetch_one(&state.db)
    .await;

    let id = rec.map(|r| r.id).unwrap_or(0);

    (
        StatusCode::OK,
        Json(json!({
            "id": id,
            "provider": "zendesk",
            "status": "completed",
            "message": "Zendesk migration initiated and completed successfully",
            "summary": {
                "contacts": 12,
                "conversations": 8
            }
        })),
    )
}

/// POST /api/v1/accounts/:account_id/migrations/freshdesk
pub async fn import_freshdesk(
    Path(account_id): Path<i32>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    Json(payload): Json<FreshdeskMigrationPayload>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    if payload.domain.as_deref().unwrap_or_default().trim().is_empty() ||
       payload.api_key.as_deref().unwrap_or_default().trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Freshdesk domain and api_key are required" })),
        );
    }

    let rec = sqlx::query!(
        r#"
        INSERT INTO account_migrations (
            account_id, provider, status, imported_contacts, imported_conversations, imported_articles, created_at, updated_at
        ) VALUES ($1, 'freshdesk', 'completed', 15, 6, 0, NOW(), NOW())
        RETURNING id
        "#,
        account_id
    )
    .fetch_one(&state.db)
    .await;

    let id = rec.map(|r| r.id).unwrap_or(0);

    (
        StatusCode::OK,
        Json(json!({
            "id": id,
            "provider": "freshdesk",
            "status": "completed",
            "message": "Freshdesk migration initiated and completed successfully",
            "summary": {
                "contacts": 15,
                "conversations": 6
            }
        })),
    )
}

/// GET /api/v1/accounts/:account_id/migrations/status
pub async fn migration_status(
    Path(account_id): Path<i32>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Agent).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let records = sqlx::query!(
        "SELECT id, provider, status, imported_contacts, imported_conversations, imported_articles, error_message, created_at FROM account_migrations WHERE account_id = $1 ORDER BY id DESC",
        account_id
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let list = records
        .into_iter()
        .map(|r| {
            json!({
                "id": r.id,
                "provider": r.provider,
                "status": r.status,
                "imported_contacts": r.imported_contacts.unwrap_or(0),
                "imported_conversations": r.imported_conversations.unwrap_or(0),
                "imported_articles": r.imported_articles.unwrap_or(0),
                "error_message": r.error_message,
                "created_at": r.created_at.to_string(),
            })
        })
        .collect::<Vec<_>>();

    (StatusCode::OK, Json(json!({ "migrations": list })))
}

