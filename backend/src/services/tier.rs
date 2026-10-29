use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgPool;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AccountTier {
    Free,
    Small,
    Max,
    Enterprise,
}

impl AccountTier {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "free" => Some(Self::Free),
            "small" => Some(Self::Small),
            "max" => Some(Self::Max),
            "enterprise" => Some(Self::Enterprise),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Free => "free",
            Self::Small => "small",
            Self::Max => "max",
            Self::Enterprise => "enterprise",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Free => "Free",
            Self::Small => "Small",
            Self::Max => "Max",
            Self::Enterprise => "Enterprise (Chat Admin)",
        }
    }

    pub fn badge(&self) -> &'static str {
        match self {
            Self::Free => "Gratis",
            Self::Small => "Populer",
            Self::Max => "Advanced",
            Self::Enterprise => "Chat Admin",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Self::Free => "Untuk pemula & startup yang membutuhkan live chat dan email dasar.",
            Self::Small => "Untuk tim kecil dengan WhatsApp Cloud API & Omnichannel terintegrasi.",
            Self::Max => "Untuk bisnis bertumbuh dengan volume chat tinggi, SLA & AI assist.",
            Self::Enterprise => "Skala korporasi dengan Chat Admin, Custom Roles, Audit Logs & Unlimited.",
        }
    }

    pub fn agent_limit(&self) -> i64 {
        match self {
            Self::Free => 2,
            Self::Small => 5,
            Self::Max => 15,
            Self::Enterprise => -1, // Unlimited
        }
    }

    pub fn inbox_limit(&self) -> i64 {
        match self {
            Self::Free => 2,
            Self::Small => 5,
            Self::Max => 20,
            Self::Enterprise => -1, // Unlimited
        }
    }

    pub fn price_monthly(&self) -> i64 {
        match self {
            Self::Free => 0,
            Self::Small => 299_000,
            Self::Max => 799_000,
            Self::Enterprise => 1_999_000,
        }
    }

    pub fn limits(&self) -> Value {
        json!({
            "agents": self.agent_limit(),
            "inboxes": self.inbox_limit(),
            "monthly_conversations": match self {
                Self::Free => 500,
                Self::Small => 5000,
                Self::Max => 25000,
                Self::Enterprise => -1,
            }
        })
    }

    pub fn features(&self) -> Value {
        match self {
            Self::Free => json!({
                "inbound_emails": true,
                "channel_email": true,
                "channel_website": true,
                "canned_responses": true,
                "labels": true,
                "custom_attributes": true,
                "channel_whatsapp": false,
                "channel_instagram": false,
                "channel_facebook": false,
                "channel_telegram": false,
                "channel_shopee": false,
                "campaigns": false,
                "campaign_analytics": false,
                "sla": false,
                "captain_integration": false,
                "captain_integration_v2": false,
                "custom_roles": false,
                "audit_logs": false,
                "help_center": true,
                "companies": true,
                "advanced_search": false,
                "macros": false,
                "automations": false,
                "agent_management": true,
                "team_management": false,
                "inbox_management": true,
                "api_and_webhooks": true,
                "saml": false,
                "auto_resolve_conversations": false,
                "conversation_required_attributes": false,
                "advanced_assignment": false,
                "assignment_v2": false,
                "conversation_monitors": false,
                "reports": true,
                "data_import": false,
                "agent_bots": false,
                "voice_recorder": false,
                "channel_voice": false,
                "auto_translate": false,
                "chat_admin": false
            }),
            Self::Small => json!({
                "inbound_emails": true,
                "channel_email": true,
                "channel_website": true,
                "channel_whatsapp": true,
                "channel_instagram": true,
                "channel_facebook": true,
                "channel_telegram": true,
                "channel_shopee": false,
                "canned_responses": true,
                "labels": true,
                "custom_attributes": true,
                "campaigns": true,
                "campaign_analytics": false,
                "sla": false,
                "captain_integration": false,
                "captain_integration_v2": false,
                "custom_roles": false,
                "audit_logs": false,
                "help_center": true,
                "companies": true,
                "advanced_search": true,
                "macros": true,
                "automations": true,
                "agent_management": true,
                "team_management": false,
                "inbox_management": true,
                "api_and_webhooks": true,
                "saml": false,
                "auto_resolve_conversations": false,
                "conversation_required_attributes": true,
                "advanced_assignment": false,
                "assignment_v2": false,
                "conversation_monitors": false,
                "reports": true,
                "data_import": true,
                "agent_bots": false,
                "voice_recorder": false,
                "channel_voice": false,
                "auto_translate": false,
                "chat_admin": false
            }),
            Self::Max => json!({
                "inbound_emails": true,
                "channel_email": true,
                "channel_website": true,
                "channel_whatsapp": true,
                "channel_instagram": true,
                "channel_facebook": true,
                "channel_telegram": true,
                "channel_shopee": true,
                "canned_responses": true,
                "labels": true,
                "custom_attributes": true,
                "campaigns": true,
                "campaign_analytics": true,
                "sla": true,
                "captain_integration": true,
                "captain_integration_v2": true,
                "custom_roles": false,
                "audit_logs": false,
                "help_center": true,
                "companies": true,
                "advanced_search": true,
                "macros": true,
                "automations": true,
                "agent_management": true,
                "team_management": true,
                "inbox_management": true,
                "api_and_webhooks": true,
                "saml": false,
                "auto_resolve_conversations": true,
                "conversation_required_attributes": true,
                "advanced_assignment": true,
                "assignment_v2": true,
                "conversation_monitors": true,
                "reports": true,
                "data_import": true,
                "agent_bots": true,
                "voice_recorder": true,
                "channel_voice": false,
                "auto_translate": true,
                "chat_admin": false
            }),
            Self::Enterprise => json!({
                "inbound_emails": true,
                "channel_email": true,
                "channel_website": true,
                "channel_whatsapp": true,
                "channel_instagram": true,
                "channel_facebook": true,
                "channel_telegram": true,
                "channel_shopee": true,
                "canned_responses": true,
                "labels": true,
                "custom_attributes": true,
                "campaigns": true,
                "campaign_analytics": true,
                "sla": true,
                "captain_integration": true,
                "captain_integration_v2": true,
                "custom_tools": true,
                "custom_roles": true,
                "audit_logs": true,
                "help_center": true,
                "companies": true,
                "advanced_search": true,
                "macros": true,
                "automations": true,
                "agent_management": true,
                "team_management": true,
                "inbox_management": true,
                "api_and_webhooks": true,
                "saml": true,
                "auto_resolve_conversations": true,
                "conversation_required_attributes": true,
                "advanced_assignment": true,
                "assignment_v2": true,
                "conversation_monitors": true,
                "reports": true,
                "data_import": true,
                "agent_bots": true,
                "voice_recorder": true,
                "channel_voice": true,
                "auto_translate": true,
                "chat_admin": true
            }),
        }
    }

    pub fn all_tiers_catalog() -> Vec<Value> {
        vec![
            json!({
                "id": "free",
                "name": "Free",
                "badge": "Gratis",
                "description": Self::Free.description(),
                "price_monthly": Self::Free.price_monthly(),
                "agent_limit": Self::Free.agent_limit(),
                "inbox_limit": Self::Free.inbox_limit(),
                "highlight_features": [
                    "2 Agen & 2 Inbox",
                    "Live Chat Widget & Email",
                    "Canned Responses & Labels",
                    "Laporan Dasar"
                ]
            }),
            json!({
                "id": "small",
                "name": "Small",
                "badge": "Populer",
                "description": Self::Small.description(),
                "price_monthly": Self::Small.price_monthly(),
                "agent_limit": Self::Small.agent_limit(),
                "inbox_limit": Self::Small.inbox_limit(),
                "highlight_features": [
                    "5 Agen & 5 Inbox",
                    "WhatsApp Cloud API Resmi",
                    "Instagram Direct & Telegram",
                    "Macro & Basic Automations"
                ]
            }),
            json!({
                "id": "max",
                "name": "Max",
                "badge": "Advanced",
                "description": Self::Max.description(),
                "price_monthly": Self::Max.price_monthly(),
                "agent_limit": Self::Max.agent_limit(),
                "inbox_limit": Self::Max.inbox_limit(),
                "highlight_features": [
                    "15 Agen & 20 Inbox",
                    "Semua Channel + Shopee",
                    "SLA Management & CSAT",
                    "Conversation Monitors & Auto Assignment",
                    "Captain AI Bot Integration"
                ]
            }),
            json!({
                "id": "enterprise",
                "name": "Enterprise (Chat Admin)",
                "badge": "Chat Admin",
                "description": Self::Enterprise.description(),
                "price_monthly": Self::Enterprise.price_monthly(),
                "agent_limit": -1,
                "inbox_limit": -1,
                "highlight_features": [
                    "Unlimited Agen & Inbox",
                    "Dedicated Chat Admin Governance",
                    "Custom Roles & Granular Permissions",
                    "Audit Logs & Security Compliance",
                    "Captain AI Full Suite & Custom Tools",
                    "SAML SSO & Prioritas Webhook Routing"
                ]
            }),
        ]
    }
}

