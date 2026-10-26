use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::middleware::auth::CurrentUser;
use crate::models::call::{
    Call, CallAgentInfo, CallContactInfo, CallConversationInfo, CallInboxInfo,
    CallListMeta, CallListResponse, CallResponse, TriggerContactCallPayload,
};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct CallsQuery {
    pub page: Option<i64>,
    pub status: Option<String>,
    pub direction: Option<String>,
    pub inbox_id: Option<i64>,
    pub agent_id: Option<i64>,
    pub since: Option<i64>,
    pub until: Option<i64>,
}

pub async fn list_calls(
    Path(account_id): Path<i64>,
    Query(query): Query<CallsQuery>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let page = query.page.unwrap_or(1).max(1);
    let per_page: i64 = 25;
    let offset = (page - 1) * per_page;

    // Check user role in account
    let role: Option<(i32,)> = sqlx::query_as(
        "SELECT role FROM account_users WHERE account_id = $1 AND user_id = $2",
    )
    .bind(account_id as i32)
    .bind(current_user.id)
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None);

    let is_admin = current_user.is_super_admin() || role.map(|r| r.0 == 1).unwrap_or(false);

    let status_filter = query.status.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(|s| s.replace('-', "_"));
    let direction_filter = query.direction.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(|d| {
        if d == "outbound" || d == "outgoing" { 1 } else { 0 }
    });

    let mut conditions = vec!["c.account_id = $1".to_string()];
    let mut param_idx = 2;

    // Visibility scoping: non-admin only sees calls they accepted or ringing calls
    if !is_admin {
        conditions.push(format!(
            "(c.accepted_by_agent_id = ${} OR c.status = 'ringing')",
            param_idx
        ));
        param_idx += 1;
    }

    if status_filter.is_some() {
        conditions.push(format!("c.status = ${}", param_idx));
        param_idx += 1;
    }

    if direction_filter.is_some() {
        conditions.push(format!("c.direction = ${}", param_idx));
        param_idx += 1;
    }

    if query.inbox_id.is_some() {
        conditions.push(format!("c.inbox_id = ${}", param_idx));
        param_idx += 1;
    }

    if is_admin && query.agent_id.is_some() {
        conditions.push(format!("c.accepted_by_agent_id = ${}", param_idx));
        param_idx += 1;
    }

    if query.since.is_some() && query.until.is_some() {
        conditions.push(format!(
            "c.created_at >= ${} AND c.created_at <= ${}",
            param_idx,
            param_idx + 1
        ));
        param_idx += 2;
    }

    let where_clause = conditions.join(" AND ");

    // Query total count
    let count_sql = format!("SELECT count(*) FROM calls c WHERE {}", where_clause);
    let mut count_query = sqlx::query_as::<_, (i64,)>(&count_sql).bind(account_id);

    if !is_admin {
        count_query = count_query.bind(current_user.id as i64);
    }
    if let Some(ref st) = status_filter {
        count_query = count_query.bind(st);
    }
    if let Some(dir_int) = direction_filter {
        count_query = count_query.bind(dir_int);
    }
    if let Some(inbox_id) = query.inbox_id {
        count_query = count_query.bind(inbox_id);
    }
    if is_admin {
        if let Some(agent_id) = query.agent_id {
            count_query = count_query.bind(agent_id);
        }
    }
    if let (Some(since_ts), Some(until_ts)) = (query.since, query.until) {
        let since_dt = DateTime::<Utc>::from_timestamp(since_ts, 0)
            .map(|dt| dt.naive_utc())
            .unwrap_or_default();
        let until_dt = DateTime::<Utc>::from_timestamp(until_ts, 0)
            .map(|dt| dt.naive_utc())
            .unwrap_or_default();
        count_query = count_query.bind(since_dt).bind(until_dt);
    }

    let total_count: (i64,) = count_query.fetch_one(&state.db).await.unwrap_or((0,));

    // Query calls rows
    let select_sql = format!(
        "SELECT c.* FROM calls c WHERE {} ORDER BY c.created_at DESC LIMIT ${} OFFSET ${}",
        where_clause, param_idx, param_idx + 1
    );

    let mut rows_query = sqlx::query_as::<_, Call>(&select_sql).bind(account_id);

    if !is_admin {
        rows_query = rows_query.bind(current_user.id as i64);
    }
    if let Some(ref st) = status_filter {
        rows_query = rows_query.bind(st);
    }
    if let Some(dir_int) = direction_filter {
        rows_query = rows_query.bind(dir_int);
    }
    if let Some(inbox_id) = query.inbox_id {
        rows_query = rows_query.bind(inbox_id);
    }
    if is_admin {
        if let Some(agent_id) = query.agent_id {
            rows_query = rows_query.bind(agent_id);
        }
    }
    if let (Some(since_ts), Some(until_ts)) = (query.since, query.until) {
        let since_dt = DateTime::<Utc>::from_timestamp(since_ts, 0)
            .map(|dt| dt.naive_utc())
            .unwrap_or_default();
        let until_dt = DateTime::<Utc>::from_timestamp(until_ts, 0)
            .map(|dt| dt.naive_utc())
            .unwrap_or_default();
        rows_query = rows_query.bind(since_dt).bind(until_dt);
    }
    rows_query = rows_query.bind(per_page).bind(offset);

    let calls = rows_query.fetch_all(&state.db).await.unwrap_or_default();

    let mut payload = Vec::new();
    for call in calls {
        let conv_display_id: Option<(i32,)> = sqlx::query_as(
            "SELECT display_id FROM conversations WHERE id = $1",
        )
        .bind(call.conversation_id as i32)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten();

        let inbox_info: Option<(String, String)> = sqlx::query_as(
            "SELECT name, channel_type FROM inboxes WHERE id = $1",
        )
        .bind(call.inbox_id as i32)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten();

        let agent_info = if let Some(agent_id) = call.accepted_by_agent_id {
            let u: Option<(Option<String>, Option<String>)> = sqlx::query_as(
                "SELECT name, display_name FROM users WHERE id = $1",
            )
            .bind(agent_id as i32)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten();

            u.map(|(name, disp)| CallAgentInfo {
                id: agent_id,
                name: disp.or(name).unwrap_or_else(|| "Agent".to_string()),
                avatar: "".to_string(),
            })
        } else {
            None
        };

        let contact_info = if let Some(cid) = call.contact_id {
            let c: Option<(Option<String>, Option<String>)> = sqlx::query_as(
                "SELECT name, phone_number FROM contacts WHERE id = $1",
            )
            .bind(cid as i32)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten();

            c.map(|(name, phone)| CallContactInfo {
                id: cid,
                name: name.unwrap_or_else(|| "Customer".to_string()),
                phone_number: phone,
                avatar: "".to_string(),
            })
        } else {
            None
        };

        let rec_url = call.meta.get("recording_url").and_then(|v| v.as_str()).map(|s| s.to_string());

        payload.push(CallResponse {
            id: call.id,
            call_id: call.provider_call_id.clone(),
            provider: call.provider_str().to_string(),
            status: call.display_status(),
            direction: call.direction_label().to_string(),
            duration_seconds: call.duration_seconds,
            end_reason: call.end_reason.clone(),
            started_at: call.started_at.map(|dt| dt.and_utc().timestamp()),
            created_at: call.created_at.and_utc().timestamp(),
            message_id: call.message_id,
            recording_url: rec_url,
            transcript: call.transcript.clone(),
            conversation: CallConversationInfo {
                id: call.conversation_id,
                display_id: conv_display_id.map(|c| c.0).unwrap_or(call.conversation_id as i32),
            },
            inbox: CallInboxInfo {
                id: call.inbox_id,
                name: inbox_info.as_ref().map(|i| i.0.clone()).unwrap_or_else(|| "Voice Inbox".to_string()),
                channel_type: inbox_info.as_ref().map(|i| i.1.clone()).unwrap_or_else(|| "Channel::TwilioSms".to_string()),
                medium: None,
            },
            agent: agent_info,
            contact: contact_info,
        });
    }

    let count = total_count.0;
    let total_pages = if count == 0 { 0 } else { (count + per_page - 1) / per_page };

    (
        StatusCode::OK,
        Json(CallListResponse {
            meta: CallListMeta {
                count,
                current_page: page,
                total_pages,
            },
            payload,
        }),
    )
}

