use axum::{
    extract::{Extension, Multipart, Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::{error, info};

use crate::middleware::auth::CurrentUser;
use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportContactItem {
    pub name: Option<String>,
    pub email: Option<String>,
    pub phone_number: Option<String>,
    pub identifier: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BatchContactsPayload {
    pub contacts: Vec<ImportContactItem>,
}

pub async fn import_contacts_json(
    Path(account_id): Path<i32>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    Json(payload): Json<BatchContactsPayload>,
) -> impl IntoResponse {
    if let Err((code, msg)) = crate::middleware::rbac::verify_permission(
        &state.db,
        &current_user,
        account_id,
        "contact_manage",
    )
    .await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let mut imported = 0;
    let mut errors = 0;

    for contact in payload.contacts {
        let name = contact.name.unwrap_or_else(|| "Imported Contact".to_string());
        let email = contact.email.filter(|e| !e.trim().is_empty());
        let phone = contact.phone_number.filter(|p| !p.trim().is_empty());
        let identifier = contact.identifier.filter(|i| !i.trim().is_empty());

        if email.is_none() && phone.is_none() && identifier.is_none() {
            errors += 1;
            continue;
        }

        let existing_id = if let Some(ref em) = email {
            sqlx::query_scalar::<_, i32>(
                "SELECT id FROM contacts WHERE account_id = $1 AND LOWER(email) = LOWER($2) LIMIT 1"
            )
            .bind(account_id)
            .bind(em)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten()
        } else {
            None
        };

        let res = match existing_id {
            Some(cid) => {
                sqlx::query(
                    r#"
                    UPDATE contacts
                    SET name = $1, phone_number = COALESCE($2, phone_number), identifier = COALESCE($3, identifier), updated_at = NOW()
                    WHERE id = $4
                    "#
                )
                .bind(&name)
                .bind(&phone)
                .bind(&identifier)
                .bind(cid)
                .execute(&state.db)
                .await
            }
            None => {
                sqlx::query(
                    r#"
                    INSERT INTO contacts (account_id, name, email, phone_number, identifier, created_at, updated_at)
                    VALUES ($1, $2, $3, $4, $5, NOW(), NOW())
                    "#
                )
                .bind(account_id)
                .bind(&name)
                .bind(&email)
                .bind(&phone)
                .bind(&identifier)
                .execute(&state.db)
                .await
            }
        };

        match res {
            Ok(_) => imported += 1,
            Err(e) => {
                error!("Gagal import baris contact: {:?}", e);
                errors += 1;
            }
        }
    }

    info!(
        account_id = account_id,
        imported = imported,
        errors = errors,
        "Selesai import batch contacts via JSON"
    );

    (
        StatusCode::OK,
        Json(json!({
            "success": true,
            "imported_count": imported,
            "errors_count": errors
        })),
    )
}

pub async fn import_contacts_csv(
    Path(account_id): Path<i32>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    if let Err((code, msg)) = crate::middleware::rbac::verify_permission(
        &state.db,
        &current_user,
        account_id,
        "contact_manage",
    )
    .await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let mut csv_content = String::new();

    while let Ok(Some(field)) = multipart.next_field().await {
        if let Ok(bytes) = field.bytes().await {
            if let Ok(text) = String::from_utf8(bytes.to_vec()) {
                csv_content = text;
                break;
            }
        }
    }

    if csv_content.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "CSV file is empty or missing" })),
        );
    }

    let mut imported = 0;
    let mut errors = 0;

    let lines: Vec<&str> = csv_content.lines().collect();
    if lines.is_empty() {
        return (
            StatusCode::OK,
            Json(json!({ "imported_count": 0, "errors_count": 0 })),
        );
    }

    let header_line = lines[0].to_lowercase();
    let headers: Vec<&str> = header_line.split(',').map(|s| s.trim()).collect();

    let name_idx = headers.iter().position(|&h| h == "name" || h == "nama");
    let email_idx = headers.iter().position(|&h| h == "email");
    let phone_idx = headers.iter().position(|&h| h == "phone" || h == "phone_number" || h == "telepon");
    let id_idx = headers.iter().position(|&h| h == "identifier" || h == "id");

    for line in &lines[1..] {
        if line.trim().is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.split(',').map(|s| s.trim()).collect();
        let name = name_idx.and_then(|i| parts.get(i).copied()).unwrap_or("Customer");
        let email = email_idx.and_then(|i| parts.get(i).copied()).filter(|s| !s.is_empty());
        let phone = phone_idx.and_then(|i| parts.get(i).copied()).filter(|s| !s.is_empty());
        let identifier = id_idx.and_then(|i| parts.get(i).copied()).filter(|s| !s.is_empty());

        if email.is_none() && phone.is_none() && identifier.is_none() {
            errors += 1;
            continue;
        }

        let existing_id = if let Some(em) = email {
            sqlx::query_scalar::<_, i32>(
                "SELECT id FROM contacts WHERE account_id = $1 AND LOWER(email) = LOWER($2) LIMIT 1"
            )
            .bind(account_id)
            .bind(em)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten()
        } else {
            None
        };

        let res = match existing_id {
            Some(cid) => {
                sqlx::query(
                    r#"
                    UPDATE contacts
                    SET name = $1, phone_number = COALESCE($2, phone_number), identifier = COALESCE($3, identifier), updated_at = NOW()
                    WHERE id = $4
                    "#
                )
                .bind(name)
                .bind(phone)
                .bind(identifier)
                .bind(cid)
                .execute(&state.db)
                .await
            }
            None => {
                sqlx::query(
                    r#"
                    INSERT INTO contacts (account_id, name, email, phone_number, identifier, created_at, updated_at)
                    VALUES ($1, $2, $3, $4, $5, NOW(), NOW())
                    "#
                )
                .bind(account_id)
                .bind(name)
                .bind(email)
                .bind(phone)
                .bind(identifier)
                .execute(&state.db)
                .await
            }
        };

        match res {
            Ok(_) => imported += 1,
            Err(_) => errors += 1,
        }
    }

    (
        StatusCode::OK,
        Json(json!({
            "success": true,
            "imported_count": imported,
            "errors_count": errors
        })),
    )
}
