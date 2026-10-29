use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::NaiveDateTime;
use serde_json::json;
use tracing::warn;

use crate::models::campaign::{
    campaign_status_to_str, campaign_type_from_val, campaign_type_to_str, Campaign,
    CampaignPayloadWrapper, CampaignResponse,
};
use crate::state::AppState;

fn format_campaign_response(
    c: Campaign,
    inbox_name: Option<String>,
    sender_name: Option<String>,
) -> CampaignResponse {
    let inbox_json = Some(json!({
        "id": c.inbox_id,
        "name": inbox_name.unwrap_or_else(|| "Channel Inbox".to_string())
    }));

    let sender_json = c.sender_id.map(|sid| {
        json!({
            "id": sid,
            "name": sender_name.unwrap_or_else(|| "Agent".to_string())
        })
    });

    let status_str = campaign_status_to_str(c.campaign_status).to_string();
    let type_str = campaign_type_to_str(c.campaign_type).to_string();

    CampaignResponse {
        id: c.display_id,
        title: c.title,
        description: c.description,
        account_id: c.account_id,
        inbox: inbox_json,
        sender: sender_json,
        message: c.message,
        template_params: c.template_params,
        campaign_status: status_str,
        enabled: c.enabled.unwrap_or(true),
        campaign_type: type_str,
        scheduled_at: c.scheduled_at.map(|t| t.and_utc().timestamp()),
        started_at: c.started_at.map(|t| t.and_utc().timestamp()),
        completed_at: c.completed_at.map(|t| t.and_utc().timestamp()),
        audience: c.audience,
        trigger_rules: c.trigger_rules,
        trigger_only_during_business_hours: c.trigger_only_during_business_hours.unwrap_or(false),
        created_at: c.created_at.and_utc().timestamp(),
        updated_at: c.updated_at.and_utc().timestamp(),
    }
}

/// GET /api/v1/accounts/:account_id/campaigns
pub async fn list_campaigns(
    Path(account_id): Path<i64>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let campaigns = match sqlx::query_as::<_, Campaign>(
        "SELECT * FROM campaigns WHERE account_id = $1 ORDER BY id DESC",
    )
    .bind(account_id)
    .fetch_all(&state.db)
    .await
    {
        Ok(c) => c,
        Err(e) => {
            warn!("Failed to fetch campaigns: {}", e);
            vec![]
        }
    };

    let mut responses = Vec::new();
    for c in campaigns {
        let inbox_name: Option<String> = sqlx::query_scalar("SELECT name FROM inboxes WHERE id = $1")
            .bind(c.inbox_id)
            .fetch_optional(&state.db)
            .await
            .unwrap_or(None);

        let sender_name: Option<String> = if let Some(sid) = c.sender_id {
            sqlx::query_scalar("SELECT name FROM users WHERE id = $1")
                .bind(sid)
                .fetch_optional(&state.db)
                .await
                .unwrap_or(None)
        } else {
            None
        };

        responses.push(format_campaign_response(c, inbox_name, sender_name));
    }

    Json(responses).into_response()
}