pub async fn get_call(
    Path((account_id, id)): Path<(i64, i64)>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let call = match sqlx::query_as::<_, Call>(
        "SELECT * FROM calls WHERE account_id = $1 AND id = $2",
    )
    .bind(account_id)
    .bind(id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(c)) => c,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Call not found" })),
            )
                .into_response();
        }
    };

    let conv_display_id: Option<(i32,)> = sqlx::query_as(
        "SELECT display_id FROM conversations WHERE id = $1",
    )
    .bind(call.conversation_id as i32)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();

    let inbox_info: Option<(String, String)> = sqlx::query_as(
        "SELECT name, channel_type FROM inboxes WHERE id = $1",
    )
    .bind(call.inbox_id as i32)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();

    let agent_info = if let Some(agent_id) = call.accepted_by_agent_id {
        let u: Option<(Option<String>, Option<String>)> = sqlx::query_as(
            "SELECT name, display_name FROM users WHERE id = $1",
        )
        .bind(agent_id as i32)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten();

        u.map(|(name, disp)| CallAgentInfo {
            id: agent_id,
            name: disp.or(name).unwrap_or_else(|| "Agent".to_string()),
            avatar: "".to_string(),
        })
    } else {
        None
    };

    let contact_info = if let Some(cid) = call.contact_id {
        let c: Option<(Option<String>, Option<String>)> = sqlx::query_as(
            "SELECT name, phone_number FROM contacts WHERE id = $1",
        )
        .bind(cid as i32)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten();

        c.map(|(name, phone)| CallContactInfo {
            id: cid,
            name: name.unwrap_or_else(|| "Customer".to_string()),
            phone_number: phone,
            avatar: "".to_string(),
        })
    } else {
        None
    };

    let rec_url = call.meta.get("recording_url").and_then(|v| v.as_str()).map(|s| s.to_string());

    let resp = CallResponse {
        id: call.id,
        call_id: call.provider_call_id.clone(),
        provider: call.provider_str().to_string(),
        status: call.display_status(),
        direction: call.direction_label().to_string(),
        duration_seconds: call.duration_seconds,
        end_reason: call.end_reason.clone(),
        started_at: call.started_at.map(|dt| dt.and_utc().timestamp()),
        created_at: call.created_at.and_utc().timestamp(),
        message_id: call.message_id,
        recording_url: rec_url,
        transcript: call.transcript.clone(),
        conversation: CallConversationInfo {
            id: call.conversation_id,
            display_id: conv_display_id.map(|c| c.0).unwrap_or(call.conversation_id as i32),
        },
        inbox: CallInboxInfo {
            id: call.inbox_id,
            name: inbox_info.as_ref().map(|i| i.0.clone()).unwrap_or_else(|| "Voice Inbox".to_string()),
            channel_type: inbox_info.as_ref().map(|i| i.1.clone()).unwrap_or_else(|| "Channel::TwilioSms".to_string()),
            medium: None,
        },
        agent: agent_info,
        contact: contact_info,
    };

    (StatusCode::OK, Json(json!({ "payload": resp }))).into_response()
}

