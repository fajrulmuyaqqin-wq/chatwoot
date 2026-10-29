use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Audit {
    pub id: i32,
    pub auditable_id: Option<i64>,
    pub auditable_type: Option<String>,
    pub associated_id: Option<i64>,
    pub associated_type: Option<String>,
    pub user_id: Option<i64>,
    pub user_type: Option<String>,
    pub username: Option<String>,
    pub action: Option<String>,
    pub audited_changes: Option<serde_json::Value>,
    pub version: i32,
    pub comment: Option<String>,
    pub remote_address: Option<String>,
    pub request_uuid: Option<String>,
    pub created_at: Option<NaiveDateTime>,
    pub city: Option<String>,
    pub country: Option<String>,
    pub country_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditResponse {
    pub id: i64,
    pub auditable_id: Option<i64>,
    pub auditable_type: Option<String>,
    pub auditable: Option<serde_json::Value>,
    pub associated_id: Option<i64>,
    pub associated_type: Option<String>,
    pub user_id: Option<i64>,
    pub user_type: Option<String>,
    pub username: Option<String>,
    pub action: String,
    pub audited_changes: Option<serde_json::Value>,
    pub version: i32,
    pub comment: Option<String>,
    pub request_uuid: Option<String>,
    pub created_at: i64,
    pub remote_address: Option<String>,
    pub location: Option<String>,
}

pub fn mask_ip(ip: &str) -> String {
    if let Ok(ipv4) = ip.parse::<std::net::Ipv4Addr>() {
        let octets = ipv4.octets();
        format!("{}.{}.{}.x", octets[0], octets[1], octets[2])
    } else if let Ok(ipv6) = ip.parse::<std::net::Ipv6Addr>() {
        let segments = ipv6.segments();
        format!("{:x}:{:x}:{:x}:{:x}::", segments[0], segments[1], segments[2], segments[3])
    } else {
        ip.to_string()
    }
}

impl From<Audit> for AuditResponse {
    fn from(a: Audit) -> Self {
        let location = match (&a.city, &a.country) {
            (Some(c), Some(co)) if !c.is_empty() && !co.is_empty() => Some(format!("{}, {}", c, co)),
            (Some(c), _) if !c.is_empty() => Some(c.clone()),
            (_, Some(co)) if !co.is_empty() => Some(co.clone()),
            _ => None,
        };

        let masked_addr = a.remote_address.as_deref().map(mask_ip);

        // Sanitize audited_changes if auditable_type is Message (preserve privacy of message content)
        let sanitized_changes = if a.auditable_type.as_deref() == Some("Message") {
            if let Some(serde_json::Value::Object(mut map)) = a.audited_changes {
                map.remove("content");
                Some(serde_json::Value::Object(map))
            } else {
                a.audited_changes
            }
        } else {
            a.audited_changes
        };

        Self {
            id: a.id as i64,
            auditable_id: a.auditable_id,
            auditable_type: a.auditable_type,
            auditable: None,
            associated_id: a.associated_id,
            associated_type: a.associated_type,
            user_id: a.user_id,
            user_type: a.user_type,
            username: a.username,
            action: a.action.unwrap_or_else(|| "unknown".to_string()),
            audited_changes: sanitized_changes,
            version: a.version,
            comment: a.comment,
            request_uuid: a.request_uuid,
            created_at: a.created_at.map(|t| t.and_utc().timestamp()).unwrap_or(0),
            remote_address: masked_addr,
            location,
        }
    }
}
