use axum::http::StatusCode;
use sqlx::PgPool;

use crate::middleware::auth::CurrentUser;

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