pub async fn create_contact_call(
    Path((account_id, contact_id)): Path<(i64, i64)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    Json(payload): Json<TriggerContactCallPayload>,
) -> impl IntoResponse {
    let contact_exists: Option<(i32,)> = sqlx::query_as(
        "SELECT id FROM contacts WHERE id = $1 AND account_id = $2",
    )
    .bind(contact_id as i32)
    .bind(account_id as i32)
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None);

    if contact_exists.is_none() {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Contact not found" })),
        )
            .into_response();
    }

    // Resolve or find conversation
    let conv_id: i64 = if let Some(disp_id) = payload.conversation_id {
        let cid: Option<(i32,)> = sqlx::query_as(
            "SELECT id FROM conversations WHERE account_id = $1 AND display_id = $2",
        )
        .bind(account_id as i32)
        .bind(disp_id)
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None);

        cid.map(|c| c.0 as i64).unwrap_or(0)
    } else {
        0
    };

    let conv_id = if conv_id > 0 {
        conv_id
    } else {
        // Find existing or create placeholder conversation
        let existing: Option<(i32,)> = sqlx::query_as(
            "SELECT id FROM conversations WHERE account_id = $1 AND contact_id = $2 AND inbox_id = $3 ORDER BY id DESC LIMIT 1",
        )
        .bind(account_id as i32)
        .bind(contact_id as i32)
        .bind(payload.inbox_id as i32)
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None);

        if let Some(c) = existing {
            c.0 as i64
        } else {
            // create new conversation
            let max_display_id: (i32,) = sqlx::query_as(
                "SELECT COALESCE(MAX(display_id), 0) + 1 FROM conversations WHERE account_id = $1",
            )
            .bind(account_id as i32)
            .fetch_one(&state.db)
            .await
            .unwrap_or((1,));

            let new_conv: Option<(i32,)> = sqlx::query_as(
                r#"
                INSERT INTO conversations (account_id, inbox_id, contact_id, status, display_id, uuid, last_activity_at, created_at, updated_at)
                VALUES ($1, $2, $3, 0, $4, $5, NOW(), NOW(), NOW())
                RETURNING id
                "#,
            )
            .bind(account_id as i32)
            .bind(payload.inbox_id as i32)
            .bind(contact_id as i32)
            .bind(max_display_id.0)
            .bind(Uuid::new_v4())
            .fetch_optional(&state.db)
            .await
            .unwrap_or(None);

            new_conv.map(|c| c.0 as i64).unwrap_or(0)
        }
    };

    let provider_call_id = format!("call_{}", Uuid::new_v4());
    let conference_sid = format!("conf_account_{}_call_{}", account_id, provider_call_id);

    let new_call: Option<(i64,)> = sqlx::query_as(
        r#"
        INSERT INTO calls (
            account_id, inbox_id, conversation_id, contact_id, accepted_by_agent_id,
            provider_call_id, provider, direction, status, meta, ring_state, created_at, updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, 0, 1, 'ringing', $7, $8, NOW(), NOW())
        RETURNING id
        "#,
    )
    .bind(account_id)
    .bind(payload.inbox_id)
    .bind(conv_id)
    .bind(contact_id)
    .bind(current_user.id as i64)
    .bind(&provider_call_id)
    .bind(json!({ "conference_sid": conference_sid }))
    .bind(json!({ "ring_recipient_ids": [current_user.id] }))
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None);

    if new_call.is_none() {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "Could not create call record" })),
        )
            .into_response();
    }

    let conv_display_id: (i32,) = sqlx::query_as(
        "SELECT display_id FROM conversations WHERE id = $1",
    )
    .bind(conv_id as i32)
    .fetch_one(&state.db)
    .await
    .unwrap_or((1,));

    (
        StatusCode::OK,
        Json(json!({
            "conversation_id": conv_display_id.0,
            "inbox_id": payload.inbox_id,
            "call_sid": provider_call_id,
            "conference_sid": conference_sid
        })),
    )
        .into_response()
}

