use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::FromRow;

use crate::models::conversation::Conversation;
use crate::state::AppState;

#[derive(Debug, Deserialize, Default)]
pub struct ReportingEventsFilterQuery {
    pub page: Option<i64>,
    pub since: Option<String>,
    pub until: Option<String>,
    pub inbox_id: Option<i64>,
    pub user_id: Option<i64>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ReportingEvent {
    pub id: i32,
    pub name: Option<String>,
    pub value: Option<f64>,
    pub value_in_business_hours: Option<f64>,
    pub event_start_time: Option<chrono::NaiveDateTime>,
    pub event_end_time: Option<chrono::NaiveDateTime>,
    pub account_id: Option<i32>,
    pub inbox_id: Option<i32>,
    pub user_id: Option<i32>,
    pub conversation_id: Option<i32>,
    pub created_at: chrono::NaiveDateTime,
    pub updated_at: chrono::NaiveDateTime,
}

fn event_to_json(e: &ReportingEvent) -> serde_json::Value {
    json!({
        "id": e.id,
        "name": e.name,
        "value": e.value,
        "value_in_business_hours": e.value_in_business_hours,
        "event_start_time": e.event_start_time.map(|t| t.and_utc().to_rfc3339()),
        "event_end_time": e.event_end_time.map(|t| t.and_utc().to_rfc3339()),
        "account_id": e.account_id,
        "inbox_id": e.inbox_id,
        "user_id": e.user_id,
        "conversation_id": e.conversation_id,
        "created_at": e.created_at.and_utc().to_rfc3339(),
        "updated_at": e.updated_at.and_utc().to_rfc3339(),
    })
}

/// GET /api/v1/accounts/:account_id/reporting_events
pub async fn list_reporting_events(
    Path(account_id): Path<i64>,
    Query(query): Query<ReportingEventsFilterQuery>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let page = query.page.unwrap_or(1).max(1);
    let per_page: i64 = 25;
    let offset = (page - 1) * per_page;

    let total_count: i64 = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*)
        FROM reporting_events
        WHERE account_id = $1
          AND ($2::text IS NULL OR name = $2)
          AND ($3::int IS NULL OR inbox_id = $3)
          AND ($4::int IS NULL OR user_id = $4)
        "#,
    )
    .bind(account_id as i32)
    .bind(&query.name)
    .bind(query.inbox_id.map(|i| i as i32))
    .bind(query.user_id.map(|u| u as i32))
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    let events = sqlx::query_as::<_, ReportingEvent>(
        r#"
        SELECT id, name, value, value_in_business_hours, event_start_time, event_end_time,
               account_id, inbox_id, user_id, conversation_id, created_at, updated_at
        FROM reporting_events
        WHERE account_id = $1
          AND ($2::text IS NULL OR name = $2)
          AND ($3::int IS NULL OR inbox_id = $3)
          AND ($4::int IS NULL OR user_id = $4)
        ORDER BY created_at DESC
        LIMIT $5 OFFSET $6
        "#,
    )
    .bind(account_id as i32)
    .bind(&query.name)
    .bind(query.inbox_id.map(|i| i as i32))
    .bind(query.user_id.map(|u| u as i32))
    .bind(per_page)
    .bind(offset)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let payload: Vec<serde_json::Value> = events.iter().map(event_to_json).collect();
    let total_pages = (total_count as f64 / per_page as f64).ceil() as i64;

    (
        StatusCode::OK,
        Json(json!({
            "payload": payload,
            "meta": {
                "count": total_count,
                "current_page": page,
                "total_pages": total_pages.max(1)
            }
        })),
    )
}

/// GET /api/v1/accounts/:account_id/conversations/:display_id/reporting_events
pub async fn list_conversation_reporting_events(
    Path((account_id, display_id)): Path<(i32, i32)>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let conv = match sqlx::query_as::<_, Conversation>(
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
            );
        }
    };

    let events = sqlx::query_as::<_, ReportingEvent>(
        r#"
        SELECT id, name, value, value_in_business_hours, event_start_time, event_end_time,
               account_id, inbox_id, user_id, conversation_id, created_at, updated_at
        FROM reporting_events
        WHERE account_id = $1 AND conversation_id = $2
        ORDER BY created_at ASC
        "#,
    )
    .bind(account_id)
    .bind(conv.id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let payload: Vec<serde_json::Value> = events.iter().map(event_to_json).collect();
    (StatusCode::OK, Json(json!({ "payload": payload })))
}