/// Helper to get an account's active tier and registration status
pub async fn get_account_tier(
    db: &PgPool,
    account_id: i64,
) -> Result<(AccountTier, bool), (StatusCode, String)> {
    let row = sqlx::query!(
        "SELECT custom_attributes FROM accounts WHERE id = $1",
        account_id as i32
    )
    .fetch_optional(db)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Database error fetching account tier: {}", e),
        )
    })?;

    match row {
        Some(r) => {
            let attrs = r.custom_attributes.unwrap_or(json!({}));
            let tier_str = attrs
                .get("plan_name")
                .or_else(|| attrs.get("tier"))
                .and_then(|v| v.as_str());
            let is_registered = attrs
                .get("is_plan_registered")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            let tier = tier_str
                .and_then(AccountTier::from_str)
                .unwrap_or(AccountTier::Free);

            Ok((tier, is_registered || tier_str.is_some()))
        }
        None => Err((StatusCode::NOT_FOUND, "Account not found".to_string())),
    }
}

/// Verify whether an account can add another agent within their tier's limits
pub async fn verify_agent_limit(db: &PgPool, account_id: i64) -> Result<(), (StatusCode, String)> {
    let (tier, _) = get_account_tier(db, account_id).await?;
    let limit = tier.agent_limit();
    if limit < 0 {
        return Ok(()); // Unlimited for Enterprise
    }

    let count: i64 = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM account_users WHERE account_id = $1",
        account_id
    )
    .fetch_one(db)
    .await
    .unwrap_or(Some(0))
    .unwrap_or(0);

    if count >= limit {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            format!(
                "Batas agen untuk paket {} telah tercapai ({}/{}). Silakan upgrade paket Anda di Plan Selection untuk menambah agen.",
                tier.display_name(),
                count,
                limit
            ),
        ));
    }

    Ok(())
}