pub async fn get_whatsapp_call(
    Path((account_id, id)): Path<(i64, i64)>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    get_call(Path((account_id, id)), State(state)).await
}

pub async fn accept_whatsapp_call(
    Path((account_id, id)): Path<(i64, i64)>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let _ = sqlx::query(
        "UPDATE calls SET status = 'in_progress', started_at = NOW(), updated_at = NOW() WHERE account_id = $1 AND id = $2"
    )
    .bind(account_id)
    .bind(id)
    .execute(&state.db)
    .await;

    (StatusCode::OK, Json(json!({ "status": "in-progress" }))).into_response()
}

pub async fn reject_whatsapp_call(
    Path((account_id, id)): Path<(i64, i64)>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let _ = sqlx::query(
        "UPDATE calls SET status = 'rejected', end_reason = 'rejected', updated_at = NOW() WHERE account_id = $1 AND id = $2"
    )
    .bind(account_id)
    .bind(id)
    .execute(&state.db)
    .await;

    (StatusCode::OK, Json(json!({ "status": "rejected" }))).into_response()
}

pub async fn terminate_whatsapp_call(
    Path((account_id, id)): Path<(i64, i64)>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let _ = sqlx::query(
        "UPDATE calls SET status = 'completed', updated_at = NOW() WHERE account_id = $1 AND id = $2"
    )
    .bind(account_id)
    .bind(id)
    .execute(&state.db)
    .await;

    (StatusCode::OK, Json(json!({ "status": "completed" }))).into_response()
}

