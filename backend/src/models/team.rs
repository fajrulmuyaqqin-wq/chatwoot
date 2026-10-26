use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Serialize, Deserialize, FromRow, Clone)]
pub struct Team {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub allow_auto_assign: Option<bool>,
    pub account_id: i64,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub icon: Option<String>,
    pub icon_color: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateTeamPayload {
    pub name: String,
    pub description: Option<String>,
    pub allow_auto_assign: Option<bool>,
    pub icon: Option<String>,
    pub icon_color: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateTeamPayload {
    pub name: Option<String>,
    pub description: Option<String>,
    pub allow_auto_assign: Option<bool>,
    pub icon: Option<String>,
    pub icon_color: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TeamMembersPayload {
    pub user_ids: Vec<i64>,
}

#[derive(Debug, Serialize, Deserialize, FromRow, Clone)]
pub struct TeamMember {
    pub id: i64,
    pub team_id: i64,
    pub user_id: i64,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Debug, Serialize, Deserialize, FromRow, Clone)]
pub struct TeamMemberUser {
    pub id: i32,
    pub name: String,
    pub email: Option<String>,
    pub role: Option<String>,
}