/// POST /api/v1/accounts/:account_id/campaigns
pub async fn create_campaign(
    Path(account_id): Path<i64>,
    State(state): State<AppState>,
    Json(body): Json<CampaignPayloadWrapper>,
) -> impl IntoResponse {
    let p = body.campaign;
    let title = p.as_ref().and_then(|c| c.title.clone()).or(body.title).unwrap_or_default();
    let description = p.as_ref().and_then(|c| c.description.clone()).or(body.description);
    let message = p.as_ref().and_then(|c| c.message.clone()).or(body.message).unwrap_or_default();
    let inbox_id = p.as_ref().and_then(|c| c.inbox_id).or(body.inbox_id).unwrap_or(1);
    let sender_id = p.as_ref().and_then(|c| c.sender_id).or(body.sender_id);
    let enabled = p.as_ref().and_then(|c| c.enabled).or(body.enabled).unwrap_or(true);
    let trigger_only_during_business_hours = p.as_ref().and_then(|c| c.trigger_only_during_business_hours).or(body.trigger_only_during_business_hours).unwrap_or(false);
    let audience = p.as_ref().and_then(|c| c.audience.clone()).or(body.audience).unwrap_or_else(|| json!([]));
    let trigger_rules = p.as_ref().and_then(|c| c.trigger_rules.clone()).or(body.trigger_rules).unwrap_or_else(|| json!({}));
    let template_params = p.as_ref().and_then(|c| c.template_params.clone()).or(body.template_params);

    let type_val = p.as_ref().and_then(|c| c.campaign_type.clone()).or(body.campaign_type).unwrap_or_else(|| json!(0));
    let campaign_type = campaign_type_from_val(&type_val);

    let scheduled_at_str = p.as_ref().and_then(|c| c.scheduled_at.clone()).or(body.scheduled_at);
    let scheduled_at_dt: Option<NaiveDateTime> = scheduled_at_str
        .as_deref()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.naive_utc());

    // Generate next display_id
    let next_display_id: (i32,) = sqlx::query_as(
        "SELECT COALESCE(MAX(display_id), 0) + 1 FROM campaigns WHERE account_id = $1",
    )
    .bind(account_id)
    .fetch_one(&state.db)
    .await
    .unwrap_or((1,));

    let campaign = match sqlx::query_as::<_, Campaign>(
        r#"
        INSERT INTO campaigns (
            display_id, title, description, message, sender_id, enabled,
            account_id, inbox_id, trigger_rules, campaign_type, campaign_status,
            audience, scheduled_at, trigger_only_during_business_hours, template_params,
            created_at, updated_at
        )
        VALUES (
            $1, $2, $3, $4, $5, $6,
            $7, $8, $9, $10, 0,
            $11, $12, $13, $14,
            NOW(), NOW()
        )
        RETURNING *
        "#,
    )
    .bind(next_display_id.0)
    .bind(&title)
    .bind(&description)
    .bind(&message)
    .bind(sender_id)
    .bind(enabled)
    .bind(account_id)
    .bind(inbox_id)
    .bind(&trigger_rules)
    .bind(campaign_type)
    .bind(&audience)
    .bind(scheduled_at_dt)
    .bind(trigger_only_during_business_hours)
    .bind(&template_params)
    .fetch_one(&state.db)
    .await
    {
        Ok(c) => c,
        Err(e) => {
            warn!("Failed to create campaign: {}", e);
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error": format!("Could not create campaign: {}", e) })),
            )
                .into_response();
        }
    };

    let inbox_name: Option<String> = sqlx::query_scalar("SELECT name FROM inboxes WHERE id = $1")
        .bind(campaign.inbox_id)
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None);

    let sender_name: Option<String> = if let Some(sid) = campaign.sender_id {
        sqlx::query_scalar("SELECT name FROM users WHERE id = $1")
            .bind(sid)
            .fetch_optional(&state.db)
            .await
            .unwrap_or(None)
    } else {
        None
    };

    let resp = format_campaign_response(campaign, inbox_name, sender_name);
    (StatusCode::OK, Json(resp)).into_response()
}

/// GET /api/v1/accounts/:account_id/campaigns/:id
pub async fn get_campaign(
    Path((account_id, id_param)): Path<(i64, String)>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let parsed_id = id_param.parse::<i64>().unwrap_or(0);

    let campaign = match sqlx::query_as::<_, Campaign>(
        "SELECT * FROM campaigns WHERE account_id = $1 AND (display_id = $2 OR id = $3)",
    )
    .bind(account_id)
    .bind(parsed_id as i32)
    .bind(parsed_id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(c)) => c,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Campaign not found" })),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    let inbox_name: Option<String> = sqlx::query_scalar("SELECT name FROM inboxes WHERE id = $1")
        .bind(campaign.inbox_id)
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None);

    let sender_name: Option<String> = if let Some(sid) = campaign.sender_id {
        sqlx::query_scalar("SELECT name FROM users WHERE id = $1")
            .bind(sid)
            .fetch_optional(&state.db)
            .await
            .unwrap_or(None)
    } else {
        None
    };

    let resp = format_campaign_response(campaign, inbox_name, sender_name);
    Json(resp).into_response()
}

