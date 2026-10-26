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

    // Dapatkan current user jika ada dari extensions
    let user_opt = req.extensions().get::<CurrentUser>().cloned();

    // Jalankan request handler
    let res = next.run(req).await;

    // Jika mutasi dan status respons sukses (2xx), catat audit log secara asinkron
    if is_mutating && res.status().is_success() {
        if let Some(user) = user_opt {
            let db = state.db.clone();
            let action_str = format!("{} {}", method, uri.path());
            let path_str = uri.path().to_string();

            // Tentukan resource type dari path
            let auditable_type = if path_str.contains("/messages") {
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
            } else {
                "AccountResource"
            };

            tokio::spawn(async move {
                let res = sqlx::query(
                    r#"
                    INSERT INTO audits (
                        user_id, username, user_type, action, auditable_type,
                        created_at
                    )
                    VALUES ($1, $2, 'User', $3, $4, NOW())
                    "#,
                )
                .bind(user.id as i64)
                .bind(&user.email)
                .bind(&action_str)
                .bind(auditable_type)
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
