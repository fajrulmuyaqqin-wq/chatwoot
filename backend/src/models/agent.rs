use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CustomRoleBrief {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub permissions: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AgentResponse {
    pub id: i32,
    pub name: String,
    pub email: Option<String>,
    pub role: String,
    pub availability_status: String,
    pub auto_offline: bool,
    pub confirmed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_role_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_role: Option<CustomRoleBrief>,
}

#[derive(Debug, Deserialize)]
pub struct CreateAgentPayload {
    pub name: String,
    pub email: String,
    pub role: Option<String>,
    pub availability: Option<String>,
    pub auto_offline: Option<bool>,
    pub custom_role_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateAgentPayload {
    pub name: Option<String>,
    pub role: Option<String>,
    pub availability: Option<String>,
    pub auto_offline: Option<bool>,
    pub custom_role_id: Option<i64>,
}
