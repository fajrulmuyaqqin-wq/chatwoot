use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;

use crate::models::company::{
    AttachContactPayload, Company, CompanyContactListResponse, CompanyContactResponse,
    CompanyConversationListMeta, CompanyConversationListResponse,
    CompanyListMeta, CompanyListResponse, CompanyPayloadWrapper, CompanyResponse,
    DestroyCustomAttributesPayload,
};
use crate::models::contact::{Contact, ContactResponse};
use crate::models::conversation::Conversation;
use crate::models::message::{Message, MessageResponse};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct CompanyQuery {
    pub page: Option<i64>,
    pub sort: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CompanySearchQuery {
    pub q: Option<String>,
    pub page: Option<i64>,
    pub sort: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ContactFilterQuery {
    pub page: Option<i64>,
    pub q: Option<String>,
}

fn parse_company_sort(sort: Option<&str>) -> (&'static str, &'static str) {
    match sort {
        Some("-name") => ("name", "DESC"),
        Some("name") => ("name", "ASC"),
        Some("-domain") => ("domain", "DESC NULLS LAST"),
        Some("domain") => ("domain", "ASC NULLS LAST"),
        Some("-created_at") => ("created_at", "DESC"),
        Some("created_at") => ("created_at", "ASC"),
        Some("-last_activity_at") => ("last_activity_at", "DESC NULLS LAST"),
        Some("last_activity_at") => ("last_activity_at", "ASC NULLS LAST"),
        Some("-contacts_count") => ("contacts_count", "DESC NULLS LAST"),
        Some("contacts_count") => ("contacts_count", "ASC NULLS LAST"),
        _ => ("name", "ASC"),
    }
}

pub async fn list_companies(
    Path(account_id): Path<i64>,
    Query(query): Query<CompanyQuery>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let page = query.page.unwrap_or(1).max(1);
    let per_page: i64 = 50;
    let offset = (page - 1) * per_page;

    let (sort_col, sort_dir) = parse_company_sort(query.sort.as_deref());

    let total_count: (i64,) = sqlx::query_as(
        "SELECT count(*) FROM companies WHERE account_id = $1",
    )
    .bind(account_id)
    .fetch_one(&state.db)
    .await
    .unwrap_or((0,));

    let query_sql = format!(
        "SELECT * FROM companies WHERE account_id = $1 ORDER BY {} {} LIMIT $2 OFFSET $3",
        sort_col, sort_dir
    );

    let companies = sqlx::query_as::<_, Company>(&query_sql)
        .bind(account_id)
        .bind(per_page)
        .bind(offset)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();

    let payload: Vec<CompanyResponse> = companies.into_iter().map(CompanyResponse::from).collect();

    (
        StatusCode::OK,
        Json(CompanyListResponse {
            meta: CompanyListMeta {
                total_count: total_count.0,
                page,
            },
            payload,
        }),
    )
}

pub async fn search_companies(
    Path(account_id): Path<i64>,
    Query(query): Query<CompanySearchQuery>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let q = match query.q {
        Some(ref val) if !val.trim().is_empty() => val.trim().to_string(),
        _ => {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error": "Specify search string with parameter q" })),
            )
                .into_response();
        }
    };

    let page = query.page.unwrap_or(1).max(1);
    let per_page: i64 = 50;
    let offset = (page - 1) * per_page;
    let search_pattern = format!("%{}%", q);

    let (sort_col, sort_dir) = parse_company_sort(query.sort.as_deref());

    let total_count: (i64,) = sqlx::query_as(
        "SELECT count(*) FROM companies WHERE account_id = $1 AND (name ILIKE $2 OR domain ILIKE $2)",
    )
    .bind(account_id)
    .bind(&search_pattern)
    .fetch_one(&state.db)
    .await
    .unwrap_or((0,));

    let query_sql = format!(
        "SELECT * FROM companies WHERE account_id = $1 AND (name ILIKE $2 OR domain ILIKE $2) ORDER BY {} {} LIMIT $3 OFFSET $4",
        sort_col, sort_dir
    );

    let companies = sqlx::query_as::<_, Company>(&query_sql)
        .bind(account_id)
        .bind(&search_pattern)
        .bind(per_page)
        .bind(offset)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();

    let payload: Vec<CompanyResponse> = companies.into_iter().map(CompanyResponse::from).collect();

    (
        StatusCode::OK,
        Json(CompanyListResponse {
            meta: CompanyListMeta {
                total_count: total_count.0,
                page,
            },
            payload,
        }),
    )
        .into_response()
}

pub async fn get_company(
    Path((account_id, id)): Path<(i64, i64)>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let company = match sqlx::query_as::<_, Company>(
        "SELECT * FROM companies WHERE account_id = $1 AND id = $2",
    )
    .bind(account_id)
    .bind(id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(c)) => c,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Company not found" })),
            )
                .into_response();
        }
    };

    (
        StatusCode::OK,
        Json(json!({ "payload": CompanyResponse::from(company) })),
    )
        .into_response()
}

