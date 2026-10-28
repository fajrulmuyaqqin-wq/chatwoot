use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
    Json,
};
use serde_json::json;
use sqlx::PgPool;

use crate::middleware::auth::CurrentUser;
use crate::state::AppState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountRole {
    Agent = 0,
    Administrator = 1,
}

/// Memvalidasi hak akses user terhadap account_id sesuai role minimum yang dibutuhkan (Pundit-style check)
pub async fn verify_account_access(
    db: &PgPool,
    current_user: &CurrentUser,
    account_id: i32,
    required_role: AccountRole,
) -> Result<i32, (StatusCode, &'static str)> {
    // SuperAdmin selalu memiliki akses bypass penuh
    if current_user.is_super_admin() {
        return Ok(1);
    }

    let role_opt = sqlx::query_scalar::<_, Option<i32>>(
        r#"
        SELECT role FROM account_users
        WHERE account_id = $1 AND user_id = $2
        "#,
    )
    .bind(account_id as i64)
    .bind(current_user.id as i64)
    .fetch_optional(db)
    .await
    .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Database query error during authorization"))?;

    let role = match role_opt {
        Some(Some(r)) => r,
        _ => {
            return Err((
                StatusCode::FORBIDDEN,
                "You do not have access to this account",
            ));
        }
    };

    if required_role == AccountRole::Administrator && role != 1 {
        return Err((
            StatusCode::FORBIDDEN,
            "You are not authorized to perform this action. Administrator role required.",
        ));
    }

    Ok(role)
}

/// Memvalidasi apakah user adalah SuperAdmin
pub fn require_super_admin(current_user: &CurrentUser) -> Result<(), (StatusCode, &'static str)> {
    if current_user.is_super_admin() {
        Ok(())
    } else {
        Err((
            StatusCode::FORBIDDEN,
            "Access denied: SuperAdmin role required",
        ))
    }
}

/// Ekstrak account_id dari path segmen URL (misal: /api/v1/accounts/25/...)
pub fn extract_account_id_from_path(path: &str) -> Option<i32> {
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    for i in 0..segments.len() {
        if segments[i] == "accounts" && i + 1 < segments.len() {
            if let Ok(id) = segments[i + 1].parse::<i32>() {
                return Some(id);
            }
        }
    }
    None
}

/// Middleware otomatis untuk memvalidasi hak akses tenant (BOLA / IDOR protection)
pub async fn account_access_middleware(
    State(state): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    if let Some(account_id) = extract_account_id_from_path(req.uri().path()) {
        let current_user = match req.extensions().get::<CurrentUser>() {
            Some(u) => u.clone(),
            None => {
                return Err((
                    StatusCode::UNAUTHORIZED,
                    Json(json!({ "error": "Unauthorized: authentication required" })),
                ));
            }
        };

        // Verifikasi keanggotaan user pada account_id (minimal Agent)
        if let Err((code, msg)) =
            verify_account_access(&state.db, &current_user, account_id, AccountRole::Agent).await
        {
            return Err((code, Json(json!({ "error": msg }))));
        }
    }

    Ok(next.run(req).await)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_account_id_from_path() {
        assert_eq!(
            extract_account_id_from_path("/api/v1/accounts/25/conversations"),
            Some(25)
        );
        assert_eq!(
            extract_account_id_from_path("/api/v1/accounts/999/"),
            Some(999)
        );
        assert_eq!(
            extract_account_id_from_path("/api/v1/accounts/42"),
            Some(42)
        );
        assert_eq!(
            extract_account_id_from_path("/api/v2/accounts/123/reports"),
            Some(123)
        );
        assert_eq!(
            extract_account_id_from_path("/enterprise/api/v1/accounts/7/limits"),
            Some(7)
        );
        assert_eq!(extract_account_id_from_path("/api/v1/accounts"), None);
        assert_eq!(extract_account_id_from_path("/api/v1/profile"), None);
        assert_eq!(extract_account_id_from_path("/auth/sign_in"), None);
    }
}
