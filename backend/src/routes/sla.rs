use axum::{
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::IntoResponse,
    Json,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::json;
use sqlx::Row;

use crate::models::conversation::Conversation;
use crate::models::sla::{AppliedSla, SlaPolicy};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct CreateSlaPayload {
    pub name: String,
    pub description: Option<String>,
    pub first_response_time_threshold: Option<f64>,
    pub resolution_time_threshold: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct ApplySlaPayload {
    pub sla_policy_id: i64,
}

#[derive(Debug, Deserialize, Default)]
pub struct SlaFilterQuery {
    pub page: Option<i64>,
    pub since: Option<String>,
    pub until: Option<String>,
    pub inbox_id: Option<i64>,
    pub team_id: Option<i64>,
    pub assigned_agent_id: Option<i64>,
    pub sla_policy_id: Option<i64>,
    pub label_list: Option<String>,
}

fn sla_status_to_str(status: i32) -> &'static str {
    match status {
        0 => "active",
        1 => "hit",
        2 => "missed",
        3 => "active_with_misses",
        _ => "active",
    }
}

/// GET /api/v1/accounts/:account_id/sla_policies
pub async fn list_sla_policies(
    Path(account_id): Path<i32>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let policies = sqlx::query_as::<_, SlaPolicy>(
        "SELECT * FROM sla_policies WHERE account_id = $1 ORDER BY id DESC",
    )
    .bind(account_id as i64)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    (StatusCode::OK, Json(json!({ "payload": policies })))
}

/// POST /api/v1/accounts/:account_id/sla_policies
pub async fn create_sla_policy(
    Path(account_id): Path<i32>,
    State(state): State<AppState>,
    Json(payload): Json<CreateSlaPayload>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        crate::services::tier::verify_feature_access(&state.db, account_id as i64, "sla").await
    {
        return (
            code,
            Json(json!({ "error": msg, "error_code": "TIER_FEATURE_RESTRICTED" })),
        );
    }

    let policy = match sqlx::query_as::<_, SlaPolicy>(
        r#"
        INSERT INTO sla_policies (
            name, description, first_response_time_threshold,
            resolution_time_threshold, account_id, created_at, updated_at
        )
        VALUES ($1, $2, $3, $4, $5, NOW(), NOW())
        RETURNING *
        "#,
    )
    .bind(&payload.name)
    .bind(&payload.description)
    .bind(payload.first_response_time_threshold)
    .bind(payload.resolution_time_threshold)
    .bind(account_id as i64)
    .fetch_one(&state.db)
    .await
    {
        Ok(p) => p,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Failed to create SLA policy: {}", e) })),
            );
        }
    };

    (StatusCode::CREATED, Json(json!(policy)))
}

/// POST /api/v1/accounts/:account_id/conversations/:display_id/apply_sla
pub async fn apply_sla_to_conversation(
    Path((account_id, display_id)): Path<(i32, i32)>,
    State(state): State<AppState>,
    Json(payload): Json<ApplySlaPayload>,
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

    let applied = match sqlx::query_as::<_, AppliedSla>(
        r#"
        INSERT INTO applied_slas (
            account_id, sla_policy_id, conversation_id, sla_status, created_at, updated_at
        )
        VALUES ($1, $2, $3, 0, NOW(), NOW())
        ON CONFLICT (account_id, sla_policy_id, conversation_id) DO UPDATE
        SET sla_status = 0, updated_at = NOW()
        RETURNING *
        "#,
    )
    .bind(account_id as i64)
    .bind(payload.sla_policy_id)
    .bind(conv.id as i64)
    .fetch_one(&state.db)
    .await
    {
        Ok(a) => a,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Failed to apply SLA: {}", e) })),
            );
        }
    };

    (StatusCode::OK, Json(json!(applied)))
}