pub async fn create_company(
    Path(account_id): Path<i64>,
    State(state): State<AppState>,
    Json(payload): Json<CompanyPayloadWrapper>,
) -> impl IntoResponse {
    let fields = payload.into_fields();
    let name = match fields.name {
        Some(ref n) if !n.trim().is_empty() => n.trim().to_string(),
        _ => {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error": "Company name is required" })),
            )
                .into_response();
        }
    };

    let domain = fields.domain.filter(|d| !d.trim().is_empty());
    let description = fields.description;
    let additional_attributes = fields.additional_attributes.unwrap_or_else(|| json!({}));
    let custom_attributes = fields.custom_attributes.unwrap_or_else(|| json!({}));

    if let Some(ref d) = domain {
        let existing: Option<(i64,)> = sqlx::query_as(
            "SELECT id FROM companies WHERE account_id = $1 AND domain ILIKE $2",
        )
        .bind(account_id)
        .bind(d)
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None);

        if existing.is_some() {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error": "Domain has already been taken" })),
            )
                .into_response();
        }
    }

    let company = match sqlx::query_as::<_, Company>(
        r#"
        INSERT INTO companies (name, domain, description, account_id, contacts_count, additional_attributes, custom_attributes, created_at, updated_at)
        VALUES ($1, $2, $3, $4, 0, $5, $6, NOW(), NOW())
        RETURNING *
        "#,
    )
    .bind(name)
    .bind(domain)
    .bind(description)
    .bind(account_id)
    .bind(additional_attributes)
    .bind(custom_attributes)
    .fetch_one(&state.db)
    .await
    {
        Ok(c) => c,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    (
        StatusCode::OK,
        Json(json!({ "payload": CompanyResponse::from(company) })),
    )
        .into_response()
}

pub async fn update_company(
    Path((account_id, id)): Path<(i64, i64)>,
    State(state): State<AppState>,
    Json(payload): Json<CompanyPayloadWrapper>,
) -> impl IntoResponse {
    let company = match sqlx::query_as::<_, Company>(
        "SELECT * FROM companies WHERE account_id = $1 AND id = $2",
    )
    .bind(account_id)
    .bind(id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(c)) => c,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Company not found" })),
            )
                .into_response();
        }
    };

    let fields = payload.into_fields();

    let name = fields.name.unwrap_or(company.name);
    let domain = if fields.domain.is_some() {
        fields.domain.filter(|d| !d.trim().is_empty())
    } else {
        company.domain
    };
    let description = if fields.description.is_some() {
        fields.description
    } else {
        company.description
    };

    let additional_attributes = if let Some(new_add) = fields.additional_attributes {
        let mut base = company.additional_attributes;
        if let (Some(b_obj), Some(n_obj)) = (base.as_object_mut(), new_add.as_object()) {
            for (k, v) in n_obj {
                b_obj.insert(k.clone(), v.clone());
            }
        }
        base
    } else {
        company.additional_attributes
    };

    let custom_attributes = if let Some(new_custom) = fields.custom_attributes {
        let mut base = company.custom_attributes;
        if let (Some(b_obj), Some(n_obj)) = (base.as_object_mut(), new_custom.as_object()) {
            for (k, v) in n_obj {
                b_obj.insert(k.clone(), v.clone());
            }
        }
        base
    } else {
        company.custom_attributes
    };

    if let Some(ref d) = domain {
        let existing: Option<(i64,)> = sqlx::query_as(
            "SELECT id FROM companies WHERE account_id = $1 AND domain ILIKE $2 AND id != $3",
        )
        .bind(account_id)
        .bind(d)
        .bind(id)
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None);

        if existing.is_some() {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error": "Domain has already been taken" })),
            )
                .into_response();
        }
    }

    let updated = match sqlx::query_as::<_, Company>(
        r#"
        UPDATE companies
        SET name = $1, domain = $2, description = $3, additional_attributes = $4, custom_attributes = $5, updated_at = NOW()
        WHERE id = $6 AND account_id = $7
        RETURNING *
        "#,
    )
    .bind(name)
    .bind(domain)
    .bind(description)
    .bind(additional_attributes)
    .bind(custom_attributes)
    .bind(id)
    .bind(account_id)
    .fetch_one(&state.db)
    .await
    {
        Ok(c) => c,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    (
        StatusCode::OK,
        Json(json!({ "payload": CompanyResponse::from(updated) })),
    )
        .into_response()
}

