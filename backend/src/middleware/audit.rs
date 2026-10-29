use axum::{
    body::Body,
    extract::State,
    http::{Method, Request},
    middleware::Next,
    response::Response,
};
use tracing::warn;

use crate::middleware::auth::CurrentUser;
use crate::state::AppState;

pub async fn audit_logger(
    State(state): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let method = req.method().clone();
    let uri = req.uri().clone();
    let is_mutating = matches!(
        method,
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    );

    let remote_address = req
        .headers()
        .get("x-forwarded-for")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim().to_string())
        .or_else(|| {
            req.headers()
                .get("x-real-ip")
                .and_then(|h| h.to_str().ok())
                .map(|s| s.trim().to_string())
        });

    // Dapatkan current user jika ada dari extensions
    let user_opt = req.extensions().get::<CurrentUser>().cloned();

    // Jalankan request handler
    let res = next.run(req).await;

    // Jika mutasi dan status respons sukses (2xx), catat audit log secara asinkron
    if is_mutating && res.status().is_success() {
        if let Some(user) = user_opt {
            let db = state.db.clone();
            let path_str = uri.path().to_string();

            let account_id = crate::middleware::rbac::extract_account_id_from_path(&path_str);
            let action_verb = match method {
                Method::POST => "create",
                Method::PATCH | Method::PUT => "update",
                Method::DELETE => "destroy",
                _ => "custom",
            };

            // Tentukan resource type dari path
            let auditable_type = if path_str.contains("/custom_roles") {
                "CustomRole"
            } else if path_str.contains("/messages") {
                "Message"
            } else if path_str.contains("/conversations") {
                "Conversation"
            } else if path_str.contains("/contacts") {
                "Contact"
            } else if path_str.contains("/inboxes") {
                "Inbox"
            } else if path_str.contains("/articles") {
                "Article"
            } else if path_str.contains("/sla_policies") {
                "SlaPolicy"
            } else if path_str.contains("/teams") {
                "Team"
            } else if path_str.contains("/agents") {
                "AccountUser"
            } else if path_str.contains("/webhooks") {
                "Webhook"
            } else if path_str.contains("/automation_rules") {
                "AutomationRule"
            } else if path_str.contains("/macros") {
                "Macro"
            } else {
                "Account"
            };

            let comment_str = format!("{} {}", method, uri.path());

            tokio::spawn(async move {
                let res = sqlx::query(
                    r#"
                    INSERT INTO audits (
                        user_id, username, user_type, action, auditable_type,
                        associated_id, associated_type, remote_address, comment,
                        created_at
                    )
                    VALUES ($1, $2, 'User', $3, $4, $5, 'Account', $6, $7, NOW())
                    "#,
                )
                .bind(user.id as i64)
                .bind(&user.email)
                .bind(action_verb)
                .bind(auditable_type)
                .bind(account_id.map(|id| id as i64))
                .bind(remote_address)
                .bind(&comment_str)
                .execute(&db)
                .await;

                if let Err(e) = res {
                    warn!("Failed to record audit log: {}", e);
                }
            });
        }
    }

    res
}