pub async fn upload_whatsapp_recording(
    Path((account_id, id)): Path<(i64, i64)>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let _ = sqlx::query(
        "UPDATE calls SET updated_at = NOW() WHERE account_id = $1 AND id = $2"
    )
    .bind(account_id)
    .bind(id)
    .execute(&state.db)
    .await;

    (StatusCode::OK, Json(json!({ "status": "ok" }))).into_response()
}

pub async fn initiate_whatsapp_call(
    Path(account_id): Path<i64>,
    State(_state): State<AppState>,
) -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(json!({
            "status": "initiated",
            "account_id": account_id
        })),
    )
        .into_response()
}

// ---------------------------------------------------------------------------
// Twilio Voice Calls Trunking & WebRTC Handlers
// ---------------------------------------------------------------------------

/// POST /api/v1/accounts/:account_id/calls/twilio/token
pub async fn get_twilio_client_token(
    Path(account_id): Path<i64>,
    Extension(current_user): Extension<CurrentUser>,
    State(_state): State<AppState>,
) -> impl IntoResponse {
    let identity = format!("agent_{}_{}", account_id, current_user.id);
    let mock_token = format!("mock-twilio-token-{}-{}", identity, Utc::now().timestamp());
    (
        StatusCode::OK,
        Json(json!({
            "token": mock_token,
            "identity": identity,
            "account_id": account_id
        })),
    )
}

#[derive(Debug, Deserialize)]
pub struct TwilioVoiceInboundPayload {
    #[serde(rename = "From")]
    pub from: Option<String>,
    #[serde(rename = "To")]
    pub to: Option<String>,
    #[serde(rename = "CallSid")]
    pub call_sid: Option<String>,
    #[serde(rename = "AccountSid")]
    pub account_sid: Option<String>,
}

/// POST /webhooks/voice/twilio/inbound
pub async fn twilio_voice_inbound(
    State(state): State<AppState>,
    axum::extract::Form(payload): axum::extract::Form<TwilioVoiceInboundPayload>,
) -> impl IntoResponse {
    let from_number = payload.from.unwrap_or_else(|| "Unknown".to_string());
    let to_number = payload.to.unwrap_or_default();
    let call_sid = payload.call_sid.unwrap_or_else(|| format!("CA{}", Uuid::new_v4().simple()));

    // Resolve channel
    let ch = sqlx::query!(
        "SELECT id, account_id FROM channel_twilio_sms WHERE phone_number = $1 OR account_sid = $2 LIMIT 1",
        to_number,
        payload.account_sid.unwrap_or_default()
    )
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None);

    let (account_id, inbox_id) = if let Some(c) = ch {
        let inbox = sqlx::query!(
            "SELECT id FROM inboxes WHERE channel_type = 'Channel::TwilioSms' AND channel_id = $1 LIMIT 1",
            c.id as i32
        )
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None);
        let i_id = inbox.map(|i| i.id as i64).unwrap_or(1);
        (c.account_id as i64, i_id)
    } else {
        (1, 1)
    };

    // Resolve or create contact
    let contact_id: i64 = match sqlx::query_scalar!(
        "SELECT id FROM contacts WHERE account_id = $1 AND phone_number = $2 LIMIT 1",
        account_id as i32,
        from_number
    )
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None) {
        Some(id) => id as i64,
        None => {
            sqlx::query_scalar!(
                "INSERT INTO contacts (name, phone_number, account_id, created_at, updated_at) VALUES ($1, $2, $3, NOW(), NOW()) RETURNING id",
                format!("Caller {}", from_number),
                from_number,
                account_id as i32
            )
            .fetch_one(&state.db)
            .await
            .unwrap_or(1) as i64
        }
    };

    // Resolve conversation
    let conv_id: i64 = match sqlx::query_scalar!(
        "SELECT id FROM conversations WHERE account_id = $1 AND contact_id = $2 AND inbox_id = $3 ORDER BY id DESC LIMIT 1",
        account_id as i32,
        contact_id as i32,
        inbox_id as i32
    )
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None) {
        Some(id) => id as i64,
        None => {
            let disp_id: (i32,) = sqlx::query_as("SELECT COALESCE(MAX(display_id), 0) + 1 FROM conversations WHERE account_id = $1")
                .bind(account_id as i32)
                .fetch_one(&state.db)
                .await
                .unwrap_or((1,));
            sqlx::query_scalar!(
                "INSERT INTO conversations (account_id, inbox_id, contact_id, status, display_id, created_at, updated_at) VALUES ($1, $2, $3, 0, $4, NOW(), NOW()) RETURNING id",
                account_id as i32,
                inbox_id as i32,
                contact_id as i32,
                disp_id.0
            )
            .fetch_one(&state.db)
            .await
            .unwrap_or(1) as i64
        }
    };

    // Insert call record
    let _ = sqlx::query(
        r#"
        INSERT INTO calls (
            account_id, inbox_id, conversation_id, contact_id, provider_call_id,
            provider, direction, status, meta, ring_state, created_at, updated_at
        ) VALUES ($1, $2, $3, $4, $5, 0, 0, 'ringing', '{}', '{}', NOW(), NOW())
        ON CONFLICT (provider, provider_call_id) DO NOTHING
        "#
    )
    .bind(account_id)
    .bind(inbox_id)
    .bind(conv_id)
    .bind(contact_id)
    .bind(&call_sid)
    .execute(&state.db)
    .await;

    let twiml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<Response>
    <Dial timeout="30" record="record-from-answer">
        <Client>agent_{}_inbox_{}</Client>
    </Dial>