pub async fn delete_company(
    Path((account_id, id)): Path<(i64, i64)>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let _ = sqlx::query(
        "UPDATE contacts SET company_id = NULL WHERE company_id = $1 AND account_id = $2",
    )
    .bind(id)
    .bind(account_id as i32)
    .execute(&state.db)
    .await;

    let res = sqlx::query("DELETE FROM companies WHERE id = $1 AND account_id = $2")
        .bind(id)
        .bind(account_id)
        .execute(&state.db)
        .await;

    match res {
        Ok(_) => StatusCode::OK.into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

pub async fn destroy_custom_attributes(
    Path((account_id, id)): Path<(i64, i64)>,
    State(state): State<AppState>,
    Json(payload): Json<DestroyCustomAttributesPayload>,
) -> impl IntoResponse {
    let mut company = match sqlx::query_as::<_, Company>(
        "SELECT * FROM companies WHERE account_id = $1 AND id = $2",
    )
    .bind(account_id)
    .bind(id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(c)) => c,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Company not found" })),
            )
                .into_response();
        }
    };

    if let Some(obj) = company.custom_attributes.as_object_mut() {
        for key in payload.custom_attributes {
            obj.remove(&key);
        }
    }

    let updated = match sqlx::query_as::<_, Company>(
        "UPDATE companies SET custom_attributes = $1, updated_at = NOW() WHERE id = $2 AND account_id = $3 RETURNING *",
    )
    .bind(company.custom_attributes)
    .bind(id)
    .bind(account_id)
    .fetch_one(&state.db)
    .await
    {
        Ok(c) => c,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    (
        StatusCode::OK,
        Json(json!({ "payload": CompanyResponse::from(updated) })),
    )
        .into_response()
}

pub async fn destroy_avatar(
    Path((account_id, id)): Path<(i64, i64)>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let company = match sqlx::query_as::<_, Company>(
        "SELECT * FROM companies WHERE account_id = $1 AND id = $2",
    )
    .bind(account_id)
    .bind(id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(c)) => c,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Company not found" })),
            )
                .into_response();
        }
    };

    (
        StatusCode::OK,
        Json(json!({ "payload": CompanyResponse::from(company) })),
    )
        .into_response()
}

pub async fn enrich_company(
    Path((account_id, id)): Path<(i64, i64)>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let company = match sqlx::query_as::<_, Company>(
        "SELECT * FROM companies WHERE account_id = $1 AND id = $2",
    )
    .bind(account_id)
    .bind(id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(c)) => c,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Company not found" })),
            )
                .into_response();
        }
    };

    (
        StatusCode::OK,
        Json(json!({ "payload": CompanyResponse::from(company) })),
    )
        .into_response()
}