/// PATCH /api/v1/accounts/:account_id/campaigns/:id
pub async fn update_campaign(
    Path((account_id, id_param)): Path<(i64, String)>,
    State(state): State<AppState>,
    Json(body): Json<CampaignPayloadWrapper>,
) -> impl IntoResponse {
    let parsed_id = id_param.parse::<i64>().unwrap_or(0);

    let existing = match sqlx::query_as::<_, Campaign>(
        "SELECT * FROM campaigns WHERE account_id = $1 AND (display_id = $2 OR id = $3)",
    )
    .bind(account_id)
    .bind(parsed_id as i32)
    .bind(parsed_id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(c)) => c,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Campaign not found" })),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    let p = body.campaign;
    let title = p.as_ref().and_then(|c| c.title.clone()).or(body.title).unwrap_or(existing.title);
    let description = p.as_ref().and_then(|c| c.description.clone()).or(body.description).or(existing.description);
    let message = p.as_ref().and_then(|c| c.message.clone()).or(body.message).unwrap_or(existing.message);
    let enabled = p.as_ref().and_then(|c| c.enabled).or(body.enabled).or(existing.enabled);

    let updated = match sqlx::query_as::<_, Campaign>(
        r#"
        UPDATE campaigns
        SET title = $1, description = $2, message = $3, enabled = $4, updated_at = NOW()
        WHERE id = $5 AND account_id = $6
        RETURNING *
        "#,
    )
    .bind(&title)
    .bind(&description)
    .bind(&message)
    .bind(enabled)
    .bind(existing.id)
    .bind(account_id)
    .fetch_one(&state.db)
    .await
    {
        Ok(c) => c,
        Err(e) => {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    let inbox_name: Option<String> = sqlx::query_scalar("SELECT name FROM inboxes WHERE id = $1")
        .bind(updated.inbox_id)
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None);

    let sender_name: Option<String> = if let Some(sid) = updated.sender_id {
        sqlx::query_scalar("SELECT name FROM users WHERE id = $1")
            .bind(sid)
            .fetch_optional(&state.db)
            .await
            .unwrap_or(None)
    } else {
        None
    };

    let resp = format_campaign_response(updated, inbox_name, sender_name);
    Json(resp).into_response()
}

/// DELETE /api/v1/accounts/:account_id/campaigns/:id
pub async fn delete_campaign(
    Path((account_id, id_param)): Path<(i64, String)>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let parsed_id = id_param.parse::<i64>().unwrap_or(0);

    let res = sqlx::query(
        "DELETE FROM campaigns WHERE account_id = $1 AND (display_id = $2 OR id = $3)",
    )
    .bind(account_id)
    .bind(parsed_id as i32)
    .bind(parsed_id)
    .execute(&state.db)
    .await;

    match res {
        Ok(r) if r.rows_affected() > 0 => StatusCode::OK.into_response(),
        Ok(_) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Campaign not found" })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

/// POST /api/v1/accounts/:account_id/campaigns/:id/trigger
pub async fn trigger_campaign_blast(
    Path((account_id, id_param)): Path<(i64, String)>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let parsed_id = id_param.parse::<i64>().unwrap_or(0);

    let campaign = match sqlx::query_as::<_, Campaign>(
        "SELECT * FROM campaigns WHERE account_id = $1 AND (display_id = $2 OR id = $3)",
    )
    .bind(account_id)
    .bind(parsed_id as i32)
    .bind(parsed_id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(c)) => c,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Campaign not found" })),
            )
                .into_response();
        }
    };

    let st = state.clone();
    let cid = campaign.id;
    tokio::spawn(async move {
        let _ = crate::services::campaign_executor::trigger_one_off_campaign(&st, account_id, cid).await;
    });

    (
        StatusCode::OK,
        Json(json!({
            "status": "ok",
            "message": "Broadcast blast triggered successfully",
            "campaign_id": campaign.display_id
        })),
    )
        .into_response()
}

/// GET /api/v1/accounts/:account_id/campaigns/:id/analytics/metrics
pub async fn get_campaign_metrics(
    Path((account_id, id_param)): Path<(i64, String)>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let parsed_id = id_param.parse::<i64>().unwrap_or(0);
    let campaign = match sqlx::query_as::<_, Campaign>(
        "SELECT * FROM campaigns WHERE account_id = $1 AND (display_id = $2 OR id = $3)",
    )
    .bind(account_id)
    .bind(parsed_id as i32)
    .bind(parsed_id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(c)) => c,
        _ => return (StatusCode::NOT_FOUND, Json(json!({ "error": "Campaign not found" }))).into_response(),
    };

    let audience: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM campaign_recipients WHERE campaign_id = $1")
        .bind(campaign.id)
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);

    let sent: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM campaign_recipients WHERE campaign_id = $1 AND source_id IS NOT NULL")
        .bind(campaign.id)
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);

    let delivered: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM campaign_recipients WHERE campaign_id = $1 AND status IN (2, 3)")
        .bind(campaign.id)
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);

    let read: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM campaign_recipients WHERE campaign_id = $1 AND status = 3")
        .bind(campaign.id)
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);

    let failed: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM campaign_recipients WHERE campaign_id = $1 AND status = 4")
        .bind(campaign.id)
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);

    let skipped: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM campaign_recipients WHERE campaign_id = $1 AND status = 5")
        .bind(campaign.id)
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);

    (
        StatusCode::OK,
        Json(json!({
            "audience": audience,
            "sent": sent,
            "delivered": delivered,
            "read": read,
            "failed": failed,
            "skipped": skipped,
            "status_counts": {
                "in_progress": 0,
                "sent": sent,
                "delivered": delivered,
                "read": read,
                "failed": failed,
                "skipped": skipped
            }
        })),
    ).into_response()
}

