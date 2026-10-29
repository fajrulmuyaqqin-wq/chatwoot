use axum::{
    extract::{Extension, Path, Query, RawQuery, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::json;

use crate::middleware::auth::CurrentUser;
use crate::middleware::rbac::{verify_account_access, AccountRole};
use crate::models::audit::{Audit, AuditResponse};
use crate::state::AppState;

#[derive(Debug, Deserialize, Default)]
pub struct AuditQuery {
    pub page: Option<i64>,
    pub q: Option<String>,
    pub r#type: Option<String>,
    pub types: Option<Vec<String>>,
    pub sort: Option<String>,
    pub since: Option<i64>,
    pub until: Option<i64>,
}

fn build_auditable_types(raw_query: Option<&str>, query: &AuditQuery) -> Vec<String> {
    let mut types = Vec::new();
    if let Some(raw) = raw_query {
        for pair in raw.split('&') {
            if let Some((k, v)) = pair.split_once('=') {
                let key = k.replace("%5B%5D", "[]");
                if key == "types[]" || key == "types" || key == "type" {
                    let val = v.replace("%20", " ");
                    let trimmed = val.trim();
                    if !trimmed.is_empty() && !types.iter().any(|existing: &String| existing == trimmed) {
                        types.push(trimmed.to_string());
                    }
                }
            }
        }
    }
    if let Some(ref ts) = query.types {
        for t in ts {
            let trimmed = t.trim();
            if !trimmed.is_empty() && !types.iter().any(|existing: &String| existing == trimmed) {
                types.push(trimmed.to_string());
            }
        }
    }
    if let Some(ref t) = query.r#type {
        // Can be comma-separated or single
        for part in t.split(',') {
            let trimmed = part.trim();
            if !trimmed.is_empty() && !types.iter().any(|existing: &String| existing == trimmed) {
                types.push(trimmed.to_string());
            }
        }
    }
    types
}

/// GET /api/v1/accounts/:account_id/audit_logs
pub async fn list_audit_logs(
    Path(account_id): Path<i32>,
    Query(query): Query<AuditQuery>,
    RawQuery(raw_query): RawQuery,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    if let Err((code, msg)) =
        crate::services::tier::verify_feature_access(&state.db, account_id as i64, "audit_logs").await
    {
        return (
            code,
            Json(json!({ "error": msg, "error_code": "TIER_FEATURE_RESTRICTED" })),
        );
    }
    let page = query.page.unwrap_or(1).max(1);
    let per_page: i64 = 25;
    let offset = (page - 1) * per_page;

    let auditable_types = build_auditable_types(raw_query.as_deref(), &query);
    let search_q = query.q.as_deref().map(|s| s.trim()).filter(|s| !s.is_empty());

    // 1. Query audits with dynamic conditions
    let mut builder = sqlx::QueryBuilder::<sqlx::Postgres>::new(
        "SELECT a.id, a.auditable_id, a.auditable_type, a.associated_id, a.associated_type, \
         a.user_id, a.user_type, a.username, a.action, a.audited_changes, a.version, \
         a.comment, a.remote_address, a.request_uuid, a.created_at, a.city, a.country, a.country_code \
         FROM audits a \
         LEFT JOIN users u ON u.id = a.user_id AND a.user_type = 'User' \
         WHERE (a.associated_id = ",
    );
    builder.push_bind(account_id as i64);
    builder.push(" OR (a.auditable_type = 'Account' AND a.auditable_id = ");
    builder.push_bind(account_id as i64);
    builder.push(") OR a.associated_id IS NULL)");

    if !auditable_types.is_empty() {
        builder.push(" AND a.auditable_type = ANY(");
        builder.push_bind(&auditable_types);
        builder.push(")");
    }

    if let Some(q) = search_q {
        let pattern = format!("%{}%", q);
        builder.push(" AND (a.username ILIKE ");
        builder.push_bind(pattern.clone());
        builder.push(" OR u.name ILIKE ");
        builder.push_bind(pattern.clone());
        builder.push(" OR u.email ILIKE ");
        builder.push_bind(pattern);
        builder.push(")");
    }

    if let Some(since) = query.since {
        builder.push(" AND a.created_at >= to_timestamp(");
        builder.push_bind(since as f64);
        builder.push(")");
    }

    if let Some(until) = query.until {
        builder.push(" AND a.created_at <= to_timestamp(");
        builder.push_bind(until as f64);
        builder.push(")");
    }

    if query.sort.as_deref() == Some("asc") {
        builder.push(" ORDER BY a.created_at ASC, a.id ASC");
    } else {
        builder.push(" ORDER BY a.created_at DESC, a.id DESC");
    }

    builder.push(" LIMIT ");
    builder.push_bind(per_page);
    builder.push(" OFFSET ");
    builder.push_bind(offset);

    let audits: Vec<Audit> = match builder.build_query_as::<Audit>().fetch_all(&state.db).await {
        Ok(res) => res,
        Err(e) => {
            eprintln!("Error fetching audit logs: {:?}", e);
            Vec::new()
        }
    };

    // 2. Count query with same conditions
    let mut count_builder = sqlx::QueryBuilder::<sqlx::Postgres>::new(
        "SELECT COUNT(*) FROM audits a \
         LEFT JOIN users u ON u.id = a.user_id AND a.user_type = 'User' \
         WHERE (a.associated_id = ",
    );
    count_builder.push_bind(account_id as i64);
    count_builder.push(" OR (a.auditable_type = 'Account' AND a.auditable_id = ");
    count_builder.push_bind(account_id as i64);
    count_builder.push(") OR a.associated_id IS NULL)");

    if !auditable_types.is_empty() {
        count_builder.push(" AND a.auditable_type = ANY(");
        count_builder.push_bind(&auditable_types);
        count_builder.push(")");
    }

    if let Some(q) = search_q {
        let pattern = format!("%{}%", q);
        count_builder.push(" AND (a.username ILIKE ");
        count_builder.push_bind(pattern.clone());
        count_builder.push(" OR u.name ILIKE ");
        count_builder.push_bind(pattern.clone());
        count_builder.push(" OR u.email ILIKE ");
        count_builder.push_bind(pattern);
        count_builder.push(")");
    }

    if let Some(since) = query.since {
        count_builder.push(" AND a.created_at >= to_timestamp(");
        count_builder.push_bind(since as f64);
        count_builder.push(")");
    }

    if let Some(until) = query.until {
        count_builder.push(" AND a.created_at <= to_timestamp(");
        count_builder.push_bind(until as f64);
        count_builder.push(")");
    }

    let count: i64 = count_builder
        .build_query_scalar::<i64>()
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);

    let payload: Vec<AuditResponse> = audits.into_iter().map(AuditResponse::from).collect();

    (
        StatusCode::OK,
        Json(json!({
            "account_id": account_id,
            "per_page": per_page,
            "total_entries": count,
            "current_page": page,
            "audit_logs": payload,
            "meta": {
                "count": count,
                "current_page": page,
                "per_page": per_page
            },
            "payload": payload
        })),
    )
}