pub async fn list_company_contacts(
    Path((account_id, id)): Path<(i64, i64)>,
    Query(query): Query<ContactFilterQuery>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let company = match sqlx::query_as::<_, Company>(
        "SELECT * FROM companies WHERE account_id = $1 AND id = $2",
    )
    .bind(account_id)
    .bind(id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(c)) => c,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Company not found" })),
            )
                .into_response();
        }
    };

    let page = query.page.unwrap_or(1).max(1);
    let per_page: i64 = 25;
    let offset = (page - 1) * per_page;

    let company_resp = CompanyResponse::from(company);

    let (total_count, contacts) = if let Some(ref q) = query.q.filter(|s| !s.trim().is_empty()) {
        let pattern = format!("%{}%", q.trim());
        let count: (i64,) = sqlx::query_as(
            r#"
            SELECT count(*) FROM contacts
            WHERE account_id = $1 AND company_id = $2
              AND (name ILIKE $3 OR email ILIKE $3 OR phone_number ILIKE $3 OR identifier ILIKE $3)
            "#,
        )
        .bind(account_id as i32)
        .bind(id)
        .bind(&pattern)
        .fetch_one(&state.db)
        .await
        .unwrap_or((0,));

        let rows = sqlx::query_as::<_, Contact>(
            r#"
            SELECT * FROM contacts
            WHERE account_id = $1 AND company_id = $2
              AND (name ILIKE $3 OR email ILIKE $3 OR phone_number ILIKE $3 OR identifier ILIKE $3)
            ORDER BY name ASC, id ASC
            LIMIT $4 OFFSET $5
            "#,
        )
        .bind(account_id as i32)
        .bind(id)
        .bind(&pattern)
        .bind(per_page)
        .bind(offset)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();

        (count.0, rows)
    } else {
        let count: (i64,) = sqlx::query_as(
            "SELECT count(*) FROM contacts WHERE account_id = $1 AND company_id = $2",
        )
        .bind(account_id as i32)
        .bind(id)
        .fetch_one(&state.db)
        .await
        .unwrap_or((0,));

        let rows = sqlx::query_as::<_, Contact>(
            r#"
            SELECT * FROM contacts
            WHERE account_id = $1 AND company_id = $2
            ORDER BY name ASC, id ASC
            LIMIT $3 OFFSET $4
            "#,
        )
        .bind(account_id as i32)
        .bind(id)
        .bind(per_page)
        .bind(offset)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();

        (count.0, rows)
    };

    let payload: Vec<CompanyContactResponse> = contacts
        .into_iter()
        .map(|c| CompanyContactResponse {
            id: c.id,
            name: c.name.unwrap_or_else(|| "Customer".to_string()),
            email: c.email,
            phone_number: c.phone_number,
            identifier: c.identifier,
            thumbnail: "".to_string(),
            additional_attributes: c.additional_attributes.unwrap_or_else(|| json!({})),
            custom_attributes: c.custom_attributes.unwrap_or_else(|| json!({})),
            created_at: c.created_at.and_utc().timestamp(),
            company_id: Some(id),
            linked_to_current_company: true,
            company: Some(company_resp.clone()),
        })
        .collect();

    (
        StatusCode::OK,
        Json(CompanyContactListResponse {
            meta: CompanyListMeta {
                total_count,
                page,
            },
            payload,
        }),
    )
        .into_response()
}