/// GET /api/v1/accounts/:account_id/campaigns/:id/analytics/contacts
pub async fn get_campaign_contacts(
    Path((account_id, id_param)): Path<(i64, String)>,
    axum::extract::Query(query): axum::extract::Query<std::collections::HashMap<String, String>>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let parsed_id = id_param.parse::<i64>().unwrap_or(0);
    let campaign = match sqlx::query_as::<_, Campaign>(
        "SELECT * FROM campaigns WHERE account_id = $1 AND (display_id = $2 OR id = $3)",
    )
    .bind(account_id)
    .bind(parsed_id as i32)
    .bind(parsed_id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(c)) => c,
        _ => return (StatusCode::NOT_FOUND, Json(json!({ "error": "Campaign not found" }))).into_response(),
    };

    let page: i64 = query.get("page").and_then(|p| p.parse().ok()).unwrap_or(1).max(1);
    let offset = (page - 1) * 25;

    let total_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM campaign_recipients WHERE campaign_id = $1")
        .bind(campaign.id)
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);

    let rows = sqlx::query!(
        r#"
        SELECT cr.id, cr.status, cr.message_content, cr.error_code, cr.error_title, cr.error_message,
               c.id as contact_id, c.name as contact_name, c.phone_number as contact_phone
        FROM campaign_recipients cr
        JOIN contacts c ON c.id = cr.contact_id
        WHERE cr.campaign_id = $1
        ORDER BY cr.created_at DESC
        LIMIT 25 OFFSET $2
        "#,
        campaign.id,
        offset
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let payload: Vec<_> = rows.into_iter().map(|r| {
        json!({
            "contact": {
                "id": r.contact_id,
                "name": r.contact_name,
                "phone_number": r.contact_phone
            },
            "status": r.status,
            "message_content": r.message_content,
            "error_code": r.error_code,
            "error_title": r.error_title,
            "error_message": r.error_message
        })
    }).collect();

    (
        StatusCode::OK,
        Json(json!({
            "payload": payload,
            "meta": {
                "current_page": page,
                "total_pages": (total_count + 24) / 25,
                "total_count": total_count
            }
        })),
    ).into_response()
}

/// GET /api/v1/accounts/:account_id/conversations/:display_id/campaign_history
pub async fn get_conversation_campaign_history(
    Path((account_id, display_id)): Path<(i32, i32)>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let conv = match sqlx::query_as::<_, crate::models::conversation::Conversation>(
        "SELECT * FROM conversations WHERE account_id = $1 AND display_id = $2",
    )
    .bind(account_id)
    .bind(display_id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(c)) => c,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Conversation not found" })),
            )
                .into_response();
        }
    };

    let rows = sqlx::query!(
        r#"
        SELECT cr.id, cr.source_id, cr.status, cr.message_content,
               cr.sent_at, cr.delivered_at, cr.read_at, cr.failed_at,
               c.display_id AS campaign_display_id, c.title AS campaign_title
        FROM campaign_recipients cr
        JOIN campaigns c ON c.id = cr.campaign_id
        WHERE cr.account_id = $1 AND cr.contact_id = $2 AND cr.inbox_id = $3
          AND cr.sent_at IS NOT NULL
        ORDER BY cr.sent_at DESC, cr.id DESC
        LIMIT 25
        "#,
        account_id as i64,
        conv.contact_id.unwrap_or(0) as i64,
        conv.inbox_id as i64
    )

    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let payload: Vec<_> = rows
        .into_iter()
        .map(|r| {
            json!({
                "id": r.id,
                "campaign": {
                    "id": r.campaign_display_id,
                    "title": r.campaign_title
                },
                "message_content": r.message_content,
                "source_id": r.source_id,
                "status": r.status,
                "sent_at": r.sent_at.map(|t| t.and_utc().timestamp()),
                "delivered_at": r.delivered_at.map(|t| t.and_utc().timestamp()),
                "read_at": r.read_at.map(|t| t.and_utc().timestamp()),
                "failed_at": r.failed_at.map(|t| t.and_utc().timestamp())
            })
        })
        .collect();

    (
        StatusCode::OK,
        Json(json!({
            "payload": payload,
            "meta": {
                "next_before": null,
                "first_message_id": 1
            }
        })),
    )
        .into_response()
}

