use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;

use crate::middleware::auth::CurrentUser;
use crate::models::contact::{Contact, ContactResponse};
use crate::state::{AppState, CableBroadcastMsg};

#[derive(Debug, Deserialize)]
pub struct ContactsQuery {
    pub page: Option<i64>,
    pub per_page: Option<i64>,
    pub q: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateContactPayload {
    pub name: Option<String>,
    pub email: Option<String>,
    pub phone_number: Option<String>,
    pub identifier: Option<String>,
    pub custom_attributes: Option<serde_json::Value>,
}

pub async fn list_contacts(
    Path(account_id): Path<i32>,
    Query(query): Query<ContactsQuery>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let page = query.page.unwrap_or(1).max(1);
    let per_page: i64 = query.per_page.unwrap_or(50);
    let offset = (page - 1) * per_page;

    let search_term = query.q.unwrap_or_default().trim().to_lowercase();

    let (contacts, total_count) = if search_term.is_empty() {
        let list = sqlx::query_as::<_, Contact>(
            r#"
            SELECT id, name, email, phone_number, account_id, identifier,
                   additional_attributes, custom_attributes, last_activity_at,
                   contact_type, middle_name, last_name, location, country_code,
                   blocked, company_id, created_at, updated_at
            FROM contacts
            WHERE account_id = $1
            ORDER BY created_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(account_id)
        .bind(per_page)
        .bind(offset)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();

        let count: (i64,) = sqlx::query_as(
            "SELECT count(*) FROM contacts WHERE account_id = $1"
        )
        .bind(account_id)
        .fetch_one(&state.db)
        .await
        .unwrap_or((0,));

        (list, count.0)
    } else {
        let pattern = format!("%{}%", search_term);
        let list = sqlx::query_as::<_, Contact>(
            r#"
            SELECT id, name, email, phone_number, account_id, identifier,
                   additional_attributes, custom_attributes, last_activity_at,
                   contact_type, middle_name, last_name, location, country_code,
                   blocked, company_id, created_at, updated_at
            FROM contacts
            WHERE account_id = $1 AND (
                LOWER(name) LIKE $2 OR
                LOWER(COALESCE(email, '')) LIKE $2 OR
                LOWER(COALESCE(phone_number, '')) LIKE $2
            )
            ORDER BY created_at DESC
            LIMIT $3 OFFSET $4
            "#,
        )
        .bind(account_id)
        .bind(&pattern)
        .bind(per_page)
        .bind(offset)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();

        let count: (i64,) = sqlx::query_as(
            r#"
            SELECT count(*) FROM contacts
            WHERE account_id = $1 AND (
                LOWER(name) LIKE $2 OR
                LOWER(COALESCE(email, '')) LIKE $2 OR
                LOWER(COALESCE(phone_number, '')) LIKE $2
            )
            "#,
        )
        .bind(account_id)
        .bind(&pattern)
        .fetch_one(&state.db)
        .await
        .unwrap_or((0,));

        (list, count.0)
    };

    let payload: Vec<ContactResponse> = contacts.into_iter().map(ContactResponse::from).collect();

    (
        StatusCode::OK,
        Json(json!({
            "meta": {
                "count": total_count,
                "current_page": page
            },
            "payload": payload
        })),
    )
}

pub async fn get_contact(
    Path((account_id, contact_id)): Path<(i32, i32)>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let contact = sqlx::query_as::<_, Contact>(
        "SELECT * FROM contacts WHERE id = $1 AND account_id = $2"
    )
    .bind(contact_id)
    .bind(account_id)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();

    match contact {
        Some(c) => (StatusCode::OK, Json(json!({ "payload": ContactResponse::from(c) }))),
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Contact not found" })),
        ),
    }
}

pub async fn create_contact(
    Path(account_id): Path<i32>,
    State(state): State<AppState>,
    Json(payload): Json<CreateContactPayload>,
) -> impl IntoResponse {
    let name = payload.name.unwrap_or_else(|| "Customer".to_string());
    let custom_attrs = payload.custom_attributes.unwrap_or_else(|| json!({}));

    let contact = sqlx::query_as::<_, Contact>(
        r#"
        INSERT INTO contacts (account_id, name, email, phone_number, identifier, custom_attributes, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, NOW(), NOW())
        RETURNING *
        "#
    )
    .bind(account_id)
    .bind(&name)
    .bind(&payload.email)
    .bind(&payload.phone_number)
    .bind(&payload.identifier)
    .bind(&custom_attrs)
    .fetch_one(&state.db)
    .await;

    match contact {
        Ok(c) => {
            let resp = ContactResponse::from(c);
            let _ = state.broadcast_tx.send(CableBroadcastMsg {
                account_id,
                event: "contact.created".to_string(),
                data: json!(resp),
            });
            (StatusCode::CREATED, Json(json!({ "payload": resp })))
        }
        Err(e) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": format!("Failed to create contact: {:?}", e) })),
        ),
    }
}

pub async fn update_contact(
    Path((account_id, contact_id)): Path<(i32, i32)>,
    State(state): State<AppState>,
    Json(payload): Json<CreateContactPayload>,
) -> impl IntoResponse {
    let current = match sqlx::query_as::<_, Contact>(
        "SELECT * FROM contacts WHERE id = $1 AND account_id = $2"
    )
    .bind(contact_id)
    .bind(account_id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(c)) => c,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Contact not found" })),
            )
        }
    };

    let name = payload.name.or(current.name).unwrap_or_else(|| "Customer".to_string());
    let email = payload.email.or(current.email);
    let phone = payload.phone_number.or(current.phone_number);
    let identifier = payload.identifier.or(current.identifier);
    let custom_attrs = payload.custom_attributes.or(current.custom_attributes).unwrap_or_else(|| json!({}));

    let updated = sqlx::query_as::<_, Contact>(
        r#"
        UPDATE contacts
        SET name = $1, email = $2, phone_number = $3, identifier = $4,
            custom_attributes = $5, updated_at = NOW()
        WHERE id = $6 AND account_id = $7
        RETURNING *
        "#
    )
    .bind(&name)
    .bind(&email)
    .bind(&phone)
    .bind(&identifier)
    .bind(&custom_attrs)
    .bind(contact_id)
    .bind(account_id)
    .fetch_one(&state.db)
    .await;

    match updated {
        Ok(c) => {
            let resp = ContactResponse::from(c);
            let _ = state.broadcast_tx.send(CableBroadcastMsg {
                account_id,
                event: "contact.updated".to_string(),
                data: json!(resp),
            });
            (StatusCode::OK, Json(json!({ "payload": resp })))
        }
        Err(e) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": format!("Failed to update contact: {:?}", e) })),
        ),
    }
}

pub async fn delete_contact(
    Path((account_id, contact_id)): Path<(i32, i32)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
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

    let res = sqlx::query(
        "DELETE FROM contacts WHERE id = $1 AND account_id = $2"
    )
    .bind(contact_id)
    .bind(account_id)
    .execute(&state.db)
    .await;

    match res {
        Ok(r) if r.rows_affected() > 0 => {
            let _ = state.broadcast_tx.send(CableBroadcastMsg {
                account_id,
                event: "contact.deleted".to_string(),
                data: json!({ "id": contact_id }),
            });
            (StatusCode::OK, Json(json!({ "success": true })))
        }
        _ => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Contact not found" })),
        ),
    }
}

/// POST /api/v1/accounts/:account_id/contacts/export
pub async fn export_contacts(
    Path(account_id): Path<i32>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
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

    (
        StatusCode::OK,
        Json(json!({
            "message": "Export contact queued successfully"
        })),
    )
}