pub async fn search_company_contacts(
    Path((account_id, id)): Path<(i64, i64)>,
    Query(query): Query<CompanySearchQuery>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let _company = match sqlx::query_as::<_, Company>(
        "SELECT * FROM companies WHERE account_id = $1 AND id = $2",
    )
    .bind(account_id)
    .bind(id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(c)) => c,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Company not found" })),
            )
                .into_response();
        }
    };

    let q = match query.q {
        Some(ref val) if !val.trim().is_empty() => val.trim().to_string(),
        _ => {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error": "Specify search string with parameter q" })),
            )
                .into_response();
        }
    };

    let page = query.page.unwrap_or(1).max(1);
    let per_page: i64 = 25;
    let offset = (page - 1) * per_page;
    let pattern = format!("%{}%", q);

    let count: (i64,) = sqlx::query_as(
        r#"
        SELECT count(*) FROM contacts
        WHERE account_id = $1 AND (company_id IS NULL OR company_id != $2)
          AND (name ILIKE $3 OR email ILIKE $3 OR phone_number ILIKE $3 OR identifier ILIKE $3)
        "#,
    )
    .bind(account_id as i32)
    .bind(id)
    .bind(&pattern)
    .fetch_one(&state.db)
    .await
    .unwrap_or((0,));

    let contacts = sqlx::query_as::<_, Contact>(
        r#"
        SELECT * FROM contacts
        WHERE account_id = $1 AND (company_id IS NULL OR company_id != $2)
          AND (name ILIKE $3 OR email ILIKE $3 OR phone_number ILIKE $3 OR identifier ILIKE $3)
        ORDER BY name ASC, id ASC
        LIMIT $4 OFFSET $5
        "#,
    )
    .bind(account_id as i32)
    .bind(id)
    .bind(&pattern)
    .bind(per_page)
    .bind(offset)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let mut payload = Vec::new();
    for c in contacts {
        let other_company = if let Some(cid) = c.company_id {
            sqlx::query_as::<_, Company>("SELECT * FROM companies WHERE account_id = $1 AND id = $2")
                .bind(account_id)
                .bind(cid)
                .fetch_optional(&state.db)
                .await
                .ok()
                .flatten()
                .map(CompanyResponse::from)
        } else {
            None
        };

        payload.push(CompanyContactResponse {
            id: c.id,
            name: c.name.unwrap_or_else(|| "Customer".to_string()),
            email: c.email,
            phone_number: c.phone_number,
            identifier: c.identifier,
            thumbnail: "".to_string(),
            additional_attributes: c.additional_attributes.unwrap_or_else(|| json!({})),
            custom_attributes: c.custom_attributes.unwrap_or_else(|| json!({})),
            created_at: c.created_at.and_utc().timestamp(),
            company_id: c.company_id,
            linked_to_current_company: c.company_id == Some(id),
            company: other_company,
        });
    }

    (
        StatusCode::OK,
        Json(CompanyContactListResponse {
            meta: CompanyListMeta {
                total_count: count.0,
                page,
            },
            payload,
        }),
    )
        .into_response()
}

pub async fn attach_company_contact(
    Path((account_id, id)): Path<(i64, i64)>,
    State(state): State<AppState>,
    Json(payload): Json<AttachContactPayload>,
) -> impl IntoResponse {
    let company = match sqlx::query_as::<_, Company>(
        "SELECT * FROM companies WHERE account_id = $1 AND id = $2",
    )
    .bind(account_id)
    .bind(id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(c)) => c,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Company not found" })),
            )
                .into_response();
        }
    };

    let contact = match sqlx::query_as::<_, Contact>(
        "SELECT * FROM contacts WHERE account_id = $1 AND id = $2",
    )
    .bind(account_id as i32)
    .bind(payload.contact_id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(c)) => c,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Contact not found" })),
            )
                .into_response();
        }
    };

    let updated_contact = match sqlx::query_as::<_, Contact>(
        "UPDATE contacts SET company_id = $1, updated_at = NOW() WHERE id = $2 AND account_id = $3 RETURNING *",
    )
    .bind(id)
    .bind(contact.id)
    .bind(account_id as i32)
    .fetch_one(&state.db)
    .await
    {
        Ok(c) => c,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    let _ = sqlx::query(
        "UPDATE companies SET contacts_count = (SELECT count(*) FROM contacts WHERE company_id = $1 AND account_id = $2), updated_at = NOW() WHERE id = $1 AND account_id = $2",
    )
    .bind(id)
    .bind(account_id)
    .execute(&state.db)
    .await;

    let company_resp = CompanyResponse::from(company);

    let contact_resp = CompanyContactResponse {
        id: updated_contact.id,
        name: updated_contact.name.unwrap_or_else(|| "Customer".to_string()),
        email: updated_contact.email,
        phone_number: updated_contact.phone_number,
        identifier: updated_contact.identifier,
        thumbnail: "".to_string(),
        additional_attributes: updated_contact.additional_attributes.unwrap_or_else(|| json!({})),
        custom_attributes: updated_contact.custom_attributes.unwrap_or_else(|| json!({})),
        created_at: updated_contact.created_at.and_utc().timestamp(),
        company_id: Some(id),
        linked_to_current_company: true,
        company: Some(company_resp),
    };

    (
        StatusCode::OK,
        Json(json!({ "payload": contact_resp })),
    )
        .into_response()
}