/// GET /api/v1/accounts/:account_id/applied_slas
pub async fn list_applied_slas(
    Path(account_id): Path<i64>,
    Query(query): Query<SlaFilterQuery>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let page = query.page.unwrap_or(1).max(1);
    let per_page: i64 = 25;
    let offset = (page - 1) * per_page;

    // Hitung total missed SLA
    let total_count: i64 = sqlx::query_scalar!(
        r#"
        SELECT COUNT(*)
        FROM applied_slas a
        JOIN conversations c ON c.id = a.conversation_id
        WHERE a.account_id = $1
          AND a.sla_status IN (2, 3)
          AND ($2::bigint IS NULL OR a.sla_policy_id = $2)
          AND ($3::bigint IS NULL OR c.inbox_id = $3)
          AND ($4::bigint IS NULL OR c.team_id = $4)
          AND ($5::bigint IS NULL OR c.assignee_id = $5)
        "#,
        account_id,
        query.sla_policy_id,
        query.inbox_id,
        query.team_id,
        query.assigned_agent_id
    )
    .fetch_one(&state.db)
    .await
    .unwrap_or(Some(0))
    .unwrap_or(0);

    // Ambil record detail dengan JOIN ke conversations, sla_policies, contacts, dan users
    let rows = match sqlx::query(
        r#"
        SELECT 
            a.id AS applied_id,
            a.sla_policy_id,
            a.sla_status,
            a.created_at AS applied_created_at,
            a.updated_at AS applied_updated_at,
            a.completed_at AS applied_completed_at,
            p.name AS sla_name,
            p.description AS sla_description,
            p.first_response_time_threshold,
            p.next_response_time_threshold,
            p.only_during_business_hours,
            p.resolution_time_threshold,
            c.display_id,
            c.cached_label_list,
            ct.name AS contact_name,
            u.id AS assignee_id,
            u.name AS assignee_name,
            u.email AS assignee_email
        FROM applied_slas a
        JOIN conversations c ON c.id = a.conversation_id
        JOIN sla_policies p ON p.id = a.sla_policy_id
        LEFT JOIN contacts ct ON ct.id = c.contact_id
        LEFT JOIN users u ON u.id = c.assignee_id
        WHERE a.account_id = $1
          AND a.sla_status IN (2, 3)
          AND ($2::bigint IS NULL OR a.sla_policy_id = $2)
          AND ($3::bigint IS NULL OR c.inbox_id = $3)
          AND ($4::bigint IS NULL OR c.team_id = $4)
          AND ($5::bigint IS NULL OR c.assignee_id = $5)
        ORDER BY a.created_at DESC
        LIMIT $6 OFFSET $7
        "#,
    )
    .bind(account_id)
    .bind(query.sla_policy_id)
    .bind(query.inbox_id)
    .bind(query.team_id)
    .bind(query.assigned_agent_id)
    .bind(per_page)
    .bind(offset)
    .fetch_all(&state.db)
    .await
    {
        Ok(r) => r,
        Err(_) => vec![],
    };

    let mut payload = Vec::new();
    for row in rows {
        let applied_id: i64 = row.get("applied_id");
        let sla_policy_id: i64 = row.get("sla_policy_id");
        let status_num: i32 = row.get("sla_status");
        let created_at: chrono::NaiveDateTime = row.get("applied_created_at");
        let updated_at: chrono::NaiveDateTime = row.get("applied_updated_at");
        let completed_at: Option<chrono::NaiveDateTime> = row.get("applied_completed_at");
        let sla_name: String = row.get("sla_name");
        let sla_description: Option<String> = row.get("sla_description");
        let frt: Option<f64> = row.get("first_response_time_threshold");
        let nrt: Option<f64> = row.get("next_response_time_threshold");
        let odbh: bool = row.get("only_during_business_hours");
        let rt: Option<f64> = row.get("resolution_time_threshold");

        let display_id: i32 = row.get("display_id");
        let label_str: Option<String> = row.get("cached_label_list");
        let contact_name: Option<String> = row.get("contact_name");
        let assignee_id: Option<i32> = row.get("assignee_id");
        let assignee_name: Option<String> = row.get("assignee_name");
        let assignee_email: Option<String> = row.get("assignee_email");

        let labels: Vec<String> = label_str
            .map(|s| s.split(',').map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect())
            .unwrap_or_default();

        let assignee = assignee_id.map(|aid| {
            json!({
                "id": aid,
                "name": assignee_name,
                "email": assignee_email
            })
        });

        payload.push(json!({
            "applied_sla": {
                "id": applied_id,
                "sla_id": sla_policy_id,
                "sla_status": sla_status_to_str(status_num),
                "created_at": created_at.and_utc().timestamp(),
                "updated_at": updated_at.and_utc().timestamp(),
                "sla_completed_at": completed_at.map(|t| t.and_utc().timestamp()),
                "sla_name": sla_name,
                "sla_description": sla_description,
                "sla_first_response_time_threshold": frt,
                "sla_next_response_time_threshold": nrt,
                "sla_only_during_business_hours": odbh,
                "sla_resolution_time_threshold": rt
            },
            "conversation": {
                "id": display_id,
                "contact": {
                    "name": contact_name
                },
                "labels": labels,
                "assignee": assignee
            },
            "sla_events": []
        }));
    }

    (
        StatusCode::OK,
        Json(json!({
            "payload": payload,
            "meta": {
                "count": total_count,
                "current_page": page
            }
        })),
    )
}