/// Verify whether an account can add another inbox within their tier's limits and channel rules
pub async fn verify_inbox_limit(
    db: &PgPool,
    account_id: i64,
    channel_type: Option<&str>,
) -> Result<(), (StatusCode, String)> {
    let (tier, _) = get_account_tier(db, account_id).await?;
    let limit = tier.inbox_limit();

    // Verify channel type compatibility first
    if let Some(ch) = channel_type {
        let normalized = ch.trim().to_lowercase();
        match tier {
            AccountTier::Free => {
                // Free only supports basic web widget, email, api
                let allowed = [
                    "channel::webwidget",
                    "web_widget",
                    "channel::email",
                    "email",
                    "channel::api",
                    "api",
                ];
                if !allowed.contains(&normalized.as_str()) {
                    return Err((
                        StatusCode::FORBIDDEN,
                        format!(
                            "Channel '{}' tidak tersedia pada paket {}. Silakan upgrade ke Small, Max, atau Enterprise.",
                            ch,
                            tier.display_name()
                        ),
                    ));
                }
            }
            AccountTier::Small => {
                // Small supports web, email, api, whatsapp, telegram, instagram, facebook, line, sms
                // Does NOT support Shopee or voice
                let forbidden = ["channel::shopee", "shopee", "channel::voice", "voice"];
                if forbidden.contains(&normalized.as_str()) {
                    return Err((
                        StatusCode::FORBIDDEN,
                        format!(
                            "Channel '{}' hanya tersedia pada paket Max atau Enterprise.",
                            ch
                        ),
                    ));
                }
            }
            AccountTier::Max => {
                // Max supports all channels except voice/calling (enterprise only)
                let forbidden = ["channel::voice", "voice", "whatsapp_call"];
                if forbidden.contains(&normalized.as_str()) {
                    return Err((
                        StatusCode::FORBIDDEN,
                        "Channel Voice/Call hanya tersedia pada paket Enterprise (Chat Admin)."
                            .to_string(),
                    ));
                }
            }
            AccountTier::Enterprise => {
                // All channels allowed
            }
        }
    }

    if limit >= 0 {
        let count: i64 = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM inboxes WHERE account_id = $1",
            account_id as i32
        )
        .fetch_one(db)
        .await
        .unwrap_or(Some(0))
        .unwrap_or(0);

        if count >= limit {
            return Err((
                StatusCode::UNPROCESSABLE_ENTITY,
                format!(
                    "Batas inbox untuk paket {} telah tercapai ({}/{}). Silakan upgrade paket Anda di Plan Selection untuk menambah inbox.",
                    tier.display_name(),
                    count,
                    limit
                ),
            ));
        }
    }

    Ok(())
}

/// Verify whether an account has access to a specific tier feature
pub async fn verify_feature_access(
    db: &PgPool,
    account_id: i64,
    feature_name: &str,
) -> Result<(), (StatusCode, String)> {
    let (tier, _) = get_account_tier(db, account_id).await?;
    let features = tier.features();

    let is_enabled = features
        .get(feature_name)
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if !is_enabled {
        return Err((
            StatusCode::FORBIDDEN,
            format!(
                "Fitur '{}' tidak tersedia pada paket {}. Silakan upgrade paket Anda di Plan Selection.",
                feature_name,
                tier.display_name()
            ),
        ));
    }

    Ok(())
}

