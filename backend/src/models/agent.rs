use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AgentResponse {
    pub id: i32,
    pub name: String,
    pub email: Option<String>,
    pub role: String,
    pub availability_status: String,
    pub auto_offline: bool,
    pub confirmed: bool,
}

#[derive(Debug, Deserialize)]
pub struct CreateAgentPayload {
    pub name: String,
    pub email: String,
    pub role: Option<String>,
    pub availability: Option<String>,
    pub auto_offline: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateAgentPayload {
    pub name: Option<String>,
    pub role: Option<String>,
    pub availability: Option<String>,
    pub auto_offline: Option<bool>,
}
