use dashmap::DashMap;
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::broadcast;

use crate::config::Config;

pub type AccountId = i32;
pub type UserId = i32;

#[allow(dead_code)]
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct CableBroadcastMsg {
    pub account_id: AccountId,
    pub event: String,
    pub data: serde_json::Value,
}

#[allow(dead_code)]
#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub config: Arc<Config>,
    pub presence: Arc<DashMap<AccountId, DashMap<UserId, Instant>>>,
    pub broadcast_tx: broadcast::Sender<CableBroadcastMsg>,
}

impl AppState {
    pub fn new(db: PgPool, config: Config) -> Self {
        let (broadcast_tx, _broadcast_rx) = broadcast::channel(1024);
        Self {
            db,
            config: Arc::new(config),
            presence: Arc::new(DashMap::new()),
            broadcast_tx,
        }
    }
}