</Response>"#,
        account_id, inbox_id
    );

    ([(axum::http::header::CONTENT_TYPE, "application/xml")], twiml).into_response()
}

#[derive(Debug, Deserialize)]
pub struct TwilioVoiceStatusPayload {
    #[serde(rename = "CallSid")]
    pub call_sid: Option<String>,
    #[serde(rename = "CallStatus")]
    pub call_status: Option<String>,
    #[serde(rename = "CallDuration")]
    pub call_duration: Option<String>,
}

/// POST /webhooks/voice/twilio/status
pub async fn twilio_voice_status(
    State(state): State<AppState>,
    axum::extract::Form(payload): axum::extract::Form<TwilioVoiceStatusPayload>,
) -> impl IntoResponse {
    if let Some(ref sid) = payload.call_sid {
        let status_str = payload.call_status.as_deref().unwrap_or("completed");
        let internal_status = match status_str {
            "in-progress" => "in_progress",
            "completed" => "completed",
            "busy" => "busy",
            "no-answer" => "no_answer",
            "canceled" => "canceled",
            "failed" => "failed",
            s => s,
        };
        let duration: Option<i32> = payload.call_duration.as_deref().and_then(|d| d.parse().ok());

        let _ = sqlx::query(
            "UPDATE calls SET status = $1, duration_seconds = COALESCE($2, duration_seconds), updated_at = NOW() WHERE provider_call_id = $3"
        )
        .bind(internal_status)
        .bind(duration)
        .bind(sid)
        .execute(&state.db)
        .await;
    }

    StatusCode::OK
}

#[derive(Debug, Deserialize)]
pub struct TwilioVoiceRecordingPayload {
    #[serde(rename = "CallSid")]
    pub call_sid: Option<String>,
    #[serde(rename = "RecordingUrl")]
    pub recording_url: Option<String>,
    #[serde(rename = "RecordingDuration")]
    pub recording_duration: Option<String>,
}

/// POST /webhooks/voice/twilio/recordings
pub async fn twilio_voice_recordings(
    State(state): State<AppState>,
    axum::extract::Form(payload): axum::extract::Form<TwilioVoiceRecordingPayload>,
) -> impl IntoResponse {
    if let (Some(ref sid), Some(ref rec_url)) = (payload.call_sid, payload.recording_url) {
        let duration: Option<i32> = payload.recording_duration.as_deref().and_then(|d| d.parse().ok());

        let _ = sqlx::query(
            r#"
            UPDATE calls
            SET meta = jsonb_set(meta, '{recording_url}', to_jsonb($1::text)),
                duration_seconds = COALESCE($2, duration_seconds),
                updated_at = NOW()
            WHERE provider_call_id = $3
            "#
        )
        .bind(rec_url)
        .bind(duration)
        .bind(sid)
        .execute(&state.db)
        .await;
    }

    StatusCode::OK
}

