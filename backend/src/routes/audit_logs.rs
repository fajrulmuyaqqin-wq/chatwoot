use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;

use crate::middleware::auth::CurrentUser;
use crate::middleware::rbac::{verify_account_access, AccountRole};
use crate::models::audit::{Audit, AuditResponse};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct AuditQuery {
    pub page: Option<i64>,
}

/// GET /api/v1/accounts/:account_id/audit_logs
pub async fn list_audit_logs(
    Path(account_id): Path<i32>,
    Query(query): Query<AuditQuery>,
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

    let audits = sqlx::query_as::<_, Audit>(
        r#"
        SELECT * FROM audits
        ORDER BY created_at DESC
        LIMIT $1 OFFSET $2
        "#,
    )
    .bind(per_page)
    .bind(offset)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let count: (i64,) = sqlx::query_as("SELECT count(*) FROM audits")
        .fetch_one(&state.db)
        .await
        .unwrap_or((0,));

    let payload: Vec<AuditResponse> = audits.into_iter().map(AuditResponse::from).collect();

    (
        StatusCode::OK,
        Json(json!({
            "account_id": account_id,
            "meta": {
                "count": count.0,
                "current_page": page,
                "per_page": per_page
            },
            "payload": payload
        })),
    )
}