/// GET /api/v1/accounts/:account_id/applied_slas/metrics
pub async fn get_applied_sla_metrics(
    Path(account_id): Path<i64>,
    Query(query): Query<SlaFilterQuery>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let total_applied: i64 = sqlx::query_scalar!(
        r#"
        SELECT COUNT(*)
        FROM applied_slas a
        JOIN conversations c ON c.id = a.conversation_id
        WHERE a.account_id = $1
          AND ($2::bigint IS NULL OR a.sla_policy_id = $2)
          AND ($3::bigint IS NULL OR c.inbox_id = $3)
          AND ($4::bigint IS NULL OR c.team_id = $4)
          AND ($5::bigint IS NULL OR c.assignee_id = $5)
        "#,
        account_id,
        query.sla_policy_id,
        query.inbox_id,
        query.team_id,
        query.assigned_agent_id
    )
    .fetch_one(&state.db)
    .await
    .unwrap_or(Some(0))
    .unwrap_or(0);

    let number_of_misses: i64 = sqlx::query_scalar!(
        r#"
        SELECT COUNT(*)
        FROM applied_slas a
        JOIN conversations c ON c.id = a.conversation_id
        WHERE a.account_id = $1
          AND a.sla_status IN (2, 3)
          AND ($2::bigint IS NULL OR a.sla_policy_id = $2)
          AND ($3::bigint IS NULL OR c.inbox_id = $3)
          AND ($4::bigint IS NULL OR c.team_id = $4)
          AND ($5::bigint IS NULL OR c.assignee_id = $5)
        "#,
        account_id,
        query.sla_policy_id,
        query.inbox_id,
        query.team_id,
        query.assigned_agent_id
    )
    .fetch_one(&state.db)
    .await
    .unwrap_or(Some(0))
    .unwrap_or(0);

    let hit_rate = if total_applied == 0 || number_of_misses == 0 {
        "100%".to_string()
    } else {
        let rate = ((total_applied - number_of_misses) as f64 / total_applied as f64) * 100.0;
        format!("{:.1}%", rate)
    };

    (
        StatusCode::OK,
        Json(json!({
            "total_applied_slas": total_applied,
            "number_of_sla_misses": number_of_misses,
            "hit_rate": hit_rate
        })),
    )
}

/// GET /api/v1/accounts/:account_id/applied_slas/download
pub async fn download_applied_slas_csv(
    Path(account_id): Path<i64>,
    Query(query): Query<SlaFilterQuery>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let rows = sqlx::query(
        r#"
        SELECT 
            c.display_id,
            ct.name AS contact_name,
            u.name AS assignee_name,
            p.name AS sla_name,
            a.sla_status,
            a.created_at
        FROM applied_slas a
        JOIN conversations c ON c.id = a.conversation_id
        JOIN sla_policies p ON p.id = a.sla_policy_id
        LEFT JOIN contacts ct ON ct.id = c.contact_id
        LEFT JOIN users u ON u.id = c.assignee_id
        WHERE a.account_id = $1
          AND a.sla_status IN (2, 3)
          AND ($2::bigint IS NULL OR a.sla_policy_id = $2)
          AND ($3::bigint IS NULL OR c.inbox_id = $3)
          AND ($4::bigint IS NULL OR c.team_id = $4)
          AND ($5::bigint IS NULL OR c.assignee_id = $5)
        ORDER BY a.created_at DESC
        LIMIT 5000
        "#,
    )
    .bind(account_id)
    .bind(query.sla_policy_id)
    .bind(query.inbox_id)
    .bind(query.team_id)
    .bind(query.assigned_agent_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let mut csv_out = String::from("Conversation ID,Contact Name,Assignee,SLA Policy,Status,Created At\n");
    for row in rows {
        let display_id: i32 = row.get("display_id");
        let contact_name: Option<String> = row.get("contact_name");
        let assignee_name: Option<String> = row.get("assignee_name");
        let sla_name: String = row.get("sla_name");
        let status: i32 = row.get("sla_status");
        let created_at: chrono::NaiveDateTime = row.get("created_at");

        let clean_contact = contact_name.unwrap_or_default().replace(',', " ");
        let clean_assignee = assignee_name.unwrap_or_default().replace(',', " ");
        let clean_sla = sla_name.replace(',', " ");

        csv_out.push_str(&format!(
            "{},\"{}\",\"{}\",\"{}\",{},{}\n",
            display_id, clean_contact, clean_assignee, clean_sla, sla_status_to_str(status), created_at
        ));
    }

    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "text/csv"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=breached_conversation.csv",
            ),
        ],
        csv_out,
    )
}