pub async fn remove_company_contact(
    Path((account_id, id, contact_id)): Path<(i64, i64, i32)>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let _ = sqlx::query(
        "UPDATE contacts SET company_id = NULL, updated_at = NOW() WHERE id = $1 AND account_id = $2 AND company_id = $3",
    )
    .bind(contact_id)
    .bind(account_id as i32)
    .bind(id)
    .execute(&state.db)
    .await;

    let _ = sqlx::query(
        "UPDATE companies SET contacts_count = (SELECT count(*) FROM contacts WHERE company_id = $1 AND account_id = $2), updated_at = NOW() WHERE id = $1 AND account_id = $2",
    )
    .bind(id)
    .bind(account_id)
    .execute(&state.db)
    .await;

    StatusCode::OK.into_response()
}

pub async fn list_company_conversations(
    Path((account_id, id)): Path<(i64, i64)>,
    Query(query): Query<CompanyQuery>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let page = query.page.unwrap_or(1).max(1);
    let per_page: i64 = 25;
    let offset = (page - 1) * per_page;

    let all_count: (i64,) = sqlx::query_as(
        r#"
        SELECT count(*) FROM conversations
        WHERE account_id = $1 AND contact_id IN (SELECT id FROM contacts WHERE company_id = $2 AND account_id = $1)
        "#,
    )
    .bind(account_id as i32)
    .bind(id)
    .fetch_one(&state.db)
    .await
    .unwrap_or((0,));

    let open_count: (i64,) = sqlx::query_as(
        r#"
        SELECT count(*) FROM conversations
        WHERE account_id = $1 AND status = 0 AND contact_id IN (SELECT id FROM contacts WHERE company_id = $2 AND account_id = $1)
        "#,
    )
    .bind(account_id as i32)
    .bind(id)
    .fetch_one(&state.db)
    .await
    .unwrap_or((0,));

    let conversations = sqlx::query_as::<_, Conversation>(
        r#"
        SELECT * FROM conversations
        WHERE account_id = $1 AND contact_id IN (SELECT id FROM contacts WHERE company_id = $2 AND account_id = $1)
        ORDER BY last_activity_at DESC
        LIMIT $3 OFFSET $4
        "#,
    )
    .bind(account_id as i32)
    .bind(id)
    .bind(per_page)
    .bind(offset)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let mut payload = Vec::new();
    for conv in conversations {
        let sender = if let Some(contact_id) = conv.contact_id {
            sqlx::query_as::<_, Contact>("SELECT * FROM contacts WHERE id = $1")
                .bind(contact_id as i32)
                .fetch_optional(&state.db)
                .await
                .ok()
                .flatten()
                .map(ContactResponse::from)
        } else {
            None
        };

        let channel_type = sqlx::query_scalar::<_, Option<String>>(
            "SELECT channel_type FROM inboxes WHERE id = $1",
        )
        .bind(conv.inbox_id)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
        .flatten()
        .unwrap_or_else(|| "Channel::Api".to_string());

        let last_msg: Option<MessageResponse> = sqlx::query_as::<_, Message>(
            "SELECT * FROM messages WHERE conversation_id = $1 ORDER BY created_at DESC LIMIT 1",
        )
        .bind(conv.id)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
        .map(|m| m.to_response(conv.display_id, None, vec![]));

        payload.push(conv.to_response(sender, channel_type, None, last_msg, 0));
    }

    (
        StatusCode::OK,
        Json(CompanyConversationListResponse {
            meta: CompanyConversationListMeta {
                total_count: all_count.0,
                all_count: all_count.0,
                open_count: open_count.0,
                page,
            },
            payload,
        }),
    )
        .into_response()
}

pub async fn filter_company_conversations(
    path: Path<(i64, i64)>,
    query: Query<CompanyQuery>,
    state: State<AppState>,
    _json: axum::body::Bytes,
) -> impl IntoResponse {
    list_company_conversations(path, query, state).await
}

pub async fn list_company_notes(
    Query(query): Query<CompanyQuery>,
) -> impl IntoResponse {
    let page = query.page.unwrap_or(1).max(1);
    (
        StatusCode::OK,
        Json(json!({
            "meta": {
                "total_count": 0,
                "page": page
            },
            "payload": []
        })),
    )
}