/// GET /api/v1/accounts/:account_id/audit_logs/download
pub async fn download_audit_logs_csv(
    Path(account_id): Path<i32>,
    Query(query): Query<AuditQuery>,
    RawQuery(raw_query): RawQuery,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> Response {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg }))).into_response();
    }

    if let Err((code, msg)) =
        crate::services::tier::verify_feature_access(&state.db, account_id as i64, "audit_logs").await
    {
        return (
            code,
            Json(json!({ "error": msg, "error_code": "TIER_FEATURE_RESTRICTED" })),
        )
            .into_response();
    }

    let auditable_types = build_auditable_types(raw_query.as_deref(), &query);
    let search_q = query.q.as_deref().map(|s| s.trim()).filter(|s| !s.is_empty());

    let mut builder = sqlx::QueryBuilder::<sqlx::Postgres>::new(
        "SELECT a.id, a.auditable_id, a.auditable_type, a.associated_id, a.associated_type, \
         a.user_id, a.user_type, a.username, a.action, a.audited_changes, a.version, \
         a.comment, a.remote_address, a.request_uuid, a.created_at, a.city, a.country, a.country_code \
         FROM audits a \
         LEFT JOIN users u ON u.id = a.user_id AND a.user_type = 'User' \
         WHERE (a.associated_id = ",
    );
    builder.push_bind(account_id as i64);
    builder.push(" OR (a.auditable_type = 'Account' AND a.auditable_id = ");
    builder.push_bind(account_id as i64);
    builder.push(") OR a.associated_id IS NULL)");

    if !auditable_types.is_empty() {
        builder.push(" AND a.auditable_type = ANY(");
        builder.push_bind(&auditable_types);
        builder.push(")");
    }

    if let Some(q) = search_q {
        let pattern = format!("%{}%", q);
        builder.push(" AND (a.username ILIKE ");
        builder.push_bind(pattern.clone());
        builder.push(" OR u.name ILIKE ");
        builder.push_bind(pattern.clone());
        builder.push(" OR u.email ILIKE ");
        builder.push_bind(pattern);
        builder.push(")");
    }

    if let Some(since) = query.since {
        builder.push(" AND a.created_at >= to_timestamp(");
        builder.push_bind(since as f64);
        builder.push(")");
    }

    if let Some(until) = query.until {
        builder.push(" AND a.created_at <= to_timestamp(");
        builder.push_bind(until as f64);
        builder.push(")");
    }

    if query.sort.as_deref() == Some("asc") {
        builder.push(" ORDER BY a.created_at ASC, a.id ASC");
    } else {
        builder.push(" ORDER BY a.created_at DESC, a.id DESC");
    }

    builder.push(" LIMIT 5000");

    let audits: Vec<Audit> = match builder.build_query_as::<Audit>().fetch_all(&state.db).await {
        Ok(res) => res,
        Err(e) => {
            eprintln!("Error fetching audit logs for export: {:?}", e);
            Vec::new()
        }
    };

    let responses: Vec<AuditResponse> = audits.into_iter().map(AuditResponse::from).collect();

    let mut csv = String::from("id,action,auditable_type,auditable_id,user_id,username,remote_address,location,created_at\n");
    for item in responses {
        let id = item.id;
        let action = escape_csv(&item.action);
        let auditable_type = escape_csv(item.auditable_type.as_deref().unwrap_or(""));
        let auditable_id = item.auditable_id.map(|v| v.to_string()).unwrap_or_default();
        let user_id = item.user_id.map(|v| v.to_string()).unwrap_or_default();
        let username = escape_csv(item.username.as_deref().unwrap_or(""));
        let remote_address = escape_csv(item.remote_address.as_deref().unwrap_or(""));
        let location = escape_csv(item.location.as_deref().unwrap_or(""));
        let created_at = chrono::DateTime::from_timestamp(item.created_at, 0)
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_default();

        csv.push_str(&format!(
            "{},{},{},{},{},{},{},{},{}\n",
            id, action, auditable_type, auditable_id, user_id, username, remote_address, location, created_at
        ));
    }

    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"audit_logs.csv\"",
            ),
        ],
        csv,
    )
        .into_response()
}

fn escape_csv(val: &str) -> String {
    if val.contains(',') || val.contains('"') || val.contains('\n') || val.contains('\r') {
        format!("\"{}\"", val.replace('"', "\"\""))
    } else {
        val.to_string()
    }
}
